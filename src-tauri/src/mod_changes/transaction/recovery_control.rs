//! Read and control recovery decisions without modifying the journal or its materials.
use super::{PreparedTransaction, diagnostics, durable, paths, recovery::CLEANED_PREFIX, types::*};
use record_paths::{expand_paths, validate_journal, validate_selected_roots};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};
use uuid::Uuid;

mod record_paths;

pub(super) const DECISION_FILE: &str = "recovery-decision.json";
const CHANGED: &str = "modChanges.recoveryChanged";
const HIGH_RISK: &str = "modChanges.recoveryHighRiskRequired";
const MISSING: &str = "missing";

struct RecoveryRecord {
    directory: PathBuf,
    entry: RecoveryEntry,
    journal: Option<Journal>,
    validation_error: Option<String>,
    journal_stamp: Option<String>,
}

fn records(transaction_root: &Path) -> Result<Vec<PathBuf>, String> {
    paths::validate_absolute(transaction_root)?;
    let entries = match fs::read_dir(transaction_root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(paths::io_at(transaction_root, "list_journals", error)),
    };
    let mut directories = Vec::new();
    for entry in entries {
        let path = entry.map_err(paths::io_error)?.path();
        let metadata = fs::symlink_metadata(&path).map_err(paths::io_error)?;
        if paths::is_link(&metadata) || metadata.is_dir() && !is_cleaned(&path) {
            directories.push(path);
        }
    }
    directories.sort_unstable();
    Ok(directories)
}

fn is_cleaned(directory: &Path) -> bool {
    let Some(id) = directory
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix(CLEANED_PREFIX))
        .filter(|id| Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == *id))
    else {
        return false;
    };
    let Ok(mut entries) = fs::read_dir(directory) else {
        return false;
    };
    let Some(first) = entries.next() else {
        return true;
    };
    if !std::iter::once(first).chain(entries).all(|entry| {
        entry.is_ok_and(|entry| {
            matches!(
                entry.file_name().to_str(),
                Some(durable::JOURNAL_FILE | DECISION_FILE)
            )
        })
    }) {
        return false;
    }
    let Ok(journal) = durable::read_journal(directory) else {
        return false;
    };
    journal.state == JournalState::Cleaned
        && directory
            .parent()
            .is_some_and(|root| validate_journal(&journal, id, root).is_ok())
        && match durable::read_record(&directory.join(DECISION_FILE), 4096) {
            Ok(None) => true,
            Ok(Some(bytes)) => parse_decision(id, &bytes)
                .is_ok_and(|decision| decision.state == RecoveryDecisionState::Paused),
            Err(_) => false,
        }
}

