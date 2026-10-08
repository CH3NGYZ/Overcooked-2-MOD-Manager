use crate::config::{ConfigError, validate_repository, validate_repository_source_region};
use crate::models::{AppConfig, GitHubProxy, RepositoryExport};
use crate::releases;
use serde::{Deserialize, Deserializer};
use std::collections::HashSet;
use std::sync::OnceLock;
use url::Url;

pub const BUILTIN_INDEX: &str = "repositories.toml";
pub const DEFAULT_INDEX_ID: &str = "official-repository-index";
pub const DEFAULT_INDEX_GITEE_URL: &str =
    "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/repositories.toml";
pub const DEFAULT_INDEX_GITHUB_URL: &str = "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Releases/raw/refs/heads/main/repositories.toml";

pub fn default_index_url(region: &str) -> &'static str {
    if region == crate::config::REGION_CHINESE_MAINLAND {
        DEFAULT_INDEX_GITEE_URL
    } else {
        DEFAULT_INDEX_GITHUB_URL
    }
}
const MAX_SOURCE_BYTES: usize = 1024 * 1024;

include!(concat!(env!("OUT_DIR"), "/repository_sources.rs"));

pub(crate) fn safe_log_address(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return value.to_string();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

pub(crate) fn error_category(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::Directory(_) => "directory",
        ConfigError::Read(_) => "read",
        ConfigError::Parse(_) => "parse",
        ConfigError::Write(_) => "write",
        ConfigError::Invalid(_) => "invalid",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLocation {
    Remote(Url),
    Bundled(String),
}

impl SourceLocation {
    pub fn parse(reference: &str) -> Result<Self, ConfigError> {
        let reference_kind = match Url::parse(reference.trim()) {
            Ok(_) => "absolute",
            Err(url::ParseError::RelativeUrlWithoutBase) => "relative",
            Err(_) => "invalid",
        };
        log::trace!("解析仓库源位置：引用类型={reference_kind}");
        Self::resolve(reference, None)
    }

    fn resolve(reference: &str, parent: Option<&Self>) -> Result<Self, ConfigError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(ConfigError::Invalid("仓库源地址不能为空".into()));
        }
        match Url::parse(reference) {
            Ok(url) => Self::remote(url),
            Err(url::ParseError::RelativeUrlWithoutBase) => match parent {
                Some(Self::Remote(base)) => Self::resolve_remote_path(reference, base),
                _ => {
                    let base = match parent {
                        Some(Self::Bundled(path)) if is_file_relative_path(reference) => {
                            path.rsplit_once('/').map(|(dir, _)| dir)
                        }
                        _ => None,
                    };
                    Self::bundled_path(reference, base)
                }
            },
            Err(error) => Err(ConfigError::Invalid(format!("仓库源地址无效: {error}"))),
        }
    }

    fn resolve_remote_path(reference: &str, base: &Url) -> Result<Self, ConfigError> {
        let Some(root) = raw_repository_root(base) else {
            return Self::remote(
                base.join(reference)
                    .map_err(|error| ConfigError::Invalid(format!("仓库源地址无效: {error}")))?,
            );
        };
        if reference.starts_with("//") {
            return Self::remote(
                base.join(reference)
                    .map_err(|error| ConfigError::Invalid(format!("仓库源地址无效: {error}")))?,
            );
        }
        let resolved = if is_file_relative_path(reference) || reference.starts_with(['?', '#']) {
            base.join(reference)
        } else {
            root.join(reference.trim_start_matches('/'))
        }
        .map_err(|error| ConfigError::Invalid(format!("仓库源地址无效: {error}")))?;
        if resolved.origin() != root.origin() || !resolved.path().starts_with(root.path()) {
            return Err(ConfigError::Invalid(
                "仓库源路径不能超出当前仓库分支".into(),
            ));
        }
        Self::remote(resolved)
    }

    fn remote(url: Url) -> Result<Self, ConfigError> {
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ConfigError::Invalid(
                "仓库源必须使用 HTTP 或 HTTPS 地址".into(),
            ));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ConfigError::Invalid("仓库源地址不能包含认证信息".into()));
        }
        Ok(Self::Remote(url))
    }

    fn bundled_path(reference: &str, base: Option<&str>) -> Result<Self, ConfigError> {
        if reference.starts_with(['/', '\\']) || reference.contains(['\\', ':', '?', '#']) {
            return Err(ConfigError::Invalid("内置仓库源路径不安全".into()));
        }
        let mut components = base
            .map(|base| base.split('/').collect::<Vec<_>>())
            .unwrap_or_default();
        for component in reference.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(ConfigError::Invalid("内置仓库源路径不安全".into()));
                    }
                }
                value => components.push(value),
            }
        }
        if components.is_empty()
            || !components
                .last()
                .is_some_and(|name| name.ends_with(".toml"))
        {
            return Err(ConfigError::Invalid("内置仓库源必须是 TOML 文件".into()));
        }
        Ok(Self::Bundled(components.join("/")))
    }

    pub fn address(&self) -> String {
        match self {
            Self::Remote(url) => url.to_string(),
            Self::Bundled(path) => path.clone(),
        }
    }

    pub fn bundled_document(&self) -> Result<RepositorySourceDocument, ConfigError> {
        // Cache only the immutable embedded index. Real source refreshes use
        // load_document and always request the remote document again.
        static INDEX: OnceLock<Result<RepositorySourceDocument, ConfigError>> = OnceLock::new();
        if matches!(self, Self::Bundled(path) if path.eq_ignore_ascii_case(BUILTIN_INDEX)) {
            return INDEX.get_or_init(|| self.parse_bundled_document()).clone();
        }
        self.parse_bundled_document()
    }

    fn parse_bundled_document(&self) -> Result<RepositorySourceDocument, ConfigError> {
        log::debug!("读取内置仓库源文件：{}", safe_log_address(&self.address()));
        let text = self.bundled_text()?;
        let document = parse_document(text, self);
        match &document {
            Ok(document) => log::debug!(
                "内置仓库源解析完成：地址={}，仓库={}，子源={}，代理源={}",
                safe_log_address(&self.address()),
                document.repositories.len(),
                document.repository_sources.len(),
                document.github_proxy_sources.len()
            ),
            Err(error) => log::error!(
                "内置仓库源解析失败：地址={}，错误类别={}",
                safe_log_address(&self.address()),
                error_category(error)
            ),
        }
        document
    }

    pub fn bundled_text(&self) -> Result<&'static str, ConfigError> {
        let Self::Bundled(path) = self else {
            return Err(ConfigError::Invalid("仓库源不是内置文件".into()));
        };
        BUNDLED_SOURCES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(path))
            .map(|(_, text)| *text)
            .ok_or_else(|| ConfigError::Read(format!("内置仓库源文件不存在: {path}")))
    }
}

