use crate::locale_tag::with_params;

fn invalid_repository(value: &str) -> String {
    with_params("mapPublish.invalidRepository", &[("value", value)])
}

fn invalid_tag(value: &str) -> String {
    with_params("mapPublish.invalidTag", &[("value", value)])
}

pub(super) fn invalid_asset(reason: &str) -> String {
    with_params("mapPublish.invalidAsset", &[("reason", reason)])
}

/// GitHub 的 slug：owner / 仓库名都只由这些字符构成。
///
/// 只做**形状**校验，不做真实性校验（真实与否由 API 判定）。拒绝 `/` 与
/// `..` 之类能改变请求路径的东西 —— 后续 URL 是字符串拼接出来的。
fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && !value.starts_with('.')
        && !value.ends_with('.')
        && value
            .chars()
            .all(|item| item.is_ascii_alphanumeric() || matches!(item, '-' | '_' | '.'))
}

pub(super) fn validate_coordinates(
    owner: &str,
    repository: &str,
) -> Result<(String, String), String> {
    let owner = owner.trim();
    let repository = repository.trim();
    if !valid_slug(owner) || !valid_slug(repository) {
        return Err(invalid_repository(&format!("{owner}/{repository}")));
    }
    Ok((owner.to_string(), repository.to_string()))
}

/// tag 是**一张地图**在仓库里的唯一 ID，不是版本号。
///
/// 消费端（`map_sources/release_tags.rs::load_release_tag_maps`）只把这个字符串当身份写进
/// `MapRepositoryConfig.tag`；版本号来自该 Release 的附件名 —— 管理器用
/// `map_note::versioned_asset_rule(metadata.asset, tag)` 的正则从附件名里抽
/// `[0-9]+(?:\.[0-9]+)*`。所以 tag 可以是 `chan`、中文名，只要是一个合法的
/// git ref；同名 tag 下换附件就是发新版本。
///
/// 拒绝项取自 git 的 ref 规则：空格、`~ ^ : ? * [ \`、`..`、`@{`、控制字符、
/// `/`（保持「一个 tag 一张图」的平面结构，也避免进 URL 时编码歧义）、`.` 或
/// `-` 开头（前者 git 不认，后者会被当成命令行选项）、`.` 结尾、`.lock` 结尾
/// （git 自己也不允许）以及超长值。
pub(super) fn validate_tag(tag: &str) -> Result<String, String> {
    let tag = tag.trim();
    let lower = tag.to_lowercase();
    let forbidden = |item: char| {
        item.is_control() || matches!(item, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\')
    };
    let ok = !tag.is_empty()
        && tag.len() <= 100
        && !tag.starts_with('-')
        && !tag.starts_with('.')
        && !tag.ends_with('.')
        && !lower.ends_with(".lock")
        && !tag.contains('/')
        && !tag.contains("..")
        && !tag.contains("@{")
        && !tag.chars().any(forbidden);
    if !ok {
        return Err(invalid_tag(tag));
    }
    Ok(tag.to_string())
}

pub(super) fn validate_new_tag(tag: &str) -> Result<String, String> {
    let tag = tag.trim();
    if tag.is_empty() || tag.len() > 100 || !tag.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(invalid_tag(tag));
    }
    Ok(tag.to_string())
}

/// 附件名必须是单个安全文件名；旧 tag 和显式模板可包含中文。
pub(super) fn validate_asset_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name.chars().all(|item| {
            !item.is_control()
                && !matches!(item, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        })
        && !name.ends_with('.')
        && !name.ends_with(' ');
    if !ok {
        return Err(invalid_asset("name"));
    }
    Ok(name.to_string())
}

/// 严格百分号编码（不保留 `/`，与 `locale_tag::encode` 的宽松口径不同：
/// 这里的值要进 URL 查询串）。
pub(super) fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'_' | b'-' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests;
