use super::context::MapSourceContext;
use super::document::MapSourceDocument;
use super::document::parse_document;
use crate::config::ConfigError;
use crate::download::planning::{
    RemoteUrlKind, classify_remote_url, plan_source_document, provider_for_remote_url,
};
use crate::models::{NetworkPlanSelection, RepositoryKind};
use crate::releases::RepositoryRequest;
use crate::repository_sources::{SourceLocation, VerifiedDocument};

/// 读取地图源。官方索引来自当前源码包，仓库文档沿用继承的地图请求计划。
pub(crate) async fn load_document(
    location: &SourceLocation,
    ctx: &MapSourceContext<'_>,
    inherited: &NetworkPlanSelection,
) -> Result<MapSourceDocument, ConfigError> {
    let parse = |text: &str, origin: &SourceLocation| parse_document(text, origin, inherited);
    if crate::repository_sources::package_document_path(location).is_none()
        && let SourceLocation::Remote(url) = location
    {
        let reference = crate::releases::release_asset_reference(url);
        let provider = reference
            .as_ref()
            .map(|reference| reference.provider.clone())
            .or_else(|| provider_for_remote_url(url.as_str()));
        if let Some(provider) = provider {
            let is_raw = reference.is_none()
                && classify_remote_url(url.as_str()) != RemoteUrlKind::ReleaseAsset;
            let plan = plan_source_document(
                &provider,
                RepositoryKind::Map,
                inherited,
                &ctx.config.region,
                url.as_str(),
                is_raw,
            )
            .map_err(ConfigError::Read)?;
            let verified = match reference {
                Some(reference) => release_integrity(url, reference, ctx, inherited)
                    .await
                    .unwrap_or_default(),
                None => VerifiedDocument::default(),
            };
            return crate::repository_sources::load_verified_text_document(
                location,
                plan,
                &verified,
                &ctx.config.network,
                parse,
            )
            .await;
        }
    }
    crate::repository_sources::load_text_document(location, &ctx.config.network, parse).await
}

/// 根据同一资产条目的 API 元数据校验。API 不可用时仍使用同一文档下载计划。
async fn release_integrity(
    url: &url::Url,
    reference: crate::providers::ReleaseAssetReference,
    ctx: &MapSourceContext<'_>,
    inherited: &NetworkPlanSelection,
) -> Option<VerifiedDocument> {
    let probe = RepositoryRequest {
        id: "release-asset-document".into(),
        provider: reference.provider.clone(),
        username: reference.username.clone(),
        repository: reference.repository.clone(),
        kind: RepositoryKind::Map,
        network_plan: inherited.clone(),
        source_id: None,
    };
    let releases = match crate::releases::cached_releases(
        ctx.cache,
        ctx.client,
        &probe,
        &ctx.config.region,
        ctx.token(&probe.provider),
        ctx.pass_id,
        ctx.force,
    )
    .await
    {
        Ok(releases) => releases,
        Err(error) => {
            log::warn!(
                "文档校验信息获取失败，继续使用文档下载计划：地址={}，原因={}",
                crate::repository_sources::safe_log_address(url.as_str()),
                crate::locale_tag::diagnostic_tag(&error)
            );
            return None;
        }
    };
    let Some(release) = releases
        .iter()
        .find(|release| release.tag_name.trim() == reference.tag)
    else {
        log::warn!(
            "文档所在 Release 不在仓库列表里：地址={}，tag={}",
            crate::repository_sources::safe_log_address(url.as_str()),
            reference.tag
        );
        return None;
    };
    let Some(asset) = release
        .assets
        .iter()
        .find(|asset| asset.name == reference.asset)
    else {
        log::warn!(
            "文档资产不在 Release 里：地址={}，资产={}",
            crate::repository_sources::safe_log_address(url.as_str()),
            reference.asset
        );
        return None;
    };
    log::info!(
        "文档带校验信息：地址={}，字节={}，摘要={}",
        crate::repository_sources::safe_log_address(url.as_str()),
        asset.size,
        if asset.digest.is_some() { "有" } else { "无" }
    );
    Some(VerifiedDocument {
        // API 没给大小时不校验：`0` 会被当成「预期 0 字节」，反而拒掉正常文件
        expected_size: (asset.size > 0).then_some(asset.size),
        digest: asset.digest.clone(),
    })
}
