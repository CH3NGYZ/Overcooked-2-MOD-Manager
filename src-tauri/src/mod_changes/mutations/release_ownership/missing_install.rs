//! Recover MOD release records with no installed primary DLL.
use super::claims::ReleaseTargetError;
use super::loose_files::LooseFile;
use super::*;
use crate::mod_changes::mutations::records::{InstallRecord, install_record_path};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mod_changes::mutations) struct MissingInstallRecord {
    pub relative: String,
    pub replacement: TransactionPath,
    pub expected: Fingerprint,
    pub retired_id: Option<String>,
    previous_paths: BTreeSet<String>,
    vacant_paths: BTreeSet<String>,
    pub(super) retired_map_paths: Vec<String>,
}

pub(super) struct RecordRecovery {
    pub(super) record: MissingInstallRecord,
    pub(super) files: BTreeMap<String, LooseFile>,
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

pub(super) fn endpoints(canonical: &str) -> impl Iterator<Item = String> {
    std::iter::once(canonical.into()).chain(
        mods::file_state::is_mod_file(Path::new(canonical))
            .then(|| mods::disabled_dll_file(canonical)),
    )
}

impl Builder<'_> {
    fn cross_id_record(
        &self,
        owner: &str,
        payload: &Payload,
        repository_id: &str,
        core: &ReleaseCore<'_, '_>,
    ) -> Result<Option<&InstallRecord>, ReleaseTargetError> {
        let mut matches = self.records.bound_records().filter(|(_, previous)| {
            previous.matches_identity(core.repo) && previous.channel == core.binding.channel
        });
        let Some((record, previous)) = matches.next() else {
            return Ok(None);
        };
        let old = &record.manifest;
        let old_id = &old.repository_id;
        let denial =
            if !payload.downloaded || payload.candidate.raw || payload.release_files.is_none() {
                "cross_id_reinstall_requires_verified_release"
            } else if matches.next().is_some() {
                "cross_id_reinstall_ambiguous_source"
            } else if uuid::Uuid::parse_str(old_id).is_err()
                || old.is_manual()
                || !core.matches_prior_binding(previous)
                || previous.tag != old.release_tag
                || !self
                    .records
                    .unique_record(old_id)
                    .is_some_and(|unique| unique.relative == record.relative)
            {
                "cross_id_reinstall_invalid_prior_record"
            } else if repository_id_is_configured(&self.session.trusted.config, old_id) {
                "cross_id_reinstall_previous_id_configured"
            } else if !old
                .files
                .iter()
                .any(|path| inventory::key(&mods::canonical_dll_name(path)) == core.key)
            {
                "cross_id_reinstall_primary_path_changed"
            } else {
                let target = install_record_path(repository_id);
                if self.records.records_for_id(repository_id).next().is_none()
                    && Fingerprint::capture(&target.resolve(&self.roots)?)? == Fingerprint::Missing
                {
                    return Ok(Some(record));
                }
                "install_record_path_occupied"
            };
        let target = if denial == "install_record_path_occupied" {
            install_record_path(repository_id).relative
        } else {
            core.path.into()
        };
        self.diagnose_record_claim(
            owner,
            &target,
            Some(core.path),
            denial,
            std::iter::once(record),
        );
        Ok(None)
    }

    fn recovered_resource(
        &self,
        owner: &str,
        source: &str,
        incoming: Option<(&str, &Fingerprint)>,
        proof: ResourceProof<'_>,
        registry: &[crate::level_sets::RegisteredLevelSetPlugin],
    ) -> Result<Option<LooseFile>, ReleaseTargetError> {
        let Some((physical, expected)) = self
            .live_endpoint(source, false)
            .map_err(|error| error.report(owner))?
        else {
            return Err(InstallBlock::VerificationChanged
                .report(owner, source)
                .into());
        };
        self.ensure_loose_path_unclaimed(
            &physical,
            proof.directory,
            registry,
            proof.allowed,
            false,
            Some(owner),
        )
        .map_err(|block| block.report(owner, &physical))?;
        let Some((incoming, digest)) = incoming else {
            return Ok(None);
        };
        if incoming != source {
            let spelled = game(incoming).resolve(&self.roots)?;
            let known = game(&physical).resolve(&self.roots)?;
            if !matches!((fs::canonicalize(spelled), fs::canonicalize(known)),
                (Ok(spelled), Ok(known)) if spelled == known)
            {
                return Err(InstallBlock::AliasesOrLink.report(owner, incoming).into());
            }
        }
        let changes_content = !expected.same_content(digest);
        if changes_content && proof.allowed.is_none() {
            return Err(InstallBlock::ResidueDiffers.report(owner, &physical).into());
        }
        Ok(Some(LooseFile {
            relative: physical,
            expected,
            loose_node: None,
            changes_content,
        }))
    }

