use crate::models::MapRepositoryConfig;

/// 地图的身份键：`author + levelSetName`，写了中文名时再加上它。
///
/// 这是源条目去重用的精确键，不是本地安装身份的二元比较器：本地比较只在
/// 双方都有中文名时比中文名，源键则把当前条目非空中文名纳入。
///
/// 中文名只在**填写了**的时候参与：老源都没有这个字段，此时拼出的字符串与
/// 旧版逐字节相同，去重结果与 `map_repository_id` 派生的 ID 都不变。
pub fn map_identity(entry: &MapRepositoryConfig) -> String {
    let name_zh = entry.level_set_name_zh.trim().to_lowercase();
    if name_zh.is_empty() {
        format!(
            "{}\u{1}{}",
            entry.author.trim().to_lowercase(),
            entry.level_set_name.trim().to_lowercase()
        )
    } else {
        format!(
            "{}\u{1}{}\u{1}{}",
            entry.author.trim().to_lowercase(),
            entry.level_set_name.trim().to_lowercase(),
            name_zh
        )
    }
}

/// 条目声明的固定 Release tag（空串与缺省等价）。
///
/// 只认「有内容」的 tag：源文档里 `tag = ""` 是作者清空残留的写法，不能当成
/// 「固定一个名叫空串的 tag」。
pub(crate) fn map_tag(entry: &MapRepositoryConfig) -> Option<&str> {
    entry
        .tag
        .as_deref()
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
}

/// 报错与日志里指代一张地图的统一写法：有名字用名字，没有用仓库坐标。
pub(super) fn map_label(entry: &MapRepositoryConfig) -> String {
    if entry.name.trim().is_empty() {
        format!("{}/{}", entry.username, entry.repository)
    } else {
        entry.name.clone()
    }
}

/// 由「源 ID + 地图身份」派生稳定 ID。
///
/// 不用 UUID：源条目每次刷新重建，ID 必须能接回缓存与关系快照；源 ID、
/// 作者、集名及可选中文名按既有规则编码，不因展示名或备注改变而漂移。
pub(crate) fn map_repository_id(source_id: &str, map: &MapRepositoryConfig) -> String {
    let slug = |value: &str| {
        value
            .trim()
            .to_lowercase()
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect::<String>()
            .trim_matches('-')
            .to_string()
    };
    let mut id = format!(
        "{}::{}::{}",
        slug(source_id),
        slug(&map.author),
        slug(&map.level_set_name)
    );
    /*
     * 中文名只有填写时才追加一段：老源没有这个字段，拼出的 ID 与旧版逐字节
     * 相同，已安装地图的绑定与缓存不会因为升级到这个版本而失联。
     *
     * 纯中文名 `slug` 之后会变成空串，那样两张同集名的地图仍会派生同一个
     * ID，所以空 slug 时改用内容哈希前缀减少碰撞；这不是无碰撞承诺。
     */
    let name_zh = map.level_set_name_zh.trim();
    if !name_zh.is_empty() {
        let slug_zh = slug(name_zh);
        id.push_str("::");
        if slug_zh.is_empty() {
            id.push_str(&format!("~{}", short_hash(name_zh)));
        } else {
            id.push_str(&slug_zh);
        }
    }
    id
}

/// 纯中文名经 `slug` 之后会变成空串，用内容哈希区分不同中文名。
fn short_hash(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(value.as_bytes());
    digest[..4]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
