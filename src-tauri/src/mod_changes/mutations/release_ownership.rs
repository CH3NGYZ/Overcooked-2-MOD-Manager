//! A verified upgrade may claim identical loose files in its own plugin directory.
use super::*;
use crate::mod_changes::session::Payload;
use same_file::Handle;
use std::io::ErrorKind;

impl Builder<'_> {
    pub(super) fn release_path_unclaimed(
        &self,
        relative: &str,
        target: &Path,
        directory: &Path,
    ) -> bool {
        // APFS can ignore case even though normal inventory keys preserve it.
        // An alternate spelling in a manifest must never authorize adoption.
        let key = path_key(Path::new(relative));
        if self
            .ownership
            .keys()
            .any(|path| path_key(Path::new(path)) == key)
        {
            return false;
        }
        let Ok(real_path) = fs::canonicalize(target) else {
            return false;
        };
        if mods::canonical_dll_name(&real_path.to_string_lossy())
            .to_ascii_lowercase()
            .ends_with(".dll")
            || !real_path.starts_with(directory)
            || self.protected_maps.iter().any(|map| {
                game(map)
                    .resolve(&self.roots)
                    .and_then(|path| crate::game::resolve_existing_ancestor(&path))
                    .map_or(true, |path| real_path.starts_with(path))
            })
        {
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
        self.ownership_paths.iter().all(|claim| {
            if !excludes_target(claim) {
                return false;
            }
            let canonical = mods::canonical_dll_name(claim);
            if !canonical.to_ascii_lowercase().ends_with(".dll") {
                return true;
            }
            let other = if canonical == *claim {
                mods::disabled_dll_file(&canonical)
            } else {
                canonical
            };
            excludes_target(&other)
        })
    }

    pub(super) fn release_directory(
        &self,
        owner: &str,
        payload: &Payload,
        manifest: &archive::InstallManifest,
    ) -> Option<PathBuf> {
        // A tag, preview origin or matching local copies alone don't prove ZIP contents.
        payload.release_files.as_ref()?;
        let repo = self
            .session
            .trusted
            .config
            .repositories
            .iter()
            .find(|repo| Some(&repo.id) == payload.candidate.source_id.as_ref())?;
        let old = self.session.trusted.files.get(owner)?;
        let [previous] = old.manifests.as_slice() else {
            return None;
        };
        let previous_binding = previous.repository_binding.as_ref()?;
        if previous.repository_id != manifest.repository_id
            || !previous_binding.is_valid()
            || !previous_binding.matches_repository(repo)
            // Channel::All can find a Release, but doesn't make two channels the same owner.
            || previous_binding.channel != manifest.repository_binding.as_ref()?.channel
        {
            return None;
        }

        let guid = repo.plugin_guid.trim();
        if guid.is_empty() {
            return None;
        }
        let mut installed = old.summary.members.iter().filter(|member| {
            member
                .guid
                .as_deref()
                .is_some_and(|value| value.eq_ignore_ascii_case(guid))
        });
        let installed_dll = mods::canonical_dll_name(&installed.next()?.dll_file);
        let mut incoming = payload
            .candidate
            .provides
            .iter()
            .filter(|member| member.guid.eq_ignore_ascii_case(guid));
        let incoming_key = incoming.next()?.member_key.as_deref()?;
        if installed.next().is_some()
            || incoming.next().is_some()
            || inventory::key(&installed_dll) != incoming_key
            || !previous
                .files
                .iter()
                .any(|path| inventory::key(&mods::canonical_dll_name(path)) == incoming_key)
            || self.ensure_owner(&installed_dll, owner).is_err()
        {
            return None;
        }

        let directory = Path::new(&installed_dll).parent()?.to_str()?;
        let plugins = mods::file_state::PLUGINS_DIR;
        if !within(directory, plugins) || inventory::key(directory) == inventory::key(plugins) {
            return None;
        }
        let prefix = format!("{}/", path_key(Path::new(directory)));
        if self.ownership.iter().any(|(path, owners)| {
            let folded = path_key(Path::new(path));
            folded.starts_with(&prefix)
                && folded.ends_with(".dll")
                && (owners.len() != 1 || !owners.contains(owner))
        }) {
            return None;
        }

        // Check the live DLL variants too: the scan may hide a disabled twin.
        let physical = self.physical(owner, incoming_key).ok()?;
        let live = fs::canonicalize(game(&physical).resolve(&self.roots).ok()?).ok()?;
        let plugins = fs::canonicalize(game(plugins).resolve(&self.roots).ok()?).ok()?;
        let directory = live.parent()?;
        if directory == plugins || !directory.starts_with(&plugins) {
            return None;
        }
        let actual = mods::read_dll_details(&live, &physical);
        actual
            .guid
            .is_some_and(|value| value.eq_ignore_ascii_case(guid))
            .then(|| directory.to_path_buf())
    }
}
