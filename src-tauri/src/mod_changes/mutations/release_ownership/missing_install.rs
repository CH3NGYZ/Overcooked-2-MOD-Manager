//! Release installs may replace their own record after every registered file is gone.
use super::claims::ReleaseTargetError;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mod_changes::mutations) struct MissingInstallRecord {
    pub relative: String,
    pub expected: Fingerprint,
    previous_paths: BTreeSet<String>,
    vacant_paths: BTreeSet<String>,
    pub(super) retired_map_paths: Vec<String>,
}

impl MissingInstallRecord {
    pub(in crate::mod_changes::mutations) fn owns_previous_path(&self, key: &str) -> bool {
        self.previous_paths.contains(key)
    }

    pub(in crate::mod_changes::mutations) fn expects_missing(&self, path: &str) -> bool {
        self.vacant_paths.contains(path)
    }

    pub(super) fn unexpected_path(
        &self,
        roots: &TransactionRoots,
        written: &BTreeMap<String, String>,
    ) -> Result<Option<String>, String> {
        for relative in self
            .vacant_paths
            .iter()
            .chain(self.retired_map_paths.iter())
        {
            if self.retired_map_paths.contains(relative)
                && written.values().any(|target| within(relative, target))
            {
                continue;
            }
            let expected = written.get(&path_key(Path::new(relative)));
            if expected == Some(relative) {
                continue;
            }
            let path = game(relative).resolve(roots)?;
            match fs::symlink_metadata(&path) {
                Err(error) if error.kind() == ErrorKind::NotFound => continue,
                Ok(meta) if meta.is_file() => {
                    if let Some(expected) = expected
                        && let Ok(actual) = fs::canonicalize(&path)
                        && fs::canonicalize(game(expected).resolve(roots)?)
                            .is_ok_and(|planned| planned == actual)
                    {
                        continue;
                    }
                }
                _ => (),
            }
            return Ok(Some(relative.clone()));
        }
        Ok(None)
    }
}

fn endpoints(canonical: &str) -> impl Iterator<Item = String> {
    std::iter::once(canonical.into()).chain(
        mods::file_state::is_mod_file(Path::new(canonical))
            .then(|| mods::disabled_dll_file(canonical)),
    )
}

