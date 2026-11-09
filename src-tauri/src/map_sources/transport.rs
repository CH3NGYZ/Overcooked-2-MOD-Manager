use super::context::MapSourceContext;
use super::document::MapSourceDocument;
use super::document::parse_document;
use crate::config::ConfigError;
use crate::releases::RepositoryRequest;
use crate::repository_sources::SourceLocation;

/// 读取并解析一个地图源文档（内置资源或远程地址）。
///
/// 下载部分直接复用 `repository_sources` 的实现：超时（30 秒）、禁缓存头、
/// 大小上限、重定向后的相对地址解析、错误分类 —— 这些逐项都踩过坑，复制一份
/// 只会得到两套会各自漂移的规则。
///
/// 唯一的加成是「文档放在 Release 资产里」这一形态：先按资产地址反查仓库与
/// tag，从 Release API 拿到 size 与 sha256，再带着这两项去竞速下载并校验。
/// 拿不到校验信息时**退回普通下载**：校验是增强，不该变成新的失败点。
pub(crate) async fn load_document(
    location: &SourceLocation,
    ctx: &MapSourceContext<'_>,
    inherited: &crate::models::NetworkPlanSelection,
) -> Result<MapSourceDocument, ConfigError> {
    match verified_document(location, ctx).await {
        Some(verified) => {
            crate::repository_sources::load_verified_text_document(
                location,
                &verified,
                |text, origin| parse_document(text, origin, inherited),
            )
            .await
        }
        None => {
            crate::repository_sources::load_text_document(location, |text, origin| {
                parse_document(text, origin, inherited)
            })
            .await
        }
    }
}

/// 文档地址若是 Release 资产，就去 API 取它的 size/sha256 与下载候选。
///
/// 返回 `None` 表示「这份文档不需要/拿不到校验信息」，调用方按老路子读。
/// 这里**不报错**：Release API 挂了不该让整个地图源树加载失败，作者写的
/// 地址本身仍然可用 —— 只是少了摘要核对。
async fn verified_document(
    location: &SourceLocation,
    ctx: &MapSourceContext<'_>,
) -> Option<crate::repository_sources::VerifiedDocument> {
    let SourceLocation::Remote(url) = location else {
        return None;
    };
    let reference = crate::releases::release_asset_reference(url)?;
    let probe = RepositoryRequest {
        id: "release-asset-document".into(),
        provider: reference.provider.clone(),
        username: reference.username.clone(),
        repository: reference.repository.clone(),
        // Preserve the original default-plan selection for source documents.
        kind: crate::models::RepositoryKind::Mod,
        network_plan: Default::default(),
        source_id: None,
    };
    let key = crate::releases::repository_cache_key(
        reference.provider.clone(),
        &reference.username,
        &reference.repository,
    );
    let token = ctx.token(&reference.provider).map(str::to_string);
    let official_api_only = ctx.config.auth.official_api_only;
    let api_proxies = ctx.api_proxies(&reference.provider).to_vec();
    let releases = match ctx
        .cache
        .list(key, ctx.pass_id, ctx.force, || async move {
            crate::releases::fetch_releases(
                ctx.client,
                &probe,
                token.as_deref(),
                official_api_only,
                &api_proxies,
            )
            .await
            .map_err(|error| error.to_string())
        })
        .await
    {
        Ok(releases) => releases,
        Err(error) => {
            log::warn!(
                "文档校验信息获取失败，退回普通下载：地址={}，原因={}",
                crate::repository_sources::safe_log_address(url.as_str()),
                error
            );
            return None;
        }
    };
    let Some(release) = releases
        .iter()
        .find(|release| release.tag_name.trim() == reference.tag)
    else {
        log::warn!(
            "文档所在 Release 不在仓库列表里，退回普通下载：地址={}，tag={}",
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
            "文档资产不在 Release 里，退回普通下载：地址={}，资产={}",
            crate::repository_sources::safe_log_address(url.as_str()),
            reference.asset
        );
        return None;
    };
    /*
     * 候选走与普通资产下载同一条路由（直连 + 代理竞速），因为这就是一次资产
     * 下载：只是产物是文本而不是 ZIP。
     */
    let candidates = match crate::download::planning::plan_remote_download_candidates(
        ctx.config,
        &reference.provider,
        crate::download::planning::RemoteUrlKind::ReleaseAsset,
        url.as_str(),
        crate::config::current_user_region(),
    ) {
        Ok(candidates) => candidates,
        Err(error) => {
            log::warn!(
                "文档下载候选规划失败，退回普通下载：地址={}，原因={}",
                crate::repository_sources::safe_log_address(url.as_str()),
                error
            );
            return None;
        }
    };
    log::info!(
        "文档带校验信息：地址={}，字节={}，摘要={}",
        crate::repository_sources::safe_log_address(url.as_str()),
        asset.size,
        if asset.digest.is_some() { "有" } else { "无" }
    );
    Some(crate::repository_sources::VerifiedDocument {
        // API 没给大小时不校验：`0` 会被当成「预期 0 字节」，反而拒掉正常文件
        expected_size: (asset.size > 0).then_some(asset.size),
        digest: asset.digest.clone(),
        candidates,
    })
}
