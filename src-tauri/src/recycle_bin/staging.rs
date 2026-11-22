use super::delete::remove_entry_directory;
use super::errors::UNSAFE_PATH;
use super::errors::io_error;
use super::errors::{log_failure, log_path};
use super::file_io::fingerprint;
use super::file_io::move_file;
use super::file_io::rollback_moves;
use super::file_io::write_atomic;
use super::manifests::set_active_manifest;
use super::paths::checked_path;
use super::paths::manifest_path;
use super::paths::record_directory;
use super::paths::validate_absolute_path;
use super::records::RecordState;
use super::records::RecycleRecord;
use super::records::RelatedManifest;
use super::records::StagedFile;
use super::records::save_record;
use crate::archive;
use crate::archive::InstallManifest;
use crate::file_check;
use crate::models::ModRecycleEntry;
use chrono::Utc;
use std::collections::HashSet;
use std::fs;
use std::path::Path;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)]
/// 把一批游戏相对路径移进回收站。
///
/// `pub(crate)` 是因为关卡集（`level_sets/operations.rs`）也要用它：一个关卡集是整目录，
/// 但回收记录按**文件**存，所以调用方把目录展开成文件列表再传进来 —— 恢复时
/// 就能按原路径逐一还原，与 MOD 的恢复走同一条代码路径。
///
/// 不新增一套「目录回收」：那会让回收站的记录格式分叉，恢复、彻底删除、
/// 失败回滚全都要写两遍。
pub(crate) fn stage(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    name: String,
    manifest: Option<&InstallManifest>,
    original_manifest: Option<&[u8]>,
    removed_manifest_files: Vec<String>,
    actual_files: Vec<String>,
    remaining_manifest: Option<&InstallManifest>,
    after_move: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ModRecycleEntry, String> {
    stage_with_related_manifests(
        root,
        manifests_dir,
        game_root,
        name,
        manifest,
        original_manifest,
        removed_manifest_files,
        actual_files,
        remaining_manifest,
        Vec::new(),
        false,
        after_move,
    )
}

pub(super) fn stage_with_related_manifests(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    name: String,
    manifest: Option<&InstallManifest>,
    original_manifest: Option<&[u8]>,
    mut removed_manifest_files: Vec<String>,
    mut actual_files: Vec<String>,
    remaining_manifest: Option<&InstallManifest>,
    related_manifests: Vec<RelatedManifest>,
    diychef_resources_imported: bool,
    after_move: &mut dyn FnMut() -> Result<(), String>,
) -> Result<ModRecycleEntry, String> {
    validate_absolute_path(root)?;
    validate_absolute_path(game_root)?;
    // User-added legacy models must be journaled before removing their DLL:
    // a same-directory upgrade may replace these filenames immediately after.
    let legacy_files = if diychef_resources_imported {
        vec![]
    } else {
        super::legacy_resources::legacy_resource_files(
            game_root,
            manifests_dir,
            &actual_files,
            manifest,
        )?
    };
    let legacy_keys = legacy_files
        .iter()
        .map(|file| super::paths::file_key(file))
        .collect::<HashSet<_>>();
    if let Some(manifest) = manifest {
        for file in &manifest.files {
            if legacy_keys.contains(&super::paths::file_key(file))
                && !removed_manifest_files
                    .iter()
                    .any(|removed| super::paths::file_key(removed) == super::paths::file_key(file))
            {
                removed_manifest_files.push(file.clone());
            }
        }
    }
    let remaining_manifest = remaining_manifest
        .cloned()
        .map(|mut manifest| {
            manifest
                .files
                .retain(|file| !legacy_keys.contains(&super::paths::file_key(file)));
            manifest
        })
        .filter(|manifest| !manifest.files.is_empty());
    let remaining_manifest = remaining_manifest.as_ref();
    actual_files.extend(legacy_files);
    let canonical_game = fs::canonicalize(game_root).map_err(io_error)?;
    // A manager placed inside plugins must not have its staged DLLs scanned as
    // active MODs. Resolve the nearest existing ancestor before creating the bin.
    let existing_ancestor = root
        .ancestors()
        .find(|path| path.exists())
        .ok_or(UNSAFE_PATH)?;
    let resolved_ancestor = fs::canonicalize(existing_ancestor).map_err(io_error)?;
    let resolved_root = resolved_ancestor.join(
        root.strip_prefix(existing_ancestor)
            .map_err(|_| UNSAFE_PATH.to_string())?,
    );
    let inside_plugins = resolved_root
        .strip_prefix(&canonical_game)
        .ok()
        .is_some_and(|relative| {
            let mut parts = relative.components();
            parts
                .next()
                .is_some_and(|part| part.as_os_str().eq_ignore_ascii_case("BepInEx"))
                && parts
                    .next()
                    .is_some_and(|part| part.as_os_str().eq_ignore_ascii_case("plugins"))
        });
    if inside_plugins {
        return Err("recycle.insideModsDirectory".into());
    }
    let id = Uuid::new_v4().to_string();
    let directory = record_directory(root, &id)
        .inspect_err(|error| log_failure("暂存", &id, "定位回收记录", root, error))?;
    let mut unique = HashSet::new();
    let mut files = Vec::new();
    for relative in actual_files {
        if unique.insert(relative.to_lowercase()) {
            let path = checked_path(game_root, &relative).inspect_err(|error| {
                log_failure("暂存", &id, "核对源路径", &game_root.join(&relative), error);
            })?;
            let (size, sha256) = fingerprint(&path, None)
                .inspect_err(|error| log_failure("暂存", &id, "读取源文件指纹", &path, error))?;
            files.push(StagedFile {
                relative,
                size,
                sha256,
            });
        }
    }
    let mut record = RecycleRecord {
        id: id.clone(),
        name,
        // 仓库名的多语言快照：回收站列表要能随界面语言渲染，而删除之后
        // 仓库可能已从源文档下线，无处可查，所以在这里固化下来。
        name_i18n: manifest
            .map(|manifest| manifest.package_name_i18n.clone())
            .unwrap_or_default(),
        deleted_at: Utc::now().to_rfc3339(),
        game_path: canonical_game.to_string_lossy().into_owned(),
        package_id: manifest.map(|manifest| manifest.repository_id.clone()),
        release_tag: manifest.map(|manifest| manifest.release_tag.clone()),
        removed_manifest_files,
        related_manifests,
        files,
        state: RecordState::Preparing,
        diychef_resources_imported,
    };
    let failed = |phase, path: &Path, error: &str| log_failure("暂存", &id, phase, path, error);
    fs::create_dir_all(&directory)
        .map_err(io_error)
        .inspect_err(|error| failed("创建回收记录目录", &directory, error))?;
    let payload = checked_path(&directory, "files")
        .inspect_err(|error| failed("定位文件暂存目录", &directory, error))?;
    fs::create_dir_all(&payload)
        .map_err(io_error)
        .inspect_err(|error| failed("创建文件暂存目录", &payload, error))?;
    log::info!(
        "MOD 移入回收站开始：ID={}，名称={:?}，文件数={}，暂存={:?}，游戏={:?}",
        record.id,
        file_check::log_field(&record.name),
        record.files.len(),
        log_path(&payload),
        log_path(game_root),
    );
    // Journal exists before any file is removed, including interrupted operations.
    let journal_result: Result<(), String> = (|| {
        save_record(&directory, &record)?;
        if let Some(bytes) = original_manifest {
            write_atomic(&checked_path(&directory, "manifest.toml")?, bytes)?;
        }
        Ok(())
    })();
    if let Err(error) = journal_result {
        failed(
            "写入回收记录或原清单（尚未移动游戏文件）",
            &directory,
            &error,
        );
        let _ = remove_entry_directory(&directory).inspect_err(|error| {
            failed("清理未完成的回收记录目录", &directory, error);
        });
        return Err(error);
    }
    let mut moved = Vec::new();
    let mut manifest_changed = false;
    let mut related_changed = 0usize;
    let mut phase = "移动游戏文件";
    let result = (|| -> Result<(), String> {
        for file in &record.files {
            phase = "移动游戏文件";
            let source = checked_path(game_root, &file.relative).inspect_err(|error| {
                failed("核对移动源路径", &game_root.join(&file.relative), error);
            })?;
            let target = checked_path(&payload, &file.relative).inspect_err(|error| {
                failed("核对移动目标路径", &payload.join(&file.relative), error);
            })?;
            move_file(&source, &target, file, false)
                .inspect_err(|error| failed(phase, &source, error))?;
            moved.push(file.clone());
            phase = "移动后复核";
            after_move()?;
        }
        phase = "提交主安装清单";
        if let Some(manifest) = manifest {
            set_active_manifest(manifests_dir, &manifest.repository_id, remaining_manifest)?;
            manifest_changed = true;
        }
        phase = "提交关联安装清单";
        for related in &record.related_manifests {
            set_active_manifest(
                manifests_dir,
                &related.package_id,
                related.remaining()?.as_ref(),
            )
            .inspect_err(|error| {
                failed(
                    phase,
                    &manifests_dir.join(format!("{}.toml", related.package_id)),
                    error,
                );
            })?;
            related_changed += 1;
        }
        phase = "提交回收记录为就绪";
        record.state = RecordState::Ready;
        save_record(&directory, &record)
    })();
    if let Err(error) = result {
        let files_rolled_back = rollback_moves(&payload, game_root, &moved);
        let mut manifest_rolled_back = !manifest_changed
            || manifest
                .zip(original_manifest)
                .is_some_and(|(manifest, bytes)| {
                    manifest_path(manifests_dir, &manifest.repository_id)
                        .and_then(|path| write_atomic(&path, bytes))
                        .inspect_err(|error| {
                            failed(
                                "回滚主安装清单",
                                &manifests_dir.join(format!("{}.toml", manifest.repository_id)),
                                error,
                            );
                        })
                        .is_ok()
                });
        for related in record.related_manifests[..related_changed].iter().rev() {
            manifest_rolled_back &= manifest_path(manifests_dir, &related.package_id)
                .and_then(|path| write_atomic(&path, related.original.as_bytes()))
                .inspect_err(|error| {
                    failed(
                        "回滚关联安装清单",
                        &manifests_dir.join(format!("{}.toml", related.package_id)),
                        error,
                    );
                })
                .is_ok();
        }
        if files_rolled_back && manifest_rolled_back {
            let cleaned = remove_entry_directory(&directory)
                .inspect_err(|error| {
                    failed("回滚后清理回收目录", &directory, error);
                })
                .is_ok();
            log::warn!(
                "MOD 回收失败，回滚已完成：ID={}，目录={:?}，阶段={phase}，原因={}，回滚文件数={}，回滚清单数={}，回收目录已清理={cleaned}",
                record.id,
                log_path(&directory),
                crate::locale_tag::diagnostic_tag(&error),
                moved.len(),
                related_changed + if manifest_changed { 1 } else { 0 },
            );
            return Err(error);
        }
        record.state = RecordState::Preparing;
        let saved = save_record(&directory, &record)
            .inspect_err(|error| {
                failed(
                    "保存未完成回收的记录",
                    &directory.join("record.json"),
                    error,
                );
            })
            .is_ok();
        log::error!(
            "MOD 回收回滚未完成，保留回收目录以便排查：ID={}，目录={:?}，阶段={phase}，原因={}，文件回滚={files_rolled_back}，清单回滚={manifest_rolled_back}，未完成状态已保存={saved}",
            record.id,
            log_path(&directory),
            crate::locale_tag::diagnostic_tag(&error),
        );
        return Err("recycle.partialFailure".into());
    }
    for file in &record.files {
        if let Some(parent) = game_root.join(&file.relative).parent() {
            if let Err(error) = archive::prune_empty_parent_dirs(game_root, parent) {
                log::warn!("回收后空目录清理失败：{}", error);
            }
        }
    }
    log::info!("MOD 移入回收站完成：ID={}", record.id);
    Ok(record.display())
}
