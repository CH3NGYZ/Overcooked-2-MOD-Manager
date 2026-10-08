use crate::models::{GitHubProxy, GitHubProxyConfig, GitHubProxyManifest};
use regex::Regex;
use std::collections::HashSet;
use std::sync::OnceLock;
use url::Url;

const BUNDLED_PROXY_MANIFEST: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/repositories-proxies.toml"
));
const MAX_PROXY_COUNT: usize = 128;
const PLACEHOLDERS: &[&str] = &[
    "url",
    "url_no_scheme",
    "github_path",
    "owner",
    "repo",
    "tag",
    "asset",
    "branch",
    "path",
    "url_path",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxySourceFormat {
    Toml,
    UserScript,
}

#[derive(Debug, Clone)]
pub struct ProxySource {
    pub url: String,
    pub format: ProxySourceFormat,
    pub bundled: bool,
}

pub fn bundled_defaults() -> GitHubProxyConfig {
    static DEFAULTS: OnceLock<GitHubProxyConfig> = OnceLock::new();
    DEFAULTS.get_or_init(parse_bundled_defaults).clone()
}

fn parse_bundled_defaults() -> GitHubProxyConfig {
    let manifest = match parse_manifest(BUNDLED_PROXY_MANIFEST) {
        Ok(config) => config,
        Err(error) => {
            log::error!("内置 GitHub 代理清单解析失败：{}", error);
            GitHubProxyConfig::default()
        }
    };
    let config = merge_source_configs(&manifest, bundled_api_defaults());
    log::info!(
        "内置代理加载完成：Release={}，Raw={}，GitHub API={}，Gitee API={}",
        config.release_proxies.len(),
        config.raw_proxies.len(),
        config.github_api_proxies.len(),
        config.gitee_api_proxies.len()
    );
    config
}

/// Read the API proxy entries from the bundled repository index. Keeping these
/// entries in `repositories.toml` lets the same maintained index describe the
/// built-in GitHub and Gitee API channels without hard-coding their URLs here.
pub fn bundled_api_defaults() -> GitHubProxyConfig {
    static DEFAULTS: OnceLock<GitHubProxyConfig> = OnceLock::new();
    DEFAULTS.get_or_init(parse_bundled_api_defaults).clone()
}

fn parse_bundled_api_defaults() -> GitHubProxyConfig {
    let document =
        crate::repository_sources::SourceLocation::parse(crate::repository_sources::BUILTIN_INDEX)
            .and_then(|location| location.bundled_document());
    let Ok(document) = document else {
        log::warn!("读取内置 API 代理配置失败，使用空配置");
        return GitHubProxyConfig::default();
    };
    fn normalize_bundled(proxies: Vec<GitHubProxy>) -> Vec<GitHubProxy> {
        proxies
            .into_iter()
            .filter_map(|mut proxy| {
                proxy.builtin = true;
                proxy.remote = false;
                match normalize_proxy(&mut proxy) {
                    Ok(()) => Some(proxy),
                    Err(error) => {
                        log::warn!("忽略无效的内置 API 代理 {}: {error}", proxy.id);
                        None
                    }
                }
            })
            .collect()
    }
    let config = GitHubProxyConfig {
        release_proxies: Vec::new(),
        raw_proxies: Vec::new(),
        github_api_proxies: deduplicate(normalize_bundled(document.github_api_proxies)),
        gitee_api_proxies: deduplicate(normalize_bundled(document.gitee_api_proxies)),
    };
    log::debug!(
        "内置 API 代理解析完成：GitHub={}，Gitee={}",
        config.github_api_proxies.len(),
        config.gitee_api_proxies.len()
    );
    config
}

/// Synchronize entries shipped with the application into an existing config.
///
/// Add missing bundled entries without replacing editable fields. Asset lists
/// keep user order within each source group, with userscripts before fallbacks.
pub fn synchronize_bundled_defaults(config: &mut GitHubProxyConfig) -> bool {
    let previous = config.clone();
    let bundled = bundled_defaults();
    synchronize_bundled_list(&mut config.release_proxies, &bundled.release_proxies);
    synchronize_bundled_list(&mut config.raw_proxies, &bundled.raw_proxies);
    synchronize_bundled_list(&mut config.github_api_proxies, &bundled.github_api_proxies);
    synchronize_bundled_list(&mut config.gitee_api_proxies, &bundled.gitee_api_proxies);
    prioritize_remote(&mut config.release_proxies);
    prioritize_remote(&mut config.raw_proxies);
    for proxy in config
        .github_api_proxies
        .iter_mut()
        .chain(config.gitee_api_proxies.iter_mut())
    {
        proxy.remote = false;
    }
    let changed = *config != previous;
    if !changed {
        log::debug!("内置代理配置无需同步");
        return false;
    }
    log::info!("已同步内置代理配置");
    true
}

fn synchronize_bundled_list(current: &mut Vec<GitHubProxy>, bundled: &[GitHubProxy]) {
    for proxy in bundled {
        let existing = current
            .iter()
            .position(|entry| entry.id.eq_ignore_ascii_case(&proxy.id))
            .or_else(|| {
                current
                    .iter()
                    .position(|entry| template_key(entry) == template_key(proxy))
            });
        if let Some(index) = existing {
            // Source aliases share the bundled identity without resetting edits.
            current[index].id = proxy.id.clone();
            current[index].builtin = true;
        } else {
            current.push(proxy.clone());
        }
    }
    let preferred = bundled
        .iter()
        .map(|proxy| proxy.id.to_ascii_lowercase())
        .collect();
    *current = deduplicate_with_preferred_ids(std::mem::take(current), &preferred);
}

pub fn parse_manifest(text: &str) -> Result<GitHubProxyConfig, String> {
    log::debug!("开始解析 GitHub 代理 TOML 清单：字节数={}", text.len());
    if text.len() > 1024 * 1024 {
        return Err("GitHub 代理清单超过 1 MiB".into());
    }
    let mut manifest: GitHubProxyManifest =
        toml::from_str(text).map_err(|error| format!("GitHub 代理清单解析失败: {error}"))?;
    if manifest.schema_version != 1 {
        return Err(format!(
            "不支持 GitHub 代理清单版本 {}",
            manifest.schema_version
        ));
    }
    for proxy in manifest
        .release_proxies
        .iter_mut()
        .chain(manifest.raw_proxies.iter_mut())
        .chain(manifest.github_api_proxies.iter_mut())
        .chain(manifest.gitee_api_proxies.iter_mut())
    {
        proxy.builtin = true;
        proxy.remote = false;
        normalize_proxy(proxy)?;
    }
    if manifest.release_proxies.len() > MAX_PROXY_COUNT
        || manifest.raw_proxies.len() > MAX_PROXY_COUNT
        || manifest.github_api_proxies.len() > MAX_PROXY_COUNT
        || manifest.gitee_api_proxies.len() > MAX_PROXY_COUNT
    {
        return Err(format!("每类代理最多允许 {MAX_PROXY_COUNT} 条"));
    }
    let config = GitHubProxyConfig {
        release_proxies: deduplicate(manifest.release_proxies),
        raw_proxies: deduplicate(manifest.raw_proxies),
        github_api_proxies: deduplicate(manifest.github_api_proxies),
        gitee_api_proxies: deduplicate(manifest.gitee_api_proxies),
    };
    validate_unique_ids(&config)?;
    log::info!(
        "GitHub 代理 TOML 清单解析完成：Release={}，Raw={}，GitHub API={}，Gitee API={}",
        config.release_proxies.len(),
        config.raw_proxies.len(),
        config.github_api_proxies.len(),
        config.gitee_api_proxies.len()
    );
    Ok(config)
}

pub fn parse_userscript(text: &str) -> Result<GitHubProxyConfig, String> {
    log::debug!("开始解析 GitHub 代理用户脚本：字节数={}", text.len());
    if text.len() > 4 * 1024 * 1024 {
        return Err("GitHub 代理脚本超过 4 MiB".into());
    }
    let release_array = extract_js_array(text, "download_url_us")?;
    let raw_array = extract_js_array(text, "raw_url")?;
    let mut release_proxies = parse_userscript_rows(release_array, false)?;
    let mut raw_proxies = parse_userscript_rows(raw_array, true)?;
    if release_proxies.is_empty() && raw_proxies.is_empty() {
        return Err("GitHub 代理脚本中没有可识别的 Release 或 Raw 代理".into());
    }
    if release_proxies.len() > MAX_PROXY_COUNT || raw_proxies.len() > MAX_PROXY_COUNT {
        return Err(format!("每类代理最多允许 {MAX_PROXY_COUNT} 条"));
    }
    for proxy in release_proxies.iter_mut().chain(raw_proxies.iter_mut()) {
        normalize_proxy(proxy)?;
    }
    let config = GitHubProxyConfig {
        release_proxies: deduplicate(release_proxies),
        raw_proxies: deduplicate(raw_proxies),
        github_api_proxies: Vec::new(),
        gitee_api_proxies: Vec::new(),
    };
    validate_unique_ids(&config)?;
    log::info!(
        "GitHub 代理用户脚本解析完成：Release={}，Raw={}",
        config.release_proxies.len(),
        config.raw_proxies.len()
    );
    Ok(config)
}

fn extract_js_array<'a>(text: &'a str, variable: &str) -> Result<&'a str, String> {
    let assignment = Regex::new(&format!(
        r"(?m)(?:\b(?:const|let|var)\s+|,)\s*{variable}\s*=\s*\["
    ))
    .map_err(|error| error.to_string())?;
    let found = assignment
        .find(text)
        .ok_or_else(|| format!("代理脚本缺少 {variable} 列表"))?;
    let open = text[found.end() - 1..]
        .char_indices()
        .next()
        .map(|(offset, _)| found.end() - 1 + offset)
        .ok_or_else(|| format!("{variable} 列表不完整"))?;
    let bytes = text.as_bytes();
    let mut index = open + 1;
    let mut depth = 1usize;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    while index < bytes.len() {
        let byte = bytes[index];
        let next = bytes.get(index + 1).copied();
        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
        } else if block_comment {
            if byte == b'*' && next == Some(b'/') {
                block_comment = false;
                index += 1;
            }
        } else if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
        } else if byte == b'/' && next == Some(b'/') {
            line_comment = true;
            index += 1;
        } else if byte == b'/' && next == Some(b'*') {
            block_comment = true;
            index += 1;
        } else if byte == b'\'' || byte == b'"' || byte == b'`' {
            quote = Some(byte);
        } else if byte == b'[' {
            depth += 1;
        } else if byte == b']' {
            depth -= 1;
            if depth == 0 {
                return Ok(&text[open + 1..index]);
            }
        }
        index += 1;
    }
    Err(format!("{variable} 列表没有闭合"))
}

