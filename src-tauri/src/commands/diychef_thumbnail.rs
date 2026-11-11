#[tauri::command]
pub async fn diychef_thumbnail_cache(
    key: String,
    png: Option<Vec<u8>>,
    partial: Option<bool>,
) -> Result<tauri::ipc::Response, String> {
    let epoch = crate::diychef::preview::cache_epoch();
    tauri::async_runtime::spawn_blocking(move || {
        crate::diychef::thumbnail_cache::access(
            &key,
            png.as_deref(),
            partial.unwrap_or(false),
            epoch,
        )
        .map(tauri::ipc::Response::new)
    })
    .await
    .map_err(|_| "diychef.previewUnavailable".to_string())?
}

#[tauri::command]
pub async fn get_diychef_preview_fingerprint(
    state: tauri::State<'_, crate::app_state::AppState>,
    session_id: String,
    chef_id: String,
    hat_id: String,
) -> Result<String, String> {
    let epoch = crate::diychef::preview::cache_epoch();
    crate::diychef::preview::require_epoch(epoch)?;
    let state = state.inner().clone();
    let permit = crate::diychef::preview::fingerprint_permit().await?;
    crate::diychef::preview::require_epoch(epoch)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let fingerprint =
            crate::diychef::preview::fingerprint(&state, &session_id, &chef_id, &hat_id)?;
        crate::diychef::preview::require_epoch(epoch)?;
        Ok(fingerprint)
    })
    .await
    .map_err(|_| "diychef.previewUnavailable".to_string())?
}
