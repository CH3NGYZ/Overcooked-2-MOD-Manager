use crate::models::{
    AssetRule, AssetRuleType, Channel, GitHubProxy, Provider, ReleaseAsset, ReleaseInfo,
    RepositoryConfig,
};
use globset::GlobBuilder;
use regex::Regex;
use reqwest::{Client, StatusCode};
use serde_json::Value;
use url::Url;

/// Stable identifier for Cloudflare rules and access logs. The package
/// version makes every request identify the exact desktop client release.
pub const APP_USER_AGENT: &str = concat!(
    "Overcooked 2 MOD Manager/",
    env!("CARGO_PKG_VERSION"),
    " (client=desktop; os=Windows; arch=x64; tauri=2)"
);

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("Release operation failed: {0}")]
    InvalidUrl(String),
    #[error("Release operation failed: {0}")]
    Http(String),
    #[error("Release operation failed: {0}")]
    Status(StatusCode),
    /// 403/429 且带有限流特征：调用方据此给闸门上锁并改走代理。
    #[error("Release operation failed: rate limited ({status})")]
    RateLimited {
        status: StatusCode,
        signal: Box<crate::api_rate_gate::RateLimitSignal>,
    },
    #[error("Release operation failed: {0}")]
    Json(String),
}

const API_REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const REPOSITORY_API_ATTEMPTS: usize = 3;

impl ReleaseError {
    pub fn is_forbidden(&self) -> bool {
        matches!(
            self,
            Self::Status(status) if *status == StatusCode::FORBIDDEN
        ) || matches!(self, Self::RateLimited { status, .. } if *status == StatusCode::FORBIDDEN)
    }

    /// 是否因限流被拒（403/429 且带限流特征）。
    pub fn rate_limit_signal(&self) -> Option<&crate::api_rate_gate::RateLimitSignal> {
        match self {
            Self::RateLimited { signal, .. } => Some(signal),
            _ => None,
        }
    }
}

fn release_error_kind(error: &ReleaseError) -> &'static str {
    match error {
        ReleaseError::InvalidUrl(_) => "invalid_url",
        ReleaseError::Http(_) => "http",
        ReleaseError::Status(status) if *status == StatusCode::FORBIDDEN => "http_403",
        ReleaseError::Status(status) if *status == StatusCode::TOO_MANY_REQUESTS => "http_429",
        ReleaseError::Status(_) => "http_status",
        ReleaseError::RateLimited { status, .. } if *status == StatusCode::TOO_MANY_REQUESTS => {
            "rate_limited_429"
        }
        ReleaseError::RateLimited { .. } => "rate_limited_403",
        ReleaseError::Json(_) => "json",
    }
}

fn api_target_kind(
    official_url: &str,
    target_url: &str,
    request_token: Option<&str>,
) -> &'static str {
    if target_url == official_url {
        if request_token.is_some() {
            "official_token"
        } else {
            "official_direct"
        }
    } else {
        "proxy"
    }
}

fn is_official_gitee_api_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    matches!(url.host_str(), Some("gitee.com" | "www.gitee.com"))
        && url.path().starts_with("/api/v5/")
}

/// Gitee 的 access_token 通过 query 参数传递（与官方文档一致）。
/// 只有官方 Gitee API 会附加；代理地址不携带凭据。
fn gitee_api_url_with_token(url: &str, token: &str) -> Result<String, ReleaseError> {
    let mut parsed =
        Url::parse(url).map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    parsed.query_pairs_mut().append_pair("access_token", token);
    Ok(parsed.into())
}

pub fn repository_coordinates(
    username: &str,
    repository: &str,
) -> Result<(String, String), ReleaseError> {
    let username = username.trim();
    let repository = repository.trim();
    if username.is_empty()
        || repository.is_empty()
        || username
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        || repository
            .chars()
            .any(|character| character.is_whitespace() || character.is_control())
        || username.contains(['/', '\\', ':', '?', '#'])
        || repository.contains(['/', '\\', ':', '?', '#'])
        || username == "."
        || username == ".."
        || repository == "."
        || repository == ".."
    {
        return Err(ReleaseError::InvalidUrl("source.coordinateUnsafe".into()));
    }
    Ok((username.to_owned(), repository.to_owned()))
}

pub fn repository_homepage(
    provider: &Provider,
    username: &str,
    repository: &str,
) -> Result<String, ReleaseError> {
    let (username, repository) = repository_coordinates(username, repository)?;
    let base = match provider {
        Provider::Github => "https://github.com/",
        Provider::Gitee => "https://gitee.com/",
    };
    let mut url = Url::parse(base).expect("fixed repository host is a valid URL");
    url.path_segments_mut()
        .expect("fixed repository host accepts paths")
        .pop_if_empty()
        .push(&username)
        .push(&repository);
    Ok(url.into())
}

pub fn repository_release_api_url(
    provider: &Provider,
    username: &str,
    repository: &str,
    max_release_count: u32,
) -> Result<String, ReleaseError> {
    repository_release_api_url_page(provider, username, repository, max_release_count, None)
}

/// Release API 地址；`page` 为空或 1 时不写 `page` 参数。
///
/// 第 1 页的地址被既有测试与代理模板逐字匹配
/// （`...?per_page=N&direction=desc&sort=created`），多一个 `page=1` 会同时
/// 改掉两边，所以只有真正翻页时才追加参数。
pub fn repository_release_api_url_page(
    provider: &Provider,
    username: &str,
    repository: &str,
    max_release_count: u32,
    page: Option<u32>,
) -> Result<String, ReleaseError> {
    let (username, repository) = repository_coordinates(username, repository)?;
    let base = match provider {
        Provider::Github => "https://api.github.com/repos/",
        Provider::Gitee => "https://gitee.com/api/v5/repos/",
    };
    let mut url = Url::parse(base).expect("fixed Release API host is a valid URL");
    url.path_segments_mut()
        .expect("fixed Release API host accepts paths")
        .pop_if_empty()
        .push(&username)
        .push(&repository)
        .push("releases");
    url.query_pairs_mut()
        .append_pair("per_page", &max_release_count.clamp(1, 100).to_string());
    if let Some(page) = page.filter(|page| *page > 1) {
        url.query_pairs_mut().append_pair("page", &page.to_string());
    }
    url.query_pairs_mut()
        .append_pair("direction", "desc")
        .append_pair("sort", "created");
    Ok(url.into())
}

pub fn repository_file_url(
    provider: &Provider,
    username: &str,
    repository: &str,
    branch: &str,
    file_path: &str,
) -> Result<String, ReleaseError> {
    let homepage = repository_homepage(provider, username, repository)?;
    let branch = branch.trim();
    let file_path = file_path.trim();
    if file_path.contains(['*', '?']) {
        return Err(ReleaseError::InvalidUrl(
            "source.fileModeRequiresExactPath".into(),
        ));
    }
    if branch.is_empty()
        || file_path.is_empty()
        || branch.starts_with(['/', '\\'])
        || file_path.starts_with(['/', '\\'])
        || branch
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
        || file_path
            .split(['/', '\\'])
            .any(|part| part.is_empty() || part == "." || part == "..")
    {
        return Err(ReleaseError::InvalidUrl("source.fileModePathUnsafe".into()));
    }
    let mut url =
        Url::parse(&homepage).map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    {
        let mut segments = url
            .path_segments_mut()
            .map_err(|_| ReleaseError::InvalidUrl("source.fileModeUrlFailed".into()))?;
        segments.pop_if_empty();
        segments.push("raw");
        if *provider == Provider::Github {
            segments.push("refs");
            segments.push("heads");
        }
        for part in branch.split('/') {
            segments.push(part);
        }
        for part in file_path.split(['/', '\\']) {
            segments.push(part);
        }
    }
    Ok(url.into())
}

pub fn repository_file_name(file_path: &str) -> Result<String, ReleaseError> {
    let encoded = file_path
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| ReleaseError::InvalidUrl("source.fileModeNameMissing".into()))?;
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let hex = bytes
                .get(index + 1..index + 3)
                .ok_or_else(|| ReleaseError::InvalidUrl("source.fileNameEncodingInvalid".into()))?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).unwrap_or(""), 16)
                .map_err(|_| ReleaseError::InvalidUrl("source.fileNameEncodingInvalid".into()))?;
            decoded.push(value);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let name = String::from_utf8(decoded)
        .map_err(|_| ReleaseError::InvalidUrl("source.fileNameNotUtf8".into()))?;
    if name.is_empty() || name.contains(['/', '\\', ':']) || name == "." || name == ".." {
        return Err(ReleaseError::InvalidUrl("source.fileNameUnsafe".into()));
    }
    Ok(name)
}

/// 版本号解析：把各种宽写 tag 归一到三段 semver。
///
///   - 不足三段补 0：`0.4` → `0.4.0`；
///   - 第 4 段及以后丢弃（semver 只能表达三段）：
///     `0.4.0.5`、`0.4.0.0` → `0.4.0`；
///   - 保留 `-`/`+` 后缀作为预发布/构建元数据。
///
/// **MOD 更新判定不要用这个函数的返回值**，用 `compare_release_versions`：
/// 后者保留全部数字段（本项目有仓库靠第 4 段发版）。
pub fn parse_version(tag: &str) -> Option<semver::Version> {
    let cleaned = tag.trim().trim_start_matches(['v', 'V']);
    semver::Version::parse(cleaned).ok().or_else(|| {
        let re = Regex::new(r"(\d+(?:\.\d+){1,7}(?:[-+][0-9A-Za-z.-]+)?)").ok()?;
        let value = re.captures(cleaned)?.get(1)?.as_str();
        let numeric = value.split(['-', '+']).next()?;
        let mut parts = numeric
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let suffix = value.get(numeric.len()..).unwrap_or_default();
        parts.truncate(3);
        let normalized = match parts.as_slice() {
            [major, minor] => format!("{major}.{minor}.0{suffix}"),
            [major, minor, patch] => format!("{major}.{minor}.{patch}{suffix}"),
            _ => return None,
        };
        semver::Version::parse(&normalized).ok()
    })
}