fn parse_userscript_rows(array: &str, is_raw: bool) -> Result<Vec<GitHubProxy>, String> {
    let row = Regex::new(
        r"(?m)^[ \t]*\[\s*'((?:\\.|[^'\\\r\n])*)'\s*,\s*'((?:\\.|[^'\\\r\n])*)'\s*,\s*'((?:\\.|[^'\\\r\n])*)'\s*\]",
    )
    .map_err(|error| error.to_string())?;
    let mut proxies = Vec::new();
    for captures in row.captures_iter(array) {
        let prefix = js_unescape(captures.get(1).unwrap().as_str());
        let location = js_unescape(captures.get(2).unwrap().as_str());
        let description = js_unescape(captures.get(3).unwrap().as_str())
            .replace("&#10;", "\n")
            .trim()
            .to_string();
        let Some(template) = userscript_template(&prefix, is_raw) else {
            continue;
        };
        let probe = PLACEHOLDERS
            .iter()
            .fold(template.clone(), |value, placeholder| {
                value.replace(&format!("{{{placeholder}}}"), "x")
            });
        let parsed = Url::parse(&probe);
        let host = parsed
            .ok()
            .and_then(|url| url.host_str().map(str::to_string))
            .unwrap_or_else(|| "proxy".into());
        let mut regions = description
            .find('[')
            .and_then(|start| {
                description[start + 1..]
                    .find(']')
                    .map(|end| &description[start + 1..start + 1 + end])
            })
            .map(str::to_string)
            .into_iter()
            .collect::<Vec<_>>();
        if regions.is_empty() && !location.eq_ignore_ascii_case("其他") {
            regions.push(location);
        }
        proxies.push(GitHubProxy {
            id: format!("userscript-{}", host.to_ascii_lowercase().replace('.', "-")),
            name: host,
            template,
            regions,
            description,
            enabled: true,
            builtin: true,
            remote: true,
        });
    }
    Ok(proxies)
}

fn userscript_template(prefix: &str, is_raw: bool) -> Option<String> {
    if is_raw && prefix == "https://raw.githubusercontent.com" {
        return None;
    }
    let suffix = if is_raw {
        "https://raw.githubusercontent.com"
    } else {
        "https://github.com"
    };
    if let Some(base) = prefix.strip_suffix(suffix) {
        return Some(format!("{base}{{url}}"));
    }
    if !is_raw && let Some(base) = prefix.strip_suffix("/github.com") {
        return Some(format!("{base}/{{url_no_scheme}}"));
    }
    if is_raw && prefix.ends_with("/gh") {
        return Some(format!("{prefix}/{{owner}}/{{repo}}@{{branch}}/{{path}}"));
    }
    if !is_raw && prefix.ends_with("/gh") {
        return Some(format!("{prefix}/{{github_path}}"));
    }
    None
}

fn js_unescape(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(character) = chars.next() {
        if character == '\\' {
            match chars.next() {
                Some('n') => output.push('\n'),
                Some('r') => output.push('\r'),
                Some('t') => output.push('\t'),
                Some(escaped) => output.push(escaped),
                None => output.push('\\'),
            }
        } else {
            output.push(character);
        }
    }
    output
}

