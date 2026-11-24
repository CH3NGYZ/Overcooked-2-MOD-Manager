use super::super::{check, diagnostics, durable, paths, types::*};
use std::path::Path;
use uuid::Uuid;

pub(super) fn validate_journal(
    journal: &Journal,
    id: &str,
    transaction_root: &Path,
) -> Result<(), String> {
    let journal_path = transaction_root.join(id).join(durable::JOURNAL_FILE);
    let invalid_field = if !matches!(journal.version, 1 | 2 | JOURNAL_VERSION) {
        Some("schema_version")
    } else if journal.id != id || Uuid::parse_str(id).is_err() {
        Some("transaction_id")
    } else if transaction_root.parent() != Some(journal.roots.data_root.as_path()) {
        Some("roots")
    } else if journal.created_directory_count > journal.created_directories.len() {
        Some("parent_count")
    } else if journal.version >= 2
        && journal
            .records
            .iter()
            .any(|record| record.publication.is_some() != record.operation.tracks_publication())
    {
        Some("file_publication")
    } else {
        None
    };
    if let Some(field) = invalid_field {
        log::warn!(
            target: diagnostics::TARGET,
            "整链恢复记录校验失败：{}路径={:?}，字段={field}，实际版本={}，支持版本=1/2/{JOURNAL_VERSION}，ID与目录一致={}，父目录已建={}，计划父目录={}",
            diagnostics::Location, diagnostics::path(&journal_path), journal.version,
            journal.id == id, journal.created_directory_count, journal.created_directories.len(),
        );
        return Err(paths::INVALID.into());
    }
    let roots = &journal.roots;
    paths::validate_roots(transaction_root, roots)?;
    paths::validate_records(&journal.records, transaction_root, roots).inspect_err(|error| {
        log::warn!(
            target: diagnostics::TARGET,
            "整链恢复变更记录校验失败：{}路径={:?}，记录数={}，原因={:?}",
            diagnostics::Location, diagnostics::path(&journal_path), journal.records.len(),
            crate::locale_tag::diagnostic_tag(error),
        );
    })?;
    for parent in &journal.created_directories {
        let absolute = parent.resolve(roots)?;
        if paths::overlaps(&absolute, transaction_root) {
            return Err(paths::UNSAFE.into());
        }
    }
    Ok(())
}

pub(super) fn validate_selected_roots(
    journal: &Journal,
    roots: &TransactionRoots,
    journal_path: &Path,
) -> Result<(), String> {
    if journal.roots == *roots {
        return Ok(());
    }
    log::warn!(
        target: diagnostics::TARGET,
        "整链恢复根目录不符：{}路径={:?}，字段=roots，记录游戏根={:?}，当前游戏根={:?}，记录数据根={:?}，当前数据根={:?}",
        diagnostics::Location, diagnostics::path(journal_path),
        diagnostics::path(&journal.roots.game_root), diagnostics::path(&roots.game_root),
        diagnostics::path(&journal.roots.data_root), diagnostics::path(&roots.data_root),
    );
    Err(paths::INVALID.into())
}

pub(super) fn expand_paths(
    journal: &Journal,
    directory: &Path,
    output: &mut Vec<RecoveryPath>,
) -> Result<(), String> {
    use RecoveryPathKind::{Backup, Parent, Staging, Target};
    let mut endpoint = |kind, path: &TransactionPath| {
        let path = path.resolve(&journal.roots)?;
        output.push(RecoveryPath { kind, path });
        Ok::<(), String>(())
    };
    for record in &journal.records {
        match &record.operation {
            RecordedMutation::File {
                target, temporary, ..
            } => {
                endpoint(Target, target)?;
                endpoint(Staging, temporary)?;
            }
            RecordedMutation::Move { from, to, .. } => {
                endpoint(Target, from)?;
                endpoint(Target, to)?;
            }
            RecordedMutation::Tree {
                target,
                backup,
                temporary,
                ..
            } => {
                endpoint(Target, target)?;
                endpoint(Backup, backup)?;
                endpoint(Staging, temporary)?;
            }
        }
    }
    for parent in &journal.created_directories[..journal.created_directory_count] {
        endpoint(Parent, parent)?;
    }
    for record in &journal.records {
        let (backup, payload) = match &record.operation {
            RecordedMutation::File {
                backup, payload, ..
            } => (backup.as_ref(), payload.as_ref()),
            RecordedMutation::Tree { payload, .. } => (None, payload.as_ref()),
            RecordedMutation::Move { .. } => (None, None),
        };
        for (relative, kind, stage) in [
            (backup, Backup, check::BACKUP),
            (payload, Staging, check::STAGED),
        ] {
            if let Some(relative) = relative {
                let path = durable::private_path(directory, relative, stage)?;
                output.push(RecoveryPath { kind, path });
            }
        }
    }
    output.sort_unstable();
    output.dedup();
    Ok(())
}
