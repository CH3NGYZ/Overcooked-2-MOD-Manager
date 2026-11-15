use super::delete::remove_entry_directory;
use super::errors::CONFLICT;
use super::errors::INCOMPLETE;
use super::errors::io_error;
use super::file_io::matches_fingerprint;
use super::file_io::move_file;
use super::file_io::rollback_moves;
use super::file_io::write_atomic;
use super::manifests::merge_restored_manifest;
use super::manifests::original_manifest;
use super::manifests::read_active_manifest;
use super::manifests::set_active_manifest;
use super::paths::checked_path;
use super::paths::existing_variants;
use super::paths::manifest_path;
use super::paths::record_directory;
use super::paths::validate_absolute_path;
use super::payload::validate_payload;
use super::records::RecordState;
use super::records::load_record;
use super::records::save_record;
use crate::mods;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

pub fn restore(
    root: &Path,
    manifests_dir: &Path,
    game_root: &Path,
    id: &str,
) -> Result<(), String> {
    let directory = record_directory(root, id)?;
    let mut record = load_record(&directory, id)?;
    if record.state == RecordState::Restored {
        return Err("recycle.recordMissing".into());
    }
    validate_absolute_path(game_root)?;
    let recorded_game = PathBuf::from(&record.game_path);
    validate_absolute_path(&recorded_game)?;
    if fs::canonicalize(game_root).map_err(io_error)?
        != fs::canonicalize(&recorded_game).map_err(io_error)?
    {
        return Err("recycle.wrongGameDirectory".into());
    }
    let payload = checked_path(&directory, "files")?;
    validate_payload(&payload, &record)?;
    let original = original_manifest(&directory, &record)?;
    let mut originals = Vec::new();
    if let Some(original) = original {
        originals.push((original, record.removed_manifest_files.clone()));
    }
    for related in &record.related_manifests {
        originals.push((related.manifest()?, related.removed_files.clone()));
    }
    let mut restore_plans = Vec::new();
    for (original, removed) in originals {
        let previous = read_active_manifest(manifests_dir, &original.repository_id)?;
        let restored = merge_restored_manifest(
            &original,
            previous.as_ref().map(|(active, _)| active),
            &removed,
        )?;
        restore_plans.push((restored, previous.map(|(_, bytes)| bytes)));
    }
    let mut pending = Vec::new();
    for file in &record.files {
        let staged = checked_path(&payload, &file.relative)?;
        let target = checked_path(game_root, &file.relative)?;
        if !staged.try_exists().map_err(io_error)? {
            if record.state == RecordState::Preparing
                && target.try_exists().map_err(io_error)?
                && matches_fingerprint(&target, file)?
            {
                continue;
            }
            return Err(INCOMPLETE.into());
        }
        if !matches_fingerprint(&staged, file)? {
            return Err(INCOMPLETE.into());
        }
        let canonical = mods::canonical_dll_name(&file.relative);
        if !existing_variants(game_root, &canonical)?.is_empty() {
            return Err(CONFLICT.into());
        }
        pending.push(file.clone());
    }
    log::info!("恢复 MOD 开始：ID={}，文件数={}", id, pending.len());
    let mut moved = Vec::new();
    let mut manifests_changed = 0usize;
    let result = (|| {
        for file in &pending {
            move_file(
                &checked_path(&payload, &file.relative)?,
                &checked_path(game_root, &file.relative)?,
                file,
            )?;
            moved.push(file.clone());
        }
        for (manifest, _) in &restore_plans {
            set_active_manifest(manifests_dir, &manifest.repository_id, Some(manifest))?;
            manifests_changed += 1;
        }
        record.state = RecordState::Restored;
        save_record(&directory, &record)
    })();
    if let Err(error) = result {
        let files_rolled_back = rollback_moves(game_root, &payload, &moved);
        let mut manifest_rolled_back = true;
        for (manifest, previous) in restore_plans[..manifests_changed].iter().rev() {
            manifest_rolled_back &= match previous {
                Some(bytes) => manifest_path(manifests_dir, &manifest.repository_id)
                    .and_then(|path| write_atomic(&path, bytes))
                    .is_ok(),
                None => set_active_manifest(manifests_dir, &manifest.repository_id, None).is_ok(),
            };
        }
        if !files_rolled_back || !manifest_rolled_back {
            record.state = RecordState::Preparing;
            let _ = save_record(&directory, &record);
            log::error!("MOD 恢复回滚不完整，保留回收记录：ID={}", id);
            return Err("recycle.partialFailure".into());
        }
        return Err(error);
    }
    // The restored journal is hidden if cleanup fails; recovered files are never deleted.
    if let Err(error) = remove_entry_directory(&directory) {
        log::warn!("已恢复 MOD 的回收记录清理失败：ID={}，原因={}", id, error);
    }
    log::info!("MOD 恢复完成：ID={}", id);
    Ok(())
}