pub fn merge_source_configs(
    left: &GitHubProxyConfig,
    right: GitHubProxyConfig,
) -> GitHubProxyConfig {
    let mut config = GitHubProxyConfig {
        release_proxies: deduplicate(
            left.release_proxies
                .iter()
                .cloned()
                .chain(right.release_proxies)
                .collect(),
        ),
        raw_proxies: deduplicate(
            left.raw_proxies
                .iter()
                .cloned()
                .chain(right.raw_proxies)
                .collect(),
        ),
        github_api_proxies: deduplicate(
            left.github_api_proxies
                .iter()
                .cloned()
                .chain(right.github_api_proxies)
                .collect(),
        ),
        gitee_api_proxies: deduplicate(
            left.gitee_api_proxies
                .iter()
                .cloned()
                .chain(right.gitee_api_proxies)
                .collect(),
        ),
    };
    prioritize_remote(&mut config.release_proxies);
    prioritize_remote(&mut config.raw_proxies);
    config
}

pub fn validate_config(mut config: GitHubProxyConfig) -> Result<GitHubProxyConfig, String> {
    if config.release_proxies.len() > MAX_PROXY_COUNT
        || config.raw_proxies.len() > MAX_PROXY_COUNT
        || config.github_api_proxies.len() > MAX_PROXY_COUNT
        || config.gitee_api_proxies.len() > MAX_PROXY_COUNT
    {
        return Err(format!("每类代理最多允许 {MAX_PROXY_COUNT} 条"));
    }
    for proxy in config
        .release_proxies
        .iter_mut()
        .chain(config.raw_proxies.iter_mut())
        .chain(config.github_api_proxies.iter_mut())
        .chain(config.gitee_api_proxies.iter_mut())
    {
        normalize_proxy(proxy)?;
    }
    validate_unique_ids(&config)?;
    config.release_proxies = deduplicate(config.release_proxies);
    config.raw_proxies = deduplicate(config.raw_proxies);
    config.github_api_proxies = deduplicate(config.github_api_proxies);
    config.gitee_api_proxies = deduplicate(config.gitee_api_proxies);
    prioritize_remote(&mut config.release_proxies);
    prioritize_remote(&mut config.raw_proxies);
    Ok(config)
}

/// Built-in identity is owned by the saved config and bundled resources, not
/// by the editable `builtin` flag supplied through IPC.
pub fn validate_saved_config(
    mut proposed: GitHubProxyConfig,
    current: &GitHubProxyConfig,
) -> Result<GitHubProxyConfig, String> {
    let bundled = bundled_defaults();
    protect_builtin_entries(&mut proposed, current, &bundled)?;
    proposed = validate_config(proposed)?;
    // Template deduplication must not silently remove another built-in ID.
    protect_builtin_entries(&mut proposed, current, &bundled)?;
    Ok(proposed)
}

fn protect_builtin_entries(
    proposed: &mut GitHubProxyConfig,
    current: &GitHubProxyConfig,
    bundled: &GitHubProxyConfig,
) -> Result<(), String> {
    for (kind, proposed, current, bundled) in [
        (
            "Release",
            &mut proposed.release_proxies,
            &current.release_proxies,
            &bundled.release_proxies,
        ),
        (
            "Raw",
            &mut proposed.raw_proxies,
            &current.raw_proxies,
            &bundled.raw_proxies,
        ),
        (
            "GitHub API",
            &mut proposed.github_api_proxies,
            &current.github_api_proxies,
            &bundled.github_api_proxies,
        ),
        (
            "Gitee API",
            &mut proposed.gitee_api_proxies,
            &current.gitee_api_proxies,
            &bundled.gitee_api_proxies,
        ),
    ] {
        let required = current
            .iter()
            .filter(|proxy| proxy.builtin)
            .chain(bundled.iter().map(|proxy| {
                current
                    .iter()
                    .find(|entry| entry.id.eq_ignore_ascii_case(&proxy.id))
                    .or_else(|| {
                        current
                            .iter()
                            .find(|entry| template_key(entry) == template_key(proxy))
                    })
                    .unwrap_or(proxy)
            }))
            .map(|proxy| proxy.id.trim().to_ascii_lowercase())
            .collect::<HashSet<_>>();
        for proxy in proposed.iter_mut() {
            proxy.builtin = required.contains(&proxy.id.trim().to_ascii_lowercase());
            proxy.remote = matches!(kind, "Release" | "Raw")
                && current
                    .iter()
                    .find(|entry| entry.id.trim().eq_ignore_ascii_case(proxy.id.trim()))
                    .is_some_and(|entry| entry.remote);
        }
        if let Some(missing) = required.iter().find(|id| {
            !proposed
                .iter()
                .any(|proxy| proxy.id.trim().eq_ignore_ascii_case(id))
        }) {
            log::warn!("拒绝删除或移动内置代理：类型={}，ID={}", kind, missing);
            return Err(format!(
                "内置 {kind} 代理不可删除或移动到其他类型: {missing}"
            ));
        }
    }
    Ok(())
}

fn normalize_proxy(proxy: &mut GitHubProxy) -> Result<(), String> {
    proxy.id = proxy.id.trim().to_string();
    proxy.name = proxy.name.trim().to_string();
    proxy.template = proxy.template.trim().to_string();
    proxy.description = proxy.description.trim().to_string();
    proxy.regions = proxy
        .regions
        .iter()
        .map(|region| region.trim().to_string())
        .filter(|region| !region.is_empty())
        .collect();
    proxy.regions.sort_by_key(|region| region.to_lowercase());
    proxy
        .regions
        .dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    if proxy.id.is_empty() || proxy.name.is_empty() || proxy.template.is_empty() {
        return Err("代理 ID、名称和模板都不能为空".into());
    }
    let mut probe = String::with_capacity(proxy.template.len());
    let mut rest = proxy.template.as_str();
    while let Some(start) = rest.find('{') {
        probe.push_str(&rest[..start]);
        let token_start = start + 1;
        let end = rest[token_start..]
            .find('}')
            .map(|offset| token_start + offset)
            .ok_or_else(|| "代理模板包含未闭合的占位符".to_string())?;
        let token = &rest[token_start..end];
        if !PLACEHOLDERS.contains(&token) {
            return Err(format!("代理模板占位符不受支持: {{{token}}}"));
        }
        probe.push('x');
        rest = &rest[end + 1..];
    }
    probe.push_str(rest);
    if probe.contains('}') {
        return Err("代理模板包含无效的大括号".into());
    }
    let parsed = Url::parse(&probe).map_err(|error| format!("代理模板 URL 无效: {error}"))?;
    if parsed.scheme() != "https" || parsed.host_str().is_none() {
        return Err("GitHub 代理模板必须是 HTTPS URL".into());
    }
    Ok(())
}

fn template_key(proxy: &GitHubProxy) -> String {
    proxy
        .template
        .trim()
        .trim_end_matches('/')
        .replace("/{url_path}", "{url_path}")
        .to_ascii_lowercase()
}

fn prioritize_remote(proxies: &mut [GitHubProxy]) {
    // Stable sorting preserves the user's relative order within each group.
    proxies.sort_by_key(|proxy| !proxy.remote);
}