fn is_file_relative_path(reference: &str) -> bool {
    reference.starts_with("./") || reference.starts_with("../")
}

fn raw_repository_root(url: &Url) -> Option<Url> {
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let ref_start = match url.host_str()? {
        "github.com"
        | "www.github.com"
        | "gitee.com"
        | "www.gitee.com"
        | "raw.giteeusercontent.com"
            if segments.get(2) == Some(&"raw") =>
        {
            3
        }
        "raw.githubusercontent.com" | "raw.github.com" => 2,
        _ => return None,
    };
    if segments.first()?.is_empty() || segments.get(1)?.is_empty() {
        return None;
    }
    let ref_length = if segments.get(ref_start) == Some(&"refs")
        && matches!(segments.get(ref_start + 1), Some(&"heads" | &"tags"))
    {
        3
    } else {
        1
    };
    let root_length = ref_start + ref_length;
    if segments
        .get(ref_start..root_length)?
        .iter()
        .any(|segment| segment.is_empty())
    {
        return None;
    }
    let mut root = url.clone();
    {
        let mut path = root.path_segments_mut().ok()?;
        for _ in root_length..segments.len() {
            path.pop();
        }
        path.push("");
    }
    root.set_query(None);
    root.set_fragment(None);
    Some(root)
}

fn remote_document_location(
    requested: &Url,
    redirected: Url,
) -> Result<SourceLocation, ConfigError> {
    let redirected = SourceLocation::remote(redirected)?;
    // Raw endpoints can redirect to a CDN with a different path layout. Keep
    // the original repository ref as the base for its relative child sources.
    if raw_repository_root(requested).is_some() {
        Ok(SourceLocation::Remote(requested.clone()))
    } else {
        Ok(redirected)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositorySourceDeclaration {
    pub id: String,
    pub nickname: String,
    #[serde(default)]
    pub nickname_i18n: std::collections::BTreeMap<String, String>,
    pub url: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default, deserialize_with = "deserialize_builtin")]
    pub builtin: Option<bool>,
}

