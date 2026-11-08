use super::error::ReleaseError;
use super::version::is_prerelease_version;
use super::version::is_strict_semver;
use super::version::parse_version;
use crate::models::Channel;
use crate::models::Provider;
use crate::models::ReleaseAsset;
use crate::models::ReleaseInfo;
use serde_json::Value;

pub fn filter_channel(mut releases: Vec<ReleaseInfo>, channel: &Channel) -> Vec<ReleaseInfo> {
    let before = releases.len();
    releases.retain(|release| {
        !release.draft
            && match channel {
                Channel::Stable => !release.prerelease,
                Channel::Beta => release.prerelease,
                Channel::All => true,
            }
    });
    releases.sort_by(|a, b| {
        match (
            a.version.as_deref().and_then(parse_version),
            b.version.as_deref().and_then(parse_version),
        ) {
            (Some(left), Some(right)) => right.cmp(&left),
            _ => b.published_at.cmp(&a.published_at),
        }
    });
    log::debug!(
        "Release 渠道筛选完成: channel={:?} before={} after={}",
        channel,
        before,
        releases.len()
    );
    releases
}

pub fn parse_release_json(
    value: &Value,
    provider: &Provider,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let owned;
    let array = if let Some(array) = value.as_array() {
        array
    } else if value.is_object() {
        owned = vec![value.clone()];
        &owned
    } else {
        log::warn!(
            "Release API 响应结构无效: provider={:?} expected=array_or_object",
            provider
        );
        return Err(ReleaseError::Json("source.releaseResponseInvalid".into()));
    };
    log::debug!(
        "开始解析 Release API 响应: provider={:?} item_count={}",
        provider,
        array.len()
    );
    let mut releases = Vec::new();
    let mut skipped_without_tag = 0usize;
    for item in array {
        let tag = item
            .get("tag_name")
            .or_else(|| item.get("tagName"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if tag.is_empty() {
            skipped_without_tag += 1;
            continue;
        }
        /*
         * 发布契约检查：只记日志，绝不拒绝。
         *
         * 解析器继续宽容处理历史 tag（四段序号、`v0.10` 两段号），
         * 因为已发布的 tag 改不了，拒绝只会让老用户看到「版本未知」。
         * 但新发布应当符合 semver，不合规要能被发现 —— 在这里用 debug
         * 级记录仓库与 tag，排查「某个仓库的更新总是不对」时看得到根因。
         */
        if !is_strict_semver(&tag) {
            log::debug!(
                "Release tag 不符合严格 semver（已按宽容规则解析）: provider={:?} tag={}",
                provider,
                tag
            );
        }
        let assets = item
            .get("assets")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|asset| {
                if *provider == Provider::Gitcode && crate::providers::is_generated_asset(&asset) {
                    return None;
                }
                let name = asset.get("name").and_then(Value::as_str)?.to_string();
                let download_url = [
                    "browser_download_url",
                    "browserDownloadUrl",
                    "download_url",
                    "downloadUrl",
                    "url",
                ]
                .iter()
                .filter_map(|key| asset.get(*key).and_then(Value::as_str))
                .find(|url| !url.trim().is_empty())?
                .to_string();
                let digest = asset
                    .get("digest")
                    .and_then(Value::as_str)
                    .map(str::to_string)
                    .or_else(|| {
                        asset
                            .get("sha256")
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    });
                Some(ReleaseAsset {
                    name,
                    size: asset
                        .get("size")
                        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
                        .unwrap_or(0),
                    download_count: asset
                        .get("download_count")
                        .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok())),
                    download_url,
                    digest,
                })
            })
            .collect();
        /*
         * 预发布 = API 标记 或 版本号本身是预发布（四段 beta 序号 / `-` 后缀）。
         *
         * 必须在这里算好、随 ReleaseInfo 一起下发：前端只能拿到
         * `version`（已被 parse_version 截成三段，第 4 段丢失）和
         * `tag_name` 两串文本，让各处自己反推必然出现不一致 ——
         * 实际发生过：卡片读 tag_name 判成预发布，安装确认弹窗读
         * version 判成正式版，同一个版本两种结论。
         *
         * 不再用「tag 含 beta/alpha 字样」兜底：判定统一走
         * is_prerelease_version，与比较、渠道筛选共用同一套 semver 规则。
         */
        let prerelease = item
            .get("prerelease")
            .and_then(Value::as_bool)
            .unwrap_or(false)
            || is_prerelease_version(&tag);
        let date = item
            .get("published_at")
            .or_else(|| item.get("created_at"))
            .and_then(Value::as_str)
            .map(str::to_string);
        releases.push(ReleaseInfo {
            release_id: item
                .get("id")
                .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok())),
            version: parse_version(&tag).map(|value| value.to_string()),
            tag_name: tag,
            name: item
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            body: item
                .get("body")
                .or_else(|| item.get("description"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            published_at: date,
            prerelease,
            draft: item.get("draft").and_then(Value::as_bool).unwrap_or(false),
            assets,
        });
    }
    log::debug!(
        "Release API 响应解析完成: provider={:?} release_count={} skipped_without_tag={}",
        provider,
        releases.len(),
        skipped_without_tag
    );
    Ok(releases)
}

#[cfg(test)]
mod tests;
