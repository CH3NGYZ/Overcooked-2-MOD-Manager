//! A verified Release can claim loose files only inside a proven MOD directory.
use super::*;
use crate::mod_changes::evaluate::same_guid;
use crate::mod_changes::session::Payload;
use same_file::Handle;
use std::io::ErrorKind;

mod endpoints;
mod loose_files;
mod search;
pub(in crate::mod_changes) use loose_files::LooseFileClaims;
pub(in crate::mod_changes) use search::LooseReleaseSearch;

enum PackageProof {
    /// The live core is absent, so the precise bytes must prove provenance.
    Residue,
    /// Same installed and verified incoming core, at the exact same path.
    Replacement,
}

struct PackageScope<'a> {
    directory: &'a str,
    primary_key: &'a str,
    proof: PackageProof,
}

struct ScopeFailure {
    block: InstallBlock,
    primary: Option<String>,
}

impl ScopeFailure {
    fn at_primary(block: InstallBlock, primary: String) -> Self {
        Self {
            block,
            primary: Some(primary),
        }
    }

    fn report(&self, owner: &str, overlap: &str) -> String {
        self.block
            .report(owner, self.primary.as_deref().unwrap_or(overlap))
    }
}

impl From<InstallBlock> for ScopeFailure {
    fn from(block: InstallBlock) -> Self {
        Self {
            block,
            primary: None,
        }
    }
}

impl PackageScope<'_> {
    fn changes_file(
        &self,
        payload: &Payload,
        source: &str,
        key: &str,
        physical: &str,
        before: &Fingerprint,
        own: bool,
    ) -> Result<bool, InstallBlock> {
        let digest = payload
            .release_files
            .as_ref()
            .and_then(|files| files.get(&path_key(Path::new(source))))
            .ok_or(InstallBlock::NeedsVerifiedRelease)?;
        if !within(physical, self.directory) {
            return Err(InstallBlock::OutsidePackage);
        }
        let changed = !before.same_content(digest);
        match self.proof {
            PackageProof::Replacement => Ok(changed),
            PackageProof::Residue if key != self.primary_key && !changed && !own => Ok(false),
            PackageProof::Residue
                if changed
                    && matches!(before, Fingerprint::File { .. })
                    && matches!(digest, Fingerprint::File { .. }) =>
            {
                Err(InstallBlock::ResidueDiffers)
            }
            PackageProof::Residue => Err(InstallBlock::Unverified),
        }
    }
}

