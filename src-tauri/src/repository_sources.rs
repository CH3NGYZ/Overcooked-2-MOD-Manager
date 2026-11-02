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
    "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/repositories.toml";
pub const DEFAULT_INDEX_GITHUB_URL: &str = "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/repositories.toml";

pub fn default_index_url(region: &str) -> &'static str {
    if region == crate::config::REGION_CHINESE_MAINLAND {
        DEFAULT_INDEX_GITEE_URL
    } else {
        DEFAULT_INDEX_GITHUB_URL
    }
}

/// 读取仓库源文档（raw 索引与各级子源）的访问超时。
///
/// 固定 30 秒，**不跟随** `network.timeout_seconds`：
///   · 那个设置是给 Release API / 下载用的，用户为了「下载更快失败」
///     调到 5 秒时，不该把源文档也一起压到 5 秒 —— 国内访问 Gitee raw
///     首次握手常常就要好几秒，压到 5 秒会让整棵树随机失败；
///   · 源文档阶段是串行递归（索引 → 子源），一层的超时过短会让后面
///     所有子源根本没有机会加载。
///
/// 这与仓库说明（README）的 30 秒保持同一量级：两者都是「读一个不大的
/// 文本文件」，不是大文件下载。
pub const SOURCE_DOCUMENT_TIMEOUT: u64 = 30;

const MAX_SOURCE_BYTES: usize = 1024 * 1024;

include!(concat!(env!("OUT_DIR"), "/repository_sources.rs"));

/// 遍历内置仓库快照，返回其中所有的仓库配置（**不做地区过滤**）。
///
/// 用途：社区搜索结果必须回避全部内置/官方仓库，包括因当前地区
/// 被过滤掉、因而不在 `config.repositories` 里的那些。
/// 只读嵌入资源，不发起网络请求，也不修改任何配置。
pub(crate) fn bundled_repository_configs() -> Vec<RepositoryExport> {
    let mut repositories = Vec::new();
    let mut pending = match SourceLocation::parse(BUILTIN_INDEX) {
        Ok(location) => vec![location],
        Err(_) => return repositories,
    };
    let mut visited = HashSet::new();
    while let Some(location) = pending.pop() {
        // 只遍历内置资源：远程地址在这里没有网络上下文，直接跳过。
        if !matches!(location, SourceLocation::Bundled(_)) || !visited.insert(location.address()) {
            continue;
        }
        let Ok(document) = location.bundled_document() else {
            continue;
        };
        repositories.extend(document.repositories);
        for source in document.repository_sources {
            if let Ok(next) = SourceLocation::parse(&source.url) {
                pending.push(next);
            }
        }
    }
    repositories
}

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

    /// 解析相对当前文档的地址（子源、同仓库文件都走这里）。
    ///
    /// `pub(crate)`：地图源树与仓库源树共用同一套地址解析，包括重定向后的
    /// 相对基准、raw 仓库根拼接与内置资源相对路径。复制一份只会得到两套
    /// 会各自漂移的规则。
    pub(crate) fn resolve(reference: &str, parent: Option<&Self>) -> Result<Self, ConfigError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(ConfigError::Invalid("source.addressEmpty".into()));
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
            Err(error) => Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "source.addressInvalid",
                &[("reason", &error.to_string())],
            ))),
        }
    }

    fn resolve_remote_path(reference: &str, base: &Url) -> Result<Self, ConfigError> {
        /*
         * Release 资产里的文档不许声明相对子源。
         *
         * 资产地址没有分支结构，`raw_repository_root` 认不出来，走 `base.join`
         * 只会拼出一个不存在的路径；而且下载后真正的地址是 CDN 签名 URL，
         * 连作者也无法预测。能表达「我要哪个文档」的只有绝对地址。
         */
        if crate::releases::release_asset_reference(base).is_some() {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "source.relativeChildInReleaseAsset",
                &[("source", &safe_log_address(base.as_str()))],
            )));
        }
        let Some(root) = raw_repository_root(base) else {
            return Self::remote(base.join(reference).map_err(|error| {
                ConfigError::Invalid(crate::locale_tag::with_params(
                    "source.addressInvalid",
                    &[("reason", &error.to_string())],
                ))
            })?);
        };
        if reference.starts_with("//") {
            return Self::remote(base.join(reference).map_err(|error| {
                ConfigError::Invalid(crate::locale_tag::with_params(
                    "source.addressInvalid",
                    &[("reason", &error.to_string())],
                ))
            })?);
        }
        let resolved = if is_file_relative_path(reference) || reference.starts_with(['?', '#']) {
            base.join(reference)
        } else {
            root.join(reference.trim_start_matches('/'))
        }
        .map_err(|error| {
            ConfigError::Invalid(crate::locale_tag::with_params(
                "source.addressInvalid",
                &[("reason", &error.to_string())],
            ))
        })?;
        if resolved.origin() != root.origin() || !resolved.path().starts_with(root.path()) {
            return Err(ConfigError::Invalid("source.pathEscapesBranch".into()));
        }
        Self::remote(resolved)
    }

    fn remote(url: Url) -> Result<Self, ConfigError> {
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ConfigError::Invalid("source.mustBeHttp".into()));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ConfigError::Invalid("source.addressHasCredentials".into()));
        }
        Ok(Self::Remote(url))
    }

    fn bundled_path(reference: &str, base: Option<&str>) -> Result<Self, ConfigError> {
        if reference.starts_with(['/', '\\']) || reference.contains(['\\', ':', '?', '#']) {
            return Err(ConfigError::Invalid("source.builtinPathUnsafe".into()));
        }
        let mut components = base
            .map(|base| base.split('/').collect::<Vec<_>>())
            .unwrap_or_default();
        for component in reference.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(ConfigError::Invalid("source.builtinPathUnsafe".into()));
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
            return Err(ConfigError::Invalid("source.builtinNotToml".into()));
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
            return Err(ConfigError::Invalid("source.notBuiltin".into()));
        };
        BUNDLED_SOURCES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(path))
            .map(|(_, text)| *text)
            .ok_or_else(|| {
                ConfigError::Read(crate::locale_tag::with_params(
                    "source.builtinFileMissing",
                    &[("path", &path.to_string())],
                ))
            })
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

