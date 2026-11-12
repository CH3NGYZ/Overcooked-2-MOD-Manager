use super::http::api_client;
use super::http::authorized;
use super::http::ensure_success;
use super::http::get_json;
use super::http::request_error;
use super::session::publish_token;
use super::validation::encode_component;
use super::validation::validate_coordinates;
use super::validation::validate_new_tag;
use super::validation::validate_tag;
use crate::api_scheduler::ApiRequestExt;
use crate::app_state::AppState;
use crate::map_note::publish_metadata_from_note;
use crate::map_note::render_release_body;
use crate::map_note::replace_body_keeping_block;
use crate::map_note::strip_note_block;
use crate::models::MapPublishMetadata;
use crate::models::OperationResult;
use crate::models::PublishAsset;
use crate::models::PublishDeleteReleaseRequest;
use crate::models::PublishProvider;
use crate::models::PublishRelease;
use crate::models::PublishSaveRequest;
use serde::Deserialize;
use serde_json::json;
use tauri::State;

#[derive(Deserialize)]
pub(super) struct ApiAsset {
    #[serde(deserialize_with = "api_id")]
    pub(super) id: String,
    pub(super) name: String,
    #[serde(default)]
    pub(super) size: u64,
    #[serde(default, rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
pub(super) struct ApiRelease {
    #[serde(default, deserialize_with = "api_id")]
    id: String,
    pub(super) tag_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    created_at: String,
    #[serde(default)]
    published_at: Option<String>,
    #[serde(default)]
    pub(super) body: String,
    #[serde(default, deserialize_with = "release_assets")]
    pub(super) assets: Vec<ApiAsset>,
}

fn release_assets<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<ApiAsset>, D::Error> {
    let assets = Vec::<serde_json::Value>::deserialize(deserializer)?;
    assets
        .into_iter()
        // GitCode includes generated source archives without attachment IDs.
        // Ignore them before enforcing the ID contract for uploaded assets;
        // listing, saving and post-upload verification all use this decoder.
        .filter(|asset| asset.get("type").and_then(|kind| kind.as_str()) != Some("source"))
        .map(|asset| serde_json::from_value(asset).map_err(serde::de::Error::custom))
        .collect()
}

impl ApiAsset {
    pub(super) fn is_attachment(&self) -> bool {
        self.kind == "attach"
    }
}

pub(super) fn asset_from_api(asset: ApiAsset) -> PublishAsset {
    PublishAsset {
        id: asset.id,
        name: asset.name,
        size: asset.size,
    }
}

/// 把一个 Release 拆成「块外正文 + 解析出的元数据」。
///
/// 解析失败不当作整条列表失败：作者可能手写了半截块，界面至少要能把这一版
/// 列出来并让他改回去。失败时正文用 `strip_note_block` 的近似结果（去掉块），
/// 元数据留空。
fn release_from_api(release: ApiRelease, provider: PublishProvider) -> PublishRelease {
    let (body, metadata) = match crate::map_note::parse_release_note(&release.body) {
        Ok(Some(block)) => (
            block.body,
            Some(publish_metadata_from_note(&block.metadata)),
        ),
        Ok(None) => (release.body.clone(), None),
        Err(error) => {
            log::warn!(
                "Release 的 oc2-map 块解析失败，按无元数据展示：tag={}，原因={error}",
                release.tag_name
            );
            (strip_note_block(&release.body), None)
        }
    };
    PublishRelease {
        id: if provider == PublishProvider::Gitcode || release.id.is_empty() {
            release.tag_name.clone()
        } else {
            release.id
        },
        tag: release.tag_name,
        title: release.name,
        draft: release.draft,
        prerelease: release.prerelease,
        created_at: release.created_at,
        published_at: release.published_at,
        body,
        assets: release
            .assets
            .into_iter()
            .filter(|asset| provider == PublishProvider::Github || asset.is_attachment())
            .map(asset_from_api)
            .collect(),
        metadata,
    }
}

#[tauri::command]
pub async fn list_publish_releases(
    state: State<'_, AppState>,
    provider: PublishProvider,
    owner: String,
    repository: String,
) -> Result<Vec<PublishRelease>, String> {
    let token = publish_token(&state, provider)?;
    let api_endpoint = provider.api_endpoint();
    let (owner, repository) = validate_coordinates(&owner, &repository)?;
    let client = api_client()?;
    let releases = crate::releases::pagination::collect_release_pages(|page| {
        let url = format!(
            "{api_endpoint}/repos/{owner}/{repository}/releases?per_page={}&page={page}",
            crate::releases::pagination::RELEASE_PAGE_SIZE
        );
        let request = authorized(client.get(url), &token, provider);
        async move {
            let response = request
                .send_api(provider.api_provider())
                .await
                .map_err(|error| request_error(&error.to_string()))?;
            let releases: Vec<ApiRelease> = ensure_success(response, "list-releases")
                .await?
                .json()
                .await
                .map_err(|error| request_error(&error.to_string()))?;
            let count = releases.len();
            Ok::<_, String>((releases, count))
        }
    })
    .await?;
    let mut result = releases
        .into_iter()
        .map(|release| release_from_api(release, provider))
        .collect::<Vec<_>>();
    // tag 是地图身份、不是版本号，按它排没有语义，只提供一个稳定顺序；
    // 真正的版本号来自各 Release 的附件名。
    result.sort_by(|left, right| right.tag.cmp(&left.tag));
    log::info!(
        "发布页 Release 列表加载完成：仓库={owner}/{repository}，条目={}",
        result.len()
    );
    Ok(result)
}

/// 将表尾非空值同步到发布 DTO 的兼容标量，空值保留原标量。
/// 写块与消费端仍以 asset_versions 为准，不能据此回退非空表的未知版本。
fn apply_table_tail(metadata: &mut MapPublishMetadata) {
    let Some(last) = metadata.asset_versions.last() else {
        return;
    };
    let mapset_version = last.mapset_version.trim();
    if !mapset_version.is_empty() {
        metadata.latest_mapset_version = mapset_version.to_string();
    }
    let info_version = last.info_version.trim();
    if !info_version.is_empty() {
        metadata.latest_info_version = info_version.to_string();
    }
}

fn validate_publish_relations(metadata: &MapPublishMetadata) -> Result<(), String> {
    let invalid = || "mapPublish.invalidRelations".to_string();
    for table in [
        &metadata.requires,
        &metadata.compatible,
        &metadata.conflicts,
    ] {
        for (guid, range) in table {
            if guid.trim().is_empty() {
                return Err(invalid());
            }
            crate::config::check_version_spec(range).map_err(|_| invalid())?;
        }
    }
    for entry in &metadata.release_relations {
        let range =
            crate::version_range::VersionRange::parse(&entry.version).map_err(|_| invalid())?;
        if range.is_any() || entry.relations.is_empty() {
            return Err(invalid());
        }
        crate::config::check_version_spec(&entry.version).map_err(|_| invalid())?;
        for relation in &entry.relations {
            if relation.guid.trim().is_empty() {
                return Err(invalid());
            }
            crate::config::check_version_spec(&relation.version).map_err(|_| invalid())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn save_publish_release(
    state: State<'_, AppState>,
    request: PublishSaveRequest,
) -> Result<PublishRelease, String> {
    let provider = request.provider;
    let api_endpoint = provider.api_endpoint();
    let token = publish_token(&state, provider)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let tag = if request.release_id.is_none() {
        validate_new_tag(&request.tag)?
    } else {
        validate_tag(&request.tag)?
    };
    let client = api_client()?;
    // 请求按是否携带 metadata 分流，不按界面步骤推断：带资料时整体写块，
    // 只改日志时保留远端块，新建且无资料时直接写正文。
    let body = match request.metadata.clone() {
        Some(mut metadata) => {
            validate_publish_relations(&metadata)?;
            metadata.level_set_name = metadata.level_set_name.trim().to_string();
            metadata.level_set_name_zh = metadata.level_set_name_zh.trim().to_string();
            metadata.author = metadata.author.trim().to_string();
            if metadata.level_set_name.is_empty() {
                return Err("mapPublish.missingLevelSetName".to_string());
            }
            // 身份锁：已有 oc2-map 块时，英文名 / 中文名 / 作者不允许被改写，
            // 否则已安装的用户会因身份不符而检测错乱。
            if let Some(release_id) = request.release_id.as_deref() {
                let release_id = encode_component(release_id);
                let remote: ApiRelease = get_json(
                    &client,
                    &token,
                    provider,
                    &format!("{api_endpoint}/repos/{owner}/{repository}/releases/{release_id}"),
                    "read-release",
                )
                .await?;
                if let Ok(Some(note)) = crate::map_note::parse_release_note(&remote.body) {
                    let locked = &note.metadata;
                    if locked.level_set_name != metadata.level_set_name
                        || locked.level_set_name_zh != metadata.level_set_name_zh
                        || locked.author != metadata.author
                    {
                        return Err("mapPublish.zipIdentityMismatch".to_string());
                    }
                }
            }
            apply_table_tail(&mut metadata);
            render_release_body(&metadata, &request.body)
        }
        None => match request.release_id.as_deref() {
            Some(release_id) => {
                let release_id = encode_component(release_id);
                let remote: ApiRelease = get_json(
                    &client,
                    &token,
                    provider,
                    &format!("{api_endpoint}/repos/{owner}/{repository}/releases/{release_id}"),
                    "read-release",
                )
                .await?;
                replace_body_keeping_block(&remote.body, &request.body)
            }
            None => request.body.clone(),
        },
    };
    let payload = release_payload(provider, &tag, &body);
    log::info!(
        "发布页 Release 写入：平台={provider:?}，操作={}",
        if request.release_id.is_none() {
            "create"
        } else {
            "update"
        }
    );
    let response = match request.release_id.as_deref() {
        Some(id) => {
            authorized(
                client.patch(format!(
                    "{api_endpoint}/repos/{owner}/{repository}/releases/{}",
                    encode_component(id)
                )),
                &token,
                provider,
            )
            .json(&payload)
            .send_api(provider.api_provider())
            .await
        }
        None => {
            authorized(
                client.post(format!(
                    "{api_endpoint}/repos/{owner}/{repository}/releases"
                )),
                &token,
                provider,
            )
            .json(&payload)
            .send_api(provider.api_provider())
            .await
        }
    }
    .map_err(|error| request_error(&error.to_string()))?;
    let release: ApiRelease = ensure_success(response, "save-release")
        .await?
        .json()
        .await
        .map_err(|error| request_error(&error.to_string()))?;
    log::info!(
        "Release 已保存：仓库={owner}/{repository}，tag={}，新建={}",
        release.tag_name,
        request.release_id.is_none()
    );
    Ok(release_from_api(release, provider))
}

#[tauri::command]
pub async fn delete_publish_release(
    state: State<'_, AppState>,
    request: PublishDeleteReleaseRequest,
) -> Result<OperationResult, String> {
    if request.provider == PublishProvider::Gitcode {
        return Err("mapPublish.releaseDeleteUnavailable".into());
    }
    let provider = request.provider;
    let api_endpoint = provider.api_endpoint();
    let token = publish_token(&state, provider)?;
    let (owner, repository) = validate_coordinates(&request.owner, &request.repository)?;
    let client = api_client()?;
    let response = authorized(
        client.delete(format!(
            "{api_endpoint}/repos/{owner}/{repository}/releases/{}",
            encode_component(&request.release_id)
        )),
        &token,
        provider,
    )
    .send_api(provider.api_provider())
    .await
    .map_err(|error| request_error(&error.to_string()))?;
    ensure_success(response, "delete-release").await?;
    log::info!(
        "Release 已删除：仓库={owner}/{repository}，id={}",
        request.release_id
    );
    if request.delete_tag {
        let tag = validate_tag(&request.tag)?;
        // tag 允许中文等非 ASCII 字符，进 URL 路径前必须百分号编码。
        let tag_path = encode_component(&tag);
        let response = authorized(
            client.delete(format!(
                "{api_endpoint}/repos/{owner}/{repository}/git/refs/tags/{tag_path}"
            )),
            &token,
            provider,
        )
        .send_api(provider.api_provider())
        .await
        .map_err(|error| request_error(&error.to_string()))?;
        let status = response.status();
        if status.is_success() {
            log::info!("同名 tag 已删除：{tag}");
        } else {
            // Release 已删除；现有策略仅记录 tag 删除的非成功响应，不回滚或上抛。
            log::warn!(
                "同名 tag 删除失败（忽略，视为已删除）：{tag}，状态={}",
                status.as_u16()
            );
        }
    }
    Ok(OperationResult {
        ok: true,
        message: "message.mapPublish.releaseDeleted".to_string(),
    })
}

#[cfg(test)]
mod tests;

fn api_id<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::String(value) => Ok(value),
        serde_json::Value::Number(value) => Ok(value.to_string()),
        _ => Err(serde::de::Error::custom("invalid identifier")),
    }
}
fn release_payload(provider: PublishProvider, tag: &str, body: &str) -> serde_json::Value {
    match provider {
        PublishProvider::Github => {
            json!({"tag_name": tag, "name": tag, "body": body, "draft": false, "prerelease": false})
        }
        PublishProvider::Gitcode => {
            json!({"tag_name": tag, "name": tag, "body": body, "release_status": "latest"})
        }
    }
}
