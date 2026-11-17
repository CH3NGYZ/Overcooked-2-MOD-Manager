//! Validate a downloaded preview and unpack it into a private simulated game root.
//! All returned paths are game-relative; this module never installs into the live game.
use super::{
    inventory::{self, TrustedInventory},
    model::*,
    session::Payload,
    transaction::Fingerprint,
};
use crate::{
    app_state::AppState,
    archive,
    metadata_read::{self, MetadataKind, MetadataRequest},
    mod_preview::staging,
    models::*,
    mods, releases,
};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::Path,
};

pub(crate) fn from_preview(
    state: &AppState,
    trusted: &TrustedInventory,
    preview_id: &str,
    node_id: &str,
    candidate: &Candidate,
    asset_name: Option<&str>,
    stage_root: &Path,
) -> Result<Payload, String> {
    let id = uuid::Uuid::parse_str(preview_id).map_err(|_| "modPreview.invalidId")?;
    if id.to_string() != preview_id {
        return Err("modPreview.invalidId".into());
    }
    let preview = staging::preview_root(state).join(preview_id);
    if !matches!(
        Fingerprint::capture(&preview)?,
        Fingerprint::Directory { .. }
    ) {
        return Err("modPreview.expired".into());
    }
    let mut sources = Vec::new();
    for entry in fs::read_dir(&preview).map_err(|_| "modPreview.expired")? {
        let entry = entry.map_err(|_| "modPreview.expired")?;
        if entry.file_name() == staging::ORIGIN_FILE {
            continue;
        }
        if !matches!(
            Fingerprint::capture(&entry.path())?,
            Fingerprint::File { .. }
        ) {
            return Err("archive.pathEscape".into());
        }
        sources.push(entry.path());
    }
    if sources.len() != 1 {
        return Err("modPreview.expired".into());
    }
    let source = sources.remove(0);
    let file_name = source
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("mods.invalidFileName")?;
    let origin_path = preview.join(staging::ORIGIN_FILE);
    let origin = if matches!(Fingerprint::capture(&origin_path)?, Fingerprint::Missing) {
        None
    } else {
        let metadata = fs::metadata(&origin_path).map_err(|_| "modPreview.expired")?;
        if metadata.len() > 1024 * 1024 {
            return Err("modPreview.expired".into());
        }
        Some(
            serde_json::from_slice::<staging::PreviewOriginRecord>(
                &fs::read(&origin_path).map_err(|_| "modPreview.expired")?,
            )
            .map_err(|_| "modPreview.expired")?,
        )
    };
    let repo = candidate
        .source_id
        .as_ref()
        .map(|id| {
            trusted
                .config
                .repositories
                .iter()
                .find(|repo| &repo.id == id)
                .ok_or("source.repositoryMissing")
        })
        .transpose()?;
    match (&origin, repo) {
        (Some(origin), Some(repo))
            if origin.repository_id == repo.id
                && candidate.tag.as_deref().unwrap_or_default() == origin.release_tag =>
        {
            ()
        }
        (None, None) => (),
        _ => return Err("modChanges.targetChanged".into()),
    }
    let prerelease = origin.as_ref().is_some_and(|value| value.prerelease);
    if let Some(repo) = repo {
        if let Some(tag) = &candidate.tag {
            let release = repo
                .cached_releases
                .iter()
                .find(|release| &release.tag_name == tag)
                .ok_or("modChanges.targetChanged")?;
            let expected_prerelease =
                releases::resolve_prerelease(release.api_prerelease, release.prerelease, Some(tag));
            if release.draft || expected_prerelease != prerelease {
                return Err("modChanges.targetChanged".into());
            }
            let request = releases::RepositoryRequest::from(repo);
            let asset =
                releases::select_install_asset(&request, &repo.asset_rules, release, asset_name)
                    .ok_or("modChanges.targetChanged")?;
            if asset.name != file_name {
                return Err("modChanges.targetChanged".into());
            }
            archive::validate_download_file(
                &source,
                (asset.size > 0).then_some(asset.size),
                asset.digest.as_deref(),
            )?;
        } else if !candidate.raw {
            return Err("modChanges.targetChanged".into());
        }
    }
    let source_fingerprint = Fingerprint::capture(&source)?;
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !extension.eq_ignore_ascii_case("zip") && !extension.eq_ignore_ascii_case("dll") {
        return Err("mods.unsupportedFileType".into());
    }
    archive::validate_download_file(&source, None, None)?;
    if extension.eq_ignore_ascii_case("zip") {
        archive::validate_zip(&source)?;
    }
    create_sandbox(state, trusted, stage_root)?;
    let result = (|| {
        let pending = if extension.eq_ignore_ascii_case("zip") {
            if let Some(repo) = repo {
                let plan = archive::prepare_repository_mod_zip(
                    &source,
                    stage_root,
                    &repo.install_target,
                    file_name,
                )?;
                archive::extract_prepared_mod_zip(&source, stage_root, &plan)?
            } else {
                archive::extract_local_mod_zip_pending(&source, stage_root, file_name)?
            }
        } else {
            archive::copy_asset_pending(
                &source,
                stage_root,
                repo.map(|repo| repo.install_target.as_str())
                    .unwrap_or("BepInEx/plugins"),
                file_name,
            )?
        };
        let mut files = pending.commit();
        if Fingerprint::capture(&source)? != source_fingerprint {
            return Err("modChanges.targetChanged".into());
        }
        let provides = read_members(trusted, node_id, stage_root, &mut files, prerelease)?;
        if provides.is_empty() {
            return Err("archive.noInstallableContent".into());
        }
        if let Some(primary) = repo
            .map(|repo| repo.plugin_guid.trim())
            .filter(|guid| !guid.is_empty())
        {
            match provides
                .iter()
                .filter(|member| member.guid.eq_ignore_ascii_case(primary))
                .count()
            {
                0 => return Err("modChanges.targetChanged".into()),
                1 => (),
                _ => return Err("modChanges.ambiguousOwnership".into()),
            }
        }
        let mut actual = candidate.clone();
        actual.provides = provides;
        actual.verification_pending = false;
        let relations = repo
            .map(|repo| mods::effective_relations(repo, candidate.tag.as_deref()))
            .unwrap_or_default();
        actual.relations = relations
            .iter()
            .cloned()
            .map(|relation| OwnedRelation {
                owner_member_key: None,
                owner_guid: repo
                    .filter(|repo| !repo.plugin_guid.trim().is_empty())
                    .map(|repo| repo.plugin_guid.clone()),
                relation,
            })
            .collect();
        for member in &actual.provides {
            if member.guid.is_empty()
                || repo.is_some_and(|repo| repo.plugin_guid.eq_ignore_ascii_case(&member.guid))
            {
                continue;
            }
            let (relations, known) =
                super::inventory_maps::member_relations(&trusted.config, member);
            if known == Some(false) {
                return Err("modChanges.ambiguousOwnership".into());
            }
            actual.relations.extend(relations);
        }
        actual.relations_known = true;
        actual.version = repo
            .and_then(|repo| {
                actual
                    .provides
                    .iter()
                    .find(|plugin| plugin.guid.eq_ignore_ascii_case(&repo.plugin_guid))
            })
            .or_else(|| {
                actual
                    .provides
                    .iter()
                    .find(|plugin| !plugin.guid.is_empty())
            })
            .and_then(|plugin| plugin.version.clone());
        let manifest = create_manifest(
            trusted,
            node_id,
            candidate,
            repo,
            preview_id,
            file_name,
            files.clone(),
            relations,
            prerelease,
        );
        Ok(Payload {
            candidate: actual,
            stage_root: stage_root.to_path_buf(),
            files,
            manifest: Some(manifest),
        })
    })();
    if result.is_err() {
        // This exact directory was created above; never remove the user's original asset.
        if let Err(error) = fs::remove_dir_all(stage_root) {
            log::warn!("清理批次资产沙箱失败：{error}");
        }
    }
    result
}

