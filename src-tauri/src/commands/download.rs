use crate::archive;
use crate::download::control::{self, RaceAction};
use crate::models::OperationResult;

#[tauri::command]
pub fn cancel_download() -> Result<OperationResult, String> {
    log::info!("收到取消下载请求");
    if !archive::cancel_download() {
        return Err("update.inProgress".into());
    }
    Ok(OperationResult {
        ok: true,
        message: "download.cancelling".into(),
    })
}

#[tauri::command]
pub fn control_download_race(
    id: String,
    action: String,
    source: Option<usize>,
) -> Result<(), String> {
    let action = match (action.as_str(), source) {
        ("lock", Some(source)) => RaceAction::Lock(source),
        ("retry", None) => RaceAction::Retry,
        ("requeue", Some(source)) if !archive::is_download_cancelled() => {
            RaceAction::Requeue(source)
        }
        ("unlock", None) => RaceAction::Unlock,
        _ => return Err("download.raceUnavailable".into()),
    };
    control::request(&id, action)
        .then_some(())
        .ok_or_else(|| "download.raceUnavailable".into())
}