fn deserialize_builtin<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Boolean(bool),
        Text(String),
    }
    match Flag::deserialize(deserializer)? {
        Flag::Boolean(value) => Ok(Some(value)),
        Flag::Text(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Flag::Text(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Flag::Text(_) => Err(serde::de::Error::custom("builtin 必须是 true 或 false")),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositorySourceDocument {
    #[serde(default)]
    pub repositories: Vec<RepositoryExport>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySourceDeclaration>,
    #[serde(default)]
    pub github_proxy_sources: Vec<GitHubProxySourceDeclaration>,
    /// API proxy entries may be declared directly in the repository index so
    /// the maintained `repositories.toml` remains the source of truth.
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitHubProxySourceDeclaration {
    pub url: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub base_url: Option<String>,
}

/// Resolve the built-in proxy manifests for one user region.
///
/// A manifest without `region` (or with an empty value) is intentionally
/// shared by every region. Region-specific manifests remain restricted to
/// their matching channel.
pub fn github_proxy_sources(
    region: &str,
) -> Result<Vec<crate::github_proxies::ProxySource>, ConfigError> {
    log::debug!("读取 GitHub 代理来源配置：地区={}", region);
    let location = SourceLocation::parse(BUILTIN_INDEX)?;
    let document = location.bundled_document()?;
    let region = crate::config::normalize_repository_region(Some(region));
    let sources = document
        .github_proxy_sources
        .into_iter()
        .filter(|source| proxy_source_matches_region(source.region.as_deref(), &region))
        .map(|source| {
            let format = match source.format.as_deref().unwrap_or("toml") {
                "toml" => crate::github_proxies::ProxySourceFormat::Toml,
                "userscript" => crate::github_proxies::ProxySourceFormat::UserScript,
                other => {
                    return Err(ConfigError::Invalid(format!(
                        "不支持 GitHub 代理来源格式: {other}"
                    )));
                }
            };
            let resolved = match Url::parse(&source.url) {
                Ok(url) => SourceLocation::remote(url)?,
                Err(url::ParseError::RelativeUrlWithoutBase) => {
                    let base = source
                        .base_url
                        .as_deref()
                        .map(SourceLocation::parse)
                        .transpose()?
                        .unwrap_or_else(|| location.clone());
                    SourceLocation::resolve(&source.url, Some(&base))?
                }
                Err(error) => {
                    return Err(ConfigError::Invalid(format!("代理来源地址无效: {error}")));
                }
            };
            let (url, bundled) = match resolved {
                SourceLocation::Remote(url) => {
                    if url.scheme() != "https" || url.host_str().is_none() {
                        return Err(ConfigError::Invalid("代理来源地址必须是 HTTPS URL".into()));
                    }
                    (url.to_string(), false)
                }
                SourceLocation::Bundled(path) => (path, true),
            };
            Ok(crate::github_proxies::ProxySource {
                url,
                format,
                bundled,
            })
        })
        .collect::<Vec<_>>();
    if sources.is_empty() {
        log::warn!("未找到 GitHub 代理来源配置：地区={}", region);
        return Err(ConfigError::Read(format!(
            "没有为地区 {region} 配置 GitHub 代理清单地址"
        )));
    }
    let sources = sources.into_iter().collect::<Result<Vec<_>, _>>();
    if let Ok(sources) = &sources {
        log::info!(
            "GitHub 代理来源解析完成：地区={}，来源数={}",
            region,
            sources.len()
        );
    }
    sources
}

fn proxy_source_matches_region(configured_region: Option<&str>, region: &str) -> bool {
    configured_region
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none_or(|configured| {
            crate::config::normalize_repository_region(Some(configured)) == region
        })
}

pub(crate) fn source_address_key(url: &str, region: Option<&str>) -> (String, String) {
    (
        url.trim().to_ascii_lowercase(),
        crate::config::normalize_repository_region(region),
    )
}

pub(crate) fn parse_document(
    text: &str,
    location: &SourceLocation,
) -> Result<RepositorySourceDocument, ConfigError> {
    log::debug!(
        "开始解析仓库源文档：地址={}，字节数={}",
        safe_log_address(&location.address()),
        text.len()
    );
    if text.len() > MAX_SOURCE_BYTES {
        log::error!(
            "仓库源文档超过大小限制：地址={}，字节数={}",
            safe_log_address(&location.address()),
            text.len()
        );
        return Err(ConfigError::Invalid("仓库源文件超过 1 MiB".into()));
    }
    let mut document: RepositorySourceDocument = toml::from_str(text)
        .map_err(|error| ConfigError::Invalid(format!("仓库源 TOML 解析失败: {error}")))?;
    if document.repositories.is_empty()
        && document.repository_sources.is_empty()
        && document.github_proxy_sources.is_empty()
        && document.github_api_proxies.is_empty()
        && document.gitee_api_proxies.is_empty()
    {
        return Err(ConfigError::Invalid(
            "仓库源至少需要包含一个仓库或一个子仓库源".into(),
        ));
    }
    for repository in &mut document.repositories {
        repository.region = crate::config::normalize_repository_region_for_storage(
            repository.region.as_deref(),
            &format!("仓库 {}", repository.name),
        );
        validate_repository(repository)?;
    }
    for proxy_source in &mut document.github_proxy_sources {
        proxy_source.region = crate::config::normalize_repository_region_for_storage(
            proxy_source.region.as_deref(),
            "GitHub 代理来源",
        );
        validate_repository_source_region(proxy_source.region.as_deref())?;
        match proxy_source.format.as_deref().unwrap_or("toml") {
            "toml" | "userscript" => {}
            other => {
                return Err(ConfigError::Invalid(format!(
                    "不支持 GitHub 代理来源格式: {other}"
                )));
            }
        }
        let resolved = match Url::parse(&proxy_source.url) {
            Ok(url) => SourceLocation::remote(url)?,
            Err(url::ParseError::RelativeUrlWithoutBase) => {
                let base = proxy_source
                    .base_url
                    .as_deref()
                    .map(SourceLocation::parse)
                    .transpose()?
                    .unwrap_or_else(|| location.clone());
                SourceLocation::resolve(&proxy_source.url, Some(&base))?
            }
            Err(error) => return Err(ConfigError::Invalid(format!("代理来源地址无效: {error}"))),
        };
        if let SourceLocation::Remote(url) = resolved {
            if url.scheme() != "https" || url.host_str().is_none() {
                return Err(ConfigError::Invalid("代理来源地址必须是 HTTPS URL".into()));
            }
        }
    }
    let mut ids = std::collections::HashSet::new();
    let mut urls = std::collections::HashSet::new();
    for source in &mut document.repository_sources {
        source.id = source.id.trim().to_string();
        source.nickname = source.nickname.trim().to_string();
        if source.id.is_empty() || source.nickname.is_empty() {
            return Err(ConfigError::Invalid("仓库源 ID 和昵称不能为空".into()));
        }
        source.region = crate::config::normalize_repository_region_for_storage(
            source.region.as_deref(),
            &format!("仓库源 {}", source.id),
        );
        validate_repository_source_region(source.region.as_deref())?;
        source.url = SourceLocation::resolve(&source.url, Some(location))?.address();
        if !ids.insert(source.id.clone()) {
            return Err(ConfigError::Invalid(format!(
                "仓库源标识重复: {}",
                source.id
            )));
        }
    }
    document
        .repository_sources
        .retain(|source| urls.insert(source_address_key(&source.url, source.region.as_deref())));
    log::debug!(
        "仓库源文档解析完成：地址={}，仓库={}，子源={}，代理源={}，GitHub API代理={}，Gitee API代理={}",
        safe_log_address(&location.address()),
        document.repositories.len(),
        document.repository_sources.len(),
        document.github_proxy_sources.len(),
        document.github_api_proxies.len(),
        document.gitee_api_proxies.len()
    );
    Ok(document)
}

/// Discard persisted source content before requesting any fresh documents.
/// Keep the selected source headings and all unrelated configuration.
pub(crate) fn clear_source_cache(config: &mut AppConfig, source_ids: &[String]) -> (usize, usize) {
    let roots = source_ids.iter().cloned().collect::<HashSet<_>>();
    let mut descendants = roots.clone();
    loop {
        let before = descendants.len();
        for source in &config.repository_sources {
            if source
                .parent_id
                .as_ref()
                .is_some_and(|id| descendants.contains(id))
            {
                descendants.insert(source.id.clone());
            }
        }
        if descendants.len() == before {
            break;
        }
    }
    let repository_count = config.repositories.len();
    // Clear all fetched content, but keep the IDs referenced by install
    // manifests even if this refresh fails and the application restarts.
    for repo in &config.repositories {
        if let Some(source_id) = &repo.source_id
            && descendants.contains(source_id)
        {
            let key =
                crate::config::source_repository_key(source_id, &crate::config::repo_key(repo));
            config.source_repository_ids.insert(key, repo.id.clone());
        }
    }
    config.repositories.retain(|repo| {
        repo.source_id
            .as_ref()
            .is_none_or(|id| !descendants.contains(id))
    });
    let source_count = config.repository_sources.len();
    config
        .repository_sources
        .retain(|source| roots.contains(&source.id) || !descendants.contains(&source.id));
    (
        repository_count - config.repositories.len(),
        source_count - config.repository_sources.len(),
    )
}

pub async fn load_document(
    location: &SourceLocation,
    timeout_seconds: u64,
) -> Result<RepositorySourceDocument, ConfigError> {
    let SourceLocation::Remote(url) = location else {
        let safe_url = safe_log_address(&location.address());
        log::info!("开始加载内置仓库源：地址={}", safe_url);
        let document = location.bundled_document()?;
        log::info!(
            "内置仓库源加载完成：地址={}，仓库={}，子源={}，代理源={}，GitHub API代理={}，Gitee API代理={}",
            safe_url,
            document.repositories.len(),
            document.repository_sources.len(),
            document.github_proxy_sources.len(),
            document.github_api_proxies.len(),
            document.gitee_api_proxies.len()
        );
        return Ok(document);
    };
    let started_at = std::time::Instant::now();
    let safe_url = safe_log_address(url.as_str());
    log::info!(
        "开始加载远程仓库源：地址={}，超时={}秒",
        safe_url,
        timeout_seconds
    );
    let client = releases::build_client(timeout_seconds).map_err(|error| {
        log::error!(
            "创建仓库源 HTTP 客户端失败：地址={}，原因={}",
            safe_url,
            error
        );
        ConfigError::Read(error.to_string())
    })?;
    let mut response = client
        .get(url.clone())
        .header("User-Agent", releases::APP_USER_AGENT)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache")
        .send()
        .await
        .map_err(|error| {
            log::error!(
                "请求远程仓库源失败：地址={}，超时={}，连接错误={}",
                safe_url,
                error.is_timeout(),
                error.is_connect()
            );
            ConfigError::Read(error.to_string())
        })?;
    let status = response.status();
    let resolved_url = safe_log_address(response.url().as_str());
    log::debug!(
        "远程仓库源响应已收到：地址={}，最终地址={}，状态码={}，声明字节数={:?}",
        safe_url,
        resolved_url,
        status.as_u16(),
        response.content_length()
    );
    if let Err(error) = response.error_for_status_ref() {
        log::error!(
            "远程仓库源 HTTP 请求失败：地址={}，最终地址={}，状态码={}，耗时={}ms",
            safe_url,
            resolved_url,
            status.as_u16(),
            started_at.elapsed().as_millis()
        );
        return Err(ConfigError::Read(error.to_string()));
    }
    if !status.is_success() {
        log::warn!(
            "远程仓库源未返回新内容：地址={}，状态码={}",
            safe_url,
            status
        );
        return Err(ConfigError::Read(format!("HTTP {status}")));
    }
    let location = remote_document_location(url, response.url().clone()).map_err(|error| {
        log::error!(
            "解析远程仓库源最终地址失败：地址={}，状态码={}，错误类别={}",
            safe_url,
            status.as_u16(),
            error_category(&error)
        );
        error
    })?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        log::error!(
            "读取远程仓库源响应失败：地址={}，已读取字节数={}，超时={}，连接错误={}",
            safe_url,
            bytes.len(),
            error.is_timeout(),
            error.is_connect()
        );
        ConfigError::Read(error.to_string())
    })? {
        if bytes.len() + chunk.len() > MAX_SOURCE_BYTES {
            log::error!("远程仓库源文件超过大小限制：地址={}", safe_url);
            return Err(ConfigError::Invalid("仓库源文件超过 1 MiB".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    let byte_count = bytes.len();
    let text = String::from_utf8(bytes).map_err(|error| {
        log::error!(
            "远程仓库源不是有效 UTF-8：地址={}，字节数={}，原因={}",
            safe_url,
            byte_count,
            error
        );
        ConfigError::Parse(error.to_string())
    })?;
    let result = parse_document(&text, &location);
    match &result {
        Ok(document) => log::info!(
            "远程仓库源加载完成：地址={}，状态码={}，字节数={}，仓库={}，子源={}，耗时={}ms",
            safe_url,
            status.as_u16(),
            byte_count,
            document.repositories.len(),
            document.repository_sources.len(),
            started_at.elapsed().as_millis()
        ),
        Err(error) => log::error!(
            "远程仓库源解析失败：地址={}，状态码={}，字节数={}，耗时={}ms，错误类别={}",
            safe_url,
            status.as_u16(),
            byte_count,
            started_at.elapsed().as_millis(),
            error_category(error)
        ),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_cache_clearing_removes_complete_trees_and_preserves_settings() {
        let mut config: AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version": 1, "ui_zoom": 1.5, "onboarding_seen_steps": [],
            "game": {"path": "custom-game", "auto_detect": false},
            "network": {"timeout_seconds": 5, "download_threads": 1,
                "max_retry_count": 3, "check_updates_on_start": false},
            "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
            "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
            "repository_sources": [
                {"id": "leaf", "parent_id": "child", "nickname": "Leaf", "url": "leaf.toml"},
                {"id": "second", "nickname": "Second", "url": "second.toml"},
                {"id": "child", "parent_id": "first", "nickname": "Child", "url": "child.toml"},
                {"id": "first", "nickname": "First", "url": "first.toml", "builtin": true},
                {"id": "other", "nickname": "Other", "url": "other.toml"}
            ],
            "repositories": (["first", "second", "child", "leaf", "other", "manual"]
                .iter().map(|id| serde_json::json!({
                    "id": id, "name": id, "provider": "github", "username": "example",
                    "repository": "mod", "install_target": "BepInEx/plugins",
                    "enabled": false, "builtin": false,
                    "source_id": if *id == "manual" { None } else { Some(*id) },
                    "etag": "old-etag", "last_error": "old-error",
                    "cached_releases": [{"tag_name": "old", "name": "Old", "body": "",
                        "prerelease": false, "draft": false, "assets": []}]
                })).collect::<Vec<_>>())
        }))
        .unwrap();
        let original = serde_json::to_value(&config).unwrap();
        let source_ids = vec!["first".into(), "second".into()];
        assert_eq!(clear_source_cache(&mut config, &source_ids), (4, 2));
        assert_eq!(
            config
                .repositories
                .iter()
                .map(|repo| repo.id.as_str())
                .collect::<Vec<_>>(),
            ["other", "manual"]
        );
        assert_eq!(
            config
                .repository_sources
                .iter()
                .map(|source| source.id.as_str())
                .collect::<Vec<_>>(),
            ["second", "first", "other"]
        );
        let cleared = serde_json::to_value(&config).unwrap();
        for field in ["game", "network", "launch", "auth", "ui_zoom"] {
            assert_eq!(cleared[field], original[field]);
        }
        assert_eq!(cleared["repositories"][0], original["repositories"][4]);
        assert_eq!(cleared["repositories"][1], original["repositories"][5]);
        assert_eq!(clear_source_cache(&mut config, &source_ids), (0, 0));
        assert_eq!(clear_source_cache(&mut config, &[]), (0, 0));
    }

    fn serve_fresh_responses(
        path: &'static str,
        responses: Vec<(&'static str, String)>,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            for (status, body) in responses {
                let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(
                                std::time::Instant::now() < deadline,
                                "request did not arrive"
                            );
                            std::thread::sleep(std::time::Duration::from_millis(10));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                // Accepted sockets inherit the listener's mode on Windows.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
                let headers = request.to_ascii_lowercase();
                assert!(headers.contains("cache-control: no-cache, no-store, max-age=0\r\n"));
                assert!(headers.contains("pragma: no-cache\r\n"));
                assert!(!headers.contains("if-none-match:"));
                assert!(!headers.contains("if-modified-since:"));
                write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nCache-Control: public, max-age=86400\r\nETag: old-etag\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
        });
        (format!("http://{address}{path}"), server)
    }

    #[tokio::test]
    async fn remote_source_reloads_changed_content_and_rejects_not_modified() {
        let document = |name| {
            format!(
                "[[repositories]]\nname = \"{name}\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{ type = \"glob\", value = \"*.dll\" }}]\n"
            )
        };
        let (url, server) = serve_fresh_responses(
            "/source.toml",
            vec![
                ("200 OK", document("Old")),
                ("200 OK", document("New")),
                ("304 Not Modified", String::new()),
            ],
        );
        let location = SourceLocation::parse(&url).unwrap();
        assert_eq!(
            load_document(&location, 5).await.unwrap().repositories[0].name,
            "Old"
        );
        assert_eq!(
            load_document(&location, 5).await.unwrap().repositories[0].name,
            "New"
        );
        assert!(load_document(&location, 5).await.is_err());
        server.join().unwrap();
    }

    #[test]
    fn embedded_index_templates_are_returned_as_independent_copies() {
        let location = SourceLocation::parse(BUILTIN_INDEX).unwrap();
        let original = location.bundled_document().unwrap();
        let mut draft = location.bundled_document().unwrap();
        draft.repository_sources[0].nickname = "Edited draft".into();
        draft.repository_sources.clear();
        let reloaded = location.bundled_document().unwrap();
        assert_eq!(
            reloaded.repository_sources.len(),
            original.repository_sources.len()
        );
        assert_eq!(
            reloaded.repository_sources[0].nickname,
            original.repository_sources[0].nickname
        );
    }

    #[test]
    fn official_channels_resolve_children_in_the_selected_remote_repository() {
        let text = "[[repository_sources]]\nid = \"mods\"\nnickname = \"MODs\"\nurl = \"repositories-gua.toml\"\n";
        for (region, expected) in [
            (
                crate::config::REGION_CHINESE_MAINLAND,
                "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/repositories-gua.toml",
            ),
            (
                crate::config::REGION_INTERNATIONAL,
                "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Releases/raw/refs/heads/main/repositories-gua.toml",
            ),
        ] {
            let location = SourceLocation::parse(default_index_url(region)).unwrap();
            let document = parse_document(text, &location).unwrap();
            assert_eq!(document.repository_sources[0].url, expected);
        }
    }

    #[tokio::test]
    async fn release_api_reloads_changed_content_and_rejects_not_modified() {
        let document = |tag| {
            serde_json::json!([{
                "tag_name": tag, "name": tag, "body": "", "prerelease": false,
                "draft": false, "assets": []
            }])
            .to_string()
        };
        let (url, server) = serve_fresh_responses(
            "/releases",
            vec![
                ("200 OK", document("v1.0.0")),
                ("200 OK", document("v2.0.0")),
                ("304 Not Modified", String::new()),
            ],
        );
        let client = releases::build_client(5).unwrap();
        for tag in ["v1.0.0", "v2.0.0"] {
            assert_eq!(
                releases::fetch_release_api(&client, &url, &crate::models::Provider::Github, None)
                    .await
                    .unwrap()[0]
                    .tag_name,
                tag
            );
        }
        assert!(
            releases::fetch_release_api(&client, &url, &crate::models::Provider::Github, None)
                .await
                .is_err()
        );
        server.join().unwrap();
    }

    #[test]
    fn log_addresses_preserve_source_identity_without_credentials_or_queries() {
        assert_eq!(
            safe_log_address(
                "https://user:password@example.com/scripts/proxies.user.js?token=secret#private"
            ),
            "https://example.com/scripts/proxies.user.js"
        );
        assert_eq!(
            safe_log_address("repositories-proxies.toml"),
            "repositories-proxies.toml"
        );
    }

    #[test]
    fn proxy_sources_keep_region_order_format_and_relative_resolution() {
        let mainland = github_proxy_sources("chinese_mainland").unwrap();
        assert_eq!(mainland.len(), 2);
        assert_eq!(mainland[0].url, "repositories-proxies.toml");
        assert!(mainland[0].bundled);
        assert_eq!(
            mainland[1].format,
            crate::github_proxies::ProxySourceFormat::UserScript
        );
        let international = github_proxy_sources("international").unwrap();
        assert_eq!(international.len(), 2);
        assert_eq!(international[0].url, "repositories-proxies.toml");
        assert!(international[0].bundled);
        assert_eq!(
            international[1].format,
            crate::github_proxies::ProxySourceFormat::UserScript
        );
    }

    #[test]
    fn unscoped_proxy_sources_are_available_in_every_region() {
        assert!(proxy_source_matches_region(None, "chinese_mainland"));
        assert!(proxy_source_matches_region(Some(""), "international"));
        assert!(proxy_source_matches_region(
            Some("chinese_mainland"),
            "chinese_mainland"
        ));
        assert!(!proxy_source_matches_region(
            Some("chinese_mainland"),
            "international"
        ));
    }

    #[test]
    fn unscoped_proxy_source_is_preserved_when_parsing_a_manifest() {
        let location = SourceLocation::parse("repositories.toml").unwrap();
        let document = parse_document(
            r#"
[[github_proxy_sources]]
url = "repositories-proxies.toml"
"#,
            &location,
        )
        .unwrap();
        let source = &document.github_proxy_sources[0];
        assert!(source.region.is_none());
        assert!(proxy_source_matches_region(
            source.region.as_deref(),
            "chinese_mainland"
        ));
        assert!(proxy_source_matches_region(
            source.region.as_deref(),
            "international"
        ));
    }

    #[test]
    fn relative_child_uses_its_parent_file_directory() {
        let local = SourceLocation::parse("region/nested/source.toml").unwrap();
        assert_eq!(
            SourceLocation::resolve("../mods.toml", Some(&local))
                .unwrap()
                .address(),
            "region/mods.toml"
        );
        let remote =
            SourceLocation::parse("https://example.com/region/nested/source.toml").unwrap();
        assert_eq!(
            SourceLocation::resolve("../mods.toml", Some(&remote))
                .unwrap()
                .address(),
            "https://example.com/region/mods.toml"
        );
        assert_eq!(
            SourceLocation::resolve("mods.toml", Some(&remote))
                .unwrap()
                .address(),
            "https://example.com/region/nested/mods.toml"
        );
    }

    #[test]
    fn bundled_and_raw_plain_paths_use_their_repository_root() {
        let local = SourceLocation::parse("config/nested/index.toml").unwrap();
        assert_eq!(
            SourceLocation::resolve("mods/child.toml", Some(&local))
                .unwrap()
                .address(),
            "mods/child.toml"
        );
        assert_eq!(
            SourceLocation::resolve("./child.toml", Some(&local))
                .unwrap()
                .address(),
            "config/nested/child.toml"
        );
        for prefix in [
            "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/",
            "https://raw.giteeusercontent.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/",
            "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/",
            "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/master/",
            "https://raw.githubusercontent.com/kfyangc/Overcooked2-DIYLevelFastInit/refs/heads/master/",
            "https://raw.githubusercontent.com/kfyangc/Overcooked2-DIYLevelFastInit/master/",
            "https://github.com/owner/repo/raw/refs/tags/v1.0/",
            "https://github.com/owner/repo/raw/refs/heads/feature%2Fmods/",
        ] {
            let parent =
                SourceLocation::parse(&format!("{prefix}config/nested/index.toml?raw=true"))
                    .unwrap();
            assert_eq!(
                SourceLocation::resolve("mods/child.toml", Some(&parent))
                    .unwrap()
                    .address(),
                format!("{prefix}mods/child.toml")
            );
            assert_eq!(
                SourceLocation::resolve("./child.toml", Some(&parent))
                    .unwrap()
                    .address(),
                format!("{prefix}config/nested/child.toml")
            );
            assert_eq!(
                SourceLocation::resolve("../shared.toml", Some(&parent))
                    .unwrap()
                    .address(),
                format!("{prefix}config/shared.toml")
            );
            assert_eq!(
                SourceLocation::resolve("/mods/child.toml", Some(&parent))
                    .unwrap()
                    .address(),
                format!("{prefix}mods/child.toml")
            );
        }
    }

    #[test]
    fn raw_file_paths_cannot_escape_the_repository_ref() {
        let parent =
            SourceLocation::parse("https://gitee.com/owner/repo/raw/main/config/index.toml")
                .unwrap();
        for path in [
            "../../outside.toml",
            "%2e%2e/outside.toml",
            "\\\\other.example.com/outside.toml",
        ] {
            assert!(
                SourceLocation::resolve(path, Some(&parent)).is_err(),
                "{path}"
            );
        }
        assert!(SourceLocation::resolve("https://example.com/other.toml", Some(&parent)).is_ok());
    }

    #[test]
    fn absolute_child_source_switches_the_repository_for_its_descendants() {
        let gitee =
            SourceLocation::parse("https://gitee.com/owner/repo/raw/main/index.toml").unwrap();
        let parent_document = parse_document(r#"
[[repository_sources]]
id = "github"
nickname = "GitHub"
url = "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/config/index.toml"
"#, &gitee).unwrap();
        let github = SourceLocation::parse(&parent_document.repository_sources[0].url).unwrap();
        let child_document = parse_document(
            r#"
[[repository_sources]]
id = "mods"
nickname = "Mods"
url = "mods/child.toml"
"#,
            &github,
        )
        .unwrap();
        assert_eq!(
            child_document.repository_sources[0].url,
            "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/mods/child.toml"
        );
        let child = SourceLocation::parse(&child_document.repository_sources[0].url).unwrap();
        assert_eq!(
            SourceLocation::resolve("next.toml", Some(&child))
                .unwrap()
                .address(),
            "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/next.toml"
        );
    }

    #[test]
    fn raw_redirects_keep_repository_context_and_other_direct_links_use_the_destination() {
        let requested =
            Url::parse("https://github.com/owner/repo/raw/refs/heads/master/config/index.toml")
                .unwrap();
        for redirected in [
            "https://raw.githubusercontent.com/owner/repo/refs/heads/master/config/index.toml",
            "https://cdn.example.com/files/index.toml",
        ] {
            let location =
                remote_document_location(&requested, Url::parse(redirected).unwrap()).unwrap();
            assert_eq!(
                SourceLocation::resolve("mods/child.toml", Some(&location))
                    .unwrap()
                    .address(),
                "https://github.com/owner/repo/raw/refs/heads/master/mods/child.toml"
            );
        }
        let direct_link = Url::parse("https://example.com/download").unwrap();
        let location = remote_document_location(
            &direct_link,
            Url::parse("https://raw.githubusercontent.com/other/repo/main/config/index.toml")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            SourceLocation::resolve("mods/child.toml", Some(&location))
                .unwrap()
                .address(),
            "https://raw.githubusercontent.com/other/repo/main/mods/child.toml"
        );
    }

    #[test]
    fn absolute_url_overrides_both_local_and_remote_base() {
        for base in ["index.toml", "https://example.com/index.toml"] {
            let base = SourceLocation::parse(base).unwrap();
            assert_eq!(
                SourceLocation::resolve("https://other.example.com/mods.toml", Some(&base))
                    .unwrap()
                    .address(),
                "https://other.example.com/mods.toml"
            );
        }
    }

    #[test]
    fn unsafe_local_paths_and_non_http_urls_are_rejected() {
        for reference in [
            "../outside.toml",
            "/outside.toml",
            "C:/outside.toml",
            "file:///outside.toml",
            "ftp://example.com/source.toml",
        ] {
            assert!(SourceLocation::parse(reference).is_err(), "{reference}");
        }
        assert!(
            SourceLocation::parse("builtin-tutorial.toml")
                .unwrap()
                .bundled_document()
                .is_err()
        );
    }

    #[test]
    fn all_bundled_source_documents_use_the_shared_parser() {
        for (path, _) in BUNDLED_SOURCES {
            if path.eq_ignore_ascii_case("repositories-proxies.toml") {
                continue;
            }
            let location = SourceLocation::parse(path).unwrap();
            let document = location
                .bundled_document()
                .unwrap_or_else(|error| panic!("{path}: {error}"));
            for child in document.repository_sources {
                if let SourceLocation::Bundled(_) = SourceLocation::parse(&child.url).unwrap() {
                    assert!(
                        BUNDLED_SOURCES
                            .iter()
                            .any(|(path, _)| path.eq_ignore_ascii_case(&child.url)),
                        "{}",
                        child.url
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn builtin_index_loads_only_declared_sources_and_relative_repository_documents() {
        let location = SourceLocation::parse(BUILTIN_INDEX).unwrap();
        let declared_index: RepositorySourceDocument =
            toml::from_str(location.bundled_text().unwrap()).unwrap();
        let index = load_document(&location, 5).await.unwrap();
        assert!(index.repositories.is_empty());
        assert!(!index.repository_sources.is_empty());
        assert_eq!(
            index
                .repository_sources
                .iter()
                .map(|source| (&source.id, &source.nickname))
                .collect::<Vec<_>>(),
            declared_index
                .repository_sources
                .iter()
                .map(|source| (&source.id, &source.nickname))
                .collect::<Vec<_>>()
        );

        for (source, declared) in index
            .repository_sources
            .iter()
            .zip(&declared_index.repository_sources)
        {
            assert!(source.region.is_none(), "{}", source.id);
            let child_location = SourceLocation::resolve(&declared.url, Some(&location)).unwrap();
            assert!(matches!(&child_location, SourceLocation::Bundled(_)));
            assert_eq!(source.url, child_location.address());
            let declared_child: RepositorySourceDocument =
                toml::from_str(child_location.bundled_text().unwrap()).unwrap();
            let child = load_document(&child_location, 5)
                .await
                .unwrap_or_else(|error| panic!("{}: {error}", source.url));
            assert!(!child.repositories.is_empty(), "{}", source.url);
            assert_eq!(
                child
                    .repositories
                    .iter()
                    .map(|repository| (
                        &repository.name,
                        &repository.provider,
                        &repository.username,
                        &repository.repository
                    ))
                    .collect::<Vec<_>>(),
                declared_child
                    .repositories
                    .iter()
                    .map(|repository| (
                        &repository.name,
                        &repository.provider,
                        &repository.username,
                        &repository.repository
                    ))
                    .collect::<Vec<_>>(),
                "{}",
                source.url
            );
            assert_eq!(
                child
                    .repository_sources
                    .iter()
                    .map(|source| (&source.id, &source.nickname))
                    .collect::<Vec<_>>(),
                declared_child
                    .repository_sources
                    .iter()
                    .map(|source| (&source.id, &source.nickname))
                    .collect::<Vec<_>>(),
                "{}",
                source.url
            );
            for repository in child.repositories {
                assert!(matches!(
                    repository.region.as_deref(),
                    None | Some("chinese_mainland" | "international")
                ));
            }
        }
    }

    #[test]
    fn source_declarations_keep_order_and_resolve_relative_urls() {
        let text = r#"
[[repository_sources]]
id = "second"
nickname = "Second"
url = "second.toml"
builtin = "false"
[[repository_sources]]
id = "first"
nickname = "First"
url = "first.toml"
builtin = true
"#;
        let location = SourceLocation::parse("https://example.com/sources/index.toml").unwrap();
        let document = parse_document(text, &location).unwrap();
        assert_eq!(document.repository_sources[0].id, "second");
        assert_eq!(
            document.repository_sources[0].url,
            "https://example.com/sources/second.toml"
        );
        assert_eq!(document.repository_sources[0].builtin, Some(false));
        assert_eq!(document.repository_sources[1].id, "first");
        assert_eq!(document.repository_sources[1].builtin, Some(true));
        assert!(parse_document(&text.replace("\"false\"", "\"invalid\""), &location).is_err());
        assert!(parse_document("[session-id] Repository or file not found", &location).is_err());
    }

    #[test]
    fn source_nickname_translations_are_optional_and_survive_parsing() {
        let location = SourceLocation::parse("index.toml").unwrap();
        let legacy = r#"
[[repository_sources]]
id = "mods"
nickname = "Fallback name"
url = "mods.toml"
"#;
        let document = parse_document(legacy, &location).unwrap();
        assert!(document.repository_sources[0].nickname_i18n.is_empty());
        let translated = format!(
            "{legacy}\n[repository_sources.nickname_i18n]\nzh-CN = \"中文昵称\"\nzh-TW = \"繁體暱稱\"\nen = \"Translated name\"\nko = \"번역 이름\"\n"
        );
        let document = parse_document(&translated, &location).unwrap();
        let source = &document.repository_sources[0];
        assert_eq!(source.nickname, "Fallback name");
        assert_eq!(source.nickname_i18n.len(), 4);
        assert_eq!(source.nickname_i18n["en"], "Translated name");
        assert_eq!(source.nickname_i18n["ko"], "번역 이름");
        assert!(
            parse_document(
                &translated.replace("en = \"Translated name\"", "en = 1"),
                &location
            )
            .is_err()
        );
    }

    #[test]
    fn same_source_url_can_have_distinct_regional_names() {
        let text = r#"
[[repository_sources]]
id = "china"
nickname = "China"
url = "mods.toml"
region = "chinese_mainland"
[repository_sources.nickname_i18n]
en = "China Mods"
[[repository_sources]]
id = "international"
nickname = "International"
url = "mods.toml"
region = "international"
[repository_sources.nickname_i18n]
en = "International Mods"
[[repository_sources]]
id = "china-duplicate"
nickname = "Duplicate"
url = "mods.toml"
region = "mainland-china"
"#;
        let location = SourceLocation::parse("https://example.com/index.toml").unwrap();
        let document = parse_document(text, &location).unwrap();
        assert_eq!(document.repository_sources.len(), 2);
        assert_eq!(document.repository_sources[0].id, "china");
        assert_eq!(document.repository_sources[1].id, "international");
        assert_eq!(
            document.repository_sources[1].nickname_i18n["en"],
            "International Mods"
        );
    }

    #[tokio::test]
    async fn remote_source_resolves_children_from_its_redirect_destination() {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let mut count = 0;
            while count < 2 && std::time::Instant::now() < deadline {
                let (mut stream, _) = match listener.accept() {
                    Ok(connection) => connection,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(10));
                        continue;
                    }
                    Err(error) => panic!("{error}"),
                };
                stream
                    .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let response = if count == 0 {
                    assert!(request.starts_with("GET /start.toml "));
                    "HTTP/1.1 302 Found\r\nLocation: /nested/index.toml\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
                } else {
                    assert!(request.starts_with("GET /nested/index.toml "));
                    let body = "[[repository_sources]]\nid = \"mods\"\nnickname = \"Mods\"\nurl = \"../mods.toml\"\n";
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    )
                };
                stream.write_all(response.as_bytes()).unwrap();
                count += 1;
            }
            assert_eq!(count, 2);
        });
        let location = SourceLocation::parse(&format!("http://{address}/start.toml")).unwrap();
        let result = load_document(&location, 5).await;
        server.join().unwrap();
        assert_eq!(
            result.unwrap().repository_sources[0].url,
            format!("http://{address}/mods.toml")
        );
    }
}