fn create_sandbox(
    state: &AppState,
    trusted: &TrustedInventory,
    stage_root: &Path,
) -> Result<(), String> {
    let root = state
        .config
        .config_path()
        .parent()
        .ok_or("archive.pathEscape")?
        .to_path_buf();
    // Fingerprint also rejects reparse points in existing ancestors.
    if !stage_root.is_absolute()
        || !stage_root.starts_with(&root)
        || stage_root == root
        || stage_root.starts_with(&trusted.game_root)
        || !matches!(Fingerprint::capture(stage_root)?, Fingerprint::Missing)
    {
        return Err("archive.pathEscape".into());
    }
    let parent = stage_root.parent().ok_or("archive.pathEscape")?;
    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    fs::create_dir(stage_root).map_err(|error| error.to_string())
}

fn read_members(
    trusted: &TrustedInventory,
    node_id: &str,
    stage_root: &Path,
    files: &mut [String],
    prerelease: bool,
) -> Result<Vec<ProvidedPlugin>, String> {
    let indices = files
        .iter()
        .enumerate()
        .filter_map(|(index, path)| {
            let lower = path.to_ascii_lowercase();
            (lower.ends_with(".dll") || lower.ends_with(".dll.disabled")).then_some(index)
        })
        .collect::<Vec<_>>();
    let requests = indices
        .iter()
        .map(|&index| MetadataRequest {
            path: stage_root.join(&files[index]),
            display_name: files[index].clone(),
            kind: MetadataKind::ModDll,
        })
        .collect::<Vec<_>>();
    let outcomes = metadata_read::read_metadata_batch(
        &requests,
        crate::concurrency::workers_from(trusted.config.mod_metadata_concurrency),
        &|_, _| {},
    );
    let previous = trusted
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| node.current.as_ref());
    let mut keys = HashSet::new();
    let mut provides = Vec::new();
    for ((&index, request), outcome) in indices.iter().zip(&requests).zip(outcomes) {
        let details = outcome.mod_dll().ok_or("metadataRead.taskFailed")?;
        let canonical = mods::canonical_dll_name(&files[index]).to_string();
        let member_key = inventory::key(&canonical);
        if !keys.insert(member_key.clone()) {
            return Err("modChanges.targetChanged".into());
        }
        let guid = details.guid.clone().unwrap_or_default();
        let exact = previous.and_then(|previous| {
            previous
                .provides
                .iter()
                .find(|plugin| plugin.member_key.as_deref() == Some(&member_key))
        });
        let by_guid = previous
            .map(|previous| {
                previous
                    .provides
                    .iter()
                    .filter(|plugin| !guid.is_empty() && plugin.guid.eq_ignore_ascii_case(&guid))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if exact.is_none() && by_guid.len() > 1 {
            return Err("modChanges.targetChanged".into());
        }
        let enabled = exact
            .or_else(|| by_guid.first().copied())
            .map(|plugin| plugin.enabled)
            .unwrap_or_else(|| !files[index].to_ascii_lowercase().ends_with(".disabled"));
        let destination = if enabled {
            canonical
        } else {
            format!("{canonical}.disabled")
        };
        if files[index] != destination {
            let target = crate::game::safe_game_path(stage_root, &destination)?;
            if target.exists() {
                return Err("archive.targetExists".into());
            }
            fs::rename(&request.path, target).map_err(|error| error.to_string())?;
            files[index] = destination;
        }
        provides.push(ProvidedPlugin {
            member_key: Some(member_key),
            guid,
            version: outcome
                .value
                .as_ref()
                .and_then(|value| value.version())
                .map(str::to_string),
            prerelease: Some(prerelease),
            enabled,
        });
    }
    Ok(provides)
}

fn create_manifest(
    trusted: &TrustedInventory,
    node_id: &str,
    candidate: &Candidate,
    repo: Option<&ModRepositoryConfig>,
    preview_id: &str,
    file_name: &str,
    files: Vec<String>,
    relations: Vec<RepositoryRelation>,
    prerelease: bool,
) -> archive::InstallManifest {
    let package_id = trusted
        .files
        .get(node_id)
        .and_then(|files| files.summary.package_id.as_deref());
    let generated = format!("manual-{preview_id}");
    let id = package_id
        .or_else(|| repo.map(|repo| repo.id.as_str()))
        .unwrap_or(&generated);
    let names = repo
        .map(|repo| {
            repo.name_i18n
                .iter()
                .filter(|(_, value)| !value.trim().is_empty())
                .map(|(language, value)| {
                    (
                        language.clone(),
                        archive::package_display_name(value, &repo.provider),
                    )
                })
                .collect()
        })
        .unwrap_or_else(|| BTreeMap::from([("zh-CN".into(), mods::file_stem_name(file_name))]));
    let notes = repo.map(|repo| repo.note_i18n.clone()).unwrap_or_default();
    let mut manifest = archive::new_manifest(
        id,
        candidate.tag.as_deref().unwrap_or_default(),
        names,
        notes,
        files,
        relations,
    );
    if let Some(repo) = repo {
        manifest.repository_binding =
            ModRepositoryBinding::from_release(repo, candidate.tag.as_deref().unwrap_or_default());
        manifest.group_i18n = repo.group_i18n.clone();
    }
    manifest.prerelease = prerelease;
    manifest
}

#[cfg(test)]
#[path = "payload/tests.rs"]
mod tests;
