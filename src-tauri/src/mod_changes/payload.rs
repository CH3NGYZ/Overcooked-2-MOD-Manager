//! Validate an asset and unpack it into a private simulated game root.
//! All returned paths are game-relative; this module never installs into the live game.
use super::{
    inventory::{self, TrustedInventory},
    model::*,
    mutations::{package_identity_conflict, report_install_block},
    session::Payload,
    transaction::{Fingerprint, path_key},
};
use crate::{
    app_state::AppState,
    archive::{self, release_download::SplitZipProof},
    metadata_read::{self, MetadataKind, MetadataRequest},
    mod_preview::staging,
    models::*,
    mods::{self, repository_download::RepositoryAssetDownload},
    releases,
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
    sync::Arc,
};

pub(super) mod manual_relations;
mod retired_copies;

enum SourceOrigin<'a> {
    Manual(&'a str),
    Repository {
        id: &'a str,
        tag: &'a str,
        prerelease: bool,
        split_proof: Option<&'a SplitZipProof>,
    },
}

struct PayloadSource<'a> {
    path: &'a Path,
    file_name: &'a str,
    origin: SourceOrigin<'a>,
    downloaded: bool,
}

pub(super) fn from_downloaded(
    state: &AppState,
    trusted: &TrustedInventory,
    download: RepositoryAssetDownload,
    node_id: &str,
    candidate: &Candidate,
    asset_name: Option<&str>,
    stage_root: &Path,
) -> Result<Payload, String> {
    // The transport owns this temporary file until validation and extraction finish.
    // Its random disk name is never used to determine the type or layout.
    let proof = download.file.split_proof()?;
    stage_asset(
        state,
        trusted,
        PayloadSource {
            path: download.file.path(),
            file_name: &download.file_name,
            origin: SourceOrigin::Repository {
                id: &download.repository_id,
                tag: &download.release_tag,
                prerelease: download.prerelease,
                split_proof: proof.as_ref(),
            },
            downloaded: true,
        },
        node_id,
        candidate,
        asset_name,
        stage_root,
    )
}

pub(crate) fn from_preview(
    state: &AppState,
    trusted: &TrustedInventory,
    preview_id: &str,
    node_id: &str,
    candidate: &Candidate,
    asset_name: Option<&str>,
    split_proof: Option<&SplitZipProof>,
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
    let origin = match &origin {
        Some(record) => SourceOrigin::Repository {
            id: &record.repository_id,
            tag: &record.release_tag,
            prerelease: record.prerelease,
            split_proof,
        },
        None if split_proof.is_none() => SourceOrigin::Manual(preview_id),
        None => return Err("modChanges.targetChanged".into()),
    };
    stage_asset(
        state,
        trusted,
        PayloadSource {
            path: &source,
            file_name,
            origin,
            downloaded: false,
        },
        node_id,
        candidate,
        asset_name,
        stage_root,
    )
}

