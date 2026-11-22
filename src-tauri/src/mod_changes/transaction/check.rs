use super::{
    paths,
    types::{Fingerprint, RootKind, TransactionPath, TransactionRoots},
};
use crate::file_check;
use std::path::Path;

pub(super) const TARGET: &str = "modChanges.verifyInstallTarget";
pub(super) const STAGED: &str = "modChanges.verifyStagedFile";
pub(super) const BACKUP: &str = "modChanges.verifyInstallBackup";
pub(super) const RESULT: &str = "modChanges.verifyInstallResult";
pub(super) const WRITE: &str = "modChanges.writeInstallFile";
pub(super) const WRITE_BACKUP: &str = "modChanges.writeInstallBackup";
const RECOVERY_PENDING: &str = "modChanges.installRecoveryPending";

pub(super) fn resolve(
    target: &TransactionPath,
    roots: &TransactionRoots,
    stage: &str,
) -> Result<std::path::PathBuf, String> {
    target.resolve(roots).map_err(|error| {
        let root = match target.root {
            RootKind::Game => &roots.game_root,
            RootKind::Data => &roots.data_root,
        };
        context(stage, &root.join(&target.relative), &error)
    })
}

pub(super) fn capture(path: &Path, stage: &str) -> Result<Fingerprint, String> {
    Fingerprint::capture(path).map_err(|error| context(stage, path, &error))
}

pub(super) fn context(stage: &str, path: &Path, error: &str) -> String {
    let reason = match error {
        paths::MODIFIED => file_check::CONTENT,
        paths::UNSAFE => file_check::UNSAFE,
        paths::CONFLICT => file_check::EXISTS,
        paths::IO_ERROR => {
            if matches!(stage, WRITE | WRITE_BACKUP) {
                file_check::WRITE
            } else {
                file_check::READ
            }
        }
        _ => return error.to_owned(),
    };
    file_check::report(stage, path, reason)
}

pub(super) fn compare(
    path: &Path,
    stage: &str,
    observed: &Fingerprint,
    expected: &Fingerprint,
    content_only: bool,
) -> Result<(), String> {
    let same_content = observed.same_content(expected);
    if observed == expected || content_only && same_content {
        return Ok(());
    }
    let reason = match (observed, expected) {
        (Fingerprint::Missing, _) => file_check::MISSING,
        (_, Fingerprint::Missing) => file_check::EXISTS,
        (Fingerprint::Directory { .. }, Fingerprint::File { .. })
        | (Fingerprint::File { .. }, Fingerprint::Directory { .. }) => file_check::TYPE,
        _ if same_content => file_check::PERMISSIONS,
        _ => file_check::CONTENT,
    };
    Err(file_check::report(stage, path, reason))
}

pub(super) fn expect(path: &Path, stage: &str, expected: &Fingerprint) -> Result<(), String> {
    compare(path, stage, &capture(path, stage)?, expected, false)
}

pub(super) fn writing(path: &Path, error: &str) -> String {
    context(WRITE, path, error)
}

pub(super) fn recovery_pending(reason: &str) -> String {
    crate::locale_tag::with_params(RECOVERY_PENDING, &[("reason", reason)])
}

pub(in crate::mod_changes) fn needs_recovery(error: &str) -> bool {
    error.split('?').next() == Some(RECOVERY_PENDING)
}
