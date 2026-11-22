//! Logical member keys authorize an owner; live renames retain filesystem spelling.
use super::*;
use same_file::Handle;
use std::io::ErrorKind;

impl Builder<'_> {
    pub(super) fn verify_declared_bundle_write(
        &self,
        owner: &str,
        key: &str,
        destination: &str,
        release_stamp: Option<&Fingerprint>,
    ) -> Result<Option<(String, Fingerprint)>, String> {
        let Some(previous) = self
            .session
            .trusted
            .declared_bundle_members(owner)
            .find(|member| member.member_key.as_deref() == Some(key))
        else {
            return Ok(None);
        };
        let physical = self.physical(owner, key)?;
        self.ensure_owner(&physical, owner)?;
        let source_path = game(&physical).resolve(&self.roots)?;
        let destination_path = game(destination).resolve(&self.roots)?;
        if path_key(&source_path) != path_key(&destination_path) {
            return Err(InstallBlock::BundlePathChanged.report(owner, destination));
        }
        let file_handle = |path: &Path, relative: &str| {
            Handle::from_path(path).map_err(|error| {
                log::debug!("无法核对随包插件文件：路径={relative}，系统错误={error}");
                let reason = if error.kind() == ErrorKind::NotFound {
                    InstallBlock::VerificationChanged
                } else {
                    InstallBlock::Unreadable
                };
                reason.report(owner, relative)
            })
        };
        let source_file = file_handle(&source_path, &physical)?;
        if file_handle(&destination_path, destination)? != source_file {
            return Err(InstallBlock::AliasesOrLink.report(owner, destination));
        }
        if !has_single_link(source_file.as_file()) {
            return Err(InstallBlock::AliasesOrLink.report(owner, destination));
        }
        // Ownership is indexed by logical keys. Also reject aliases to the same
        // file (e.g. a manifest spelling a Windows short name or APFS alias).
        let has_other_claim = self.ownership_paths.iter().any(|claim| {
            if inventory::key(&mods::canonical_dll_name(claim)) == key {
                return false; // ensure_owner checked these spellings together.
            }
            let resolves_to_source = |relative: &str| {
                game(relative).resolve(&self.roots).map_or(true, |path| {
                    match Handle::from_path(path) {
                        Ok(file) => file == source_file,
                        Err(error) => error.kind() != ErrorKind::NotFound,
                    }
                })
            };
            let canonical = mods::canonical_dll_name(claim);
            resolves_to_source(claim)
                || (canonical.to_ascii_lowercase().ends_with(".dll")
                    && (resolves_to_source(&canonical)
                        || resolves_to_source(&mods::disabled_dll_file(&canonical))))
        });
        let before = Fingerprint::capture(&source_path)?;
        let identity = mods::read_dll_details(&source_path, &physical).guid;
        let identity_check = if previous.guid.trim().is_empty() {
            // The legacy GUID-less stub's structural declaration is only a
            // switch claim; an overlapping install must prove identical bytes.
            match (identity, release_stamp) {
                (None, Some(stamp)) if before.same_content(stamp) => Ok(()),
                (None, Some(_)) => Err(InstallBlock::BundleFileDiffers),
                (None, None) => Err(InstallBlock::NeedsVerifiedRelease),
                (Some(_), _) => Err(InstallBlock::PluginMismatch),
            }
        } else {
            match identity.as_deref() {
                Some(guid) if crate::mod_changes::evaluate::same_guid(guid, &previous.guid) => {
                    Ok(())
                }
                Some(guid) if !guid.trim().is_empty() => Err(InstallBlock::PluginMismatch),
                _ => Err(InstallBlock::PluginIdentityUnknown),
            }
        };
        if has_other_claim {
            return Err(InstallBlock::AliasesOrLink.report(owner, &physical));
        }
        match before {
            Fingerprint::File { .. } => (),
            Fingerprint::Directory { .. } => {
                return Err(InstallBlock::Directory.report(owner, &physical));
            }
            Fingerprint::Missing => {
                return Err(InstallBlock::VerificationChanged.report(owner, &physical));
            }
        }
        identity_check.map_err(|reason| reason.report(owner, &physical))?;
        if Fingerprint::capture(&source_path)? != before {
            return Err(InstallBlock::VerificationChanged.report(owner, &physical));
        }
        Ok(Some((physical, before)))
    }

    pub(super) fn switch_member(
        &mut self,
        owner: &str,
        key: &str,
        enabled: bool,
    ) -> Result<(), String> {
        let from = self.physical(owner, key)?;
        let canonical = mods::canonical_dll_name(&from);
        let to = if enabled {
            canonical
        } else {
            mods::disabled_dll_file(&canonical)
        };
        if inventory::key(&from) != inventory::key(&to) {
            self.ensure_owner(&from, owner)?;
            self.add_move(owner, Some(key), &from, &to)?;
        }
        Ok(())
    }

    pub(super) fn physical(&self, owner: &str, member_key: &str) -> Result<String, String> {
        let files = self
            .session
            .trusted
            .files
            .get(owner)
            .ok_or_else(|| InstallBlock::VerificationChanged.report(owner, member_key))?;
        let key = inventory::key(&mods::canonical_dll_name(member_key));
        let originals = files
            .summary
            .members
            .iter()
            .map(|member| member.dll_file.as_str())
            .chain(files.summary.dll_files.iter().map(String::as_str))
            .map(mods::canonical_dll_name)
            .filter(|path| inventory::key(path) == key)
            .collect::<BTreeSet<_>>();
        let root = crate::game::resolve_existing_ancestor(&self.roots.game_root)?;
        let mut physical = BTreeSet::new();
        for canonical in originals {
            let disabled = mods::disabled_dll_file(&canonical);
            // Resolve through transaction validation before canonicalization, so
            // a matching key cannot hide a symlink or junction inside the game.
            let active = game(&canonical).resolve(&self.roots)?;
            let inactive = game(&disabled).resolve(&self.roots)?;
            let relative = match (active.is_file(), inactive.is_file()) {
                (true, false) => canonical,
                (false, true) => disabled,
                (true, true) => return Err(InstallBlock::BothStates.report(owner, &canonical)),
                (false, false) => return Err(InstallBlock::Missing.report(owner, &canonical)),
            };
            let actual = crate::game::safe_game_path(&self.roots.game_root, &relative)?;
            physical.insert(inventory::relative(&root, &actual)?);
        }
        if physical.len() > 1 {
            return Err(InstallBlock::AliasesOrLink.report(owner, member_key));
        }
        physical
            .pop_first()
            .ok_or_else(|| InstallBlock::VerificationChanged.report(owner, member_key))
    }
}