/*
 * MOD 版本比较：**全部数字段都参与** + 标准 semver 的预发布语义。
 *
 * 本项目实际有仓库用第 4 段发版（`1.8.7.21` → `1.8.7.23`）。若按标准
 * semver 只比三段，这两个版本会归一成相同的 `1.8.7`，永远显示「最新版」。
 * 所以：
 *   - 所有数字段逐段比较，缺的段补 0：
 *     `0.4` = `0.4.0` = `0.4.0.0`，而 `1.8.7.23` > `1.8.7.21` > `1.8.7`；
 *   - 带 `-` 后缀的是预发布，低于同号正式版（`0.4.0-beta < 0.4.0`）；
 *   - 预发布标识逐段比较，数字段低于字母段
 *     （`alpha < alpha.1 < beta`）。
 */

/// 多段版本号：任意段数字 + 可选预发布标识，按上述规则排序。
#[derive(Debug, Clone, PartialEq, Eq)]
struct SegmentVersion {
    numbers: Vec<u64>,
    prerelease: Vec<String>,
}

/// 把版本号尾部的「非数字部分」归一成预发布标识。
///
/// 本项目实际出现过四种写法，语义完全一致，都必须识别成 beta：
/// ```text
/// 1.8.5-pre.01      → [pre, 01]
/// 1.9.7-beta.4      → [beta, 4]
/// 1.8.7-beta1       → [beta1]
/// 1.8.7.beta4       → [beta4]
/// ```
///
/// 调用方已用贪婪数字正则吃掉了所有前导数字段（`1.8.7.23` 的 `23` 属于
/// 数字段，不在 rest 里），所以这里剩下的内容整体都是标识符 ——
/// 其中的纯数字段（`-beta.4` 的 `4`）是 semver 的预发布序号，必须保留。
///
/// 空结果表示正式版。
fn suffix_identifiers(rest: &str) -> Vec<String> {
    rest.trim_start_matches(['-', '+', '.'])
        .split(['-', '+', '.'])
        .filter(|part| !part.is_empty())
        .flat_map(split_alpha_numeric)
        .collect()
}

/// 把 `beta1` 这类「字母 + 数字」的标识符拆成两段（`beta`、`1`）。
///
/// 语义上等同于 `beta.1`，让序号按数值比较。
///
/// 为什么必须拆：严格按 semver，`-beta1` 是**单个**字母数字标识符，
/// 按 ASCII 字典序比较，于是 `beta10 < beta9`（`'1' < '9'`）——
/// 发到第 10 个测试版时更新检测会静默失效。规范自己举的例子是
/// `-beta.2 < -beta.11`，就是因为点号分隔才能按数值比较。
///
/// 本项目实际用 `-beta1` 这种紧凑写法，所以在解析层统一拆开，
/// 让两种写法得到同一结论。这是对规范的**有意放宽**，不是误读。
fn split_alpha_numeric(part: &str) -> Vec<String> {
    let split_at = part.find(|c: char| c.is_ascii_digit());
    let Some(index) = split_at else {
        return vec![part.to_owned()];
    };
    let (head, tail) = part.split_at(index);
    let valid = !head.is_empty()
        && head.chars().all(|c| c.is_ascii_alphabetic())
        && tail.chars().all(|c| c.is_ascii_digit());
    if !valid {
        return vec![part.to_owned()];
    }
    // 去掉前导零：semver 禁止它们，但真实仓库出现过 `pre.01`
    let serial = tail.trim_start_matches('0');
    vec![
        head.to_owned(),
        if serial.is_empty() { "0" } else { serial }.to_owned(),
    ]
}

impl SegmentVersion {
    fn parse(value: &str) -> Option<Self> {
        let cleaned = value.trim().trim_start_matches(['v', 'V']);
        /*
         * 先切出「前导数字段」和「其余部分」。
         *
         * 用 `^(\d+(\.\d+)*)` 而不是找第一个 `-`：本项目的 beta 写法不统一，
         * 除了标准 semver 的 `-beta.4`，还有 `1.8.7-beta1`（无分隔符）
         * 和 `1.8.7.beta4`（点号分隔）—— 后者用旧的 `-` 正则会把整个号
         * 解析失败，于是它既不参与比较也判不出预发布。
         */
        let re = Regex::new(r"^(\d+(?:\.\d+)*)(.*)$").ok()?;
        let caps = re.captures(cleaned)?;
        let parts = caps
            .get(1)?
            .as_str()
            .split('.')
            .map(str::parse::<u64>)
            .collect::<Result<Vec<_>, _>>()
            .ok()?;
        let numbers = parts.iter().take(3).copied().collect::<Vec<_>>();
        let suffix = suffix_identifiers(caps.get(2).map_or("", |value| value.as_str()));
        /*
         * 第 4 段及以后是前三位的 beta 序号：`1.8.7.23` ≡ `1.8.7-beta.23`。
         * 全 0 视为正式版（1.8.7.0 与 1.8.7 相同）；有非 0 段则拼进预发布标识，
         * 数字段参与 semver 的预发布比较，所以 beta 23 > beta 21。
         */
        let build = parts.iter().skip(3).map(u64::to_string).collect::<Vec<_>>();
        let has_build = build.iter().any(|part| part != "0");
        let prerelease = if !suffix.is_empty() {
            let mut merged = suffix;
            if has_build {
                merged.extend(build);
            }
            merged
        } else if has_build {
            let mut merged = vec!["beta".to_string()];
            merged.extend(build);
            merged
        } else {
            Vec::new()
        };
        Some(Self {
            numbers,
            prerelease,
        })
    }
}

impl Ord for SegmentVersion {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        // 三段比较，缺的段按 0：0.4 == 0.4.0
        let length = self.numbers.len().max(other.numbers.len());
        for index in 0..length {
            let left = self.numbers.get(index).copied().unwrap_or(0);
            let right = other.numbers.get(index).copied().unwrap_or(0);
            match left.cmp(&right) {
                Ordering::Equal => continue,
                ordering => return ordering,
            }
        }
        // 数字段相同：正式版高于预发布；两个预发布按标识符逐段比较
        // （第 4 段已并入预发布，所以 beta.23 > beta.21 在这里判定）
        match (self.prerelease.is_empty(), other.prerelease.is_empty()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Greater,
            (false, true) => Ordering::Less,
            (false, false) => compare_prerelease_parts(&self.prerelease, &other.prerelease),
        }
    }
}

impl PartialOrd for SegmentVersion {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

fn compare_prerelease_parts(left: &[String], right: &[String]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let count = left.len().max(right.len());
    for index in 0..count {
        match (left.get(index), right.get(index)) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a), Some(b)) => {
                let ordering = match (a.parse::<u64>(), b.parse::<u64>()) {
                    (Ok(x), Ok(y)) => x.cmp(&y),
                    // 数字标识符低于字母标识符
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => a.cmp(b),
                };
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
        }
    }
    Ordering::Equal
}

/// 比较两个 Release 标签/版本号，用于判断 MOD 是否有更新。
///
/// 全部数字段参与比较 + 标准 semver 的预发布语义。
/// 任一无法解析时返回 `None`，调用方应显示「版本未知」而不是臆断更新。
pub fn compare_release_versions(remote: &str, local: &str) -> Option<std::cmp::Ordering> {
    Some(SegmentVersion::parse(remote)?.cmp(&SegmentVersion::parse(local)?))
}

