use crate::models::OperationResult;

#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    log::debug!("读取剪贴板文本");
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<OperationResult, String> {
    log::debug!("复制文本到剪贴板：长度={}", text.len());
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "common.action.copied".into(),
    })
}
