use super::error::ReleaseError;
use crate::models::Provider;
use url::Url;

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

#[cfg(test)]
mod tests;