impl Builder<'_> {
    pub(super) fn missing_install_record(
        &self,
        owner: &str,
        payload: &Payload,
        bundled_levels: Option<&str>,
    ) -> Result<Option<MissingInstallRecord>, ReleaseTargetError> {
        if self
            .session
            .trusted
            .inventory
            .nodes
            .iter()
            .any(|old| old.id == owner && old.current.is_some())
        {
            return Ok(None);
        }
        let Some(repository_id) = payload.candidate.source_id.as_deref() else {
            return Ok(None);
        };
        let Some(record) = self.records.unique_record(repository_id) else {
            return Ok(None);
        };
        let Ok(core) = self.release_core(payload) else {
            return Ok(None);
        };
        if payload
            .manifest
            .as_ref()
            .is_none_or(|manifest| manifest.repository_id != repository_id)
            || match &record.manifest.repository_binding {
                Some(previous) => !core.matches_prior_binding(previous),
                None => {
                    payload.release_files.is_none()
                        || !self
                            .records
                            .claims(core.key)
                            .any(|claim| claim.relative == record.relative)
                }
            }
        {
            return Ok(None);
        }

        if !record
            .manifest
            .files
            .iter()
            .any(|file| mods::file_state::is_mod_file(Path::new(file)))
            || bundled_levels.is_some_and(|levels| within(core.path, levels))
            || self
                .session
                .trusted
                .inventory
                .nodes
                .iter()
                .filter_map(|node| node.current.as_ref())
                .flat_map(|candidate| &candidate.provides)
                .any(|plugin| same_guid(&plugin.guid, &core.repo.plugin_guid))
        {
            return Ok(None);
        }
        let previous_paths = record
            .manifest
            .files
            .iter()
            .map(|path| inventory::key(&mods::canonical_dll_name(path)))
            .collect::<BTreeSet<_>>();
        let incoming = payload
            .files
            .iter()
            .filter(|source| !bundled_levels.is_some_and(|levels| within(source, levels)));
        let resolve = |relative: &str| {
            let path = game(relative).resolve(&self.roots)?;
            crate::game::resolve_existing_ancestor(&path)
        };
        let map_roots = self
            .protected_maps
            .iter()
            .map(|path| resolve(path))
            .collect::<Result<Vec<_>, _>>()?;
        let registered = self
            .registered_map_plugins()
            .map_err(|_| InstallBlock::MapRecordsUnreadable.report(owner, core.path))?;
        let map_claims = registered
            .iter()
            .map(|claim| mods::manifest_dll_key(&claim.dll))
            .collect::<BTreeSet<_>>();
        let mut vacant_paths = BTreeSet::new();
        let mut retired_map_paths = Vec::new();
        for source in record.manifest.files.iter().chain(incoming) {
            let canonical = mods::canonical_dll_name(source);
            if !within(&canonical, mods::file_state::PLUGINS_DIR) {
                return Ok(None);
            }
            if self
                .records
                .portable_claims(&canonical)
                .any(|claim| claim.relative != record.relative)
            {
                return Err(ReleaseTargetError::ForeignManifest {
                    owner: owner.into(),
                    message: InstallBlock::InstallRecord.report(owner, &canonical),
                });
            }
            if self
                .ownership
                .get(&inventory::key(&canonical))
                .is_some_and(|owners| owners.len() != 1 || !owners.contains(&record.owner))
            {
                return Ok(None);
            }
            if let Some(levels) = bundled_levels.filter(|levels| {
                payload.downloaded
                    && within(source, levels)
                    && !mods::file_state::is_mod_file(Path::new(source))
                    && payload
                        .files
                        .iter()
                        .any(|file| inventory::key(file) == inventory::key(source))
            }) {
                let name = inventory::key(source)
                    .strip_prefix(&inventory::key(levels))
                    .and_then(|relative| relative.strip_prefix('/'))
                    .and_then(|relative| relative.split_once('/').map(|(name, _)| name))
                    .map(str::to_owned);
                if let Some(name) = name {
                    if crate::mod_changes::level_trees::has_verified_bundled_map(
                        payload, levels, &name,
                    ) {
                        let path = game(source).resolve(&self.roots)?;
                        if Fingerprint::capture(&path)? == Fingerprint::Missing
                            && !map_claims.contains(&mods::manifest_dll_key(source))
                        {
                            retired_map_paths.push(source.clone());
                            log::debug!(
                                "旧安装记录所登记的随包地图文件已缺失，计划交回地图目录处理：节点={owner}，记录={}，路径={source}",
                                record.relative
                            );
                            continue;
                        }
                    }
                }
            }
            for relative in endpoints(&canonical) {
                let path = game(&relative).resolve(&self.roots)?;
                if !fs::symlink_metadata(&path)
                    .is_err_and(|error| error.kind() == ErrorKind::NotFound)
                {
                    return Ok(None);
                }
                let target = crate::game::resolve_existing_ancestor(&path)?;
                if self
                    .protected_maps
                    .iter()
                    .any(|root| within(&relative, root))
                    || map_roots.iter().any(|root| target.starts_with(root))
                    || map_claims.contains(&mods::manifest_dll_key(&relative))
                {
                    return Err(InstallBlock::MapProtected.report(owner, &relative).into());
                }
                vacant_paths.insert(relative);
            }
        }
        let vacant_keys = vacant_paths
            .iter()
            .map(|path| (path_key(Path::new(path)), path))
            .collect::<BTreeMap<_, _>>();
        for (key, relative) in &vacant_keys {
            if Path::new(key).ancestors().skip(1).any(|parent| {
                parent
                    .to_str()
                    .is_some_and(|path| vacant_keys.contains_key(path))
            }) {
                return Err(InstallBlock::PathCollision.report(owner, relative).into());
            }
        }
        Ok(Some(MissingInstallRecord {
            relative: record.relative.clone(),
            expected: record.verified_stamp(&self.roots, owner)?,
            previous_paths,
            vacant_paths,
            retired_map_paths,
        }))
    }
}