/// 是否符合 semver 2.0.0 的三字段语法（`x.y.z[-tag.w][+build]`）。
///
/// **这是发布契约，不是解析门槛。** 解析器（`SegmentVersion::parse`）继续
/// 宽容接受历史写法（四段序号、`v0.10` 这类两段号、点号分隔的 `.beta4`），
/// 因为已经发出去的 tag 不会改变，拒绝它们只会让老用户看到「版本未知」。
/// 这个函数回答的是「新发布是否合规」：
///
/// ```text
/// 1.9.8              ✅
/// 1.9.8-beta.1       ✅   首选：beta / alpha / rc 统一写成 x.y.z-tag.w
/// 1.9.8-rc.2         ✅
/// 1.9.8-beta1        ⚠️   语法合法（单个字母数字标识符），但不推荐
/// 1.9.8.1            ❌   四段是历史写法，semver 只允许三段
/// 1.9.8.beta.1       ❌   预发布必须跟在 `-` 后
/// v0.10              ❌   必须三段
/// 01.9.8             ❌   数字段不许前导零
/// ```
///
/// `-beta1` 为什么标 ⚠️ 而不是 ❌：它在语法上完全合法，但作为**单个**
/// 标识符要按 ASCII 字典序比较（`beta9 > beta10`）。本项目在解析层把
/// 「字母+数字」拆开规避了这个问题，但新发布统一用 `-beta.1` 更清楚，
/// 也与规范 §11 自己的示例一致（`1.0.0-beta.2 < 1.0.0-beta.11`）。
///
/// 三条硬性规则：
///   1. 恰好三段，每段无前导零；
///   2. 预发布用 `-` 分隔，标识符逐段点号分隔，数字段无前导零；
///   3. 标识符只含 `[0-9A-Za-z-]`。
pub fn is_strict_semver(value: &str) -> bool {
    let trimmed = value.trim();
    // 允许常见的 `v` 前缀（GitHub/Gitee tag 惯例），但版本本身仍须严格
    let text = trimmed.strip_prefix(['v', 'V']).unwrap_or(trimmed);
    // 构建元数据（+）不参与排序，格式仍要合法
    let (without_build, build) = match text.split_once('+') {
        Some((core, build)) => (core, Some(build)),
        None => (text, None),
    };
    if let Some(build) = build {
        if !is_valid_identifier_sequence(build) {
            return false;
        }
    }
    let (core, prerelease) = match without_build.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (without_build, None),
    };
    // 恰好三段
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 || parts.iter().any(|part| !is_valid_numeric_segment(part)) {
        return false;
    }
    // 预发布：至少一段合法标识符
    match prerelease {
        None => true,
        Some(pre) => {
            !pre.is_empty()
                && pre
                    .split('.')
                    .all(|part| !part.is_empty() && is_valid_identifier(part))
        }
    }
}

/// 数字段：纯数字且无前导零（`0` 本身允许）。
fn is_valid_numeric_segment(value: &str) -> bool {
    !value.is_empty()
        && value.chars().all(|c| c.is_ascii_digit())
        && (value == "0" || !value.starts_with('0'))
}

/// 预发布标识符：纯数字时同样不许前导零；含字母时只允许字母数字与连字符。
fn is_valid_identifier(value: &str) -> bool {
    let numeric = value.chars().all(|c| c.is_ascii_digit());
    if numeric {
        return is_valid_numeric_segment(value);
    }
    value.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn is_valid_identifier_sequence(value: &str) -> bool {
    !value.is_empty()
        && value
            .split('.')
            .all(|part| !part.is_empty() && is_valid_identifier(part))
}

/// 版本号本身是否带预发布标识（`-beta`、`-rc.1` 等）。
///
/// 与 Release 元数据里的 `prerelease` 标记是**或**的关系：作者没勾标记，
/// 但版本叫 `0.4.0-beta`、`0.4.0.5-beta` 时同样按预发布对待。
///
/// 四段版本按 semver 的 beta 序号处理：`1.8.7.23` ≡ `1.8.7-beta.23`，
/// 因此**是预发布**。这条规则必须全局一致，否则同一版本在比较、
/// 渠道筛选和界面提示里会得到三种答案。
pub fn is_prerelease_version(version: &str) -> bool {
    SegmentVersion::parse(version).is_some_and(|parsed| !parsed.prerelease.is_empty())
}

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

pub fn asset_matches(asset: &ReleaseAsset, rules: &[AssetRule]) -> bool {
    rules.iter().any(|rule| match rule.rule_type {
        AssetRuleType::Exact => asset.name.eq_ignore_ascii_case(&rule.value),
        AssetRuleType::Glob => GlobBuilder::new(&rule.value)
            .case_insensitive(true)
            .build()
            .ok()
            .map(|glob| glob.compile_matcher().is_match(&asset.name))
            .unwrap_or(false),
        AssetRuleType::Regex => Regex::new(&rule.value)
            .ok()
            .map(|regex| regex.is_match(&asset.name))
            .unwrap_or(false),
    })
}

pub fn select_install_asset<'a>(
    repo: &RepositoryConfig,
    release: &'a ReleaseInfo,
    asset_name: Option<&str>,
) -> Option<&'a ReleaseAsset> {
    release.assets.iter().find(|asset| {
        !crate::community::is_source_archive(repo, &release.tag_name, &asset.name)
            && asset_name
                .map(|name| asset.name == name)
                .unwrap_or_else(|| asset_matches(asset, &repo.asset_rules))
    })
}

/// 一个 Release 资产 URL 拆出来的仓库坐标。
///
/// 地图源允许把整份 TOML 放进 Release 资产
/// （`.../releases/download/maps/map-sources.toml`）。这个地址既不是 raw 也没有
/// 分支路径，`SourceLocation` 的相对解析救不了它（重定向后是 CDN 签名地址），
/// 所以必须先认出来，再按 `(provider, username, repository)` 查 API 拿 size 与
/// sha256 —— 校验信息只有 API 有。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseAssetReference {
    pub provider: Provider,
    pub username: String,
    pub repository: String,
    pub tag: String,
    pub asset: String,
}

/// 识别 `/{owner}/{repo}/releases/download/{tag}/{file…}` 形态的资产地址。
///
/// 只认 github.com 与 gitee.com 两个宿主：代理地址的宿主是第三方域名，路径
/// 也可能是任意重写，从代理 URL 反推仓库等于猜。原始地址已经够用。
pub fn release_asset_reference(url: &Url) -> Option<ReleaseAssetReference> {
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    let provider = if host == "github.com" || host == "www.github.com" {
        Provider::Github
    } else if host == "gitee.com" || host == "www.gitee.com" {
        Provider::Gitee
    } else {
        return None;
    };
    let segments = url
        .path_segments()?
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if segments.len() < 6 || segments[2] != "releases" || segments[3] != "download" {
        return None;
    }
    Some(ReleaseAssetReference {
        provider,
        username: segments[0].to_string(),
        repository: segments[1].to_string(),
        tag: segments[4].to_string(),
        asset: segments[5..].join("/"),
    })
}

/// Release 列表缓存的键：同一个仓库只该请求一次 API。
///
/// 小写归一：源文档里大小写随手写是常态（`CH3NGYZ` vs `ch3ngyz`），API 对
/// 大小写不敏感，缓存却敏感 —— 不归一会让「同一个仓库」被算成两个，正好毁掉
/// 这个缓存存在的意义。
pub fn repository_cache_key(provider: Provider, username: &str, repository: &str) -> String {
    format!(
        "{:?}:{}:{}",
        provider,
        username.trim().to_lowercase(),
        repository.trim().to_lowercase()
    )
}

pub async fn fetch_releases(
    client: &Client,
    repo: &RepositoryConfig,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let per_page = repo.max_release_count.clamp(1, 100);
    fetch_releases_page(
        client,
        repo,
        per_page,
        None,
        token,
        official_api_only,
        api_proxies,
    )
    .await
}

/// 分页读取一个仓库的全部 Release。
///
/// 只有「一个仓库多张地图」的源需要它：那种源的地图数量与 Release 数量同阶，
/// 只读第 1 页会漏掉排在后面的地图。MOD 与普通地图仍走 `fetch_releases`
/// （只取第 1 页），不因为这条新路径变成每次都翻几十页。
///
/// 停止条件三条，缺一不可：
/// 1. 返回不足一页 —— GitHub/Gitee 都用「不满即最后一页」；
/// 2. 翻到 `max_pages` 上限 —— 防止一次刷新把几千个 Release 全拉下来；
/// 3. 中途失败 —— 已经取到的部分照常返回：前几页的地图不会因为第 N 页
///    限流而整块消失（首屏换一次 warn 日志）。
pub async fn fetch_all_releases(
    client: &Client,
    repo: &RepositoryConfig,
    per_page: u32,
    max_pages: u32,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let per_page = per_page.clamp(1, 100);
    let max_pages = max_pages.clamp(1, 100);
    let mut all = Vec::new();
    for index in 0..max_pages {
        let page = index + 1;
        let requested = (page > 1).then_some(page);
        let page_releases = match fetch_releases_page(
            client,
            repo,
            per_page,
            requested,
            token,
            official_api_only,
            api_proxies,
        )
        .await
        {
            Ok(releases) => releases,
            Err(error) => {
                if all.is_empty() {
                    return Err(error);
                }
                log::warn!(
                    "分页读取 Release 中途失败，保留已取到的部分: repository_id={} page={} error_kind={} got={}",
                    repo.id,
                    page,
                    release_error_kind(&error),
                    all.len()
                );
                return Ok(all);
            }
        };
        let received = page_releases.len();
        all.extend(page_releases);
        if received < per_page as usize {
            break;
        }
    }
    Ok(all)
}

async fn fetch_releases_page(
    client: &Client,
    repo: &RepositoryConfig,
    per_page: u32,
    page: Option<u32>,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let per_page = per_page.clamp(1, 100);
    let official = match repository_release_api_url_page(
        &repo.provider,
        &repo.username,
        &repo.repository,
        per_page,
        page,
    ) {
        Ok(url) => url,
        Err(error) => {
            log::warn!(
                "Release API 仓库地址校验失败: repository_id={} provider={:?} error_kind={}",
                repo.id,
                repo.provider,
                release_error_kind(&error)
            );
            return Err(error);
        }
    };
    let request_targets = api_request_targets(&official, token, official_api_only, api_proxies);
    log::debug!(
        "开始获取 Release: repository_id={} provider={:?} channel={:?} per_page={} page={} token_configured={} official_api_only={} api_target_count={}",
        repo.id,
        repo.provider,
        repo.channel,
        per_page,
        page.unwrap_or(1),
        token.is_some(),
        official_api_only,
        request_targets.len()
    );
    fetch_releases_from_targets(client, repo, &official, request_targets).await
}