fn deduplicate(proxies: Vec<GitHubProxy>) -> Vec<GitHubProxy> {
    deduplicate_with_preferred_ids(proxies, &HashSet::new())
}

fn deduplicate_with_preferred_ids(
    proxies: Vec<GitHubProxy>,
    preferred: &HashSet<String>,
) -> Vec<GitHubProxy> {
    let mut unique = Vec::<GitHubProxy>::new();
    let mut indexes = std::collections::HashMap::<String, usize>::new();
    for mut proxy in proxies {
        let key = template_key(&proxy);
        if let Some(index) = indexes.get(&key).copied() {
            let existing = &mut unique[index];
            let incoming_preferred = preferred.contains(&proxy.id.to_ascii_lowercase());
            let existing_preferred = preferred.contains(&existing.id.to_ascii_lowercase());
            if (incoming_preferred && !existing_preferred)
                || (incoming_preferred == existing_preferred
                    && proxy.builtin
                    && (!existing.builtin || (!proxy.remote && existing.remote)))
            {
                std::mem::swap(existing, &mut proxy);
            }
            for region in proxy.regions {
                if !existing
                    .regions
                    .iter()
                    .any(|current| current.eq_ignore_ascii_case(&region))
                {
                    existing.regions.push(region);
                }
            }
            if existing.description.is_empty() {
                existing.description = proxy.description;
            }
            // A duplicate must never undo a stored opt-out.
            existing.enabled &= proxy.enabled;
            existing.builtin |= proxy.builtin;
            existing.remote |= proxy.remote;
        } else {
            indexes.insert(key, unique.len());
            unique.push(proxy);
        }
    }
    unique
}

pub struct RawProxyContext<'a> {
    pub branch: &'a str,
    pub file_path: &'a str,
}

pub fn proxy_urls(
    proxies: &[GitHubProxy],
    target: &str,
    is_raw: bool,
    raw_context: Option<RawProxyContext<'_>>,
) -> Vec<String> {
    let (url, owner, repo, branch, path, tag, asset) = proxy_context(target, is_raw, raw_context);
    let mut seen = HashSet::new();
    let urls = proxies
        .iter()
        .filter(|proxy| proxy.enabled)
        .filter_map(|proxy| {
            let mut rendered = proxy.template.clone();
            let url_path = github_path(target);
            let target_url_path = target_url_path(target);
            if target_url_path.starts_with('/') {
                rendered = rendered.replace("/{url_path}", "{url_path}");
            }
            let encoded_branch = encode_slash_path(&branch);
            let encoded_path = encode_slash_path(&path);
            let values = [
                ("url", url.as_str()),
                ("url_no_scheme", url.trim_start_matches("https://")),
                ("github_path", url_path.as_str()),
                ("owner", owner.as_str()),
                ("repo", repo.as_str()),
                ("tag", tag.as_str()),
                ("asset", asset.as_str()),
                ("branch", encoded_branch.as_str()),
                ("path", encoded_path.as_str()),
                ("url_path", target_url_path.as_str()),
            ];
            for (key, value) in values {
                rendered = rendered.replace(&format!("{{{key}}}"), value);
            }
            Url::parse(&rendered)
                .ok()
                .filter(|parsed| parsed.scheme() == "https")
                .map(|_| rendered)
        })
        .filter(|url| seen.insert(url.clone()))
        .collect::<Vec<_>>();
    log::trace!(
        "生成代理地址：代理候选={}，可用地址={}，Raw={}",
        proxies.len(),
        urls.len(),
        is_raw
    );
    urls
}

fn proxy_context(
    target: &str,
    is_raw: bool,
    raw_context: Option<RawProxyContext<'_>>,
) -> (String, String, String, String, String, String, String) {
    let mut owner = String::new();
    let mut repo = String::new();
    let mut branch = String::new();
    let mut path = String::new();
    let mut tag = String::new();
    let mut asset = String::new();
    if let Ok(url) = Url::parse(target) {
        let segments = url
            .path_segments()
            .map(|segments| segments.collect::<Vec<_>>())
            .unwrap_or_default();
        owner = segments.first().copied().unwrap_or_default().to_string();
        repo = segments.get(1).copied().unwrap_or_default().to_string();
        if is_raw {
            if let Some(raw_index) = segments.iter().position(|part| *part == "raw") {
                let branch_index = if segments.get(raw_index + 1) == Some(&"refs")
                    && segments.get(raw_index + 2) == Some(&"heads")
                {
                    raw_index + 3
                } else {
                    raw_index + 1
                };
                branch = segments
                    .get(branch_index)
                    .copied()
                    .unwrap_or_default()
                    .to_string();
                path = segments
                    .get(branch_index + 1..)
                    .unwrap_or_default()
                    .join("/");
            }
        } else if let Some(download_index) = segments.iter().position(|part| *part == "download") {
            tag = segments
                .get(download_index + 1)
                .copied()
                .unwrap_or_default()
                .to_string();
            asset = segments
                .get(download_index + 2..)
                .unwrap_or_default()
                .join("/");
        } else if let Some(archive_index) = segments.iter().position(|part| *part == "archive") {
            if segments.get(archive_index + 1) == Some(&"refs")
                && segments.get(archive_index + 2) == Some(&"tags")
            {
                tag = segments
                    .get(archive_index + 3)
                    .copied()
                    .unwrap_or_default()
                    .to_string();
                asset = segments
                    .get(archive_index + 4)
                    .copied()
                    .unwrap_or_default()
                    .to_string();
            }
        }
    }
    if is_raw && let Some(context) = raw_context {
        branch = context.branch.to_string();
        path = context.file_path.to_string();
    }
    let canonical = if is_raw {
        let raw_branch = if branch.is_empty() { "main" } else { &branch };
        github_raw_url(&owner, &repo, raw_branch, &path).unwrap_or_else(|| {
            format!("https://raw.githubusercontent.com/{owner}/{repo}/{raw_branch}/{path}")
        })
    } else {
        target.to_string()
    };
    (canonical, owner, repo, branch, path, tag, asset)
}

fn github_raw_url(owner: &str, repo: &str, branch: &str, path: &str) -> Option<String> {
    let mut url = Url::parse("https://raw.githubusercontent.com").ok()?;
    {
        let mut segments = url.path_segments_mut().ok()?;
        segments.push(owner);
        segments.push(repo);
        for segment in branch.split('/') {
            segments.push(segment);
        }
        for segment in path.split('/') {
            segments.push(segment);
        }
    }
    Some(url.into())
}

fn encode_slash_path(value: &str) -> String {
    let mut url = Url::parse("https://path.invalid").expect("static URL is valid");
    {
        let mut segments = url.path_segments_mut().expect("URL supports path segments");
        for segment in value.split('/') {
            segments.push(segment);
        }
    }
    url.path().trim_start_matches('/').to_string()
}

fn github_path(target: &str) -> String {
    Url::parse(target)
        .ok()
        .map(|url| url.path().trim_start_matches('/').to_string())
        .unwrap_or_default()
}

fn target_url_path(target: &str) -> String {
    let Ok(url) = Url::parse(target) else {
        return String::new();
    };
    let mut path = url.path().to_string();
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    path
}

