use super::http::API_ENDPOINT;
use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::get_json;
use super::http::request_error;
use super::http::upload_client;
use super::releases::ApiAsset;
use super::releases::ApiRelease;
use super::releases::asset_from_api;
use super::session::publish_token;
use super::validation::encode_component;
use super::validation::invalid_asset;
use super::validation::validate_asset_name;
use super::validation::validate_coordinates;
use super::zip::MAX_ASSET_BYTES;
use super::zip::zip_cache;
use crate::api_scheduler::ApiRequestExt;
use crate::app_state::AppState;
use crate::locale_tag::with_params;
use crate::map_note::VERSION_PLACEHOLDER;
use crate::models::OperationResult;
use crate::models::PublishAsset;
use crate::models::PublishUploadRequest;
use crate::models::PublishZipInspection;
use tauri::State;

const UPLOAD_ENDPOINT: &str = "https://uploads.github.com";

#[tauri::command]
pub async fn upload_publish_asset(
    state: State<'_, AppState>,
    request: PublishUploadRequest,
    on_progress: tauri::ipc::Channel<crate::models::PublishUploadProgress>,
) -> Result<PublishAsset, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let client = api_client()?;
    let remote: ApiRelease = get_json(
        &client,
        &token,
        &format!(
            "{API_ENDPOINT}/repos/{owner}/{repository}/releases/{}",
            request.release_id
        ),
        "read-release",
    )
    .await?;
    let path = std::path::PathBuf::from(request.path.trim());
    let inspect_path = path.clone();
    let inspection =
        tauri::async_runtime::spawn_blocking(move || zip_cache::inspect(&inspect_path))
            .await
            .map_err(|error| request_error(&error.to_string()))??;
    let note =
        crate::map_note::parse_release_note(&remote.body).map_err(|error| invalid_asset(&error))?;
    validate_zip_identity(note.as_ref().map(|note| &note.metadata), &inspection)?;
    let name = upload_asset_name(
        &remote.tag_name,
        note.as_ref()
            .and_then(|note| note.metadata.asset.as_deref()),
        &inspection.info_version,
    )?;
    let file_meta = std::fs::metadata(&path).map_err(|error| invalid_asset(&error.to_string()))?;
    if !file_meta.is_file() {
        return Err(invalid_asset("not a file"));
    }
    if file_meta.len() > MAX_ASSET_BYTES {
        return Err(with_params(
            "mapPublish.assetTooLarge",
            &[("limit", &(MAX_ASSET_BYTES / 1024 / 1024).to_string())],
        ));
    }
    let total = file_meta.len();
    let file = tokio::fs::File::open(&path)
        .await
        .map_err(|error| invalid_asset(&error.to_string()))?;
    let progress = on_progress.clone();
    let _ = progress.send(crate::models::PublishUploadProgress {
        sent: 0,
        total,
        phase: "uploading",
    });
    let stream = futures_util::stream::try_unfold((file, 0u64), move |(mut file, sent)| {
        let progress = progress.clone();
        async move {
            use tokio::io::AsyncReadExt;
            let mut buffer = vec![0u8; 64 * 1024];
            let count = file.read(&mut buffer).await?;
            if count == 0 {
                return Ok::<_, std::io::Error>(None);
            }
            buffer.truncate(count);
            let sent = sent + count as u64;
            let _ = progress.send(crate::models::PublishUploadProgress {
                sent,
                total,
                phase: if sent >= total {
                    "confirming"
                } else {
                    "uploading"
                },
            });
            Ok(Some((buffer, (file, sent))))
        }
    });
    let client = upload_client()?;
    let url = format!(
        "{UPLOAD_ENDPOINT}/repos/{owner}/{repository}/releases/{}/assets?name={}",
        request.release_id,
        encode_component(&name)
    );
    let response = authorized(client.post(url.as_str()), &token)
        .header("Content-Type", "application/zip")
        .header("Content-Length", total)
        .body(reqwest::Body::wrap_stream(stream))
        .send_api(crate::models::Provider::Github)
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    let asset: ApiAsset = ensure_success(response, "upload-asset")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    log::info!(
        "附件已上传：仓库={owner}/{repository}，Release={}，名称={}，大小={}",
        request.release_id,
        asset.name,
        asset.size
    );
    let _ = on_progress.send(crate::models::PublishUploadProgress {
        sent: total,
        total,
        phase: "done",
    });
    Ok(asset_from_api(asset))
}

fn validate_zip_identity(
    metadata: Option<&crate::map_note::MapNoteMetadata>,
    inspection: &PublishZipInspection,
) -> Result<(), String> {
    if inspection.set_count != 1 {
        return Err(invalid_asset("ZIP must contain exactly one level set"));
    }
    if let Some(metadata) = metadata {
        if metadata.level_set_name != inspection.level_set_name
            || metadata.level_set_name_zh != inspection.level_set_name_zh
            || metadata.author != inspection.author
        {
            return Err("mapPublish.zipIdentityMismatch".to_string());
        }
    }
    Ok(())
}

fn upload_asset_name(tag: &str, template: Option<&str>, version: &str) -> Result<String, String> {
    if version.is_empty() {
        return Err(invalid_asset("missing info version"));
    }
    let default_template = format!("{tag}_v{VERSION_PLACEHOLDER}.zip");
    let template = template
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&default_template);
    if !template.contains(VERSION_PLACEHOLDER) {
        return Err(invalid_asset("missing version placeholder"));
    }
    let name = validate_asset_name(&template.replace(VERSION_PLACEHOLDER, version))?;
    if !name.to_ascii_lowercase().ends_with(".zip") {
        return Err(invalid_asset("extension"));
    }
    Ok(name)
}

#[tauri::command]
pub async fn delete_publish_asset(
    state: State<'_, AppState>,
    owner: String,
    repository: String,
    asset_id: u64,
) -> Result<OperationResult, String> {
    let token = publish_token(&state)?;
    let (owner, repository) = validate_coordinates(&owner, &repository)?;
    let client = api_client()?;
    let response = authorized(
        client.delete(format!(
            "{API_ENDPOINT}/repos/{owner}/{repository}/releases/assets/{asset_id}"
        )),
        &token,
    )
    .send_api(crate::models::Provider::Github)
    .await
    .map_err(|error| request_error(&error.to_string()))?;
    ensure_success(response, "delete-asset").await?;
    log::info!("附件已删除：仓库={owner}/{repository}，id={asset_id}");
    Ok(OperationResult {
        ok: true,
        message: "message.mapPublish.assetDeleted".to_string(),
    })
}
