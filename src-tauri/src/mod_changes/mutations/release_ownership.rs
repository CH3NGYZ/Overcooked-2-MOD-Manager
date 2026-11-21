//! A verified Release can claim loose files only inside a proven MOD directory.
use super::*;
use crate::mod_changes::evaluate::same_guid;
use crate::mod_changes::session::Payload;
use same_file::Handle;
use std::io::ErrorKind;

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

impl PackageScope<'_> {
    fn changes_file(
        &self,
        payload: &Payload,
        source: &str,
        key: &str,
        physical: &str,
        before: &Fingerprint,
        own: bool,
    ) -> Option<bool> {
        let digest = payload
            .release_files
            .as_ref()?
            .get(&path_key(Path::new(source)))?;
        let changed = !before.same_content(digest);
        (within(physical, self.directory)
            && (matches!(self.proof, PackageProof::Replacement)
                || (key != self.primary_key && !changed && !own)))
            .then_some(changed)
    }
}

impl Builder<'_> {
    fn registered_map_plugins(
        &self,
    ) -> Result<Vec<crate::level_sets::RegisteredLevelSetPlugin>, String> {
        crate::level_sets::checked_registered_plugins(&self.roots.data_root.join("manifests"), true)
    }

    fn physically_unclaimed<'a>(
        &self,
        relative: &str,
        real_path: &Path,
        claims: impl Iterator<Item = &'a str>,
    ) -> bool {
        if self.protected_maps.iter().any(|map| {
            game(map)
                .resolve(&self.roots)
                .and_then(|path| crate::game::resolve_existing_ancestor(&path))
                .map_or(true, |path| real_path.starts_with(path))
        }) {
            return false;
        }
        let Ok(target) = Handle::from_path(real_path) else {
            return false;
        };
        if !has_single_link(target.as_file()) {
            return false;
        }
        // Short names and Unicode aliases can reach the same file. Include
        // claims with no scanned MOD and check either physical DLL variant.
        let excludes_target = |claim: &str| {
            let Ok(path) = game(claim).resolve(&self.roots) else {
                return false;
            };
            match Handle::from_path(path) {
                Ok(claim) => target != claim,
                Err(error) => error.kind() == ErrorKind::NotFound,
            }
        };
        let key = path_key(Path::new(&mods::canonical_dll_name(relative)));
        claims.into_iter().all(|claim| {
            let canonical = mods::canonical_dll_name(claim);
            if path_key(Path::new(&canonical)) == key {
                return false;
            }
            if !excludes_target(claim) {
                return false;
            }
            if !canonical.to_ascii_lowercase().ends_with(".dll") {
                return true;
            }
            let other = if canonical == claim {
                mods::disabled_dll_file(&canonical)
            } else {
                canonical
            };
            excludes_target(&other)
        })
    }

    fn package_scope<'a>(&self, owner: &str, payload: &'a Payload) -> Option<PackageScope<'a>> {
        // A tag or preview origin alone doesn't prove ZIP contents.
        payload.release_files.as_ref()?;
        let manifest = payload.manifest.as_ref()?;
        let binding = manifest.repository_binding.as_ref()?;
        let repo = self
            .session
            .trusted
            .config
            .repositories
            .iter()
            .find(|repo| Some(&repo.id) == payload.candidate.source_id.as_ref())?;
        let guid = repo.plugin_guid.trim();
        if guid.is_empty()
            || !binding.is_valid()
            || !binding.matches_repository(repo)
            || Some(&binding.tag) != payload.candidate.tag.as_ref()
        {
            return None;
        }
        let mut incoming = payload
            .candidate
            .provides
            .iter()
            .filter(|member| same_guid(&member.guid, guid));
        let primary_key = incoming.next()?.member_key.as_deref()?;
        if incoming.next().is_some() {
            return None;
        }
        let primary = payload
            .files
            .iter()
            .find(|path| inventory::key(&mods::canonical_dll_name(path)) == primary_key)?;
        let directory = Path::new(primary).parent()?.to_str()?;
        let plugins = mods::file_state::PLUGINS_DIR;
        if !within(directory, plugins)
            || path_key(Path::new(directory)) == path_key(Path::new(plugins))
        {
            return None;
        }

        let old = self.session.trusted.files.get(owner)?;
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
            return old.manifests.is_empty().then_some(PackageScope {
                directory,
                primary_key,
                proof: PackageProof::Residue,
            });
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
            return None;
        }
        let physical = self.physical(owner, primary_key).ok()?;
        self.ensure_owner(&physical, owner).ok()?;
        let live = game(&physical).resolve(&self.roots).ok()?;
        // The original on-disk path must be the same physical file that the
        // selected Release would replace; normalized keys alone are not proof.
        let canonical = mods::canonical_dll_name(primary);
        let requested = if physical.to_ascii_lowercase().ends_with(".dll.disabled") {
            mods::disabled_dll_file(&canonical)
        } else {
            canonical
        };
        let resolve = |name: &str| fs::canonicalize(game(name).resolve(&self.roots).ok()?).ok();
        let handle = Handle::from_path(&live).ok()?;
        if !has_single_link(handle.as_file())
            || resolve(&requested)? != fs::canonicalize(&live).ok()?
        {
            return None;
        }
        mods::read_dll_details(&live, &physical)
            .guid
            .is_some_and(|value| value.eq_ignore_ascii_case(guid))
            .then_some(PackageScope {
                directory,
                primary_key,
                proof: PackageProof::Replacement,
            })
    }
}