async fn fetch_releases_from_targets(
    client: &Client,
    repo: &RepositoryConfig,
    official: &str,
    request_targets: Vec<(String, Option<&str>)>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let mut last_error = None;
    let mut skip_gitee_official = false;
    let gate = crate::api_rate_gate::global_gate();
    for (index, (url, request_token)) in request_targets.into_iter().enumerate() {
        if skip_gitee_official && url == official {
            continue;
        }
        // 官方地址才受限流闸门约束；代理请求不消耗官方配额
        let gate_kind = if url == official {
            if request_token.is_some() {
                crate::api_rate_gate::GateKind::Token
            } else {
                crate::api_rate_gate::GateKind::Anonymous
            }
        } else {
            crate::api_rate_gate::GateKind::Proxy
        };
        if let Err(wait) = gate.check(&repo.provider, gate_kind) {
            log::warn!(
                "Release API 被限流闸门跳过: repository_id={} provider={:?} target_kind={} 剩余等待={}秒",
                repo.id,
                repo.provider,
                api_target_kind(official, &url, request_token),
                wait.as_secs()
            );
            last_error = Some(ReleaseError::Http("source.apiRateLimited".into()));
            continue;
        }
        let target_kind = api_target_kind(official, &url, request_token);
        log::debug!(
            "尝试 Release API: repository_id={} provider={:?} target_kind={} target_index={} token_attached={}",
            repo.id,
            repo.provider,
            target_kind,
            index + 1,
            request_token.is_some()
        );
        match fetch_release_api_with_retries(client, &url, &repo.provider, request_token).await {
            Ok(mut releases) => {
                gate.record_success(&repo.provider, gate_kind);
                for release in &mut releases {
                    release.assets.retain(|asset| {
                        !crate::community::is_source_archive(repo, &release.tag_name, &asset.name)
                    });
                }
                log::info!(
                    "Release API 获取成功: repository_id={} provider={:?} target_kind={} release_count={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    releases.len()
                );
                return Ok(releases);
            }
            Err(error) => {
                // 命中限流特征 → 给该类凭据上锁，后续同类官方请求改走代理
                if let Some(signal) = error.rate_limit_signal() {
                    if let Some(block) = gate.record_forbidden(&repo.provider, gate_kind, signal) {
                        log::warn!(
                            "Release API 触发限流锁: provider={:?} target_kind={} 锁定={}秒",
                            repo.provider,
                            target_kind,
                            block.as_secs()
                        );
                    }
                }
                log::warn!(
                    "Release API 目标失败: repository_id={} provider={:?} target_kind={} error_kind={}",
                    repo.id,
                    repo.provider,
                    target_kind,
                    release_error_kind(&error)
                );
                if repo.provider == Provider::Gitee && error.is_forbidden() {
                    if url != official {
                        log::warn!(
                            "Gitee API 代理返回 403，结束刷新: repository_id={}",
                            repo.id
                        );
                        return Err(error);
                    }
                    log::debug!(
                        "Gitee 官方 API 返回 403，跳过官方 Token 目标并转 API 代理: repository_id={}",
                        repo.id
                    );
                    skip_gitee_official = true;
                }
                last_error = Some(error);
            }
        }
    }
    let error = last_error.expect("release API request targets must not be empty");
    log::error!(
        "Release API 所有目标均失败: repository_id={} provider={:?} error_kind={}",
        repo.id,
        repo.provider,
        release_error_kind(&error)
    );
    Err(error)
}

/// Build the ordered API fallback sequence for one provider.
///
/// 配了 Token 就直奔官方 + Token：Token 有独立配额，没必要先用匿名额度把
/// 匿名限流额度耗掉（用户明确要求「有 Token 就别走匿名」）。
/// 只有没有 Token 时才走官方匿名；两条路径失败后都落到配置的代理端点。
/// Credentials are never sent to a proxy endpoint.
fn api_request_targets<'a>(
    official_url: &str,
    token: Option<&'a str>,
    _official_api_only: bool,
    api_proxies: &[GitHubProxy],
) -> Vec<(String, Option<&'a str>)> {
    let mut targets = Vec::new();
    match token {
        Some(token) => targets.push((official_url.to_string(), Some(token))),
        None => targets.push((official_url.to_string(), None)),
    }
    targets.extend(
        crate::github_proxies::proxy_urls(api_proxies, official_url, false, None)
            .into_iter()
            .map(|url| (url, None)),
    );
    targets
}

async fn fetch_release_api_with_retries(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    fetch_api_with_retries(client, url, provider, token, |value| {
        parse_release_json(value, provider)
    })
    .await
}

async fn fetch_api_with_retries<T>(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
    parse: impl Fn(&Value) -> Result<T, ReleaseError>,
) -> Result<T, ReleaseError> {
    let mut last_error = None;
    for attempt in 0..REPOSITORY_API_ATTEMPTS {
        log::debug!(
            "Release API 请求: provider={:?} attempt={}/{} token_attached={}",
            provider,
            attempt + 1,
            REPOSITORY_API_ATTEMPTS,
            token.is_some()
        );
        match fetch_api_json(client, url, provider, token)
            .await
            .and_then(|value| parse(&value))
        {
            Ok(result) => {
                log::debug!(
                    "API 请求成功: provider={:?} attempt={}",
                    provider,
                    attempt + 1
                );
                return Ok(result);
            }
            Err(error) if error.is_forbidden() => {
                log::warn!(
                    "Release API 返回 403，停止当前目标重试: provider={:?} attempt={}",
                    provider,
                    attempt + 1
                );
                return Err(error);
            }
            Err(error) => {
                log::debug!(
                    "Release API 请求失败: provider={:?} attempt={} error_kind={}",
                    provider,
                    attempt + 1,
                    release_error_kind(&error)
                );
                last_error = Some(error);
                if attempt + 1 < REPOSITORY_API_ATTEMPTS {
                    log::debug!(
                        "Release API 将重试: provider={:?} next_attempt={}/{}",
                        provider,
                        attempt + 2,
                        REPOSITORY_API_ATTEMPTS
                    );
                    continue;
                }
            }
        }
    }
    let error = last_error.expect("repository API attempts must not be empty");
    log::warn!(
        "Release API 重试耗尽: provider={:?} attempts={} error_kind={}",
        provider,
        REPOSITORY_API_ATTEMPTS,
        release_error_kind(&error)
    );
    Err(error)
}

#[cfg(test)]
pub async fn fetch_release_api(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Vec<ReleaseInfo>, ReleaseError> {
    let value = fetch_api_json(client, url, provider, token).await?;
    parse_release_json(&value, provider)
}

async fn fetch_api_json(
    client: &Client,
    url: &str,
    provider: &Provider,
    token: Option<&str>,
) -> Result<Value, ReleaseError> {
    log::trace!(
        "发送 Release API 请求: provider={:?} token_attached={}",
        provider,
        token.is_some()
    );
    // Gitee 的 access_token 走 query 参数（官方约定）；GitHub 走 Authorization
    let request_url = match (provider, token) {
        (Provider::Gitee, Some(token)) if is_official_gitee_api_url(url) => {
            gitee_api_url_with_token(url, token)?
        }
        _ => url.to_string(),
    };
    let mut request = client
        .get(&request_url)
        .timeout(API_REQUEST_TIMEOUT)
        .header("User-Agent", APP_USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(
            reqwest::header::CACHE_CONTROL,
            "no-cache, no-store, max-age=0",
        )
        .header(reqwest::header::PRAGMA, "no-cache");
    if let Some(token) = token {
        request = match provider {
            Provider::Github => request.bearer_auth(token),
            Provider::Gitee => request,
        };
    }
    let response = request.send().await.map_err(|e| {
        let error = ReleaseError::Http(e.without_url().to_string());
        log::debug!(
            "Release API 网络请求异常: provider={:?} error_kind={}",
            provider,
            release_error_kind(&error)
        );
        error
    })?;
    log::trace!(
        "收到 Release API 响应: provider={:?} status={}",
        provider,
        response.status()
    );
    let status = response.status();
    if !status.is_success() {
        // 403/429 可能是限流：把响应头与正文交给闸门判定，并上报给调用方
        if status == StatusCode::FORBIDDEN || status == StatusCode::TOO_MANY_REQUESTS {
            let signal = crate::api_rate_gate::RateLimitSignal {
                body_limit_message: false, // 由调用方在读取正文后补充
                remaining_exhausted: response
                    .headers()
                    .get("x-ratelimit-remaining")
                    .and_then(|value| value.to_str().ok())
                    .is_some_and(|value| value.trim() == "0"),
                reset_at: response
                    .headers()
                    .get("x-ratelimit-reset")
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok()),
                retry_after: response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok()),
            };
            let body = response.text().await.unwrap_or_default();
            let signal = crate::api_rate_gate::RateLimitSignal {
                body_limit_message: crate::api_rate_gate::body_reports_rate_limit(&body),
                ..signal
            };
            return Err(ReleaseError::RateLimited {
                status,
                signal: Box::new(signal),
            });
        }
        return Err(ReleaseError::Status(status));
    }
    let value = response.json::<Value>().await.map_err(|e| {
        let error = ReleaseError::Json(e.without_url().to_string());
        log::warn!(
            "Release API JSON 解析失败: provider={:?} error_kind={}",
            provider,
            release_error_kind(&error)
        );
        error
    })?;
    Ok(value)
}