    pub(super) fn missing_install_record(
        &self,
        owner: &str,
        payload: &Payload,
        bundled_levels: Option<&str>,
    ) -> Result<Option<RecordRecovery>, ReleaseTargetError> {
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
        let Ok(core) = self.release_core(payload) else {
            return Ok(None);
        };
        if payload
            .manifest
            .as_ref()
            .is_none_or(|manifest| manifest.repository_id != repository_id)
        {
            return Ok(None);
        }
        let (record, retired_id) = if let Some(record) = self.records.unique_record(repository_id) {
            let matching = match &record.manifest.repository_binding {
                Some(previous) => core.matches_prior_binding(previous),
                None => {
                    payload.release_files.is_some()
                        && self
                            .records
                            .claims(core.key)
                            .any(|claim| claim.relative == record.relative)
                }
            };
            if !matching {
                return Ok(None);
            }
            (record, None)
        } else {
            let Some(record) = self.cross_id_record(owner, payload, repository_id, &core)? else {
                return Ok(None);
            };
            (record, Some(record.manifest.repository_id.clone()))
        };

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
                .filter(|node| node.kind == NodeKind::Mod)
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
        let previous = record.manifest.files.iter().collect::<BTreeSet<_>>();
        let incoming_by_key = incoming
            .clone()
            .map(|file| (inventory::key(&mods::canonical_dll_name(file)), file))
            .collect::<BTreeMap<_, _>>();
        let resource_scope = record
            .manifest
            .repository_binding
            .as_ref()
            .filter(|binding| {
                retired_id.is_none()
                    && binding.tag == record.manifest.release_tag
                    && !record.manifest.is_manual()
                    && previous_paths.contains(core.key)
                    && payload.downloaded
                    && !payload.candidate.raw
            })
            .and(payload.release_files.as_ref());
        let directory = Path::new(core.path).parent().and_then(Path::to_str);
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
        let mut checked_paths = BTreeMap::new();
        let mut files = BTreeMap::<String, LooseFile>::new();
        let mut retired_map_paths = Vec::new();
        for (source, recorded) in record.manifest.files.iter().map(|file| (file, true)).chain(
            incoming
                .filter(|file| !previous.contains(file))
                .map(|file| (file, false)),
        ) {
            let canonical = mods::canonical_dll_name(source);
            if !within(&canonical, mods::file_state::PLUGINS_DIR) {
                return Ok(None);
            }
            let mut competing_records = self
                .records
                .portable_claims(&canonical)
                .filter(|claim| claim.relative != record.relative)
                .peekable();
            if competing_records.peek().is_some() {
                self.diagnose_record_claim(
                    owner,
                    &canonical,
                    Some(source),
                    "reinstall_path_claimed_by_another_record",
                    competing_records,
                );
                return Err(ReleaseTargetError::ForeignManifest {
                    owner: owner.into(),
                    message: InstallBlock::InstallRecord.report(owner, &canonical),
                });
            }
            let key = inventory::key(&canonical);
            if self
                .ownership
                .get(&key)
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
                let metadata = fs::symlink_metadata(&path);
                let missing = metadata.is_err_and(|error| error.kind() == ErrorKind::NotFound);
                let resources = resource_scope.zip(directory).filter(|(_, directory)| {
                    !mods::file_state::is_mod_file(Path::new(source))
                        && path_key(Path::new(directory))
                            != path_key(Path::new(mods::file_state::PLUGINS_DIR))
                        && within(source, directory)
                });
                if !missing && resources.is_none() {
                    if retired_id.is_some() {
                        self.diagnose_record_claim(
                            owner,
                            &relative,
                            Some(source),
                            "cross_id_reinstall_path_present",
                            std::iter::once(record),
                        );
                    }
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
                checked_paths.insert(path_key(Path::new(&relative)), relative.clone());
                if missing {
                    vacant_paths.insert(relative);
                    continue;
                }
                let Some((verified, directory)) = resources else {
                    return Ok(None);
                };
                if !recorded && files.contains_key(&key) {
                    // A matching recorded endpoint already verified this alias.
                    continue;
                }
                let proof = ResourceProof {
                    directory,
                    allowed: recorded.then_some(AllowedClaim::Recorded {
                        manifest: &record.relative,
                        file: source,
                    }),
                };
                let entry = if let Some(incoming) = incoming_by_key.get(&key) {
                    let digest = verified
                        .get(&path_key(Path::new(incoming)))
                        .ok_or_else(|| {
                            InstallBlock::NeedsVerifiedRelease.report(owner, incoming)
                        })?;
                    Some((incoming.as_str(), digest))
                } else {
                    None
                };
                if let Some(file) =
                    self.recovered_resource(owner, source, entry, proof, &registered)?
                {
                    files.insert(key.clone(), file);
                }
            }
        }
        for (key, relative) in &checked_paths {
            if Path::new(key).ancestors().skip(1).any(|parent| {
                parent
                    .to_str()
                    .is_some_and(|path| checked_paths.contains_key(path))
            }) {
                return Err(InstallBlock::PathCollision.report(owner, relative).into());
            }
        }
        Ok(Some(RecordRecovery {
            files,
            record: MissingInstallRecord {
                relative: record.relative.clone(),
                replacement: if retired_id.is_some() {
                    install_record_path(repository_id)
                } else {
                    data(&record.relative)
                },
                expected: record.verified_stamp(&self.roots, owner)?,
                retired_id,
                previous_paths,
                vacant_paths,
                retired_map_paths,
            },
        }))
    }
}

struct ResourceProof<'a> {
    directory: &'a str,
    allowed: Option<AllowedClaim<'a>>,
}

pub(super) fn repository_id_is_configured(config: &crate::models::AppConfig, old_id: &str) -> bool {
    let retired_id = uuid::Uuid::parse_str(old_id).ok();
    config
        .repositories
        .iter()
        .map(|repo| &repo.id)
        .chain(config.map_repositories.iter().map(|repo| &repo.id))
        .any(|id| {
            id.eq_ignore_ascii_case(old_id)
                || retired_id.is_some_and(|retired| uuid::Uuid::parse_str(id) == Ok(retired))
        })
}