fn inspect(directory: PathBuf, roots: Option<&TransactionRoots>) -> Option<RecoveryRecord> {
    let transaction_root = directory.parent()?;
    let id = directory.file_name()?.to_string_lossy().into_owned();
    let _scope = diagnostics::enter(&id);
    let mut validation_error = paths::validate_absolute(&directory)
        .err()
        .or_else(|| (!directory.is_dir()).then(|| paths::INVALID.into()));
    let journal_path = directory.join(durable::JOURNAL_FILE);
    let mut decision_bytes = None;
    let mut journal = None;
    let mut can_decide = false;
    let mut state = RecoveryDecisionState::Automatic;
    let mut decision = None;
    let mut journal_stamp = None;
    if validation_error.is_none() {
        let journal_bytes = durable::read_record(&journal_path, durable::MAX_JOURNAL_BYTES);
        let decision_result = durable::read_record(&directory.join(DECISION_FILE), 4096);
        if matches!(&journal_bytes, Ok(None))
            && matches!(&decision_result, Ok(None))
            && fs::read_dir(&directory).is_ok_and(|mut entries| entries.next().is_none())
        {
            return None;
        }
        let safe_id = Uuid::parse_str(&id).is_ok_and(|uuid| uuid.to_string() == id);
        match decision_result {
            Ok(Some(bytes)) => {
                match parse_decision(&id, &bytes) {
                    Ok(value) => {
                        state = value.state;
                        decision = Some(value);
                    }
                    Err(error) => validation_error = Some(error),
                }
                decision_bytes = Some(bytes);
            }
            Ok(None) => (),
            Err(error) => validation_error = Some(error),
        }
        match journal_bytes {
            Ok(Some(bytes)) => {
                can_decide = safe_id && validation_error.is_none();
                journal_stamp = Some(format!("{:x}", Sha256::digest(&bytes)));
                match durable::parse_journal(&journal_path, &bytes) {
                    Ok(value) => {
                        if let Err(error) = validate_journal(&value, &id, transaction_root) {
                            validation_error.get_or_insert(error);
                        }
                        journal = Some(value);
                    }
                    Err(error) => {
                        validation_error.get_or_insert(error);
                    }
                }
            }
            Ok(None) => {
                can_decide = safe_id && validation_error.is_none();
                journal_stamp = Some(MISSING.into());
                validation_error.get_or_insert_with(|| paths::INVALID.to_owned());
            }
            Err(error) => {
                validation_error.get_or_insert(error);
            }
        }
    }

    let decision_matches = decision.as_ref().is_some_and(|value| {
        value.state == RecoveryDecisionState::Abandoned
            && value.abandoned_journal_stamp == journal_stamp
            && journal_stamp.is_some()
    });
    let known_archive = decision_matches
        && decision
            .as_ref()
            .is_some_and(|value| !value.unknown_scope_acknowledged);
    let reserved = RecoveryPath {
        kind: RecoveryPathKind::Journal,
        path: directory.clone(),
    };
    let mut affected = vec![reserved];
    let scope_error = if validation_error.is_none() {
        journal
            .as_ref()
            .and_then(|journal| expand_paths(journal, &directory, &mut affected).err())
    } else {
        None
    };
    if scope_error.is_some() {
        affected.truncate(1);
    }
    let paths_verified = validation_error.is_none() && scope_error.is_none();
    if validation_error.is_none()
        && !known_archive
        && let (Some(journal), Some(roots)) = (&journal, roots)
    {
        validation_error = validate_selected_roots(journal, roots, &journal_path).err();
    }
    let scope_known = paths_verified && validation_error.is_none();
    let unknown_scope_acknowledged = decision_matches
        && decision
            .as_ref()
            .is_some_and(|value| value.unknown_scope_acknowledged);
    let archived = decision_matches && (scope_known || unknown_scope_acknowledged);
    let revision = format!(
        "{:x}",
        Sha256::new()
            .chain_update(id.as_bytes())
            .chain_update(b"\0")
            .chain_update(journal_stamp.as_deref().unwrap_or_default().as_bytes())
            .chain_update(b"\0")
            .chain_update(decision_bytes.as_deref().unwrap_or_default())
            .chain_update([u8::from(paths_verified), u8::from(scope_known)])
            .finalize()
    );

    let entry = RecoveryEntry {
        id: id.clone(),
        state,
        archived,
        application_error: journal
            .as_ref()
            .and_then(|value| value.application_error.clone()),
        recovery_error: journal
            .as_ref()
            .and_then(|value| value.recovery_error.clone())
            .or_else(|| validation_error.clone())
            .or(scope_error)
            .or_else(|| {
                (state == RecoveryDecisionState::Abandoned && !decision_matches)
                    .then(|| CHANGED.into())
            }),
        legacy_error: journal
            .as_ref()
            .and_then(|value| value.legacy_error.clone()),
        paths: affected,
        scope_known,
        can_decide: can_decide && !archived,
        unknown_scope_acknowledged,
        revision,
    };
    Some(RecoveryRecord {
        directory,
        entry,
        journal,
        validation_error,
        journal_stamp,
    })
}

fn parse_decision(id: &str, bytes: &[u8]) -> Result<RecoveryDecision, String> {
    let decision: RecoveryDecision = serde_json::from_slice(bytes).map_err(|_| paths::INVALID)?;
    let valid =
        match decision.state {
            RecoveryDecisionState::Paused => {
                decision.abandoned_journal_stamp.is_none() && !decision.unknown_scope_acknowledged
            }
            RecoveryDecisionState::Abandoned => decision
                .abandoned_journal_stamp
                .as_ref()
                .is_some_and(|hash| {
                    (hash == MISSING && decision.unknown_scope_acknowledged)
                        || hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
                }),
            RecoveryDecisionState::Automatic => false,
        };
    if decision.id != id || !valid {
        return Err(paths::INVALID.into());
    }
    Ok(decision)
}

pub(crate) fn list_recoveries(
    transaction_root: &Path,
    roots: Option<&TransactionRoots>,
) -> Result<Vec<RecoveryEntry>, String> {
    Ok(records(transaction_root)?
        .into_iter()
        .filter_map(|directory| inspect(directory, roots).map(|record| record.entry))
        .collect())
}

#[cfg(test)]
pub(crate) fn has_pending(transaction_root: &Path) -> Result<bool, String> {
    Ok(records(transaction_root)?
        .into_iter()
        .any(|directory| inspect(directory, None).is_some_and(|record| !record.entry.archived)))
}

pub(crate) fn has_retained_material(transaction_root: &Path) -> Result<bool, String> {
    Ok(!records(transaction_root)?.is_empty())
}