pub async fn fetch_repository_default_branch(
    client: &Client,
    repo: &RepositoryConfig,
    token: Option<&str>,
    api_proxies: &[GitHubProxy],
) -> Result<String, ReleaseError> {
    let mut url = Url::parse(&repository_release_api_url(
        &repo.provider,
        &repo.username,
        &repo.repository,
        1,
    )?)
    .map_err(|error| ReleaseError::InvalidUrl(error.to_string()))?;
    url.path_segments_mut()
        .expect("validated repository API URL")
        .pop();
    url.set_query(None);
    let official = url.to_string();
    let mut last_error = None;
    let mut skip_official = false;
    for (url, request_token) in api_request_targets(&official, token, false, api_proxies) {
        if skip_official && url == official {
            continue;
        }
        log::info!(
            "正在读取仓库默认分支：仓库={}，渠道={}",
            repo.id,
            api_target_kind(&official, &url, request_token)
        );
        let fetched =
            fetch_api_with_retries(client, &url, &repo.provider, request_token, |value| {
                let branch = value
                    .get("default_branch")
                    .and_then(Value::as_str)
                    .filter(|branch| !branch.trim().is_empty())
                    .ok_or_else(|| ReleaseError::Json("source.defaultBranchMissing".into()))?;
                repository_file_url(
                    &repo.provider,
                    &repo.username,
                    &repo.repository,
                    branch,
                    "README.md",
                )?;
                Ok(branch.to_string())
            })
            .await;
        match fetched {
            Ok(branch) => return Ok(branch),
            Err(error) => {
                if repo.provider == Provider::Gitee && error.is_forbidden() {
                    if url != official {
                        return Err(error);
                    }
                    skip_official = true;
                }
                last_error = Some(error);
            }
        }
    }
    Err(last_error.expect("repository API always has an official target"))
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

pub fn build_client(timeout_seconds: u64) -> Result<Client, ReleaseError> {
    /*
     * 参数**必须**生效。
     *
     * 这里曾经把 `timeout_seconds` 只写进日志、实际用硬编码 20 秒，
     * 于是调用方以为自己在配置超时，改配置却毫无效果 —— 仓库源想放宽到
     * 30 秒也放不出去。`max(1)` 防的是 0：reqwest 里 0 表示「永不超时」，
     * 那会在网络挂起时把刷新卡死。
     *
     * 注意：Release API 另有无条件覆盖的 `API_REQUEST_TIMEOUT`（5 秒，
     * 见 `fetch_json` 的 `.timeout(...)`），本函数的超时只对**没有**单独
     * 设置超时的请求（例如仓库源文档）生效。
     */
    let timeout = std::time::Duration::from_secs(timeout_seconds.max(1));
    Client::builder()
        .timeout(timeout)
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| {
            log::error!(
                "构建 Release API 客户端失败: requested_timeout_seconds={} error_kind=client_build",
                timeout_seconds
            );
            ReleaseError::Http(e.to_string())
        })
}

