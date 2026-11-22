use super::delete::remove_entry_directory;
use super::errors::{VERIFY_FILES, WRITE_RESTORED_FILE, io_error, log_failure, log_path};
use super::file_io::matches_fingerprint;
use super::file_io::move_file;
use super::file_io::rollback_moves;
use super::file_io::write_atomic;
use super::manifests::merge_restored_manifest;
use super::manifests::original_manifest;
use super::manifests::read_active_manifest;
use super::manifests::set_active_manifest;
use super::paths::checked_path_for;
use super::paths::existing_variants;
use super::paths::manifest_path;
use super::paths::record_directory;
use super::paths::validate_absolute_path;
use super::payload::validate_payload;
use super::records::RecordState;
use super::records::load_record;
use super::records::save_record;
use crate::{file_check, mods};
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub fn restore(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    id: &str,
) -> Result<(), String> {
    let failed = |phase, path: &Path, error: &str| log_failure("恢复", id, phase, path, error);
    let directory =
        record_directory(root, id).inspect_err(|error| failed("定位回收记录", root, error))?;
    let mut record = load_record(&directory, id)
        .inspect_err(|error| failed("读取回收记录", &directory, error))?;
    if record.state == RecordState::Restored {
        failed(
            "读取回收记录（已恢复）",
            &directory,
            "recycle.recordMissing",
        );
        return Err("recycle.recordMissing".into());
    }
    validate_absolute_path(game_root)
        .inspect_err(|error| failed("检查当前游戏目录", game_root, error))?;
    let recorded_game = PathBuf::from(&record.game_path);
    validate_absolute_path(&recorded_game)
        .inspect_err(|error| failed("检查原游戏目录", &recorded_game, error))?;
    let current_game = fs::canonicalize(game_root)
        .map_err(io_error)
        .inspect_err(|error| failed("解析当前游戏目录", game_root, error))?;
    let original_game = fs::canonicalize(&recorded_game)
        .map_err(io_error)
        .inspect_err(|error| failed("解析原游戏目录", &recorded_game, error))?;
    if current_game != original_game {
        log::warn!(
            "回收站恢复失败：ID={id}，阶段=检查游戏目录，当前={:?}，原位置={:?}，原因=recycle.wrongGameDirectory",
            log_path(&current_game),
            log_path(&original_game),
        );
        return Err("recycle.wrongGameDirectory".into());
    }
    let verify = Some(VERIFY_FILES);
    let write = Some(WRITE_RESTORED_FILE);
    let payload = checked_path_for(&directory, "files", verify)?;
    validate_payload(&payload, &record)?;
    let original = original_manifest(&directory, &record)
        .inspect_err(|error| failed("核对原安装清单", &directory.join("manifest.toml"), error))?;
    let mut originals = Vec::new();
    if let Some(original) = original {
        originals.push((original, record.removed_manifest_files.clone()));
    }
    for related in &record.related_manifests {
        let manifest = related
            .manifest()
            .inspect_err(|error| failed("解析记录内关联清单", &directory, error))?;
        originals.push((manifest, related.removed_files.clone()));
    }
    let mut restore_plans = Vec::new();
    for (original, removed) in originals {
        let manifest_location = || manifests_dir.join(format!("{}.toml", original.repository_id));
        let previous = read_active_manifest(manifests_dir, &original.repository_id)
            .inspect_err(|error| failed("读取当前安装清单", &manifest_location(), error))?;
        let restored = merge_restored_manifest(
            &original,
            previous.as_ref().map(|(active, _)| active),
            &removed,
        )
        .inspect_err(|error| failed("核对待合并安装清单", &manifest_location(), error))?;
        restore_plans.push((restored, previous.map(|(_, bytes)| bytes)));
    }
    let mut pending = Vec::new();
    for file in &record.files {
        let staged = checked_path_for(&payload, &file.relative, verify)?;
        let target = checked_path_for(game_root, &file.relative, write)?;
        if !staged.try_exists().map_err(|error| {
            file_check::report_io(VERIFY_FILES, &staged, error, file_check::READ)
        })? {
            if record.state == RecordState::Preparing
                && target.try_exists().map_err(|error| {
                    file_check::report_io(WRITE_RESTORED_FILE, &target, error, file_check::READ)
                })?
            {
                if matches_fingerprint(&target, file, write)? {
                    continue;
                }
                return Err(file_check::report(
                    WRITE_RESTORED_FILE,
                    &target,
                    file_check::CONTENT,
                ));
            }
            return Err(file_check::report(
                VERIFY_FILES,
                &staged,
                file_check::MISSING,
            ));
        }
        if !matches_fingerprint(&staged, file, verify)? {
            return Err(file_check::report(
                VERIFY_FILES,
                &staged,
                file_check::CONTENT,
            ));
        }
        let canonical = mods::canonical_dll_name(&file.relative);
        if let Some(existing) = existing_variants(game_root, &canonical, write)?.first() {
            let target = game_root.join(existing);
            failed("核对恢复目标", &target, file_check::EXISTS);
            return Err(file_check::report(
                WRITE_RESTORED_FILE,
                &target,
                file_check::EXISTS,
            ));
        }
        pending.push(file.clone());
    }
    log::info!(
        "恢复 MOD 开始：ID={id}，文件数={}，清单数={}，暂存={:?}，游戏={:?}",
        pending.len(),
        restore_plans.len(),
        log_path(&payload),
        log_path(game_root),
    );
    let mut moved = Vec::new();
    let mut manifests_changed = 0usize;
    let result = (|| {
        for file in &pending {
            move_file(
                &checked_path_for(&payload, &file.relative, verify)?,
                &checked_path_for(game_root, &file.relative, write)?,
                file,
                true,
            )
            .inspect_err(|error| failed("移动恢复文件", &game_root.join(&file.relative), error))?;
            moved.push(file.clone());
        }
        for (manifest, _) in &restore_plans {
            set_active_manifest(manifests_dir, &manifest.repository_id, Some(manifest))
                .inspect_err(|error| {
                    failed(
                        "写入合并安装清单",
                        &manifests_dir.join(format!("{}.toml", manifest.repository_id)),
                        error,
                    );
                })?;
            manifests_changed += 1;
        }
        record.state = RecordState::Restored;
        save_record(&directory, &record)
            .inspect_err(|error| failed("提交已恢复记录", &directory.join("record.json"), error))
    })();
    if let Err(error) = result {
        let files_rolled_back = rollback_moves(game_root, &payload, &moved);
        let mut manifest_rolled_back = true;
        for (manifest, previous) in restore_plans[..manifests_changed].iter().rev() {
            manifest_rolled_back &= match previous {
                Some(bytes) => manifest_path(manifests_dir, &manifest.repository_id)
                    .and_then(|path| write_atomic(&path, bytes)),
                None => set_active_manifest(manifests_dir, &manifest.repository_id, None),
            }
            .inspect_err(|error| {
                failed(
                    "回滚安装清单",
                    &manifests_dir.join(format!("{}.toml", manifest.repository_id)),
                    error,
                );
            })
            .is_ok();
        }
        if !files_rolled_back || !manifest_rolled_back {
            record.state = RecordState::Preparing;
            let _ = save_record(&directory, &record).inspect_err(|error| {
                failed(
                    "保存未完成恢复的记录",
                    &directory.join("record.json"),
                    error,
                );
            });
            log::error!(
                "MOD 恢复回滚不完整，保留回收目录以便排查：ID={id}，目录={:?}，文件回滚={files_rolled_back}，清单回滚={manifest_rolled_back}，原始失败={:?}",
                log_path(&directory),
                crate::locale_tag::diagnostic_tag(&error),
            );
            return Err("recycle.partialFailure".into());
        }
        log::warn!(
            "MOD 恢复失败，回滚已完成，保留原回收记录：ID={id}，目录={:?}，回滚文件数={}，回滚清单数={manifests_changed}，原因={:?}",
            log_path(&directory),
            moved.len(),
            crate::locale_tag::diagnostic_tag(&error),
        );
        return Err(error);
    }
    // The restored journal is hidden if cleanup fails; recovered files are never deleted.
    if let Err(error) = remove_entry_directory(&directory) {
        failed(
            "清理已恢复的回收目录（已恢复文件保持原位）",
            &directory,
            &error,
        );
    }
    log::info!("MOD 恢复完成：ID={}", id);
    Ok(())
}