fn reviewed_record(
    transaction_root: &Path,
    id: &str,
    revision: &str,
    roots: Option<&TransactionRoots>,
) -> Result<RecoveryRecord, String> {
    let uuid = Uuid::parse_str(id).map_err(|_| CHANGED)?;
    if uuid.to_string() != id {
        return Err(CHANGED.into());
    }
    let directory = transaction_root.join(id);
    paths::validate_absolute(&directory)?;
    let record = inspect(directory, roots).ok_or(CHANGED)?;
    if !record.entry.can_decide || record.entry.revision != revision {
        return Err(CHANGED.into());
    }
    Ok(record)
}

pub(crate) fn pause_recovery(
    transaction_root: &Path,
    id: &str,
    revision: &str,
    roots: Option<&TransactionRoots>,
) -> Result<(), String> {
    let record = reviewed_record(transaction_root, id, revision, roots)?;
    if record.entry.state != RecoveryDecisionState::Automatic {
        return Err(CHANGED.into());
    }
    durable::write_json(
        &record.directory,
        DECISION_FILE,
        &RecoveryDecision {
            id: id.to_owned(),
            state: RecoveryDecisionState::Paused,
            abandoned_journal_stamp: None,
            unknown_scope_acknowledged: false,
        },
    )
}

pub(crate) fn abandon_recovery(
    transaction_root: &Path,
    id: &str,
    revision: &str,
    roots: Option<&TransactionRoots>,
    high_risk: bool,
) -> Result<(), String> {
    let record = reviewed_record(transaction_root, id, revision, roots)?;
    if !record.entry.scope_known && !high_risk {
        return Err(HIGH_RISK.into());
    }
    durable::write_json(
        &record.directory,
        DECISION_FILE,
        &RecoveryDecision {
            id: id.to_owned(),
            state: RecoveryDecisionState::Abandoned,
            abandoned_journal_stamp: record.journal_stamp,
            unknown_scope_acknowledged: !record.entry.scope_known,
        },
    )
}

/// Callers serialize recovery with writes and check that the game is stopped.
pub(crate) fn recover_all(
    transaction_root: &Path,
    roots: &TransactionRoots,
) -> Result<RecoveryReport, String> {
    paths::validate_roots(transaction_root, roots)?;
    let mut report = RecoveryReport::default();
    for directory in records(transaction_root)? {
        let Some(record) = inspect(directory, Some(roots)) else {
            continue;
        };
        if record.entry.archived {
            continue;
        }
        match record.entry.state {
            RecoveryDecisionState::Automatic => recover(record, &mut report),
            RecoveryDecisionState::Paused => report.deferred.push(record.entry.id),
            RecoveryDecisionState::Abandoned => report.blocked.push(record.entry.id),
        }
    }
    Ok(report)
}

pub(crate) fn retry_recovery(
    transaction_root: &Path,
    roots: &TransactionRoots,
    id: &str,
    revision: &str,
) -> Result<RecoveryReport, String> {
    let record = reviewed_record(transaction_root, id, revision, Some(roots))?;
    if record.entry.state == RecoveryDecisionState::Abandoned
        || record
            .journal
            .as_ref()
            .is_some_and(|journal| journal.roots != *roots)
    {
        return Err(CHANGED.into());
    }
    let mut report = RecoveryReport::default();
    recover(record, &mut report);
    Ok(report)
}

fn recover(record: RecoveryRecord, report: &mut RecoveryReport) {
    let RecoveryRecord {
        directory,
        entry: RecoveryEntry { id, .. },
        journal,
        validation_error,
        ..
    } = record;
    let _scope = diagnostics::enter(&id);
    let result = match (journal, validation_error) {
        (Some(journal), None) => {
            let mut pending = PreparedTransaction {
                directory: directory.clone(),
                journal,
            };
            let cleanup = matches!(
                pending.journal.state,
                JournalState::Committed | JournalState::Cleaned
            );
            let result = if cleanup {
                pending.cleanup()
            } else {
                pending.rollback()
            };
            result
                .map(|()| cleanup)
                .inspect_err(|error| pending.record_recovery_error(error))
        }
        (_, error) => Err(error.unwrap_or_else(|| paths::INVALID.into())),
    };
    match result {
        Ok(true) => report.cleaned.push(id.clone()),
        Ok(false) => report.recovered.push(id.clone()),
        Err(error) => {
            log::error!(
                "批次 {:?} 恢复被阻止，保留恢复材料：记录={:?}，原因={:?}",
                crate::file_check::log_field(&id),
                diagnostics::path(&directory.join(durable::JOURNAL_FILE)),
                crate::locale_tag::diagnostic_tag(&error)
            );
            report.blocked.push(id.clone());
        }
    }
}
