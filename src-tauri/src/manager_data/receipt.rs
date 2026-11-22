use super::notice::diagnostic_os_code;
use super::storage::{ResetRecord, reject_link_if_present};
use crate::file_check::log_path;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::Path;
use uuid::Uuid;

const FILE_NAME: &str = ".OC2ModManager-helper-diagnostic.json";
const MAX_BYTES: u64 = 512;

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum Stage {
    LocateDataRoot,
    ValidateDataRoot,
    SignalReadiness,
    WaitForRequest,
    OpenSessionLock,
    ReadCommittedRequest,
    WaitForSessionLock,
    CheckLegacyController,
    WaitForOwner,
    PersistOwnerWaitFailure,
    FinishRequest,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    token: Uuid,
    time: DateTime<Utc>,
    stage: Stage,
    os_code: Option<i32>,
}

pub(super) fn record_failure(control: &Path, token: Uuid, stage: Stage, error: &str) {
    let receipt = Receipt {
        token,
        time: Utc::now(),
        stage,
        os_code: diagnostic_os_code(error),
    };
    if write_receipt(control, &receipt).is_err() {
        eprintln!("[data] 无法保存数据辅助进程失败收据：请求={token}，步骤={stage:?}");
    }
}

fn write_receipt(control: &Path, receipt: &Receipt) -> Result<(), String> {
    reject_link_if_present(control)?;
    let path = control.join(FILE_NAME);
    reject_link_if_present(&path)?;
    let bytes = serde_json::to_vec(receipt).map_err(super::failed)?;
    let mut temp = tempfile::NamedTempFile::new_in(control).map_err(super::failed)?;
    temp.write_all(&bytes).map_err(super::failed)?;
    temp.as_file().sync_all().map_err(super::failed)?;
    reject_link_if_present(control)?;
    reject_link_if_present(&path)?;
    crate::mod_changes::transaction::durable_rename(&temp.into_temp_path(), &path, true)
}

pub(super) fn diagnostics(control: &Path, record: Option<&ResetRecord>) -> Option<String> {
    let path = control.join(FILE_NAME);
    match read_receipt(control, &path) {
        Ok(Some(receipt))
            if record.is_some_and(|record| record.token == receipt.token && !record.pending()) =>
        {
            None
        }
        Ok(Some(receipt)) => Some(format!(
            "[data] 历史辅助进程失败（收据生成时未记录业务终态）：请求={}，时间={}，步骤={:?}，OS码={:?}，收据={:?}",
            receipt.token,
            receipt.time.to_rfc3339(),
            receipt.stage,
            receipt.os_code,
            log_path(&path),
        )),
        Ok(None) => None,
        Err(error) => Some(format!(
            "[data] 无法读取数据辅助进程失败收据：收据={:?}，OS码={:?}",
            log_path(&path),
            diagnostic_os_code(&error),
        )),
    }
}

fn read_receipt(control: &Path, path: &Path) -> Result<Option<Receipt>, String> {
    reject_link_if_present(control)?;
    reject_link_if_present(path)?;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(super::failed(error)),
    };
    if !metadata.is_file() || metadata.len() > MAX_BYTES {
        return Err(super::UNSAFE.into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use winapi::um::winbase::FILE_FLAG_OPEN_REPARSE_POINT;
        options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(0x0100);
    }
    let input = options.open(path).map_err(super::failed)?;
    if !input.metadata().map_err(super::failed)?.is_file()
        || !crate::mod_changes::transaction::has_single_link(&input)
    {
        return Err(super::UNSAFE.into());
    }
    reject_link_if_present(path)?;
    let mut bytes = Vec::new();
    input
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(super::failed)?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(super::UNSAFE.into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(super::failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager_data::storage::{DataAction, ResetPaths, ResetPhase};

    #[test]
    fn helper_failure_receipt_is_bounded_and_does_not_touch_the_journal() {
        let root = tempfile::tempdir().unwrap();
        let paths = ResetPaths::new(&root.path().join("OC2ModManager")).unwrap();
        let token = Uuid::new_v4();
        assert!(diagnostics(&paths.parent, None).is_none());
        let error = super::super::failed("fixture-secret (os error 5)");
        record_failure(&paths.parent, token, Stage::WaitForSessionLock, &error);
        let message = diagnostics(&paths.parent, None).unwrap();
        assert!(message.contains(&token.to_string()), "{message}");
        assert!(message.contains("WaitForSessionLock"), "{message}");
        assert!(message.contains("OS码=Some(5)"), "{message}");
        assert!(!paths.journal().exists());

        let path = paths.parent.join(FILE_NAME);
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("fixture-secret"), "{text}");
        let next_token = Uuid::new_v4();
        record_failure(
            &paths.parent,
            next_token,
            Stage::FinishRequest,
            "fixture-secret",
        );
        let message = diagnostics(&paths.parent, None).unwrap();
        assert!(!message.contains(&token.to_string()), "{message}");
        assert!(message.contains(&next_token.to_string()), "{message}");
        let record = ResetRecord {
            token: next_token,
            owner_pid: 0,
            owner_started: 0,
            phase: ResetPhase::Completed,
            error: None,
            reported: false,
            locale: None,
            failed_from: None,
            action: DataAction::Clear,
            fingerprint: None,
            data_root: None,
        };
        assert!(diagnostics(&paths.parent, Some(&record)).is_none());
        assert!(path.exists());
        fs::write(&path, "fixture-secret".repeat(MAX_BYTES as usize)).unwrap();
        let message = diagnostics(&paths.parent, None).unwrap();
        assert!(message.contains("无法读取"), "{message}");
        assert!(!message.contains("fixture-secret"), "{message}");
    }
}
