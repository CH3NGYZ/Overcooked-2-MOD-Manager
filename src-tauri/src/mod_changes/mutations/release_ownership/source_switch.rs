use super::manifest_replacement::{
    OverwriteConfirmation, overlapping_endpoint, recorded_endpoints,
};
use super::missing_install::{MissingInstallRecord, RecordRecovery};
use super::*;
use crate::config::unique_mod_repository_by_id;
use crate::mod_changes::mutations::{
    install_block::report_record_claim,
    records::{RecordStamp, install_record_path},
};
use crate::mod_changes::progress::SourceSwitch;
use crate::models::{AppConfig, Channel, RelationKind};

fn source_binding<'a>(
    config: &AppConfig,
    next: &ModRepositoryConfig,
    manifest: &'a archive::InstallManifest,
) -> Option<&'a ModRepositoryBinding> {
    let binding = manifest.repository_binding.as_ref()?;
    let previous_id = uuid::Uuid::parse_str(&manifest.repository_id).ok()?;
    (uuid::Uuid::parse_str(&next.id) != Ok(previous_id)
        && unique_mod_repository_by_id(config, &next.id).is_some()
        && !manifest.is_manual()
        && binding.is_valid()
        && binding.tag == manifest.release_tag)
        .then_some(binding)
}

pub(in crate::mod_changes) fn related_release_source(
    config: &AppConfig,
    next: &ModRepositoryConfig,
    manifest: &archive::InstallManifest,
    tag: &str,
    prerelease: bool,
) -> bool {
    let (Some(source_id), Some(binding)) = (
        next.source_id.as_deref().filter(|id| !id.is_empty()),
        source_binding(config, next, manifest),
    ) else {
        return false;
    };
    let Some(previous) = unique_mod_repository_by_id(config, &manifest.repository_id) else {
        return false;
    };
    let old_region = previous.region.as_deref().unwrap_or(&config.region);
    if !binding.matches_repository(previous)
        || previous.source_id.as_deref() != Some(source_id)
        || !previous.username.eq_ignore_ascii_case(&next.username)
        || next.plugin_guid.trim().is_empty()
        || !same_guid(&previous.plugin_guid, &next.plugin_guid)
        || !crate::config::mod_repository_is_available(config, previous, old_region)
        || !crate::config::mod_repository_is_available(config, next, &config.region)
    {
        return false;
    }
    let same_repo = previous.repository.eq_ignore_ascii_case(&next.repository);
    if same_repo
        && previous.provider != next.provider
        && previous.channel == next.channel
        && manifest.prerelease == prerelease
    {
        return true;
    }
    if manifest.prerelease == prerelease {
        return false;
    }
    if same_repo && previous.provider == next.provider && previous.channel != next.channel {
        return true;
    }
    let excludes = |repo, tag, other_pre| {
        mods::effective_relations(repo, Some(tag))
            .iter()
            .any(|relation| {
                relation.kind == RelationKind::Conflicts
                    && !relation.optional
                    && relation.version.trim().is_empty()
                    && relation.prerelease == Some(other_pre)
                    && same_guid(&relation.guid, &next.plugin_guid)
            })
    };
    excludes(previous, &binding.tag, prerelease) && excludes(next, tag, manifest.prerelease)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SourceReplacement {
    pub(super) confirmation: Option<OverwriteConfirmation>,
    pub(super) target: String,
    pub(super) manifest_stamp: RecordStamp,
    pub(super) installed: Option<(String, RecordStamp)>,
}

impl Builder<'_> {
    pub(super) fn source_switch_record(
        &self,
        owner: &str,
        payload: &Payload,
        bundled_levels: Option<&str>,
    ) -> Result<Option<RecordRecovery>, String> {
        let Some(zip_digest) = payload.release_zip_sha256.as_ref() else {
            return Ok(None);
        };
        let Ok(core) = self.release_core(payload) else {
            return Ok(None);
        };
        if bundled_levels.is_some() {
            return Ok(None);
        }
        let explicit_root = matches!(self.session.request.roots.as_slice(), [root]
            if root.enabled && root.node_id == owner && root.member_key.is_none()
                && root.candidate_id.as_ref() == Some(&payload.candidate.id));
        let trusted = &self.session.trusted;
        let verified_download =
            payload.downloaded && !payload.candidate.raw && payload.release_files.is_some();
        let vacant_owner = verified_download
            && !trusted.inventory.nodes.iter().any(|node| {
                node.current.as_ref().is_some_and(|candidate| {
                    node.id == owner
                        || candidate
                            .provides
                            .iter()
                            .any(|plugin| same_guid(&plugin.guid, &core.repo.plugin_guid))
                })
            });
        let Some(prerelease) = payload
            .manifest
            .as_ref()
            .map(|manifest| manifest.prerelease)
        else {
            return Ok(None);
        };
        let selected_prerelease = explicit_root
            && prerelease
            && core.repo.channel == Channel::All
            && matches!(self.session.wire_request.roots.as_slice(), [root]
                if self.session.request.roots.first() == Some(&root.intent) && root.asset_name.is_some());
        let target = mods::canonical_dll_name(core.path);
        let projected = self
            .session
            .trusted
            .duplicate_releases
            .source_switch(owner, &payload.candidate.id);
        if !explicit_root && !vacant_owner && projected.is_none() {
            return Ok(None);
        }
        let installed_primary = projected.and_then(|copy| {
            self.session.trusted.installed_mods.iter().find(|item| {
                item.dll_file == mods::canonical_dll_name(&copy.dll_file)
                    && item.package_id == copy.manifest_id
                    && item
                        .guid
                        .as_deref()
                        .is_some_and(|guid| same_guid(guid, &core.repo.plugin_guid))
            })
        });
        let channel_allows = core.repo.channel.allows_prerelease(prerelease);
        let mut records = self
            .records
            .bound_records()
            .filter_map(|(record, previous)| {
                let manifest = &record.manifest;
                let automatic = verified_download
                    && channel_allows
                    && source_binding(&trusted.config, core.repo, manifest).is_some()
                    && (installed_primary.is_some_and(|item| {
                        item.package_id.as_deref() == Some(manifest.repository_id.as_str())
                            && item.repository_binding.as_ref() == Some(previous)
                            && item.replacement_repository_id.as_deref()
                                == Some(core.repo.id.as_str())
                    }) || (vacant_owner
                        && mods::orphan_replacement_repository(
                            &trusted.config,
                            manifest,
                            &target,
                            selected_prerelease
                                || mods::installed_prerelease(
                                    &trusted.config,
                                    Some(previous),
                                    manifest.prerelease,
                                    None,
                                ) == prerelease,
                        )
                        .is_some_and(|repo| repo.id == core.repo.id)));
                let explicit = !automatic
                    && explicit_root
                    && related_release_source(
                        &trusted.config,
                        core.repo,
                        manifest,
                        &core.binding.tag,
                        prerelease,
                    );
                (automatic || explicit).then_some((record, previous, explicit))
            });
        let Some((record, previous_binding, explicit)) = records.next() else {
            return Ok(None);
        };
        let old_id = &record.manifest.repository_id;
        let replacement = install_record_path(&core.repo.id);
        if records.next().is_some()
            || self
                .records
                .unique_record(old_id)
                .map(|unique| unique.relative.as_str())
                != Some(record.relative.as_str())
            || record.relative != install_record_path(old_id).relative
            || (projected.is_none()
                && !explicit
                && self.records.exact_orphan(old_id, self.session).is_none())
            || self.records.records_for_id(&core.repo.id).next().is_some()
            || Fingerprint::capture(&replacement.resolve(&self.roots)?)? != Fingerprint::Missing
            || !payload
                .candidate
                .provides
                .iter()
                .any(|member| member.enabled && member.member_key.as_deref() == Some(core.key))
        {
            return Ok(None);
        }
        let stamp = record.exact_stamp(&self.roots, owner)?;
        let previous = record
            .manifest
            .files
            .iter()
            .map(|path| mods::canonical_dll_name(path))
            .collect::<BTreeSet<_>>();
        let installed = if let Some(copy) = projected {
            if copy.manifest_id.as_ref() != Some(old_id)
                || copy.manifest_fingerprint.as_ref() != Some(&stamp.fingerprint)
            {
                return Ok(None);
            }
            let absolute = game(&copy.dll_file).resolve(&self.roots)?;
            let changed = || InstallBlock::VerificationChanged.report(owner, &copy.dll_file);
            let identity = crate::file_metadata::FileIdentity::capture(&absolute)
                .ok()
                .flatten()
                .ok_or_else(changed)?;
            let fingerprint = Fingerprint::capture(&absolute)?;
            let details = mods::read_dll_details(&absolute, &copy.dll_file);
            if fingerprint != copy.fingerprint
                || !details
                    .guid
                    .as_deref()
                    .is_some_and(|guid| same_guid(guid, &core.repo.plugin_guid))
                || (!explicit
                    && !mods::repository_matches_mod(
                        core.repo,
                        details.guid.as_deref(),
                        details.product_name.as_deref(),
                        details.original_filename.as_deref(),
                        details.version.as_deref(),
                    ))
                || Fingerprint::capture(&absolute)? != fingerprint
                || crate::file_metadata::FileIdentity::capture(&absolute)
                    .ok()
                    .flatten()
                    .as_ref()
                    != Some(&identity)
            {
                return Err(changed());
            }
            Some((
                copy.dll_file.clone(),
                RecordStamp {
                    fingerprint,
                    identity,
                },
            ))
        } else {
            if !previous.contains(&target)
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
            None
        };
        let canonical = previous
            .iter()
            .cloned()
            .chain(
                payload
                    .files
                    .iter()
                    .map(|path| mods::canonical_dll_name(path)),
            )
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if previous.len() != record.manifest.files.len()
            || record.manifest.files.iter().any(|file| file.contains('\\'))
            || canonical
                .iter()
                .any(|file| !within(file, mods::file_state::PLUGINS_DIR))
        {
            return Ok(None);
        }
        let endpoints = recorded_endpoints(&self.roots, owner, &canonical)?;
        if endpoints.iter().any(|path| {
            !path.missing
                && installed
                    .as_ref()
                    .is_none_or(|(dll, _)| *dll != path.relative)
        }) {
            return Ok(None);
        }
        let claim = |path| overlapping_endpoint(&self.roots, &endpoints, path);
        for (path, other) in self.records.claimed_files() {
            if other.relative != record.relative
                && let Some(endpoint) = claim(path)?
            {
                return Err(report_record_claim(
                    owner,
                    &endpoint.relative,
                    std::iter::once(other),
                ));
            }
        }
        let registered = self
            .registered_map_plugins()
            .map_err(|_| InstallBlock::MapRecordsUnreadable.report(owner, &target))?;
        for path in self
            .protected_maps
            .iter()
            .map(String::as_str)
            .chain(registered.iter().map(|entry| entry.dll.as_str()))
        {
            if let Some(endpoint) = claim(path)? {
                return Err(InstallBlock::MapProtected.report(owner, &endpoint.relative));
            }
        }
        for (node, files) in &self.session.trusted.files {
            for member in &files.summary.members {
                if node == owner
                    && installed
                        .as_ref()
                        .is_some_and(|(dll, _)| mods::canonical_dll_name(dll) == member.dll_file)
                {
                    continue;
                }
                if let Some(endpoint) = claim(&member.dll_file)? {
                    return Err(InstallBlock::PathOwned.report(owner, &endpoint.relative));
                }
            }
        }
        let vacant_paths = endpoints
            .into_iter()
            .filter(|path| path.missing)
            .map(|path| path.relative)
            .collect::<BTreeSet<_>>();
        let confirmation = explicit
            .then(|| {
                OverwriteConfirmation::new(
                    self,
                    owner,
                    &core.repo.id,
                    record,
                    (
                        &payload.candidate,
                        zip_digest,
                        &stamp,
                        &installed,
                        &vacant_paths,
                    ),
                    Some(SourceSwitch {
                        previous_source: previous_binding.clone(),
                        next_source: core.binding.clone(),
                        target_path: target.clone(),
                        installed_path: installed.as_ref().map(|(path, _)| path.clone()),
                    }),
                )
            })
            .transpose()?;
        Ok(Some(RecordRecovery {
            files: BTreeMap::new(),
            record: MissingInstallRecord {
                relative: record.relative.clone(),
                replacement,
                expected: stamp.fingerprint.clone(),
                retired_id: Some(old_id.clone()),
                previous_paths: previous.iter().map(|path| inventory::key(path)).collect(),
                vacant_paths,
                retired_map_paths: Vec::new(),
                source_switch: Some(SourceReplacement {
                    confirmation,
                    target,
                    manifest_stamp: stamp,
                    installed,
                }),
            },
        }))
    }
}
