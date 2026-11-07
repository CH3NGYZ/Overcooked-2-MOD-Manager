use crate::archive;
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