fn stage_asset(
    state: &AppState,
    trusted: &TrustedInventory,
    input: PayloadSource<'_>,
    node_id: &str,
    candidate: &Candidate,
    asset_name: Option<&str>,
    stage_root: &Path,
) -> Result<Payload, String> {
    let PayloadSource {
        path: source,
        file_name,
        origin,
        downloaded,
    } = input;
    archive::validate_asset_file_name(file_name)?;
    let source_fingerprint = Fingerprint::capture(source)?;
    let downloaded = if let SourceOrigin::Manual(preview) = &origin {
        use super::downloaded_source::Source;
        state
            .downloaded_sources
            .verify(
                uuid::Uuid::parse_str(preview).map_err(|_| "modPreview.invalidId")?,
                source,
                &source_fingerprint,
                &Source::ModUrl,
                false,
            )?
            .is_some()
    } else {
        downloaded
    };
    let Fingerprint::File { size, sha256, .. } = &source_fingerprint else {
        return Err("archive.pathEscape".into());
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
    let (prerelease, split_proof) = match (&origin, repo) {
        (
            SourceOrigin::Repository {
                id,
                tag,
                prerelease,
                split_proof,
            },
            Some(repo),
        ) if *id == repo.id && candidate.tag.as_deref().unwrap_or_default() == *tag => {
            (*prerelease, *split_proof)
        }
        (SourceOrigin::Manual(_), None) => (false, None),
        _ => return Err("modChanges.targetChanged".into()),
    };
    let mut asset_digest_verified = false;
    let mut release_asset = None;
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
            release_asset = Some((asset, &release.assets));
        } else if !candidate.raw {
            return Err("modChanges.targetChanged".into());
        }
    }
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let is_zip = extension.eq_ignore_ascii_case("zip");
    if !is_zip && !extension.eq_ignore_ascii_case("dll") {
        return Err("mods.unsupportedFileType".into());
    }
    if let Some((asset, assets)) = release_asset {
        let parts = archive::release_download::asset_set(asset, assets)?;
        if parts.len() > 1 {
            // The main .zip's API metadata describes the last volume only.
            // Verify the reassembled bytes using the backend's fresh receipt.
            let proof = split_proof.ok_or("modChanges.targetChanged")?;
            asset_digest_verified = proof.validate(&parts, *size, sha256)?;
        } else {
            if split_proof.is_some() {
                return Err("modChanges.targetChanged".into());
            }
            archive::validate_download_size(*size, (asset.size > 0).then_some(asset.size))?;
            if let Some(digest) = asset.digest.as_deref() {
                archive::validate_download_digest(sha256, digest)?;
                asset_digest_verified = true;
            }
        }
    } else {
        if split_proof.is_some() {
            return Err("modChanges.targetChanged".into());
        }
        archive::validate_download_size(*size, None)?;
    }
    if is_zip {
        archive::validate_zip(source)?;
    }
    create_sandbox(state, trusted, stage_root)?;
    let result = (|| {
        let pending = if is_zip {
            if let Some(repo) = repo {
                let plan = archive::prepare_repository_mod_zip(
                    source,
                    stage_root,
                    &repo.install_target,
                    file_name,
                )?;
                archive::extract_prepared_mod_zip(source, stage_root, &plan)?
            } else {
                archive::extract_local_mod_zip_pending(source, stage_root, file_name)?
            }
        } else {
            archive::copy_asset_pending(
                source,
                stage_root,
                repo.map(|repo| repo.install_target.as_str())
                    .unwrap_or("BepInEx/plugins"),
                file_name,
            )?
        };
        let mut files = pending.commit();
        if Fingerprint::capture(source)? != source_fingerprint {
            return Err("modChanges.targetChanged".into());
        }
        let retiring = trusted
            .duplicate_releases
            .copies_for(node_id, &candidate.id);
        let primary = repo.map(|repo| repo.plugin_guid.as_str());
        let (mut provides, member_roles) = read_members(
            trusted,
            node_id,
            stage_root,
            &mut files,
            repo.map(|_| prerelease),
            retiring.zip(primary),
        )?;
        if provides.is_empty() {
            return Err("archive.noInstallableContent".into());
        }
        if let Some(copies) = retiring {
            retired_copies::verify(
                trusted,
                node_id,
                copies,
                primary.ok_or(retired_copies::INVALID)?,
                &provides,
            )?;
        }
        if let Some(primary) = primary.map(str::trim).filter(|guid| !guid.is_empty()) {
            let matches = provides
                .iter()
                .filter(|member| member.guid.eq_ignore_ascii_case(primary));
            match matches.clone().count() {
                0 => {
                    return Err(report_install_block(
                        "modChanges.installPackageIdentityMissing",
                        node_id,
                        &[("guid", primary), ("path", file_name)],
                    ));
                }
                1 => (),
                _ => return Err(package_identity_conflict(node_id, primary, matches)),
            }
        }
        let framework = super::level_trees::unique_framework_member(node_id, &provides)?;
        let framework_version = framework
            .filter(|_| repo.is_none())
            .map(|member| member.version.clone());
        if let Some(framework) = framework {
            let levels = super::level_trees::framework_level_root(node_id, framework)?;
            // The map inventory owns existing map DLLs. Bundled defaults are
            // composed separately and must not advertise versions we skip writing.
            provides.retain(|member| {
                let bundled = member
                    .member_key
                    .as_deref()
                    .is_some_and(|path| super::mutations::within(path, &levels));
                if bundled {
                    log::debug!(
                        "地图DLL不计入框架提供项：节点={node_id}，路径={:?}",
                        member.member_key
                    );
                }
                !bundled
            });
        }
        // Only an exact repository Release can replace a declaration-only
        // member. A local preview is allowed to leave it in place.
        trusted.verify_declared_bundle_overlaps(
            node_id,
            release_asset.is_some().then_some(repo).flatten(),
            &provides,
        )?;
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
        let mut notices = Vec::new();
        if matches!(origin, SourceOrigin::Manual(_)) {
            let declarations = manual_relations::for_members(&trusted.config, &actual.provides);
            actual.relations = declarations.relations;
            notices = declarations.notices;
        } else {
            for member in &actual.provides {
                if member.guid.is_empty()
                    || repo.is_some_and(|repo| repo.plugin_guid.eq_ignore_ascii_case(&member.guid))
                {
                    continue;
                }
                let (relations, known) =
                    super::inventory_maps::member_relations(&trusted.config, member);
                if known == Some(false) {
                    return Err(report_install_block(
                        "modChanges.installMemberRelationConflict",
                        node_id,
                        &[
                            ("guid", &member.guid),
                            ("path", member.member_key.as_deref().unwrap_or(file_name)),
                        ],
                    ));
                }
                actual.relations.extend(relations);
            }
        }
        actual.relations_known = notices.is_empty();
        actual.version = repo
            .and_then(|repo| {
                actual
                    .provides
                    .iter()
                    .find(|plugin| plugin.guid.eq_ignore_ascii_case(&repo.plugin_guid))
            })
            .map(|plugin| plugin.version.clone())
            .or(framework_version)
            .unwrap_or_else(|| {
                actual
                    .provides
                    .iter()
                    .find(|plugin| !plugin.guid.is_empty())
                    .and_then(|plugin| plugin.version.clone())
            });
        let mut manifest = create_manifest(
            trusted,
            node_id,
            candidate,
            repo,
            &origin,
            file_name,
            files.clone(),
            relations,
            prerelease,
        );
        if manifest.repository_binding.is_some() {
            // Reuse the DLL metadata already read from the verified staging
            // tree; recording this does not reopen the asset or the game files.
            manifest.installed_dll_versions = actual
                .provides
                .iter()
                .filter_map(|member| {
                    let path = member.member_key.as_deref()?;
                    Some((
                        mods::manifest_dll_key(path),
                        member.version.clone().unwrap_or_default(),
                    ))
                })
                .collect();
        }
        let (stage_fingerprint, release_files) = if is_zip && asset_digest_verified {
            let mut files = HashMap::new();
            // Disk enumeration may use a different spelling of a shared parent
            // directory than the ZIP. Use the transaction's portable alias key.
            let stamp = Fingerprint::capture_with_files(stage_root, &mut |path, stamp| {
                let relative = path
                    .strip_prefix(stage_root)
                    .map_err(|_| "archive.pathEscape")?;
                if files.insert(path_key(relative), stamp.clone()).is_some() {
                    return Err(report_install_block(
                        "archive.targetExists",
                        node_id,
                        &[("path", &relative.to_string_lossy())],
                    ));
                }
                Ok(())
            })?;
            (stamp, Some(Arc::new(files)))
        } else {
            (Fingerprint::capture(stage_root)?, None)
        };
        Ok(Payload {
            candidate: actual,
            stage_root: stage_root.to_path_buf(),
            stage_fingerprint,
            release_files,
            downloaded,
            files,
            manifest: Some(manifest),
            member_roles,
            notices,
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

type VerifiedMembers = (Vec<ProvidedPlugin>, BTreeMap<String, DllRole>);

fn read_members(
    trusted: &TrustedInventory,
    node_id: &str,
    stage_root: &Path,
    files: &mut [String],
    prerelease: Option<bool>,
    retirement: Option<(&[inventory::RetiredModCopy], &str)>,
) -> Result<VerifiedMembers, String> {
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
    let (retiring, primary) = retirement.unzip();
    let previous = trusted
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| node.current.as_ref())
        .into_iter()
        .flat_map(|candidate| &candidate.provides)
        .filter(|plugin| !retired_copies::contains(retiring, plugin))
        .collect::<Vec<_>>();
    let mut keys = HashSet::new();
    let mut provides = Vec::new();
    let mut roles = BTreeMap::new();
    for ((&index, request), outcome) in indices.iter().zip(&requests).zip(outcomes) {
        let details = outcome.mod_dll().ok_or("metadataRead.taskFailed")?;
        let canonical = mods::canonical_dll_name(&files[index]).to_string();
        let member_key = inventory::key(&canonical);
        if !keys.insert(member_key.clone()) {
            return Err("modChanges.targetChanged".into());
        }
        let guid = details.guid.clone().unwrap_or_default();
        let enabled = if primary.is_some_and(|primary| super::evaluate::same_guid(primary, &guid)) {
            // A confirmed upgrade explicitly keeps its current core enabled.
            true
        } else {
            let exact = previous
                .iter()
                .copied()
                .find(|plugin| plugin.member_key.as_deref() == Some(&member_key));
            let mut by_guid = previous
                .iter()
                .copied()
                .filter(|plugin| !guid.is_empty() && plugin.guid.eq_ignore_ascii_case(&guid));
            let first = by_guid.next();
            if exact.is_none() && by_guid.next().is_some() {
                return Err("modChanges.targetChanged".into());
            }
            exact
                .or(first)
                .map(|plugin| plugin.enabled)
                .unwrap_or_else(|| !files[index].to_ascii_lowercase().ends_with(".disabled"))
        };
        let destination = if enabled {
            canonical
        } else {
            format!("{canonical}.disabled")
        };
        if files[index] != destination {
            let target = crate::game::safe_game_path(stage_root, &destination)?;
            if target.exists() {
                return Err(report_install_block(
                    "archive.targetExists",
                    node_id,
                    &[("path", &destination)],
                ));
            }
            fs::rename(&request.path, target).map_err(|error| error.to_string())?;
            files[index] = destination;
        }
        roles.insert(member_key.clone(), details.role);
        provides.push(ProvidedPlugin {
            member_key: Some(member_key),
            guid,
            version: outcome
                .value
                .as_ref()
                .and_then(|value| value.version())
                .map(str::to_string),
            prerelease,
            enabled,
        });
    }
    Ok((provides, roles))
}

fn create_manifest(
    trusted: &TrustedInventory,
    node_id: &str,
    candidate: &Candidate,
    repo: Option<&ModRepositoryConfig>,
    origin: &SourceOrigin<'_>,
    file_name: &str,
    files: Vec<String>,
    relations: Vec<RepositoryRelation>,
    prerelease: bool,
) -> archive::InstallManifest {
    let package_id = trusted
        .files
        .get(node_id)
        .and_then(|files| files.summary.package_id.as_deref());
    let id = package_id
        .map(str::to_owned)
        .unwrap_or_else(|| match origin {
            SourceOrigin::Manual(preview_id) => format!("manual-{preview_id}"),
            SourceOrigin::Repository { id, .. } => (*id).to_owned(),
        });
    let names = repo
        .map(archive::repository_package_names)
        .unwrap_or_else(|| BTreeMap::from([("zh-CN".into(), mods::file_stem_name(file_name))]));
    let notes = repo.map(|repo| repo.note_i18n.clone()).unwrap_or_default();
    let mut manifest = archive::new_manifest(
        &id,
        candidate.tag.as_deref().unwrap_or_default(),
        names,
        notes,
        files,
        relations,
    );
    manifest.manual_install = Some(matches!(origin, SourceOrigin::Manual(_)));
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