pub fn merge_downloaded(
    existing: &GitHubProxyConfig,
    downloaded: GitHubProxyConfig,
) -> GitHubProxyConfig {
    // A failed/empty remote source must not replace the local fallback set.
    let bundled = bundled_defaults();
    let mut downloaded = merge_source_configs(&downloaded, bundled.clone());
    synchronize_bundled_list(&mut downloaded.release_proxies, &bundled.release_proxies);
    synchronize_bundled_list(&mut downloaded.raw_proxies, &bundled.raw_proxies);
    synchronize_bundled_list(
        &mut downloaded.github_api_proxies,
        &bundled.github_api_proxies,
    );
    synchronize_bundled_list(
        &mut downloaded.gitee_api_proxies,
        &bundled.gitee_api_proxies,
    );
    fn merge(
        current: &[GitHubProxy],
        incoming: Vec<GitHubProxy>,
        bundled: &[GitHubProxy],
    ) -> Vec<GitHubProxy> {
        let mut incoming = incoming.into_iter().map(Some).collect::<Vec<_>>();
        let mut merged = Vec::new();
        // Refresh source identity while preserving existing edits, disabled
        // choices and the user's ordering within each source group.
        for old in current {
            let matched = incoming
                .iter()
                .position(|proxy| {
                    proxy
                        .as_ref()
                        .is_some_and(|proxy| proxy.id.eq_ignore_ascii_case(&old.id))
                })
                .or_else(|| {
                    incoming.iter().position(|proxy| {
                        proxy.as_ref().is_some_and(|proxy| {
                            template_key(proxy) == template_key(old)
                                && !current
                                    .iter()
                                    .any(|entry| entry.id.eq_ignore_ascii_case(&proxy.id))
                        })
                    })
                });
            let mut proxy = old.clone();
            if let Some(index) = matched {
                let source = incoming[index]
                    .take()
                    .expect("matched incoming proxy exists");
                proxy.id = source.id;
                proxy.builtin |= source.builtin;
                proxy.remote |= source.remote;
            }
            merged.push(proxy);
        }
        merged.extend(incoming.into_iter().flatten());
        let preferred = bundled
            .iter()
            .map(|proxy| proxy.id.to_ascii_lowercase())
            .collect();
        deduplicate_with_preferred_ids(merged, &preferred)
    }
    let mut merged = GitHubProxyConfig {
        release_proxies: merge(
            &existing.release_proxies,
            downloaded.release_proxies,
            &bundled.release_proxies,
        ),
        raw_proxies: merge(
            &existing.raw_proxies,
            downloaded.raw_proxies,
            &bundled.raw_proxies,
        ),
        github_api_proxies: merge(
            &existing.github_api_proxies,
            downloaded.github_api_proxies,
            &bundled.github_api_proxies,
        ),
        gitee_api_proxies: merge(
            &existing.gitee_api_proxies,
            downloaded.gitee_api_proxies,
            &bundled.gitee_api_proxies,
        ),
    };
    prioritize_remote(&mut merged.release_proxies);
    prioritize_remote(&mut merged.raw_proxies);
    log::debug!(
        "合并下载代理配置：Release={}，Raw={}，GitHub API={}，Gitee API={}",
        merged.release_proxies.len(),
        merged.raw_proxies.len(),
        merged.github_api_proxies.len(),
        merged.gitee_api_proxies.len()
    );
    merged
}