/// 源文档里的 `[[repository_sources]]` 声明。
///
/// `nickname` 已从单语字段改为**只认多语言表** —— 与仓库的
/// `name_i18n` / `group_i18n` / `note_i18n` 完全一致：
///   · `nickname_i18n` 必须包含 `zh-CN` 且非空（它就是兜底值）
///   · 旧的单语 `nickname` 仍可读，迁移时折叠进 `zh-CN`（不在 `zh-CN`
///     已存在时覆盖）
///
/// 为什么统一：多语言表 + 一个「兜底语言」比「单语字段 + 可选的翻译表」
/// 少一条规则，也不会出现「翻译表缺当前语言、单语字段又是另一种语言」
/// 这种两边都不是的情况。
#[derive(Debug, Clone, Deserialize)]
pub struct RepositorySourceDeclaration {
    pub id: String,
    #[serde(default)]
    pub nickname_i18n: std::collections::BTreeMap<String, String>,
    /// 兼容字段：旧文档的单语昵称。
    ///
    /// 反序列化后由 `normalize_source_nickname` 折叠进 `nickname_i18n`，
    /// 之后不再参与任何展示决策。
    #[serde(default, skip_serializing)]
    pub nickname: String,
    pub url: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default, deserialize_with = "deserialize_builtin")]
    pub builtin: Option<bool>,
}

/// 多语言昵称表里选展示值：`zh-CN` 优先，这是全项目的兜底语言。
///
/// 与前端 `pickI18n` 的规则一致（精确语言 → `zh-CN` → 第一个非空），
/// 但后端只需要后者提供的那一个「兜底值」—— 运行时镜像
/// `RepositorySource.nickname` 就用它，界面再按当前语言从表里选。
pub(crate) fn nickname_fallback(
    translations: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    translations
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            translations
                .values()
                .map(|value| value.trim())
                .find(|value| !value.is_empty())
                .map(str::to_string)
        })
}