/// Builds a client for potentially large release assets. The total request
/// timeout is disabled so a download is allowed to run as long as data keeps
/// arriving. The read timeout still aborts a connection that has gone idle.
pub fn build_download_client(timeout_seconds: u64) -> Result<Client, ReleaseError> {
    let connect_timeout = std::time::Duration::from_secs(timeout_seconds.max(1));
    Client::builder()
        .connect_timeout(connect_timeout)
        .read_timeout(std::time::Duration::from_secs(120))
        .user_agent(APP_USER_AGENT)
        .build()
        .map_err(|e| {
            log::error!(
                "构建 Release 下载客户端失败: requested_timeout_seconds={} error_kind=client_build",
                timeout_seconds
            );
            ReleaseError::Http(e.to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn failed_api_requests_do_not_expose_query_credentials() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let client = Client::builder().no_proxy().build().unwrap();
        let url = format!("http://{address}/api?access_token=placeholder-credential");
        let error = fetch_release_api(&client, &url, &Provider::Gitee, None)
            .await
            .unwrap_err()
            .to_string();
        assert!(!error.contains("access_token"));
        assert!(!error.contains("placeholder-credential"));
        assert!(!error.contains(&address.to_string()));
    }
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };

    struct ApiTestServer {
        url: String,
        requests: Arc<Mutex<Vec<String>>>,
        stopped: Arc<AtomicBool>,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl ApiTestServer {
        fn new(responses: Vec<(&'static str, &'static str)>) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            listener.set_nonblocking(true).unwrap();
            let requests = Arc::new(Mutex::new(Vec::new()));
            let stopped = Arc::new(AtomicBool::new(false));
            let server_requests = requests.clone();
            let server_stopped = stopped.clone();
            let worker = std::thread::spawn(move || {
                let mut index = 0;
                while !server_stopped.load(Ordering::Relaxed) {
                    let (mut stream, _) = match listener.accept() {
                        Ok(connection) => connection,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                            continue;
                        }
                        Err(error) => panic!("test server accept failed: {error}"),
                    };
                    // Windows accepted sockets inherit the listener's mode.
                    stream.set_nonblocking(false).unwrap();
                    stream.set_read_timeout(Some(API_REQUEST_TIMEOUT)).unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0; 1024];
                    while !request.windows(4).any(|part| part == b"\r\n\r\n") {
                        let count = stream.read(&mut buffer).unwrap();
                        if count == 0 {
                            break;
                        }
                        request.extend_from_slice(&buffer[..count]);
                    }
                    server_requests
                        .lock()
                        .unwrap()
                        .push(String::from_utf8(request).unwrap());
                    // Unexpected extra requests also get a response, so a failed
                    // assertion reports the actual count instead of hanging.
                    let (status, body) = responses
                        .get(index)
                        .copied()
                        .unwrap_or(("500 Internal Server Error", ""));
                    index += 1;
                    write!(stream, "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
                }
            });
            Self {
                url: format!("http://{address}/releases"),
                requests,
                stopped,
                worker: Some(worker),
            }
        }

        fn requests(&self) -> Vec<String> {
            self.requests.lock().unwrap().clone()
        }
    }

    impl Drop for ApiTestServer {
        fn drop(&mut self) {
            self.stopped.store(true, Ordering::Relaxed);
            self.worker.take().unwrap().join().unwrap();
        }
    }

    fn api_test_client() -> Client {
        Client::builder().no_proxy().build().unwrap()
    }

    fn api_test_repository() -> RepositoryConfig {
        serde_json::from_value(serde_json::json!({
            "id": "retry-test", "name": "Retry test", "provider": "gitee",
            "username": "owner", "repository": "repo", "install_target": "BepInEx/plugins",
            "enabled": true, "builtin": false
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn release_api_non_403_failures_try_three_times() {
        for status in [
            "404 Not Found",
            "429 Too Many Requests",
            "500 Internal Server Error",
        ] {
            let server = ApiTestServer::new(vec![(status, ""); 3]);
            let result = fetch_release_api_with_retries(
                &api_test_client(),
                &server.url,
                &Provider::Gitee,
                None,
            )
            .await;
            assert!(result.is_err());
            assert_eq!(server.requests().len(), 3, "{status}");
        }
    }

    #[tokio::test]
    async fn release_api_403_is_never_retried_even_after_an_ordinary_failure() {
        let server = ApiTestServer::new(vec![
            ("500 Internal Server Error", ""),
            ("403 Forbidden", ""),
        ]);
        let error =
            fetch_release_api_with_retries(&api_test_client(), &server.url, &Provider::Gitee, None)
                .await
                .unwrap_err();
        assert!(error.is_forbidden());
        assert_eq!(server.requests().len(), 2);
    }

    #[tokio::test]
    async fn release_api_success_stops_retries_and_remaining_targets() {
        let official = ApiTestServer::new(vec![("200 OK", "[]")]);
        let proxy = ApiTestServer::new(vec![("200 OK", "[]")]);
        let targets = vec![
            (official.url.clone(), None),
            (official.url.clone(), Some("test-token")),
            (proxy.url.clone(), None),
        ];
        assert!(
            fetch_releases_from_targets(
                &api_test_client(),
                &api_test_repository(),
                &official.url,
                targets
            )
            .await
            .is_ok()
        );
        assert_eq!(official.requests().len(), 1);
        assert!(proxy.requests().is_empty());
    }

    #[tokio::test]
    async fn gitee_official_403_skips_token_and_proxy_success_stops_fallback() {
        let official = ApiTestServer::new(vec![("403 Forbidden", "")]);
        let proxy = ApiTestServer::new(vec![("500 Internal Server Error", ""), ("200 OK", "[]")]);
        let remaining = ApiTestServer::new(vec![("200 OK", "[]")]);
        let targets = vec![
            (official.url.clone(), None),
            (official.url.clone(), Some("test-token")),
            (proxy.url.clone(), None),
            (remaining.url.clone(), None),
        ];
        assert!(
            fetch_releases_from_targets(
                &api_test_client(),
                &api_test_repository(),
                &official.url,
                targets
            )
            .await
            .is_ok()
        );
        assert_eq!(official.requests().len(), 1);
        assert_eq!(proxy.requests().len(), 2);
        assert!(remaining.requests().is_empty());
        assert!(proxy.requests().iter().all(|request| {
            !request.to_ascii_lowercase().contains("authorization:")
                && !request.contains("test-token")
        }));
    }

    #[tokio::test]
    async fn gitee_proxy_403_ends_the_refresh_immediately() {
        let official = ApiTestServer::new(vec![("403 Forbidden", "")]);
        let proxy = ApiTestServer::new(vec![("403 Forbidden", "")]);
        let remaining = ApiTestServer::new(vec![("200 OK", "[]")]);
        let targets = vec![
            (official.url.clone(), None),
            (proxy.url.clone(), None),
            (remaining.url.clone(), None),
        ];
        let error = fetch_releases_from_targets(
            &api_test_client(),
            &api_test_repository(),
            &official.url,
            targets,
        )
        .await
        .unwrap_err();
        assert!(error.is_forbidden());
        assert_eq!(official.requests().len(), 1);
        assert_eq!(proxy.requests().len(), 1);
        assert!(remaining.requests().is_empty());
    }

    #[tokio::test]
    async fn gitee_official_and_proxy_other_errors_exhaust_three_attempts_each() {
        let official = ApiTestServer::new(vec![("500 Internal Server Error", ""); 3]);
        let proxy = ApiTestServer::new(vec![("502 Bad Gateway", ""); 3]);
        let targets = vec![(official.url.clone(), None), (proxy.url.clone(), None)];
        assert!(
            fetch_releases_from_targets(
                &api_test_client(),
                &api_test_repository(),
                &official.url,
                targets
            )
            .await
            .is_err()
        );
        assert_eq!(official.requests().len(), 3);
        assert_eq!(proxy.requests().len(), 3);
    }

    #[test]
    fn cached_tag_source_archives_cannot_be_installed_even_when_explicitly_selected() {
        let mut repo =
            crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
                .unwrap();
        let cases: Vec<(String, String, bool)> = serde_json::from_str(include_str!(
            "../../tests/fixtures/gitee-source-assets.json"
        ))
        .unwrap();
        for (tag, name, allowed) in cases {
            let mut info = parse_release_json(&serde_json::json!([{"tag_name": "placeholder", "assets": [{"name": name, "browser_download_url": "https://example.com/file"}]}]), &Provider::Gitee).unwrap().remove(0);
            info.tag_name = tag;
            let selected = info.assets[0].name.clone();
            assert_eq!(
                select_install_asset(&repo, &info, None).is_some(),
                allowed,
                "{selected}"
            );
            assert_eq!(
                select_install_asset(&repo, &info, Some(&selected)).is_some(),
                allowed,
                "explicit {selected}"
            );
            let source_id = repo.source_id.take();
            assert!(
                select_install_asset(&repo, &info, Some(&selected)).is_some(),
                "custom Gitee {selected}"
            );
            repo.source_id = source_id;
            repo.provider = Provider::Github;
            assert!(
                select_install_asset(&repo, &info, Some(&selected)).is_some(),
                "GitHub {selected}"
            );
            repo.provider = Provider::Gitee;
        }
    }

    #[test]
    fn gitee_community_source_tag_zip_is_removed_after_real_api_parsing() {
        let repo =
            crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
                .unwrap();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let server = ApiTestServer::new(vec![("200 OK", r#"[{"tag_name":"v1.8.7.15","assets":[
                {"name":"v1.8.7.15.zip","browser_download_url":"https://gitee.com/Owner/Mod/archive/v1.8.7.15.zip"},
                {"name":"Mod.zip","browser_download_url":"https://gitee.com/Owner/Mod/releases/download/v1.8.7.15/Mod.zip"},
                {"name":"Mod.dll","browser_download_url":"https://gitee.com/Owner/Mod/releases/download/v1.8.7.15/Mod.dll"}
            ]}]"#)]);
            let releases = fetch_releases_from_targets(&api_test_client(), &repo, &server.url,
                vec![(server.url.clone(), None)]).await.unwrap();
            assert_eq!(releases[0].assets.iter().map(|asset| asset.name.as_str()).collect::<Vec<_>>(), ["Mod.zip", "Mod.dll"]);
            assert_eq!(server.requests().len(), 1);
        });
    }

    #[test]
    fn repository_homepage_is_built_from_provider_username_and_repository() {
        assert_eq!(
            repository_homepage(&Provider::Github, "gua248", "ManyRecipes").unwrap(),
            "https://github.com/gua248/ManyRecipes"
        );
        assert_eq!(
            repository_homepage(&Provider::Gitee, " ch3ngyz ", "repo").unwrap(),
            "https://gitee.com/ch3ngyz/repo"
        );
    }

    #[test]
    fn repository_homepage_rejects_unsafe_path_segments() {
        assert!(repository_homepage(&Provider::Github, "owner/name", "repo").is_err());
        assert!(repository_homepage(&Provider::Github, "owner", "repo name").is_err());
        assert!(repository_homepage(&Provider::Github, "", "repo").is_err());
        for username in [
            "../owner",
            "https://github.com/owner",
            "owner?token=value",
            "owner#fragment",
            "owner\\repo",
            "..",
        ] {
            assert!(repository_homepage(&Provider::Github, username, "repo").is_err());
            assert!(repository_release_api_url(&Provider::Github, username, "repo", 10).is_err());
        }
    }

    #[test]
    fn release_api_urls_are_assembled_from_structured_repository_fields() {
        assert_eq!(
            repository_release_api_url(&Provider::Github, "sinai-dev", "UnityExplorer", 10)
                .unwrap(),
            "https://api.github.com/repos/sinai-dev/UnityExplorer/releases?per_page=10&direction=desc&sort=created"
        );
        assert_eq!(
            repository_release_api_url(
                &Provider::Gitee,
                "ch3ngyz",
                "Overcooked-2-ChefRotation-Releases",
                0
            )
            .unwrap(),
            "https://gitee.com/api/v5/repos/ch3ngyz/Overcooked-2-ChefRotation-Releases/releases?per_page=1&direction=desc&sort=created"
        );
        assert!(
            repository_release_api_url(&Provider::Github, "user", "repo", 500)
                .unwrap()
                .contains("per_page=100")
        );
    }

    /// 翻页只在第 2 页起追加 `page`：第 1 页的地址被既有断言与代理模板逐字匹配。
    #[test]
    fn paged_release_api_url_only_adds_page_after_the_first() {
        let first =
            repository_release_api_url_page(&Provider::Github, "owner", "repo", 50, None).unwrap();
        assert_eq!(
            first,
            "https://api.github.com/repos/owner/repo/releases?per_page=50&direction=desc&sort=created"
        );
        // page = 1 与不传等价
        assert_eq!(
            repository_release_api_url_page(&Provider::Github, "owner", "repo", 50, Some(1))
                .unwrap(),
            first
        );
        assert_eq!(
            repository_release_api_url_page(&Provider::Github, "owner", "repo", 50, Some(2))
                .unwrap(),
            "https://api.github.com/repos/owner/repo/releases?per_page=50&page=2&direction=desc&sort=created"
        );
        // 每页最多 100：地图源的 page_size 允许配置，越界要被夹住
        assert!(
            repository_release_api_url_page(&Provider::Github, "owner", "repo", 1000, Some(3))
                .unwrap()
                .contains("per_page=100")
        );
    }

    #[test]
    fn repository_file_urls_follow_provider_raw_layout() {
        assert_eq!(
            repository_file_url(
                &Provider::Github,
                "gua248",
                "Overcooked2-ManyRecipes",
                "master",
                "bin/Release/OC2ManyRecipes.dll",
            )
            .unwrap(),
            "https://github.com/gua248/Overcooked2-ManyRecipes/raw/refs/heads/master/bin/Release/OC2ManyRecipes.dll"
        );
        assert_eq!(
            repository_file_url(
                &Provider::Gitee,
                "ch3ngyz",
                "example",
                "main",
                "bin/example.zip",
            )
            .unwrap(),
            "https://gitee.com/ch3ngyz/example/raw/main/bin/example.zip"
        );
    }

    #[test]
    fn repository_file_names_are_decoded_and_path_safe() {
        assert_eq!(
            repository_file_name("bin/My%20MOD.dll").unwrap(),
            "My MOD.dll"
        );
        assert!(repository_file_name("bin/%2Fetc.dll").is_err());
        assert!(
            repository_file_url(&Provider::Github, "gua248", "repo", "main", "../secret.dll")
                .is_err()
        );
    }

    #[test]
    fn prerelease_detection_covers_version_suffixes() {
        // 版本号带 - 后缀即预发布，与 Release 元数据标记是或关系
        assert!(is_prerelease_version("0.4.0-beta"));
        assert!(is_prerelease_version("0.4.0.5-beta"));
        assert!(is_prerelease_version("v1.0.0-rc.1"));
        // 四段 = 前三位的 beta 序号（semver：1.8.7.23 ≡ 1.8.7-beta.23）
        assert!(is_prerelease_version("0.4.0.5"));
        assert!(is_prerelease_version("1.8.7.23"));
        assert!(is_prerelease_version("v1.8.7.23"));
        /*
         * 本项目的 beta 写法不统一。下面四种语义完全相同，都必须识别。
         *
         * 曾经只有 `-x.y` 一种被识别：`1.8.7.beta4` 因为用 `-` 正则
         * 整个解析失败，既不判预发布也无法参与版本比较。
         */
        assert!(is_prerelease_version("1.8.5-pre.01"));
        assert!(is_prerelease_version("1.9.7-beta.4"));
        assert!(is_prerelease_version("1.8.7-beta1"));
        assert!(is_prerelease_version("1.8.7.beta4"));
        // 三段正式版与尾部补 0 段都不是预发布（beta 序号为 0 不算 beta）
        assert!(!is_prerelease_version("0.4.0"));
        assert!(!is_prerelease_version("0.4.0.0"));
        assert!(!is_prerelease_version("unknown"));
    }

    /*
     * 四条 beta 写法解析出的序号要能比较，否则「beta2 → beta3」
     * 这种升级会被判成「已是最新」。
     */
    #[test]
    fn every_beta_spelling_parses_and_orders_by_serial() {
        use std::cmp::Ordering;
        for (older, newer) in [
            ("1.8.7-beta1", "1.8.7-beta2"),
            ("1.9.7-beta.1", "1.9.7-beta.2"),
            ("1.8.7.beta1", "1.8.7.beta2"),
            ("1.8.5-pre.1", "1.8.5-pre.2"),
            ("1.8.7.1", "1.8.7.2"),
        ] {
            assert_eq!(
                compare_release_versions(newer, older),
                Some(Ordering::Greater),
                "{newer} 应新于 {older}"
            );
        }
    }

    /*
     * 同号的 beta 必须低于正式版，反过来也一样。
     *
     * 这是「订阅 Stable 时不接受 beta 更新」的底层依据：
     * 本地装正式版 1.8.7，仓库有 beta 号 1.8.7.23 时不能提示「有更新」。
     */
    #[test]
    fn beta_serial_never_outranks_its_stable_version() {
        use std::cmp::Ordering;
        for beta in [
            "1.8.7-beta.1",
            "1.8.7-beta.23",
            "1.8.7-beta1",
            "1.8.7.beta4",
            "1.8.7.23",
        ] {
            assert_eq!(
                compare_release_versions(beta, "1.8.7"),
                Some(Ordering::Less),
                "{beta} 应低于同号正式版 1.8.7"
            );
            assert_eq!(
                compare_release_versions("1.8.7", beta),
                Some(Ordering::Greater),
                "正式版 1.8.7 应高于 {beta}"
            );
        }
        // 下一个正式版仍然更高
        assert_eq!(
            compare_release_versions("1.8.8-beta1", "1.8.7"),
            Some(Ordering::Greater),
            "1.8.8-beta1 的次版本号更高，应视为更新"
        );
    }

    /*
     * 紧凑写法 `-betaN` 必须按**数值**比较。
     *
     * 严格 semver 里 `-beta1` 是单个字母数字标识符，按 ASCII 字典序比较，
     * 于是 `"beta10" < "beta9"`（'1' < '9'）—— 发到第 10 个测试版时
     * 更新检测会静默失效。规范推荐 `-beta.1` 正是为了避免这个。
     *
     * 本项目用紧凑写法，所以在解析层把 `beta1` 拆成 `beta` + `1`，
     * 与 `beta.1` 等价。
     */
    #[test]
    fn compact_beta_serial_compares_numerically() {
        use std::cmp::Ordering;
        for (older, newer) in [
            ("1.8.8-beta9", "1.8.8-beta10"),
            ("1.8.8-beta10", "1.8.8-beta11"),
            ("1.8.8-beta99", "1.8.8-beta100"),
            ("1.8.8-rc9", "1.8.8-rc10"),
        ] {
            assert_eq!(
                compare_release_versions(newer, older),
                Some(Ordering::Greater),
                "{newer} 应新于 {older}"
            );
        }
        // `beta1` 与 `beta.1` 是同一个序号
        assert_eq!(
            compare_release_versions("1.8.8-beta1", "1.8.8-beta.1"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_release_versions("1.8.8-beta10", "1.8.8-beta.10"),
            Some(Ordering::Equal)
        );
    }

    /// semver 2.0.0 §11 的经典优先级链，逐条必须成立。
    #[test]
    fn matches_the_semver_spec_precedence_chain() {
        use std::cmp::Ordering;
        let chain = [
            "1.0.0-alpha",
            "1.0.0-alpha.1",
            "1.0.0-alpha.beta",
            "1.0.0-beta",
            "1.0.0-beta.2",
            "1.0.0-beta.11",
            "1.0.0-rc.1",
            "1.0.0",
        ];
        for pair in chain.windows(2) {
            assert_eq!(
                compare_release_versions(pair[1], pair[0]),
                Some(Ordering::Greater),
                "{} 应低于 {}",
                pair[0],
                pair[1]
            );
        }
    }

    /*
     * 发布契约：新发布必须符合严格 semver。
     *
     * 注意这是**发布要求**，不是解析门槛 —— 解析器仍宽容接受历史 tag
     * （见下一个测试），否则老用户会看到「版本未知」。
     */
    #[test]
    fn strict_semver_accepts_canonical_and_rejects_legacy_shapes() {
        // 合规：三段，正式版与 x.y.z-tag.w 预发布
        for ok in [
            "1.9.8",
            "v1.9.8",
            "0.4.0",
            "1.9.8-beta.1",
            "1.9.8-alpha.2",
            "1.9.8-rc.1",
            "1.9.8-beta.10",
            "1.9.8-beta.1+build.5",
            "1.9.8-0",
        ] {
            assert!(is_strict_semver(ok), "{ok} 应判为合规");
        }
        // 不合规：段数不对、前导零、预发布放置错误
        for bad in [
            "1.9.8.1",       // 四段
            "1.9",           // 两段
            "v0.10",         // 两段
            "01.9.8",        // 前导零
            "1.09.8",        // 前导零
            "1.9.8.beta.1",  // 预发布未用 -
            "1.9.8-beta.01", // 标识符前导零
            "1.9.8-",        // 空标识符
            "1.9.8-beta..1", // 空段
            "1.9.8-beta 1",  // 空格
            "",              // 空
        ] {
            assert!(!is_strict_semver(bad), "{bad} 应判为不合规");
        }
    }

    /*
     * 真实仓库里存在的历史 tag：不合规但仍必须能解析与比较。
     *
     * 这是「解析宽容」的验收：`v1.8.7.23` 这类四段号是已发布的事实，
     * 拒绝它们会让已装用户看到「版本未知」并停止收到更新提示。
     *
     * 注意 `1.8.7-beta1` 不在这个列表里：它语法上合法（单个字母数字
     * 标识符），只是不再推荐 —— 新发布统一用 `1.8.7-beta.1`。
     */
    #[test]
    fn legacy_tags_stay_parseable_even_though_they_are_not_strict() {
        use std::cmp::Ordering;
        for legacy in ["v1.8.7.23", "1.8.7.23", "v0.10", "0.9", "1.8.7.beta4"] {
            // 都不符合严格 semver
            assert!(!is_strict_semver(legacy), "{legacy} 不应判为合规");
            // 但都能参与比较
            assert!(
                SegmentVersion::parse(legacy).is_some(),
                "{legacy} 必须仍可解析"
            );
        }
        // 紧凑 beta 写法语法合法（解析层会拆开让序号按数值比）
        assert!(is_strict_semver("1.8.7-beta1"));
        assert!(SegmentVersion::parse("1.8.7-beta1").is_some());
        // 四段号仍按 beta 语义排序
        assert_eq!(
            compare_release_versions("v1.8.7.23", "v1.8.7"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_release_versions("v1.8.7.23", "v1.8.7.21"),
            Some(Ordering::Greater)
        );
        // 两段号补齐为三段后仍可比较
        assert_eq!(
            compare_release_versions("v0.10", "v0.9"),
            Some(Ordering::Greater)
        );
    }

    /*
     * 现实约束：.NET 程序集版本最多四段纯数字，写不出 `1.9.8-beta.1`。
     *
     * HostUtilities 因此采用「DLL 四段数字 + GitHub tag 标准 semver」的发布模型：
     *
     *     DLL version  1.9.8.1
     *     release tag  v1.9.8-beta.1
     *
     * 后端比较必须把这二者视为同一版本，否则装了 beta.1 的本地副本
     * 检测不到 beta.2，或同一个 beta 被反复提示为更新。
     */
    #[test]
    fn dll_four_segment_version_equals_release_beta_tag() {
        use std::cmp::Ordering;
        for serial in [1, 2, 9, 10, 21, 23] {
            let dll = format!("1.9.8.{serial}");
            let tag = format!("v1.9.8-beta.{serial}");
            assert_eq!(
                compare_release_versions(&dll, &tag),
                Some(Ordering::Equal),
                "{dll} 应等于 {tag}"
            );
        }
    }

    /*
     * 本地装四段 beta 时的更新检测：
     *   · 线上更新的 beta → 有更新
     *   · 线上同基版本正式版 → 有更新（同基 beta 低于 stable）
     *   · 序号跨十位仍按数值（旧实现把 .21 判得比 .1 小）
     */
    #[test]
    fn local_four_segment_beta_upgrades_to_newer_beta_and_to_stable() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_release_versions("v1.9.8-beta.2", "1.9.8.1"),
            Some(Ordering::Greater),
            "本地 beta.1 应检测到 beta.2"
        );
        assert_eq!(
            compare_release_versions("v1.9.8-beta.1", "1.9.8.2"),
            Some(Ordering::Less),
            "本地 beta.2 不应被更旧的 beta.1 提示更新"
        );
        assert_eq!(
            compare_release_versions("v1.9.8-beta.21", "1.9.8.1"),
            Some(Ordering::Greater),
            "beta.21 应大于 beta.1"
        );
        assert_eq!(
            compare_release_versions("v1.9.8", "1.9.8.1"),
            Some(Ordering::Greater),
            "本地 beta 应能升级到同基版本正式版"
        );
        assert_eq!(
            compare_release_versions("v1.9.9-beta.1", "1.9.8.1"),
            Some(Ordering::Greater),
            "下一条产品线的 beta 也算更新"
        );
        assert_eq!(
            compare_release_versions("v1.9.7", "1.9.8.1"),
            Some(Ordering::Less),
            "上一个正式版不应提示更新"
        );
    }

    #[test]
    fn version_comparison_counts_every_segment() {
        use std::cmp::Ordering;
        // 真实场景：DLL 里是 0.4.0.0，Release tag 是 0.4.0。
        // 旧实现把第 4 段拼成 -0 当预发布，导致「可更新」误报。
        assert_eq!(
            compare_release_versions("0.4.0.0", "0.4.0"),
            Some(Ordering::Equal),
            "0.4.0.0 与 0.4.0 是同一版本"
        );
        assert_eq!(
            compare_release_versions("0.4", "0.4.0"),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_release_versions("v0.4", "0.4.0"),
            Some(Ordering::Equal)
        );

        // 第 4 段是 beta 序号：0.4.0.5 ≡ 0.4.0-beta.5，低于正式版 0.4.0
        assert_eq!(
            compare_release_versions("1.8.7.23", "1.8.7.21"),
            Some(Ordering::Greater),
            "beta 23 高于 beta 21"
        );
        assert_eq!(
            compare_release_versions("1.8.7.23", "1.8.7"),
            Some(Ordering::Less),
            "beta 低于同号正式版"
        );
        assert_eq!(
            compare_release_versions("1.8.7.23", "1.8.7.21"),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_release_versions("0.4.0", "0.4.0.0"),
            Some(Ordering::Equal),
            "尾部补 0 段无意义"
        );

        // 预发布低于同号正式版
        assert_eq!(
            compare_release_versions("0.4.0-beta", "0.4.0"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_release_versions("0.4.0.1-beta", "0.4.0.1"),
            Some(Ordering::Equal),
            "第 4 段已表达 beta 序号，带 - 时保持同一序号"
        );

        // 预发布标识逐段比较：alpha < alpha.1 < beta
        assert_eq!(
            compare_release_versions("0.4.0-alpha", "0.4.0-alpha.1"),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_release_versions("0.4.0-alpha.1", "0.4.0-beta"),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn versions_and_channels_are_supported() {
        assert_eq!(
            parse_version("v1.2.3-beta").unwrap().to_string(),
            "1.2.3-beta"
        );
        assert!(parse_version("5.4.23.5").is_some());
        let releases = vec![
            ReleaseInfo {
                release_id: None,
                tag_name: "v1.0.0".into(),
                version: Some("1.0.0".into()),
                name: String::new(),
                body: String::new(),
                published_at: None,
                prerelease: false,
                draft: false,
                assets: Vec::new(),
            },
            ReleaseInfo {
                release_id: None,
                tag_name: "v1.1.0-beta".into(),
                version: Some("1.1.0-beta".into()),
                name: String::new(),
                body: String::new(),
                published_at: None,
                prerelease: true,
                draft: false,
                assets: Vec::new(),
            },
        ];
        assert_eq!(filter_channel(releases.clone(), &Channel::Stable).len(), 1);
        assert_eq!(filter_channel(releases, &Channel::Beta).len(), 1);
    }

    /*
     * 同一个版本号，无论从哪个字段读，预发布结论必须一致。
     *
     * 真实事故：仓库卡片读 `tag_name`（完整四段）判成预发布，
     * 安装确认弹窗读 `version`（后端归一后的三段）判成正式版，
     * 同一个 Release 在两处显示相反结论。
     *
     * 根因是判定被分散到前端各处、各自选字段反推。现在唯一实现放在
     * 后端 `is_prerelease_version`，并随 ReleaseInfo 的 prerelease 下发。
     */
    #[test]
    fn prerelease_verdict_is_identical_for_tag_and_normalized_version() {
        for tag in ["v1.8.7.23", "1.8.7.23-beta", "v0.4.0", "v0.4.0.0"] {
            let from_tag = is_prerelease_version(tag);
            let normalized = parse_version(tag).map(|value| value.to_string());
            let from_version = normalized.as_deref().is_some_and(is_prerelease_version);
            // 归一化会丢第 4 段，所以 version 侧的结论只能"不少于" tag 侧：
            // 带 - 后缀的必须保留预发布语义，四段的差异由后端提前算好下发。
            if from_tag && tag.contains('-') {
                assert!(from_version, "{tag} 归一后丢了预发布语义");
            }
        }
        // 后端下发给前端的就是下面这个值，前端不再自己算
        assert!(is_prerelease_version("v1.8.7.23"));
        assert!(!is_prerelease_version("v0.4.0"));
    }

    #[test]
    fn asset_rules_match_exact_names() {
        let asset = ReleaseAsset {
            name: "HostUtilities.zip".into(),
            size: 1,
            download_count: None,
            download_url: "https://github.com/a/b/releases/download/v1/HostUtilities.zip".into(),
            digest: None,
        };
        assert!(asset_matches(
            &asset,
            &[AssetRule {
                rule_type: AssetRuleType::Exact,
                value: "HostUtilities.zip".into()
            }]
        ));
    }

    #[test]
    fn api_request_targets_order_direct_token_then_proxy() {
        let official = "https://api.github.com/repos/a/b/releases";
        let proxies = vec![GitHubProxy {
            id: "proxy".into(),
            name: "Proxy".into(),
            template: "https://proxy.example/{url}".into(),
            regions: Vec::new(),
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];

        // 配了 Token：直接用官方 + Token，不再先打匿名接口
        let targets = api_request_targets(official, Some("secret"), true, &proxies);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0], (official.to_string(), Some("secret")));
        assert_eq!(
            targets[1],
            (
                "https://proxy.example/https://api.github.com/repos/a/b/releases".to_string(),
                None,
            )
        );
        // 代理永远不带凭据
        assert_eq!(targets[1].1, None);

        // 没有 Token：官方匿名 + 代理
        let targets = api_request_targets(official, None, false, &proxies);
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0], (official.to_string(), None));
        assert_eq!(targets[1].1, None);
    }

    #[test]
    fn gitee_token_uses_access_token_query_on_official_api_only() {
        // Gitee 的 access_token 走 query 参数（官方约定），且只附加在官方地址上
        let official = "https://gitee.com/api/v5/repos/a/b/releases?per_page=10";
        assert_eq!(
            gitee_api_url_with_token(official, "secret token").unwrap(),
            "https://gitee.com/api/v5/repos/a/b/releases?per_page=10&access_token=secret+token"
        );
        // 代理地址不识别为官方接口，因此不会附加凭据
        assert!(!is_official_gitee_api_url(
            "https://oc2gtapi.ch3ng.top/api/v5/repos/a/b/releases"
        ));
    }

    #[test]
    fn release_id_and_missing_asset_size_are_parsed() {
        let value = serde_json::json!([{
            "id": "42",
            "tag_name": "v1.0.0",
            "assets": [{
                "name": "HostUtilities.zip",
                "browser_download_url": "https://gitee.com/example/download"
            }]
        }]);
        let releases = parse_release_json(&value, &Provider::Gitee).unwrap();
        assert_eq!(releases[0].release_id, Some(42));
        assert_eq!(releases[0].assets[0].size, 0);
        assert_eq!(releases[0].assets[0].download_count, None);
    }

    #[test]
    fn release_asset_download_counts_preserve_zero_and_missing_metadata() {
        for (input, expected) in [
            (Some(serde_json::json!(42)), Some(42)),
            (Some(serde_json::json!(0)), Some(0)),
            (Some(serde_json::json!("1234")), Some(1234)),
            (None, None),
            (Some(serde_json::Value::Null), None),
            (Some(serde_json::json!(-1)), None),
            (Some(serde_json::json!(1.5)), None),
            (Some(serde_json::json!("unknown")), None),
        ] {
            let mut asset = serde_json::json!({
                "name": "example.zip", "size": 1024,
                "browser_download_url": "https://github.com/example/mod/releases/download/v1/example.zip"
            });
            if let Some(input) = input {
                asset["download_count"] = input;
            }
            let releases = parse_release_json(
                &serde_json::json!([{ "tag_name": "v1.0.0", "assets": [asset] }]),
                &Provider::Github,
            )
            .unwrap();
            let asset = &releases[0].assets[0];
            assert_eq!(asset.download_count, expected);
            let cached: ReleaseAsset =
                serde_json::from_value(serde_json::to_value(asset).unwrap()).unwrap();
            assert_eq!(cached.download_count, expected);
            assert_eq!(cached.name, "example.zip");
            assert_eq!(cached.size, 1024);
        }

        let legacy: ReleaseAsset = serde_json::from_value(serde_json::json!({
            "name": "legacy.zip", "size": 1024, "download_url": "https://example.com/legacy.zip"
        }))
        .unwrap();
        assert_eq!(legacy.download_count, None);
    }

    #[test]
    fn user_agent_identifies_client_and_version() {
        assert!(APP_USER_AGENT.starts_with("Overcooked 2 MOD Manager/"));
        assert!(APP_USER_AGENT.contains("client=desktop"));
        assert!(APP_USER_AGENT.contains("arch=x64"));
    }
}