pub fn validate_unique_ids(config: &GitHubProxyConfig) -> Result<(), String> {
    for proxies in [
        &config.release_proxies,
        &config.raw_proxies,
        &config.github_api_proxies,
        &config.gitee_api_proxies,
    ] {
        let mut ids = HashSet::new();
        for proxy in proxies {
            if !ids.insert(proxy.id.to_ascii_lowercase()) {
                return Err(format!("GitHub 代理 ID 重复: {}", proxy.id));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_deduplicates_templates_and_merges_regions() {
        let manifest = r#"
schema_version = 1
[[release_proxies]]
id = "proxy-us"
name = "Proxy"
template = "https://proxy.example/{url}"
regions = ["US"]
[[release_proxies]]
id = "proxy-hk"
name = "Proxy HK"
template = "https://proxy.example/{url}"
regions = ["Hong Kong"]
"#;
        let parsed = parse_manifest(manifest).unwrap();
        assert_eq!(parsed.release_proxies.len(), 1);
        assert_eq!(parsed.release_proxies[0].regions, ["US", "Hong Kong"]);
        assert!(parsed.release_proxies[0].builtin);
        assert!(!parsed.release_proxies[0].remote);
    }

    #[test]
    fn bundled_manifest_contains_release_and_raw_proxy_lists() {
        let config = bundled_defaults();
        assert!(!config.release_proxies.is_empty());
        assert!(!config.raw_proxies.is_empty());
        assert!(
            config
                .release_proxies
                .iter()
                .any(|proxy| proxy.template == "https://gh.ddlc.top/{url}")
        );
        assert!(
            config
                .release_proxies
                .iter()
                .any(|proxy| proxy.template == "https://ghproxy.cxkpro.top/{url}")
        );
        assert!(
            config
                .raw_proxies
                .iter()
                .any(|proxy| proxy.template == "https://wget.la/{url}")
        );
        validate_unique_ids(&config).unwrap();
    }

    #[test]
    fn edits_do_not_modify_cached_bundled_proxy_templates() {
        let original = bundled_defaults();
        let mut draft = bundled_defaults();
        draft.release_proxies[0].enabled = !draft.release_proxies[0].enabled;
        draft.raw_proxies.clear();
        draft.github_api_proxies.clear();
        assert_eq!(bundled_defaults(), original);
        let original_api = bundled_api_defaults();
        let mut api_draft = bundled_api_defaults();
        api_draft.github_api_proxies.clear();
        api_draft.gitee_api_proxies.clear();
        assert_eq!(bundled_api_defaults(), original_api);
    }

    #[test]
    fn userscript_parser_reads_active_release_and_raw_entries_without_execution() {
        let script = r#"
const download_url_us = [
    ['https://release.example/https://github.com', 'US', '[US CDN] provider'],
    ['https://xget.example/gh', 'US', '[US Xget] provider'],
    // ['https://disabled.example/https://github.com', 'US', '[US] disabled'],
], clone_url = [], raw_url = [
    ['https://raw.githubusercontent.com', 'Github', '[JP] native'],
    ['https://raw-proxy.example/https://raw.githubusercontent.com', 'HK', '[HK] cached'],
    ['https://fastly.jsdelivr.net/gh', 'JP', '[Tokyo] CDN'],
], svg = [];
        "#;
        let parsed = parse_userscript(script).unwrap();
        assert_eq!(parsed.release_proxies.len(), 2);
        assert_eq!(parsed.raw_proxies.len(), 2);
        assert_eq!(
            parsed.release_proxies[0].template,
            "https://release.example/{url}"
        );
        assert_eq!(
            parsed.release_proxies[1].template,
            "https://xget.example/gh/{github_path}"
        );
        assert_eq!(
            parsed.raw_proxies[0].template,
            "https://raw-proxy.example/{url}"
        );
        assert_eq!(
            parsed.raw_proxies[1].template,
            "https://fastly.jsdelivr.net/gh/{owner}/{repo}@{branch}/{path}"
        );
        assert_eq!(parsed.raw_proxies[1].regions, ["Tokyo"]);
        assert!(
            parsed
                .release_proxies
                .iter()
                .chain(parsed.raw_proxies.iter())
                .all(|proxy| proxy.builtin && proxy.remote)
        );
    }

    #[test]
    fn formats_raw_and_release_templates_without_expression_evaluation() {
        let target = "https://github.com/acme/tool/raw/refs/heads/main/bin/tool.dll";
        let proxies = vec![GitHubProxy {
            id: "test".into(),
            name: "Test".into(),
            template: "https://proxy.example/{owner}/{repo}/{branch}/{path}?source={url}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];
        assert_eq!(
            proxy_urls(&proxies, target, true, None),
            [
                "https://proxy.example/acme/tool/main/bin/tool.dll?source=https://raw.githubusercontent.com/acme/tool/main/bin/tool.dll"
            ]
        );
        let bad = GitHubProxy {
            template: "https://proxy.example/{expression}".into(),
            ..proxies[0].clone()
        };
        assert!(normalize_proxy(&mut bad.clone()).is_err());
    }

    #[test]
    fn api_proxy_can_replace_gitee_host_with_url_path() {
        let proxies = vec![GitHubProxy {
            id: "gitee-api".into(),
            name: "Gitee API".into(),
            template: "https://oc2gtapi.example{url_path}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];
        assert_eq!(
            proxy_urls(
                &proxies,
                "https://gitee.com/api/v5/repos/acme/tool/releases?per_page=10",
                false,
                None,
            ),
            ["https://oc2gtapi.example/api/v5/repos/acme/tool/releases?per_page=10"]
        );
    }

    #[test]
    fn api_proxy_can_replace_github_host_with_url_path() {
        let proxies = vec![GitHubProxy {
            id: "github-api".into(),
            name: "GitHub API".into(),
            template: "https://oc2ghapi.example{url_path}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];
        assert_eq!(
            proxy_urls(
                &proxies,
                "https://api.github.com/repos/sinai-dev/UnityExplorer/releases?per_page=10&direction=desc&sort=created",
                false,
                None,
            ),
            [
                "https://oc2ghapi.example/repos/sinai-dev/UnityExplorer/releases?per_page=10&direction=desc&sort=created"
            ]
        );
    }

    #[test]
    fn raw_proxy_uses_configured_branch_and_path_including_branch_slashes() {
        let target = "https://github.com/acme/tool/raw/refs/heads/release/1.x/old.dll";
        let proxies = vec![GitHubProxy {
            id: "test".into(),
            name: "Test".into(),
            template: "https://cdn.example/gh/{owner}/{repo}@{branch}/{path}?source={url}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];
        let urls = proxy_urls(
            &proxies,
            target,
            true,
            Some(RawProxyContext {
                branch: "release/1.x",
                file_path: "bin/Release/My MOD.dll",
            }),
        );
        assert_eq!(
            urls,
            [
                "https://cdn.example/gh/acme/tool@release/1.x/bin/Release/My%20MOD.dll?source=https://raw.githubusercontent.com/acme/tool/release/1.x/bin/Release/My%20MOD.dll"
            ]
        );
    }

    #[test]
    fn refresh_keeps_disabled_builtin_proxy_preference() {
        let current = GitHubProxyConfig {
            release_proxies: vec![GitHubProxy {
                id: "old-id".into(),
                name: "Old name".into(),
                template: "https://proxy.example/{url}".into(),
                regions: vec![],
                description: String::new(),
                enabled: false,
                builtin: true,
                remote: false,
            }],
            raw_proxies: vec![],
            ..Default::default()
        };
        let downloaded = GitHubProxyConfig {
            release_proxies: vec![GitHubProxy {
                id: "new-id".into(),
                name: "New name".into(),
                template: "https://proxy.example/{url}".into(),
                regions: vec![],
                description: String::new(),
                enabled: true,
                builtin: true,
                remote: false,
            }],
            raw_proxies: vec![],
            ..Default::default()
        };
        let merged = merge_downloaded(&current, downloaded);
        let refreshed = merged
            .release_proxies
            .iter()
            .find(|proxy| proxy.id == "new-id")
            .unwrap();
        assert!(!refreshed.enabled);
        assert_eq!(refreshed.name, "Old name");
    }

    #[test]
    fn bundled_defaults_include_both_api_channels() {
        let config = bundled_defaults();
        assert!(
            config
                .github_api_proxies
                .iter()
                .any(|proxy| proxy.id == "oc2ghapi")
        );
        assert!(
            config
                .gitee_api_proxies
                .iter()
                .any(|proxy| proxy.id == "oc2gtapi")
        );
    }

    #[test]
    fn synchronize_bundled_defaults_adds_api_channels_to_existing_config() {
        let mut config = GitHubProxyConfig::default();
        assert!(synchronize_bundled_defaults(&mut config));
        assert!(
            config
                .github_api_proxies
                .iter()
                .any(|proxy| proxy.id == "oc2ghapi")
        );
        assert!(
            config
                .gitee_api_proxies
                .iter()
                .any(|proxy| proxy.id == "oc2gtapi")
        );
        assert!(!synchronize_bundled_defaults(&mut config));
    }

    #[test]
    fn saved_config_rejects_deleting_builtins_in_every_category() {
        let current = bundled_defaults();
        for kind in 0..4 {
            let mut proposed = current.clone();
            let list = match kind {
                0 => &mut proposed.release_proxies,
                1 => &mut proposed.raw_proxies,
                2 => &mut proposed.github_api_proxies,
                _ => &mut proposed.gitee_api_proxies,
            };
            assert!(!list.is_empty());
            list.remove(0);
            assert!(validate_saved_config(proposed, &current).is_err());
        }
    }

    #[test]
    fn saved_config_keeps_builtin_edits_order_and_disabled_state() {
        let current = bundled_defaults();
        let mut proposed = current.clone();
        proposed.release_proxies.reverse();
        let edited = &mut proposed.release_proxies[0];
        edited.name = "Edited proxy".into();
        edited.template = "https://edited.example/{url}".into();
        edited.regions = vec!["Custom region".into()];
        edited.description = "Edited description".into();
        edited.enabled = false;
        edited.builtin = false;
        let id = edited.id.clone();

        let mut saved = validate_saved_config(proposed, &current).unwrap();
        assert_eq!(saved.release_proxies[0].id, id);
        assert!(saved.release_proxies[0].builtin);
        assert_eq!(saved.release_proxies[0].name, "Edited proxy");
        assert_eq!(
            saved.release_proxies[0].template,
            "https://edited.example/{url}"
        );
        assert_eq!(saved.release_proxies[0].description, "Edited description");
        assert_eq!(saved.release_proxies[0].regions, ["Custom region"]);
        assert!(!saved.release_proxies[0].enabled);
        let before_reload = saved.clone();
        assert!(!synchronize_bundled_defaults(&mut saved));
        assert_eq!(saved, before_reload);
    }

    #[test]
    fn saved_config_protects_source_builtins_and_allows_removing_custom_entries() {
        let mut current = bundled_defaults();
        let source_proxy = GitHubProxy {
            id: "source-proxy".into(),
            name: "Source proxy".into(),
            template: "https://source-proxy.example/{url}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: true,
            remote: false,
        };
        current.release_proxies.push(source_proxy.clone());
        current.release_proxies.push(GitHubProxy {
            id: "custom-proxy".into(),
            template: "https://custom-proxy.example/{url}".into(),
            builtin: false,
            remote: false,
            ..source_proxy
        });
        let mut proposed = current.clone();
        proposed.release_proxies.retain(|proxy| !proxy.builtin);
        assert!(validate_saved_config(proposed, &current).is_err());

        let mut proposed = current.clone();
        proposed.release_proxies.retain(|proxy| proxy.builtin);
        assert!(validate_saved_config(proposed, &current).is_ok());
    }

    #[test]
    fn saved_config_rejects_moving_or_merging_distinct_builtin_ids() {
        let current = bundled_defaults();
        let mut proposed = current.clone();
        let moved = proposed.github_api_proxies.remove(0);
        proposed.gitee_api_proxies.push(moved);
        assert!(validate_saved_config(proposed, &current).is_err());

        let mut proposed = current.clone();
        proposed.release_proxies[1].template = proposed.release_proxies[0].template.clone();
        assert!(validate_saved_config(proposed, &current).is_err());
    }

    #[test]
    fn custom_duplicates_cannot_clear_builtin_identity_or_enable_a_disabled_proxy() {
        let builtin = GitHubProxy {
            id: "builtin".into(),
            name: "Builtin".into(),
            template: "https://proxy.example/{url}".into(),
            regions: vec![],
            description: String::new(),
            enabled: false,
            builtin: true,
            remote: false,
        };
        let custom = GitHubProxy {
            id: "custom".into(),
            enabled: true,
            builtin: false,
            remote: false,
            ..builtin.clone()
        };
        for entries in [
            vec![builtin.clone(), custom.clone()],
            vec![custom, builtin.clone()],
        ] {
            let merged = deduplicate(entries);
            assert_eq!(merged.len(), 1);
            assert_eq!(merged[0].id, builtin.id);
            assert!(merged[0].builtin);
            assert!(!merged[0].enabled);
        }
    }

    fn test_proxy(id: &str, template: &str, remote: bool) -> GitHubProxy {
        GitHubProxy {
            id: id.into(),
            name: id.into(),
            template: template.into(),
            regions: Vec::new(),
            description: String::new(),
            enabled: true,
            builtin: true,
            remote,
        }
    }

    #[test]
    fn toml_cannot_claim_userscript_origin() {
        let config = parse_manifest(
            r#"
[[release_proxies]]
id = "toml-proxy"
name = "TOML"
template = "https://proxy.example/{url}"
remote = true
"#,
        )
        .unwrap();
        assert!(!config.release_proxies[0].remote);
    }

    #[test]
    fn asset_order_keeps_remote_and_fallback_user_order_without_reordering_api() {
        let fallback_one = test_proxy("fallback-one", "https://fallback-one.example/{url}", false);
        let fallback_two = test_proxy("fallback-two", "https://fallback-two.example/{url}", false);
        let remote_one = test_proxy("remote-one", "https://remote-one.example/{url}", true);
        let remote_two = test_proxy("remote-two", "https://remote-two.example/{url}", true);
        let left = GitHubProxyConfig {
            release_proxies: vec![
                fallback_two.clone(),
                remote_two.clone(),
                fallback_one.clone(),
                remote_one.clone(),
            ],
            raw_proxies: vec![fallback_two, remote_two, fallback_one, remote_one],
            github_api_proxies: vec![test_proxy(
                "api-one",
                "https://api-one.example{url_path}",
                false,
            )],
            ..Default::default()
        };
        let right = GitHubProxyConfig {
            github_api_proxies: vec![test_proxy(
                "api-two",
                "https://api-two.example{url_path}",
                true,
            )],
            ..Default::default()
        };
        let merged = merge_source_configs(&left, right);
        for proxies in [&merged.release_proxies, &merged.raw_proxies] {
            assert_eq!(
                proxies
                    .iter()
                    .map(|proxy| proxy.id.as_str())
                    .collect::<Vec<_>>(),
                ["remote-two", "remote-one", "fallback-two", "fallback-one"]
            );
        }
        assert_eq!(
            merged
                .github_api_proxies
                .iter()
                .map(|proxy| proxy.id.as_str())
                .collect::<Vec<_>>(),
            ["api-one", "api-two"]
        );
        assert_eq!(validate_config(merged.clone()).unwrap(), merged);
    }

    #[test]
    fn duplicate_source_templates_keep_bundled_id_remote_label_and_disabled_choice() {
        let bundled = bundled_defaults();
        let original = bundled
            .release_proxies
            .iter()
            .find(|proxy| proxy.id == "oc2gh-release")
            .expect("bundled oc2gh Release fallback")
            .clone();
        let remote = GitHubProxyConfig {
            release_proxies: vec![GitHubProxy {
                id: "userscript-duplicate".into(),
                remote: true,
                ..original.clone()
            }],
            ..Default::default()
        };
        for incoming in [
            merge_source_configs(&bundled, remote.clone()),
            merge_source_configs(&remote, bundled.clone()),
        ] {
            let repeated = incoming
                .release_proxies
                .iter()
                .filter(|proxy| template_key(proxy) == template_key(&original))
                .collect::<Vec<_>>();
            assert_eq!(repeated.len(), 1);
            assert_eq!(repeated[0].id, original.id);
            assert!(repeated[0].remote && repeated[0].builtin);
            let mut previous = bundled.clone();
            let edited = previous
                .release_proxies
                .iter_mut()
                .find(|proxy| proxy.id == original.id)
                .unwrap();
            edited.enabled = false;
            edited.name = "User edit".into();
            let refreshed = merge_downloaded(&previous, incoming);
            let entry = refreshed
                .release_proxies
                .iter()
                .find(|proxy| proxy.id == original.id)
                .unwrap();
            assert!(entry.remote);
            assert!(!entry.enabled);
            assert_eq!(entry.name, "User edit");
            let urls = proxy_urls(
                &refreshed.release_proxies,
                "https://github.com/acme/tool/releases/download/v1/tool.zip",
                false,
                None,
            );
            assert!(
                !urls
                    .iter()
                    .any(|url| url.starts_with("https://oc2gh.ch3ng.top/"))
            );
        }
    }

    #[test]
    fn saved_source_labels_are_trusted_persisted_and_cannot_be_forged() {
        let mut current = bundled_defaults();
        current.release_proxies.push(test_proxy(
            "remote-release",
            "https://remote-release.example/{url}",
            true,
        ));
        current.raw_proxies.push(test_proxy(
            "remote-raw",
            "https://remote-raw.example/{url}",
            true,
        ));
        let mut proposed = current.clone();
        for proxy in proposed
            .release_proxies
            .iter_mut()
            .chain(proposed.raw_proxies.iter_mut())
        {
            proxy.remote = !proxy.remote;
            proxy.builtin = false;
        }
        let edited = proposed
            .release_proxies
            .iter_mut()
            .find(|proxy| proxy.id == "remote-release")
            .unwrap();
        edited.enabled = false;
        edited.name = "Edited remote".into();
        proposed.release_proxies.push(GitHubProxy {
            id: "custom-forged".into(),
            builtin: true,
            remote: true,
            ..test_proxy("unused", "https://custom.example/{url}", false)
        });
        let saved = validate_saved_config(proposed, &current).unwrap();
        let remote = saved
            .release_proxies
            .iter()
            .find(|proxy| proxy.id == "remote-release")
            .unwrap();
        assert!(remote.remote && remote.builtin);
        assert!(!remote.enabled);
        assert_eq!(remote.name, "Edited remote");
        let custom = saved
            .release_proxies
            .iter()
            .find(|proxy| proxy.id == "custom-forged")
            .unwrap();
        assert!(!custom.remote && !custom.builtin);
        assert!(
            saved
                .release_proxies
                .iter()
                .filter(|proxy| proxy.id != "remote-release")
                .all(|proxy| !proxy.remote)
        );
        assert!(saved.raw_proxies[0].remote);
        let mut restored: GitHubProxyConfig =
            toml::from_str(&toml::to_string_pretty(&saved).unwrap()).unwrap();
        assert!(!synchronize_bundled_defaults(&mut restored));
        assert_eq!(restored, saved);
        let mut deleted = saved.clone();
        deleted
            .release_proxies
            .retain(|proxy| proxy.id != "remote-release");
        assert!(validate_saved_config(deleted, &saved).is_err());
    }

    #[test]
    fn missing_or_empty_userscripts_preserve_complete_bundled_fallbacks() {
        let mut current = bundled_defaults();
        current.release_proxies.reverse();
        let original_id = current.release_proxies[0].id.clone();
        current.release_proxies[0].template = "https://edited-fallback.example/{url}".into();
        current.release_proxies[0].enabled = false;
        for script in [
            "Repository or file not found",
            "const download_url_us = [], raw_url = [];",
        ] {
            assert!(parse_userscript(script).is_err());
            let fetched = parse_userscript(script).unwrap_or_default();
            let refreshed = merge_downloaded(&current, fetched);
            assert_eq!(refreshed, current);
            assert!(
                refreshed
                    .release_proxies
                    .iter()
                    .any(|proxy| proxy.id == original_id && !proxy.enabled)
            );
            assert!(
                refreshed
                    .release_proxies
                    .iter()
                    .chain(refreshed.raw_proxies.iter())
                    .all(|proxy| proxy.builtin && !proxy.remote)
            );
        }
        let from_empty =
            merge_downloaded(&GitHubProxyConfig::default(), GitHubProxyConfig::default());
        assert_eq!(from_empty, bundled_defaults());
    }

    #[test]
    fn bundled_url_path_proxies_render_one_slash_and_request_each_target_once() {
        let bundled = bundled_defaults();
        for (proxies, target, is_raw, expected) in [
            (
                &bundled.release_proxies,
                "https://github.com/acme/tool/releases/download/v1/tool.zip",
                false,
                "https://oc2gh.ch3ng.top/acme/tool/releases/download/v1/tool.zip",
            ),
            (
                &bundled.raw_proxies,
                "https://github.com/acme/tool/raw/refs/heads/main/bin/tool.dll",
                true,
                "https://oc2gh.ch3ng.top/acme/tool/raw/refs/heads/main/bin/tool.dll",
            ),
        ] {
            let source = proxies
                .iter()
                .find(|proxy| proxy.name == "oc2gh.ch3ng.top")
                .unwrap()
                .clone();
            assert_eq!(source.template, "https://oc2gh.ch3ng.top/{url_path}");
            let duplicate = GitHubProxy {
                id: "other-id".into(),
                template: source.template.replace("/{url_path}", "{url_path}"),
                ..source.clone()
            };
            assert_eq!(
                proxy_urls(&[source, duplicate], target, is_raw, None),
                [expected]
            );
        }
    }

    #[test]
    fn external_toml_alias_uses_bundled_id_through_refresh_save_and_reload() {
        let mut current = bundled_defaults();
        let original = current.release_proxies[0].clone();
        current.release_proxies[0].enabled = false;
        current.release_proxies[0].remote = true;
        current.release_proxies[0].name = "User name".into();
        current.release_proxies[0].description = "User description".into();
        let incoming = parse_manifest(&format!(
            "[[release_proxies]]\nid = \"external-toml-alias\"\nname = \"External\"\ntemplate = {:?}\n",
            original.template
        )).unwrap();
        let mut refreshed = merge_downloaded(&current, incoming);
        let proxy = refreshed
            .release_proxies
            .iter()
            .find(|proxy| proxy.id == original.id)
            .unwrap();
        assert!(proxy.remote && proxy.builtin);
        assert!(!proxy.enabled);
        assert_eq!(proxy.name, "User name");
        assert_eq!(proxy.description, "User description");
        assert!(
            !refreshed
                .release_proxies
                .iter()
                .any(|proxy| proxy.id == "external-toml-alias")
        );
        assert!(!synchronize_bundled_defaults(&mut refreshed));
        let saved = validate_saved_config(refreshed.clone(), &refreshed).unwrap();
        assert_eq!(saved, refreshed);
        let mut restored: GitHubProxyConfig =
            toml::from_str(&toml::to_string_pretty(&saved).unwrap()).unwrap();
        assert!(!synchronize_bundled_defaults(&mut restored));
        assert_eq!(restored, saved);
        assert_eq!(
            validate_saved_config(restored.clone(), &restored).unwrap(),
            saved
        );
    }

    #[test]
    fn old_duplicate_alias_cannot_displace_bundled_identity_or_edit() {
        let mut current = bundled_defaults();
        let original = current.release_proxies[0].clone();
        current.release_proxies[0].name = "User edited bundled proxy".into();
        current.release_proxies[0].enabled = false;
        current.release_proxies[0].remote = true;
        current.release_proxies.insert(
            0,
            GitHubProxy {
                id: "old-toml-alias".into(),
                remote: false,
                ..original.clone()
            },
        );
        assert!(synchronize_bundled_defaults(&mut current));
        let proxy = current
            .release_proxies
            .iter()
            .find(|proxy| proxy.id == original.id)
            .unwrap();
        assert_eq!(proxy.name, "User edited bundled proxy");
        assert!(proxy.remote && proxy.builtin);
        assert!(!proxy.enabled);
        assert!(
            !current
                .release_proxies
                .iter()
                .any(|proxy| proxy.id == "old-toml-alias")
        );
        assert!(!synchronize_bundled_defaults(&mut current));
        assert_eq!(
            validate_saved_config(current.clone(), &current).unwrap(),
            current
        );
    }
}