impl Builder<'_> {
    fn registered_map_plugins(
        &self,
    ) -> Result<Vec<crate::level_sets::RegisteredLevelSetPlugin>, String> {
        crate::level_sets::checked_registered_plugins(&self.roots.data_root.join("manifests"), true)
    }

    fn ensure_physically_unclaimed<'a>(
        &self,
        relative: &str,
        real_path: &Path,
        claims: impl Iterator<Item = (&'a str, InstallBlock)>,
    ) -> Result<(), InstallBlock> {
        for map in &self.protected_maps {
            let map_root = game(map)
                .resolve(&self.roots)
                .and_then(|path| crate::game::resolve_existing_ancestor(&path))
                .map_err(|_| InstallBlock::Unverified)?;
            if real_path.starts_with(map_root) {
                return Err(InstallBlock::MapProtected);
            }
        }
        let target = Handle::from_path(real_path).map_err(|_| InstallBlock::Unreadable)?;
        if !has_single_link(target.as_file()) {
            return Err(InstallBlock::AliasesOrLink);
        }
        // Short names and Unicode aliases can reach the same file. Include
        // claims with no scanned MOD and check either physical DLL variant.
        let ensure_other_file = |claim: &str, reason: InstallBlock| {
            let path = game(claim)
                .resolve(&self.roots)
                .map_err(|_| InstallBlock::Unverified)?;
            match Handle::from_path(path) {
                Ok(claim) if target == claim => Err(reason),
                Ok(_) => Ok(()),
                Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
                Err(_) => Err(InstallBlock::Unverified),
            }
        };
        let relative = mods::canonical_dll_name(relative);
        let key = path_key(Path::new(&relative));
        for (claim, reason) in claims {
            let canonical = mods::canonical_dll_name(claim);
            if path_key(Path::new(&canonical)) == key {
                // A case-folded collision can also protect a distinct file on
                // a case-sensitive volume. Don't name an unconfirmed owner.
                return Err(if inventory::key(&canonical) == inventory::key(&relative) {
                    reason
                } else {
                    InstallBlock::AliasesOrLink
                });
            }
            ensure_other_file(claim, reason)?;
            if canonical.to_ascii_lowercase().ends_with(".dll") {
                let other = if canonical == claim {
                    mods::disabled_dll_file(&canonical)
                } else {
                    canonical
                };
                ensure_other_file(&other, reason)?;
            }
        }
        Ok(())
    }

    fn package_scope<'a>(
        &self,
        owner: &str,
        payload: &'a Payload,
    ) -> Result<PackageScope<'a>, ScopeFailure> {
        let unknown = InstallBlock::Unverified;
        // A tag or preview origin alone doesn't prove ZIP contents.
        payload
            .release_files
            .as_ref()
            .ok_or(InstallBlock::NeedsVerifiedRelease)?;
        let manifest = payload.manifest.as_ref().ok_or(unknown)?;
        let binding = manifest.repository_binding.as_ref().ok_or(unknown)?;
        let repo = self
            .session
            .trusted
            .config
            .repositories
            .iter()
            .find(|repo| Some(&repo.id) == payload.candidate.source_id.as_ref())
            .ok_or(unknown)?;
        let guid = repo.plugin_guid.trim();
        if guid.is_empty()
            || !binding.is_valid()
            || !binding.matches_repository(repo)
            || Some(&binding.tag) != payload.candidate.tag.as_ref()
        {
            return Err(unknown.into());
        }
        let mut incoming = payload
            .candidate
            .provides
            .iter()
            .filter(|member| same_guid(&member.guid, guid));
        let primary_key = incoming
            .next()
            .and_then(|member| member.member_key.as_deref())
            .ok_or(unknown)?;
        if incoming.next().is_some() {
            return Err(unknown.into());
        }
        let primary = payload
            .files
            .iter()
            .find(|path| inventory::key(&mods::canonical_dll_name(path)) == primary_key)
            .ok_or(unknown)?;
        let directory = Path::new(primary)
            .parent()
            .and_then(Path::to_str)
            .ok_or(unknown)?;
        let plugins = mods::file_state::PLUGINS_DIR;
        if !within(directory, plugins)
            || path_key(Path::new(directory)) == path_key(Path::new(plugins))
        {
            return Err(unknown.into());
        }

        let old = self.session.trusted.files.get(owner).ok_or(unknown)?;
        let mut installed = self
            .session
            .trusted
            .inventory
            .nodes
            .iter()
            .flat_map(|node| {
                node.current
                    .iter()
                    .flat_map(move |c| c.provides.iter().map(move |m| (node, c, m)))
            })
            .filter(|(_, _, member)| same_guid(&member.guid, guid));
        let Some((installed_node, candidate, installed_core)) = installed.next() else {
            return old
                .manifests
                .is_empty()
                .then_some(PackageScope {
                    directory,
                    primary_key,
                    proof: PackageProof::Residue,
                })
                .ok_or_else(|| unknown.into());
        };
        if installed.next().is_some()
            || installed_node.id != owner
            || !installed_node.mutable
            || candidate.source_id.as_ref() != Some(&repo.id)
            || installed_core.member_key.as_deref() != Some(primary_key)
            || old.manifests.len() > 1
            || old.manifests.iter().any(|previous| {
                previous.repository_id != manifest.repository_id
                    || previous
                        .repository_binding
                        .as_ref()
                        .is_some_and(|previous_binding| {
                            !previous_binding.is_valid()
                                || !previous_binding.matches_repository(repo)
                                || previous_binding.channel != binding.channel
                        })
            })
        {
            return Err(unknown.into());
        }
        let physical = self.physical(owner, primary_key).map_err(|_| unknown)?;
        self.ensure_owner(&physical, owner).map_err(|_| unknown)?;
        let live = game(&physical).resolve(&self.roots).map_err(|_| unknown)?;
        // The original on-disk path must be the same physical file that the
        // selected Release would replace; normalized keys alone are not proof.
        let canonical = mods::canonical_dll_name(primary);
        let requested = if physical.to_ascii_lowercase().ends_with(".dll.disabled") {
            mods::disabled_dll_file(&canonical)
        } else {
            canonical
        };
        let resolve = |name: &str| {
            let path = game(name).resolve(&self.roots).map_err(|_| unknown)?;
            fs::canonicalize(path).map_err(|_| unknown)
        };
        let handle = Handle::from_path(&live).map_err(|_| unknown)?;
        if !has_single_link(handle.as_file()) {
            return Err(ScopeFailure::at_primary(
                InstallBlock::AliasesOrLink,
                physical,
            ));
        }
        if resolve(&requested)? != fs::canonicalize(&live).map_err(|_| unknown)? {
            return Err(unknown.into());
        }
        let found = mods::read_dll_details(&live, &physical)
            .guid
            .ok_or(unknown)?;
        if !found.eq_ignore_ascii_case(guid) {
            let block = if found.trim().is_empty() || same_guid(&found, guid) {
                unknown
            } else {
                InstallBlock::PluginMismatch
            };
            return Err(ScopeFailure::at_primary(block, physical));
        }
        Ok(PackageScope {
            directory,
            primary_key,
            proof: PackageProof::Replacement,
        })
    }
}