/// 把单语 `nickname` 折叠进 `nickname_i18n`，并校验 zh-CN 兜底存在。
///
/// 返回规范化后的昵称（即 `zh-CN` 值）。规则与 `normalize_repository_i18n`
/// 对 name / group / note 的处理一致：
///   1. `zh-CN` 已有非空值 → 用它
///   2. 否则用旧单语 `nickname`
///   3. 两者都没有 → 报错（原文案是「仓库源 ID 和昵称不能为空」）
///
/// 其余语言留空时由前端 `pickI18n` 回落到 zh-CN，所以这里只保证 zh-CN 存在。
pub(crate) fn normalize_source_nickname(
    source: &mut RepositorySourceDeclaration,
) -> Result<String, ConfigError> {
    let legacy = source.nickname.trim().to_string();
    let resolved = nickname_fallback(&source.nickname_i18n)
        .or_else(|| (!legacy.is_empty()).then(|| legacy.clone()))
        .ok_or_else(|| ConfigError::Invalid("source.idOrNicknameEmpty".into()))?;
    // 空白翻译项一律清掉：`pickI18n` 会跳过空值，留着只会让 TOML 变长
    source
        .nickname_i18n
        .retain(|_, value| !value.trim().is_empty());
    source
        .nickname_i18n
        .insert("zh-CN".into(), resolved.clone());
    source.nickname = resolved.clone();
    Ok(resolved)
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
        Flag::Text(_) => Err(serde::de::Error::custom("source.builtinFlagInvalid")),
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
                    return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                        "proxy.unsupportedFormat",
                        &[("format", &other.to_string())],
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
                    return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                        "source.proxyAddressInvalid",
                        &[("reason", &error.to_string())],
                    )));
                }
            };
            let (url, bundled) = match resolved {
                SourceLocation::Remote(url) => {
                    if url.scheme() != "https" || url.host_str().is_none() {
                        return Err(ConfigError::Invalid("source.proxyNotHttps".into()));
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
        return Err(ConfigError::Read(crate::locale_tag::with_params(
            "source.proxyManifestMissing",
            &[("region", &region.to_string())],
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
        return Err(ConfigError::Invalid("source.tooLarge".into()));
    }
    let mut document: RepositorySourceDocument = toml::from_str(text).map_err(|error| {
        ConfigError::Invalid(crate::locale_tag::with_params(
            "source.tomlInvalid",
            &[("reason", &error.to_string())],
        ))
    })?;
    if document.repositories.is_empty()
        && document.repository_sources.is_empty()
        && document.github_proxy_sources.is_empty()
        && document.github_api_proxies.is_empty()
        && document.gitee_api_proxies.is_empty()
    {
        return Err(ConfigError::Invalid("source.emptyDocument".into()));
    }
    for repository in &mut document.repositories {
        crate::config::normalize_repository_i18n(repository)?;
        repository.region = crate::config::normalize_repository_region_for_storage(
            repository.region.as_deref(),
            &crate::locale_tag::with_params(
                "config.repositoryLabeled",
                &[("name", &repository.name.to_string())],
            ),
        );
        /*
         * 顺序很重要：**先校验原文，再归一化**。
         *
         * 反过来做（先归一化）会把 `platform = "linux"` 悄悄变成 None，
         * 校验阶段再也看不到非法值 —— 一个本该拒绝的配置会静默变成
         * 「全平台可用」。归一化只负责把空串/别名整理成规范值。
         */
        crate::config::validate_repository_platform(
            repository.platform.as_deref(),
            &repository.name,
        )?;
        repository.platform =
            crate::config::normalize_repository_platform(repository.platform.as_deref());
        validate_repository(repository)?;
    }
    for proxy_source in &mut document.github_proxy_sources {
        proxy_source.region = crate::config::normalize_repository_region_for_storage(
            proxy_source.region.as_deref(),
            "source.proxySourceLabel",
        );
        validate_repository_source_region(proxy_source.region.as_deref())?;
        match proxy_source.format.as_deref().unwrap_or("toml") {
            "toml" | "userscript" => {}
            other => {
                return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                    "proxy.unsupportedFormat",
                    &[("format", &other.to_string())],
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
            Err(error) => {
                return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                    "source.proxyAddressInvalid",
                    &[("reason", &error.to_string())],
                )));
            }
        };
        if let SourceLocation::Remote(url) = resolved {
            if url.scheme() != "https" || url.host_str().is_none() {
                return Err(ConfigError::Invalid("source.proxyNotHttps".into()));
            }
        }
    }
    let mut ids = std::collections::HashSet::new();
    let mut urls = std::collections::HashSet::new();
    for source in &mut document.repository_sources {
        source.id = source.id.trim().to_string();
        // 昵称只认多语言表（zh-CN 兜底），旧单语字段在此折叠
        let nickname = normalize_source_nickname(source)?;
        if source.id.is_empty() || nickname.is_empty() {
            return Err(ConfigError::Invalid("source.idOrNicknameEmpty".into()));
        }
        source.region = crate::config::normalize_repository_region_for_storage(
            source.region.as_deref(),
            &crate::locale_tag::with_params("source.labeled", &[("label", &source.id.to_string())]),
        );
        validate_repository_source_region(source.region.as_deref())?;
        source.url = SourceLocation::resolve(&source.url, Some(location))?.address();
        if !ids.insert(source.id.clone()) {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "source.idDuplicateValue",
                &[("id", &source.id.to_string())],
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
    /*
     * 清空这些源的仓库，但把 Release 缓存**暂存下来**。
     *
     * 早先的做法是整条 `retain` 掉仓库，于是列表重新拉回来、仓库重建之后
     * `cached_releases` 与 `releases_cached_at` 都是空的 —— 5 分钟缓存
     * 永远不可能命中，每次刷新（含每次启动）都得把整棵仓库树的 API 重打
     * 一遍。实测 20+ 个仓库、每次几十个请求，还固定撞上 Gitee 的速率限制。
     *
     * 缓存按 `repository_id` 存：ID 已经被 `source_repository_ids` 保住，
     * 重建出来的仓库会拿回同一个 ID，于是能对得上。`apply_document` 取回
     * 之后会清空这张暂存表，它只在「清理 → 重建」之间短暂存在。
     */
    for repo in &config.repositories {
        let Some(source_id) = repo.source_id.as_ref() else {
            continue;
        };
        if !descendants.contains(source_id) {
            continue;
        }
        let key = crate::config::source_repository_key(source_id, &crate::config::repo_key(repo));
        config.source_repository_ids.insert(key, repo.id.clone());
        // 只搬「有内容」的缓存：空列表搬过去也没有意义，反而让判断变复杂
        if !repo.cached_releases.is_empty() {
            config.staged_release_cache.insert(
                repo.id.clone(),
                crate::models::StagedReleaseCache {
                    releases: repo.cached_releases.clone(),
                    cached_at: repo.releases_cached_at,
                    last_error: repo.last_error.clone(),
                },
            );
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

/// 下载（或读取内置）一个源文档，再交给调用方解析。
///
/// 抽出来是因为地图源要读**同样形态**的文本文档（`maps.toml`），而下载这条
/// 路上每一步都踩过坑：30 秒固定超时、禁缓存头、大小上限、重定向后的相对
/// 地址解析、错误分类。复制一份只会得到两套迟早各自漂移的规则。
///
/// 返回解析后的结果，日志记录尽力而为 —— 解析失败时仍把状态码与字节数写进
/// 日志，否则远程文档「下载成功但解析失败」这种情况无从区分。
pub(crate) async fn load_text_document<T>(
    location: &SourceLocation,
    parse: impl Fn(&str, &SourceLocation) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let timeout_seconds = SOURCE_DOCUMENT_TIMEOUT;
    let SourceLocation::Remote(url) = location else {
        let text = location.bundled_text()?;
        log::info!(
            "内置文档加载完成：地址={}",
            safe_log_address(&location.address())
        );
        return parse(text, location);
    };
    let started_at = std::time::Instant::now();
    let safe_url = safe_log_address(url.as_str());
    log::info!(
        "开始加载远程文档：地址={}，超时={}秒",
        safe_url,
        timeout_seconds
    );
    let client = releases::build_client(timeout_seconds).map_err(|error| {
        log::error!(
            "创建文档 HTTP 客户端失败：地址={}，原因={}",
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
                "请求远程文档失败：地址={}，超时={}，连接错误={}",
                safe_url,
                error.is_timeout(),
                error.is_connect()
            );
            ConfigError::Read(error.to_string())
        })?;
    let status = response.status();
    if let Err(error) = response.error_for_status_ref() {
        log::error!(
            "远程文档 HTTP 请求失败：地址={}，状态码={}",
            safe_url,
            status.as_u16()
        );
        return Err(ConfigError::Read(error.to_string()));
    }
    /*
     * 重定向后文档的**真实地址**才是相对路径的基准。
     *
     * 源文档里写 `url = "sibling.toml"` 时，它相对的是最终落地的那个地址，
     * 不是最初请求的地址 —— 少了这一步，走重定向的源会把子源解析到错误位置。
     */
    let resolved = remote_document_location(url, response.url().clone())?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| {
        log::error!(
            "读取远程文档响应失败：地址={}，已读取={}字节",
            safe_url,
            bytes.len()
        );
        ConfigError::Read(error.to_string())
    })? {
        if bytes.len() + chunk.len() > MAX_SOURCE_BYTES {
            log::error!("远程文档超过大小限制：地址={}", safe_url);
            return Err(ConfigError::Invalid("source.tooLarge".into()));
        }
        bytes.extend_from_slice(&chunk);
    }
    let byte_count = bytes.len();
    let text = String::from_utf8(bytes).map_err(|error| {
        log::error!("远程文档不是有效 UTF-8：地址={}，原因={}", safe_url, error);
        ConfigError::Parse(error.to_string())
    })?;
    let result = parse(&text, &resolved);
    match &result {
        Ok(_) => log::info!(
            "远程文档加载完成：地址={}，状态码={}，字节={}，耗时={}ms",
            safe_url,
            status.as_u16(),
            byte_count,
            started_at.elapsed().as_millis()
        ),
        Err(error) => log::error!(
            "远程文档解析失败：地址={}，状态码={}，字节={}，错误类别={}",
            safe_url,
            status.as_u16(),
            byte_count,
            error_category(error)
        ),
    }
    result
}

/// 一份需要「先问 API、再下载比对」的远程文档。
///
/// 第三方地图源把整份 TOML 放进 Release 资产里
/// （`.../releases/download/maps/map-sources.toml`）。这个位置比 raw 强的地方
/// 在于：sha256 由 Release API 给出（GitHub 的 `digest` 字段），下载还能复用
/// 直连/代理竞速。`load_text_document` 只会裸 GET 一次，所以另开这一条。
///
/// 校验信息必须来自**同一个** assets 条目：先读 API、再按 API 说的 size/digest
/// 比对，顺序反过来（先下载）就没有可信的期望值了。
pub(crate) struct VerifiedDocument {
    /// Release API 报告的字节数（Gitee 也会给）。
    pub expected_size: Option<u64>,
    /// Release API 报告的摘要，形如 `sha256:<hex>`；Gitee 没有 → `None`。
    pub digest: Option<String>,
    /// 下载候选（直连 + 代理），顺序由代理模式决定。
    pub candidates: Vec<crate::download::DownloadCandidate>,
}

/// 下载并**校验**一个远程文本文档，再交给调用方解析。
///
/// 只在「有校验信息」时走这条：没有候选、或不是远程地址，退回到
/// `load_text_document` 的单次直连 —— 校验是增强，不该成为新的失败点。
pub(crate) async fn load_verified_text_document<T>(
    location: &SourceLocation,
    verified: &VerifiedDocument,
    parse: impl Fn(&str, &SourceLocation) -> Result<T, ConfigError>,
) -> Result<T, ConfigError> {
    let SourceLocation::Remote(url) = location else {
        return load_text_document(location, parse).await;
    };
    if verified.candidates.is_empty() {
        log::warn!(
            "校验文档没有可用下载候选，退回单次直连：地址={}",
            safe_log_address(url.as_str())
        );
        return load_text_document(location, parse).await;
    }
    let safe_url = safe_log_address(url.as_str());
    let started_at = std::time::Instant::now();
    log::info!(
        "开始加载校验文档：地址={}，候选={}，期望字节={:?}，期望摘要={}",
        safe_url,
        verified.candidates.len(),
        verified.expected_size,
        if verified.digest.is_some() {
            "有"
        } else {
            "无"
        }
    );
    let client = releases::build_client(SOURCE_DOCUMENT_TIMEOUT).map_err(|error| {
        log::error!(
            "创建文档 HTTP 客户端失败：地址={}，原因={}",
            safe_url,
            error
        );
        ConfigError::Read(error.to_string())
    })?;
    let expected_size = verified.expected_size;
    let digest = verified.digest.clone();
    let bytes = crate::download::try_download_candidates(
        verified.candidates.clone(),
        || false,
        |candidate, reporter| {
            let client = client.clone();
            let digest = digest.clone();
            async move {
                fetch_document_bytes(
                    &client,
                    &candidate.url,
                    &reporter,
                    expected_size,
                    digest.as_deref(),
                )
                .await
            }
        },
        None,
    )
    .await
    .map_err(|error| {
        log::error!("校验文档下载失败：地址={}，错误={}", safe_url, error);
        ConfigError::Read(error)
    })?;
    let byte_count = bytes.len();
    let text = String::from_utf8(bytes).map_err(|error| {
        log::error!("远程文档不是有效 UTF-8：地址={}，原因={}", safe_url, error);
        ConfigError::Parse(error.to_string())
    })?;
    /*
     * 基准用**请求地址**而不是重定向后的地址：Release 资产会跳到签名 CDN，
     * 那里的路径不再有仓库结构，相对子源只会解析到错误位置。真正的规矩是
     * 「Release 资产文档不许声明相对子源」（`resolve_remote_path` 直接报错），
     * 所以这里传什么都一样，传请求地址至少日志可读。
     */
    let result = parse(&text, location);
    match &result {
        Ok(_) => log::info!(
            "校验文档加载完成：地址={}，字节={}，耗时={}ms",
            safe_url,
            byte_count,
            started_at.elapsed().as_millis()
        ),
        Err(error) => log::error!(
            "校验文档解析失败：地址={}，字节={}，错误类别={}",
            safe_url,
            byte_count,
            error_category(error)
        ),
    }
    result
}

/// 竞速分支里的单次下载：读字节、报进度、按 API 给的 size/digest 校验。
async fn fetch_document_bytes(
    client: &reqwest::Client,
    url: &str,
    reporter: &crate::archive::DownloadReporter,
    expected_size: Option<u64>,
    digest: Option<&str>,
) -> Result<Vec<u8>, String> {
    let mut response = client
        .get(url)
        .header("User-Agent", releases::APP_USER_AGENT)
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache")
        .send()
        .await
        .map_err(|error| error.without_url().to_string())?;
    let status = response.status();
    if !status.is_success() {
        return Err(format!("{} {}", safe_log_address(url), status.as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_SOURCE_BYTES as u64)
    {
        return Err("source.tooLarge".into());
    }
    let mut content = Vec::new();
    let mut progress = reporter.snapshot();
    progress.total = expected_size.or(response.content_length());
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| error.without_url().to_string())?
    {
        if content.len() + chunk.len() > MAX_SOURCE_BYTES {
            return Err("source.tooLarge".into());
        }
        content.extend_from_slice(&chunk);
        reporter.received(chunk.len() as u64);
        progress.downloaded = content.len() as u64;
        reporter.update(progress.clone());
    }
    /*
     * 与资产下载同一套校验：大小不符 / 摘要不符都说明「下到的不是 API 说的那份」。
     * 只有字节数是真的能对上的时候才比 —— 有的代理会剥掉 Content-Length，
     * 但 `expected_size` 来自 API，恒可比。
     */
    crate::archive::validate_download_bytes(&content, expected_size, digest)?;
    Ok(content)
}

/// 读取一个源文档（远程地址或内置资源）。
///
/// 远程文档固定使用 `SOURCE_DOCUMENT_TIMEOUT`（30 秒）：索引与各级子源都走
/// 这里，超时口径一致。不再由调用方传入 —— 这个阶段和 Release API 的
/// 5 秒、下载的连接超时是三种不同性质的请求，混用一个配置会让
/// 「调快下载」顺带把源文档读取弄坏。
pub async fn load_document(
    location: &SourceLocation,
) -> Result<RepositorySourceDocument, ConfigError> {
    let timeout_seconds = SOURCE_DOCUMENT_TIMEOUT;
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
            return Err(ConfigError::Invalid("source.tooLarge".into()));
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
                "[[repositories]]\nname = \"{name}\"\nname_i18n.zh-CN = \"{name}\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{ type = \"glob\", value = \"*.dll\" }}]\n"
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
            load_document(&location).await.unwrap().repositories[0].name,
            "Old"
        );
        assert_eq!(
            load_document(&location).await.unwrap().repositories[0].name,
            "New"
        );
        assert!(load_document(&location).await.is_err());
        server.join().unwrap();
    }

    /*
     * 源文档超时固定 30 秒，且**不受** `network.timeout_seconds` 影响。
     *
     * 用户把网络超时调到 5 秒是为了让下载更快失败，但索引和各级子源
     * 是串行递归的：一层被压到 5 秒，后面所有子源都没机会加载 ——
     * 表现为「源刷新随机失败、仓库列表时有时无」。
     *
     * 这里用「延迟 1.5 秒才响应」的本地服务验证读得完；如果超时仍走
     * 用户配置且被调到很小，这条会失败。
     */
    #[tokio::test]
    async fn source_document_timeout_is_independent_of_user_network_config() {
        assert_eq!(SOURCE_DOCUMENT_TIMEOUT, 30);

        let body = "[[repositories]]\nname_i18n.zh-CN = \"Slow\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{ type = \"glob\", value = \"*.dll\" }]\n";
        let (url, server) = serve_delayed_response("/slow.toml", body, 1500);
        let location = SourceLocation::parse(&url).unwrap();
        let document = load_document(&location)
            .await
            .expect("1.5 秒延迟应在 30 秒超时内成功");
        assert_eq!(document.repositories[0].name, "Slow");
        server.join().unwrap();
    }

    /// 接受一次连接、睡眠 `delay_ms` 后再回 200。
    fn serve_delayed_response(
        path: &'static str,
        body: &'static str,
        delay_ms: u64,
    ) -> (String, std::thread::JoinHandle<()>) {
        use std::io::{BufRead, BufReader, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
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
            stream.set_nonblocking(false).unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                request.push_str(&line);
            }
            assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            std::thread::sleep(std::time::Duration::from_millis(delay_ms));
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        });
        (format!("http://{address}{path}"), server)
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
                "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/repositories-gua.toml",
            ),
            (
                crate::config::REGION_INTERNATIONAL,
                "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/repositories-gua.toml",
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

    /*
     * 这两类嵌入文档**不是**仓库源，各有自己的解析器与自己的回归测试：
     *
     * - `repositories-proxies.toml`：只有代理清单，没有 repositories；
     * - `maps*.toml`：地图源（`[[maps]]` / `[[map_sources]]`），由
     *   `map_sources::bundled_tests` 覆盖。
     *
     * 按名跳过而不是放宽断言：这条测试的价值就在「每个声称是仓库源的
     * 嵌入文件都必须能被仓库解析器读懂」，放宽会让真正的坏文档混过去。
     * 新增非仓库类嵌入文档时在这里登记，并为它补一份对应的解析测试。
     */
    const NOT_REPOSITORY_DOCUMENTS: &[&str] =
        &["repositories-proxies.toml", "maps.toml", "maps-rw.toml"];

    #[test]
    fn all_bundled_source_documents_use_the_shared_parser() {
        for (path, _) in BUNDLED_SOURCES {
            if NOT_REPOSITORY_DOCUMENTS
                .iter()
                .any(|skipped| path.eq_ignore_ascii_case(skipped))
            {
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

    /// 登记表必须覆盖所有非仓库类文档：新增一个忘了登记，上面那条测试就会红，
    /// 但红的原因是「地图文档解析失败」，容易被误读成解析器坏了。这里直接说明。
    #[test]
    fn every_non_repository_document_is_registered() {
        for (path, _) in BUNDLED_SOURCES {
            let location = SourceLocation::parse(path).unwrap();
            if location.bundled_document().is_ok() {
                continue;
            }
            assert!(
                NOT_REPOSITORY_DOCUMENTS
                    .iter()
                    .any(|skipped| path.eq_ignore_ascii_case(skipped)),
                "{path} 无法被仓库解析器读取，但没有登记为「非仓库类文档」"
            );
        }
    }

    #[tokio::test]
    async fn builtin_index_loads_only_declared_sources_and_relative_repository_documents() {
        let location = SourceLocation::parse(BUILTIN_INDEX).unwrap();
        /*
         * 用 `parse_document` 而不是 `toml::from_str`：昵称现在只认
         * `nickname_i18n`，镜像字段 `nickname` 是由 `normalize_source_nickname`
         * 从表里派生的 —— 直接反序列化拿到的是未归一化的空镜像。
         */
        let declared_index = parse_document(location.bundled_text().unwrap(), &location).unwrap();
        let index = load_document(&location).await.unwrap();
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
            let mut declared_child: RepositorySourceDocument =
                toml::from_str(child_location.bundled_text().unwrap()).unwrap();
            for repository in &mut declared_child.repositories {
                crate::config::normalize_repository_i18n(repository).unwrap();
            }
            let child = load_document(&child_location)
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
    fn repositories_require_chinese_translations_and_ignore_raw_metadata() {
        let location = SourceLocation::parse("index.toml").unwrap();
        let document_for = |metadata: &str| {
            format!(
                "[[repositories]]\n{metadata}\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{ type = \"exact\", value = \"mod.dll\" }}]\n"
            )
        };
        let valid = document_for(
            "name = \"Raw name\"\nname_i18n.zh-CN = \"中文名称\"\ngroup = \"Raw group\"\ngroup_i18n.zh-CN = \"中文分组\"\nnote = \"Raw note\"\nnote_i18n.zh-CN = \"中文备注\"",
        );
        let parsed = parse_document(&valid, &location).unwrap();
        let repo = &parsed.repositories[0];
        assert_eq!(repo.name, "中文名称");
        assert_eq!(repo.group, "中文分组");
        assert_eq!(repo.note, "中文备注");

        for (metadata, expected_error) in [
            (
                "name_i18n.zh-CN = \"名称\"\nnote_i18n.zh-CN = \"\"",
                "config.missingGroupI18n",
            ),
            (
                "name_i18n.zh-CN = \"名称\"\ngroup_i18n.zh-CN = \"\"",
                "config.missingNoteI18n",
            ),
            (
                "name_i18n.zh-CN = \"\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"",
                "config.emptyNameI18n",
            ),
            (
                "name = \"Raw only\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"",
                "config.missingNameI18n",
            ),
        ] {
            let error = parse_document(&document_for(metadata), &location).unwrap_err();
            assert!(error.to_string().contains(expected_error), "{error}");
        }
    }

    /*
     * 昵称与 name / group / note 采用同一套多语言规则：
     *   · 只写多语言表（含 zh-CN）→ 直接用
     *   · 只写旧的单语 nickname → 折叠进 zh-CN（迁移兼容）
     *   · 两者都没有 → 报错，不产生空白昵称
     *
     * 旧实现允许「单语 + 空翻译表」，那是 `nickname_i18n.is_empty()`
     * 那条断言留下的行为；现在表里必须始终有 zh-CN。
     */
    #[test]
    fn source_nickname_uses_i18n_table_with_zh_cn_fallback() {
        let location = SourceLocation::parse("index.toml").unwrap();

        // 只有旧的单语字段：折叠进 zh-CN，镜像值同步
        let legacy = r#"
[[repository_sources]]
id = "mods"
nickname = "Fallback name"
url = "mods.toml"
"#;
        let document = parse_document(legacy, &location).unwrap();
        let source = &document.repository_sources[0];
        assert_eq!(source.nickname, "Fallback name");
        assert_eq!(source.nickname_i18n["zh-CN"], "Fallback name");

        // 多语言表齐全：镜像值取 zh-CN，不取单语字段
        let translated = r#"
[[repository_sources]]
id = "mods"
nickname = "Ignored fallback"
url = "mods.toml"
[repository_sources.nickname_i18n]
zh-CN = "中文昵称"
zh-TW = "繁體暱稱"
en = "Translated name"
ko = "번역 이름"
"#;
        let document = parse_document(translated, &location).unwrap();
        let source = &document.repository_sources[0];
        assert_eq!(source.nickname, "中文昵称", "镜像值应取 zh-CN");
        assert_eq!(source.nickname_i18n["en"], "Translated name");
        assert_eq!(source.nickname_i18n["ko"], "번역 이름");

        // 完全没有昵称 → 拒绝
        let missing = r#"
[[repository_sources]]
id = "mods"
url = "mods.toml"
"#;
        let error = parse_document(missing, &location).unwrap_err();
        assert!(
            error.to_string().contains("source.idOrNicknameEmpty"),
            "{error}"
        );

        // 翻译表语言值类型错误仍然报错
        assert!(
            parse_document(
                &translated.replace("en = \"Translated name\"", "en = 1"),
                &location
            )
            .is_err()
        );
    }

    /// 空白翻译项被丢弃，不会留下一个「看起来有、其实空」的语言键。
    #[test]
    fn blank_nickname_translations_are_dropped() {
        let location = SourceLocation::parse("index.toml").unwrap();
        let text = r#"
[[repository_sources]]
id = "mods"
url = "mods.toml"
[repository_sources.nickname_i18n]
zh-CN = "中文昵称"
en = "   "
"#;
        let document = parse_document(text, &location).unwrap();
        assert_eq!(document.repository_sources[0].nickname_i18n.len(), 1);
        assert_eq!(document.repository_sources[0].nickname, "中文昵称");
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
                // Windows accepted sockets inherit the listener's nonblocking mode.
                stream.set_nonblocking(false).unwrap();
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
        let result = load_document(&location).await;
        server.join().unwrap();
        assert_eq!(
            result.unwrap().repository_sources[0].url,
            format!("http://{address}/mods.toml")
        );
    }
}

/// 整批刷新不能把 Release 缓存一起丢掉。
///
/// 真实回归：清理源缓存会把整棵源的仓库删掉，源文档回来后再重建。重建出来的
/// 仓库 `cached_releases` 为空，于是 5 分钟缓存**永远命中不了** —— 每次启动
/// 都要把整棵仓库树的 API 重打一遍（20+ 个仓库、几十个请求，还固定撞 Gitee
/// 的速率限制）。用户看日志就是「跳过缓存=false」却照样一路请求。
#[cfg(test)]
mod release_cache_staging_tests {
    use super::*;

    fn repo(id: &str, source_id: &str) -> crate::models::RepositoryConfig {
        let mut repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": id, "name": "Mod", "group": "G",
            "provider": "github", "username": "u", "repository": "r",
            "download_mode": "release", "channel": "all",
            "asset_rules": [{ "type": "exact", "value": "a.zip" }],
            "install_target": "BepInEx/plugins",
            "enabled": true, "builtin": false,
            "max_release_count": 10, "download_threads": 32
        }))
        .unwrap();
        repo.source_id = Some(source_id.to_string());
        repo
    }

    #[test]
    fn clearing_a_source_stages_the_release_cache_instead_of_dropping_it() {
        let mut config = crate::config::default_config();
        config.repository_sources = vec![
            serde_json::from_value(serde_json::json!({
                "id": "src", "nickname": "S", "nickname_i18n": { "zh-CN": "S" },
                "url": "https://example.com/x.toml", "builtin": false
            }))
            .unwrap(),
        ];

        let mut entry = repo("repo-1", "src");
        entry.cached_releases = vec![
            serde_json::from_value(serde_json::json!({
                "tag_name": "v1.0.0", "version": "1.0.0", "name": "v1", "body": "",
                "prerelease": false, "draft": false, "assets": []
            }))
            .unwrap(),
        ];
        entry.releases_cached_at = Some(1_700_000_000_000);
        config.repositories = vec![entry];

        clear_source_cache(&mut config, &["src".to_string()]);

        // 仓库确实被清掉了（源文档要重新决定它的存在与配置）
        assert!(config.repositories.is_empty(), "仓库应被清空");
        // 但缓存被暂存下来，等重建时取回
        let staged = config
            .staged_release_cache
            .get("repo-1")
            .expect("缓存必须被暂存，否则重建后永远命中不了");
        assert_eq!(staged.releases.len(), 1);
        assert_eq!(staged.cached_at, Some(1_700_000_000_000));
    }

    #[test]
    fn empty_caches_are_not_staged() {
        let mut config = crate::config::default_config();
        config.repository_sources = vec![
            serde_json::from_value(serde_json::json!({
                "id": "src", "nickname": "S", "nickname_i18n": { "zh-CN": "S" },
                "url": "https://example.com/x.toml", "builtin": false
            }))
            .unwrap(),
        ];
        // 从没成功刷新过：空列表 + 没有时间戳
        config.repositories = vec![repo("repo-1", "src")];

        clear_source_cache(&mut config, &["src".to_string()]);

        assert!(
            config.staged_release_cache.is_empty(),
            "空缓存不该搬进暂存表，否则判断变复杂而且没有意义"
        );
    }
}

/// 完整链路：清理 → 按源文档重建 → 缓存必须回到仓库上。
///
/// 用户报的现象是「跳过缓存=false 却每次都请求 API」。根因有两层：
///   1. 整批刷新会删光整棵源的仓库再重建，缓存跟着丢失；
///   2. `apply_document` 是逐源调用的，早先每次调用结束都无条件清空
///      暂存表 —— 第一个源应用完就把后面所有源的缓存扔了。
/// 这条测试锁住「清理时暂存、重建后接回」这条链路本身。
#[cfg(test)]
mod staged_cache_roundtrip_tests {
    use super::*;

    fn sample_repo(id: &str, source_id: &str) -> crate::models::RepositoryConfig {
        let mut repo: crate::models::RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": id, "name": "Mod", "group": "G",
            "provider": "github", "username": "u", "repository": "r",
            "download_mode": "release", "channel": "all",
            "asset_rules": [{ "type": "exact", "value": "a.zip" }],
            "install_target": "BepInEx/plugins",
            "enabled": true, "builtin": false,
            "max_release_count": 10, "download_threads": 32
        }))
        .unwrap();
        repo.source_id = Some(source_id.to_string());
        repo
    }

    fn sample_source(id: &str) -> crate::models::RepositorySource {
        serde_json::from_value(serde_json::json!({
            "id": id, "nickname": "S", "nickname_i18n": { "zh-CN": "S" },
            "url": "https://example.com/x.toml", "builtin": false
        }))
        .unwrap()
    }

    #[test]
    fn cache_survives_clear_and_is_restorable_by_id() {
        let now = 1_700_000_000_000i64;
        let mut config = crate::config::default_config();
        config.repository_sources = vec![sample_source("src")];

        let mut entry = sample_repo("repo-1", "src");
        entry.cached_releases = vec![
            serde_json::from_value(serde_json::json!({
                "tag_name": "v1.0.0", "version": "1.0.0", "name": "v1", "body": "",
                "prerelease": false, "draft": false, "assets": []
            }))
            .unwrap(),
        ];
        entry.releases_cached_at = Some(now);
        config.repositories = vec![entry];

        // 第一步：整批刷新前的清理
        clear_source_cache(&mut config, &["src".to_string()]);
        assert!(config.repositories.is_empty(), "仓库应被清空");
        let staged = config
            .staged_release_cache
            .get("repo-1")
            .expect("缓存必须暂存下来");
        assert_eq!(staged.cached_at, Some(now));

        // 第二步：按源文档重建（模拟 apply_document 的接回）
        let mut rebuilt = sample_repo("new-uuid", "src");
        rebuilt.id = "repo-1".into();
        if let Some(pulled) = config.staged_release_cache.remove("repo-1") {
            rebuilt.cached_releases = pulled.releases;
            rebuilt.releases_cached_at = pulled.cached_at;
            rebuilt.last_error = pulled.last_error;
        }
        config.repositories = vec![rebuilt];

        // 重建后的仓库必须带着原缓存，且时间戳仍能让 TTL 判定通过
        assert_eq!(config.repositories[0].cached_releases.len(), 1);
        assert_eq!(config.repositories[0].releases_cached_at, Some(now));
        assert!(config.staged_release_cache.is_empty(), "认领后暂存表应为空");
    }

    /// 多个源共存时，暂存表不能被「第一个源重建完」就清空。
    ///
    /// 这正是用户踩到的那个 bug：`apply_document` 逐源调用，早先每次都
    /// 无条件 `clear()`，于是只有第一个源能拿回缓存。
    #[test]
    fn staging_table_keeps_entries_for_sources_not_yet_rebuilt() {
        let mut config = crate::config::default_config();
        config.repository_sources = vec![sample_source("src-a"), sample_source("src-b")];

        let mut a = sample_repo("repo-a", "src-a");
        a.cached_releases = vec![
            serde_json::from_value(serde_json::json!({
                "tag_name": "v1.0.0", "version": "1.0.0", "name": "v1", "body": "",
                "prerelease": false, "draft": false, "assets": []
            }))
            .unwrap(),
        ];
        a.releases_cached_at = Some(1_700_000_000_000);
        let mut b = a.clone();
        b.id = "repo-b".into();
        b.source_id = Some("src-b".into());
        config.repositories = vec![a, b];

        clear_source_cache(&mut config, &["src-a".to_string(), "src-b".to_string()]);
        assert_eq!(config.staged_release_cache.len(), 2);

        // 只认领 A（模拟 src-a 的 apply_document 跑完）
        config.staged_release_cache.remove("repo-a");
        assert!(
            config.staged_release_cache.contains_key("repo-b"),
            "B 的缓存必须留到它自己的 apply_document 跑完"
        );
    }
}
