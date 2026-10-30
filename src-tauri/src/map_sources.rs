//! 地图仓库源的解析与同步。
//!
//! ## 与 `repository_sources.rs` 的关系
//!
//! 两者的**流水线完全一样**：
//!
//! ```text
//! 索引文档（maps.toml） → 声明若干子源 → 每个子源声明若干仓库 → 各自请求 Release
//! ```
//!
//! 所以这里复用 `SourceLocation`（地址解析、内置资源、同仓库相对路径）与
//! `releases` 的抓取逻辑，只重新定义「一条仓库长什么样」：
//!
//! | | MOD 仓库 | 地图仓库 |
//! |---|---|---|
//! | 绑定本地 | `plugin_guid` 等 DLL 内嵌元数据 | `author + level_set_name` |
//! | 安装单元 | MOD 包（可含多个 DLL） | 关卡集目录（恰好一个） |
//! | 关系语义 | 插件依赖 | 框架依赖（DIYLevel / Web Runtime） |
//!
//! 关系表复用 `RepositoryRelation`：「某 GUID 的版本必须在区间内」这个语义
//! 两边没有差别。
use crate::config::ConfigError;
use crate::models::{AppConfig, MapRepositoryConfig, RepositoryRelation};
use crate::repository_sources::SourceLocation;
use serde::Deserialize;
use std::collections::HashSet;

/// 内置地图索引的资源名。
pub const BUILTIN_MAP_INDEX: &str = "maps.toml";

/// 官方地图索引的源 ID（与仓库索引的 `official-repository-index` 对称）。
pub const DEFAULT_MAP_INDEX_ID: &str = "official-map-index";

/// 中国大陆的官方地图索引地址。
///
/// 与仓库索引放在**同一个仓库**里，只是文件名不同 —— 维护者改一处就能同时
/// 更新两类源，不需要记两个地址。
pub const DEFAULT_MAP_INDEX_GITEE_URL: &str =
    "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/maps.toml";
/// 国际地区的官方地图索引地址。
pub const DEFAULT_MAP_INDEX_GITHUB_URL: &str =
    "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/maps.toml";

/// 按地区选官方地图索引地址。
pub fn default_map_index_url(region: &str) -> &'static str {
    if region == crate::config::REGION_CHINESE_MAINLAND {
        DEFAULT_MAP_INDEX_GITEE_URL
    } else {
        DEFAULT_MAP_INDEX_GITHUB_URL
    }
}

/// 地图源文档。
///
/// 结构与 `RepositorySourceDocument` 对称：一个源文件既可以直接声明地图，
/// 也可以声明下一级子源 —— 于是「一个作者一个 TOML、里面放他的多个地图」
/// 和「按主题拆成多份子源」两种组织方式都支持。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MapSourceDocument {
    #[serde(default)]
    pub maps: Vec<MapRepositoryConfig>,
    #[serde(default)]
    pub map_sources: Vec<MapSourceDeclaration>,
}

/// 源文档里对子源的声明。
///
/// 昵称与地区只影响展示和筛选：地图页按 `group` 分组，源名本身不显示，
/// 所以这里只保留加载需要的最小字段集，避免维护一份没人读的配置。
#[derive(Debug, Clone, Deserialize)]
pub struct MapSourceDeclaration {
    pub id: String,
    /// 子源地址：完整 URL、内置资源名，或**相对当前文档**的路径。
    pub url: String,
    #[serde(default)]
    pub region: Option<String>,
    /// 是否属于内置树。省略时继承父源 —— 远程文档不能自行提权。
    #[serde(default)]
    pub builtin: Option<bool>,
}

/// 解析地图源文档。
///
/// 与仓库源解析相比刻意宽松：地图只有一个「必须」字段（`level_set_name`），
/// 其余都能从 Release 与本地 info 补齐。严格校验留给真正安装时的身份比对 ——
/// 那是唯一会因错配产生实际后果的地方。
pub fn parse_document(
    text: &str,
    origin: &SourceLocation,
) -> Result<MapSourceDocument, ConfigError> {
    let document: MapSourceDocument = toml::from_str(text).map_err(|error| {
        ConfigError::Invalid(crate::locale_tag::with_params(
            "source.tomlInvalid",
            &[("reason", &error.to_string())],
        ))
    })?;

    let mut document = document;
    /*
     * `name` / `group` / `note` 的单语言镜像从 `*_i18n.zh-CN` 折进来。
     *
     * 源文档只需要写多语言表（与仓库侧 `name_i18n` 的约定一致）：
     * 要求作者同时写 `name` 和 `name_i18n` 只会制造两处不一致。
     */
    for entry in &mut document.maps {
        fold_i18n_mirror(&mut entry.name, &entry.name_i18n);
        fold_i18n_mirror(&mut entry.group, &entry.group_i18n);
        fold_i18n_mirror(&mut entry.note, &entry.note_i18n);
    }

    let mut seen = HashSet::new();
    for entry in &document.maps {
        let key = map_identity(entry);
        if !seen.insert(key.clone()) {
            log::warn!(
                "地图源文档里出现重复地图：标识={}，来源={}",
                key,
                crate::repository_sources::safe_log_address(&origin.address())
            );
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "mapSource.duplicateEntry",
                &[("identity", &key)],
            )));
        }
        validate_map_entry(entry, origin)?;
    }
    Ok(document)
}

/// 把 `*_i18n` 的 `zh-CN` 折进单语言镜像字段（为空时才填）。
fn fold_i18n_mirror(mirror: &mut String, table: &std::collections::BTreeMap<String, String>) {
    if !mirror.trim().is_empty() {
        return;
    }
    if let Some(value) = table.get("zh-CN").filter(|value| !value.trim().is_empty()) {
        *mirror = value.clone();
    }
}

/// 地图的身份键：`author + levelSetName`。
///
/// 与本地绑定的判定**必须**是同一个函数，否则「文档里去重用的键」和
/// 「安装时比对的键」会分叉，表现为「文档里不重复，装的时候却说重复」。
pub fn map_identity(entry: &MapRepositoryConfig) -> String {
    format!(
        "{}\u{1}{}",
        entry.author.trim().to_lowercase(),
        entry.level_set_name.trim().to_lowercase()
    )
}

/// 单条地图的必填校验。
///
/// 只查「解析后一定会用到、缺失就没有意义」的字段：
///
/// - `level_set_name` / `author`：绑定身份，缺了就无法与本地关卡集对上；
/// - `provider` / `username` / `repository`：取 Release 所需；
/// - `asset_rules`：没有规则就选不出要下载哪个文件。
///
/// 名字与备注缺失**不算错**：界面会退回仓库坐标显示，总比整份源解析失败好。
fn validate_map_entry(
    entry: &MapRepositoryConfig,
    origin: &SourceLocation,
) -> Result<(), ConfigError> {
    let label = if entry.name.trim().is_empty() {
        format!("{}/{}", entry.username, entry.repository)
    } else {
        entry.name.clone()
    };
    if entry.level_set_name.trim().is_empty() {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "mapSource.missingLevelSetName",
            &[("map", &label)],
        )));
    }
    if entry.username.trim().is_empty() || entry.repository.trim().is_empty() {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "mapSource.missingCoordinates",
            &[("map", &label)],
        )));
    }
    if entry.asset_rules.is_empty() {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "mapSource.missingAssetRules",
            &[("map", &label)],
        )));
    }
    log::debug!(
        "地图条目校验通过：地图={}，身份={}，来源={}",
        label,
        map_identity(entry),
        crate::repository_sources::safe_log_address(&origin.address())
    );
    Ok(())
}

/// 读取并解析一个地图源文档（内置资源或远程地址）。
///
/// 下载部分直接复用 `repository_sources` 的实现：超时（30 秒）、禁缓存头、
/// 大小上限、重定向后的相对地址解析、错误分类 —— 这些逐项都踩过坑，复制一份
/// 只会得到两套会各自漂移的规则。
pub async fn load_document(location: &SourceLocation) -> Result<MapSourceDocument, ConfigError> {
    crate::repository_sources::load_text_document(location, parse_document).await
}

/// 用新加载的地图列表替换配置里的地图，**保留同 ID 条目的 Release 缓存**。
///
/// 每次刷新都是「源文档 → 全新条目」，直接替换会让 10 分钟缓存失效，
/// 于是每次刷新都把整批地图的 API 重打一遍（MOD 侧踩过完全相同的坑）。
///
/// 只保留缓存与失败原因，其余字段以源文档为准：
///
/// - `enabled` 必须听文档的 —— 它是作者的声明（替换或下线某张图）。管理器
///   目前没有「用户手动隐藏地图」这个操作，保留旧值只会让作者下线的图复活。
/// - 名字、资产规则、依赖版本改了就该立刻生效。
///
/// 返回是否有条目变化（新增/删除/换序），供调用方决定要不要写盘。
pub(crate) fn replace_maps_keeping_runtime_state(
    config: &mut AppConfig,
    fresh: Vec<MapRepositoryConfig>,
) -> bool {
    let previous = std::mem::take(&mut config.map_repositories);
    let before = previous.len();
    let mut merged = Vec::with_capacity(fresh.len());
    for mut map in fresh {
        if let Some(old) = previous.iter().find(|old| old.id == map.id) {
            map.cached_releases = old.cached_releases.clone();
            map.releases_cached_at = old.releases_cached_at;
            map.last_error = old.last_error.clone();
        }
        merged.push(map);
    }
    // 顺序来自源文档，长度变化也算变化
    let changed = before != merged.len()
        || previous
            .iter()
            .zip(merged.iter())
            .any(|(old, new)| old.id != new.id);
    config.map_repositories = merged;
    changed
}

/// 同步加载内置地图源树（索引 → 子源 → 地图）。
///
/// 只走编译期嵌入的资源，因此不需要网络、也没有失败路径。两个用途：
/// 1. 首次刷新前给地图页一个可用的初始列表（不必先等一次网络往返）；
/// 2. 远程索引整体不可用时的回退，与仓库侧的官方源回退同一套条件。
pub(crate) fn bundled_map_tree() -> Vec<MapRepositoryConfig> {
    let mut collected = Vec::new();
    let Ok(index) = SourceLocation::parse(BUILTIN_MAP_INDEX) else {
        return collected;
    };
    collect_bundled_maps(&index, DEFAULT_MAP_INDEX_ID, 0, &mut collected);
    collected
}

fn collect_bundled_maps(
    location: &SourceLocation,
    source_id: &str,
    depth: usize,
    collected: &mut Vec<MapRepositoryConfig>,
) {
    let Ok(text) = location.bundled_text() else {
        return;
    };
    let Ok(document) = parse_document(text, location) else {
        return;
    };
    for mut map in document.maps {
        map.source_id = Some(source_id.to_string());
        map.builtin = true;
        map.id = map_repository_id(source_id, &map);
        collected.push(map);
    }
    if depth + 1 > MAX_MAP_SOURCE_DEPTH {
        return;
    }
    for child in document.map_sources {
        let Ok(child_location) = SourceLocation::resolve(&child.url, Some(location)) else {
            continue;
        };
        collect_bundled_maps(&child_location, child.id.trim(), depth + 1, collected);
    }
}

/// 当前地区可用的地图（启用 + 地区匹配）。
///
/// 与仓库侧 `repository_is_available` 同一条语义：`enabled=false` 与
/// 「地区不适用」都表示「当前用户看不见这张地图」，前端不该为它发请求。
pub fn map_is_available(map: &MapRepositoryConfig, current_region: &str) -> bool {
    if !map.enabled {
        return false;
    }
    let normalized = map
        .region
        .as_deref()
        .map(|value| crate::config::normalize_repository_region(Some(value)))
        .filter(|value| !value.is_empty());
    match normalized {
        None => true,
        Some(value) if value == crate::config::REGION_CHINESE_MAINLAND => {
            current_region == crate::config::REGION_CHINESE_MAINLAND
        }
        Some(_) => current_region != crate::config::REGION_CHINESE_MAINLAND,
    }
}

/// 从配置里取出当前地区可用的全部地图。
pub fn available_maps(config: &AppConfig, current_region: &str) -> Vec<MapRepositoryConfig> {
    config
        .map_repositories
        .iter()
        .filter(|map| map_is_available(map, current_region))
        .cloned()
        .collect()
}

/// 地图源树的最大递归深度。
///
/// 与仓库源一样按「索引 → 作者源 → （理论上还能再分）」设计，但不设上限
/// 会让一份互相引用的远程文档把加载拖死。3 层足够，超出的层级静默丢弃
/// 并记 warn —— 宁可少显示作者源，也不要卡住整页。
const MAX_MAP_SOURCE_DEPTH: usize = 3;

/// 加载整棵地图源树，返回其中声明的全部地图。
///
/// ## 与 `repository_sources` 的差别
///
/// 仓库侧要把源树**持久化**进 `repository_sources`，因为界面要展示父子源、
/// 每个源有独立的错误状态与刷新按钮。地图页是一个扁平的分组卡片网格，
/// 源只是归类方式，所以这里只产出地图条目本身（`source_id` 记录归属），
/// 不额外维护一棵需要同步、清理、迁移的源树。
///
/// 加载顺序：索引 → 子源（深度优先、按声明顺序）→ 每个文档里的地图条目。
pub async fn load_map_tree(
    region: &str,
    builtin_only: bool,
) -> Result<Vec<MapRepositoryConfig>, ConfigError> {
    let index_reference = if builtin_only {
        BUILTIN_MAP_INDEX
    } else {
        default_map_index_url(region)
    };
    let index = SourceLocation::parse(index_reference)?;
    let root_builtin = matches!(index, SourceLocation::Bundled(_));
    log::info!(
        "开始加载地图源树：索引={}，内置={}",
        crate::repository_sources::safe_log_address(&index.address()),
        root_builtin
    );

    let mut collected = Vec::new();
    let mut visited = HashSet::new();
    visited.insert(crate::repository_sources::source_address_key(
        &index.address(),
        None,
    ));
    load_map_document(
        index,
        DEFAULT_MAP_INDEX_ID,
        None,
        root_builtin,
        region,
        0,
        &mut visited,
        &mut collected,
    )
    .await?;
    log::info!("地图源树加载完成：地图={}", collected.len());
    Ok(collected)
}

/// 读取一个地图源文档，并把它的地图与子源递归收进 `collected`。
#[allow(clippy::too_many_arguments)]
async fn load_map_document(
    location: SourceLocation,
    source_id: &str,
    parent_id: Option<&str>,
    builtin: bool,
    region: &str,
    depth: usize,
    visited: &mut HashSet<(String, String)>,
    collected: &mut Vec<MapRepositoryConfig>,
) -> Result<(), ConfigError> {
    let document = load_document(&location).await?;
    for mut map in document.maps {
        // 源归属只在本进程内有效：导出配置时写出的是作者本地的源 ID
        map.source_id = Some(source_id.to_string());
        map.builtin = builtin;
        // ID 必须跨刷新稳定：缓存、安装清单和「用户手动禁用」都挂在它上面。
        // 用「源 ID + 身份」派生，重建出来的地图能接回上一次的缓存。
        map.id = map_repository_id(source_id, &map);
        collected.push(map);
    }

    if depth + 1 > MAX_MAP_SOURCE_DEPTH {
        if !document.map_sources.is_empty() {
            log::warn!(
                "地图源层级超过上限，丢弃更深层的子源：源={}，上限={}",
                source_id,
                MAX_MAP_SOURCE_DEPTH
            );
        }
        return Ok(());
    }

    for child in document.map_sources {
        // 地区不匹配的子源整个跳过：它的地址对当前用户没有意义，
        // 请求它只会换来一次 404 和一条无从处理的错误
        if !child_matches_region(child.region.as_deref(), region) {
            log::debug!("跳过地区不匹配的地图子源：源={}", child.id);
            continue;
        }
        let child_location = match SourceLocation::resolve(&child.url, Some(&location)) {
            Ok(location) => location,
            Err(error) => {
                // 单个子源地址写错不该让整棵树失败：其余作者的地图仍然可用
                log::warn!(
                    "地图子源地址无效，跳过：源={}，地址={}，原因={}",
                    child.id,
                    crate::repository_sources::safe_log_address(&child.url),
                    error
                );
                continue;
            }
        };
        let key = crate::repository_sources::source_address_key(&child_location.address(), None);
        if !visited.insert(key) {
            log::warn!("检测到地图源循环引用，跳过：源={}", child.id);
            continue;
        }
        // 远程文档不能自行提权：省略时继承父源
        let child_builtin = builtin && child.builtin.unwrap_or(true);
        /*
         * 子源失败仍然继续加载其余子源 —— 一个作者的 TOML 404 不该让
         * 其他作者的地图从页面上消失。失败只记日志，索引层不因此报错。
         */
        let child_id = child.id.trim().to_string();
        let boxed = Box::pin(load_map_document(
            child_location,
            &child_id,
            Some(parent_id.unwrap_or(source_id)),
            child_builtin,
            region,
            depth + 1,
            visited,
            collected,
        ))
        .await;
        if let Err(error) = boxed {
            log::warn!("地图子源加载失败，跳过：源={}，原因={}", child_id, error);
        }
    }
    Ok(())
}

/// 子源声明的地区是否适用于当前地区（空 = 所有地区）。
fn child_matches_region(declared: Option<&str>, region: &str) -> bool {
    declared
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .is_none_or(|declared| crate::config::normalize_repository_region(Some(declared)) == region)
}

/// 由「源 ID + 地图身份」派生稳定 ID。
///
/// 不用 UUID：地图列表每次刷新都会从源文档重建，随机 ID 会让缓存、安装清单
/// 和用户手动禁用全部丢失。身份本身就是稳定的（`author + level_set_name`），
/// 只需要把不适合做 ID 的字符换掉。
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
    format!(
        "{}::{}::{}",
        slug(source_id),
        slug(&map.author),
        slug(&map.level_set_name)
    )
}

/// 一份地图的依赖是否满足（对照本地已安装的插件版本）。
///
/// 返回未满足的关系；空表示可以安装。这里只做**判定**，不负责安装依赖 ——
/// 地图的前置是框架级组件（DIYLevel / Web Runtime），由用户在框架管理里装，
/// 管理器不替他们决定版本。
pub fn unmet_relations<'a>(
    map: &'a MapRepositoryConfig,
    installed: impl Fn(&str) -> Option<String>,
) -> Vec<&'a RepositoryRelation> {
    map.relations
        .iter()
        .filter(|relation| !relation_satisfied(relation, &installed))
        .collect()
}

/// 单条关系是否满足。
fn relation_satisfied(
    relation: &RepositoryRelation,
    installed: &impl Fn(&str) -> Option<String>,
) -> bool {
    use crate::models::RelationKind;
    let Some(version) = installed(&relation.guid) else {
        // 没装：依赖不满足，排斥/兼容满足（没装就没有冲突）
        return relation.kind != RelationKind::Requires;
    };
    let min_ok = relation.min_version.trim().is_empty()
        || crate::releases::compare_release_versions(&version, relation.min_version.trim())
            .is_some_and(|order| order.is_ge());
    let max_ok = relation.max_version.trim().is_empty()
        || crate::releases::compare_release_versions(&version, relation.max_version.trim())
            .is_some_and(|order| order.is_le());
    let in_range = min_ok && max_ok;
    match relation.kind {
        // 依赖：必须在区间内
        RelationKind::Requires => in_range,
        // 排斥：在区间内才算冲突（不在区间内是「允许的版本」）
        RelationKind::Conflicts => !in_range,
        // 兼容：在区间内才算兼容
        RelationKind::Compatible => in_range,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal(name: &str, author: &str, set: &str) -> String {
        format!(
            r#"
[[maps]]
name = "{name}"
provider = "github"
username = "u"
repository = "r"
level_set_name = "{set}"
author = "{author}"
enabled = true
asset_rules = [{{ type = "exact", value = "map.zip" }}]
"#
        )
    }

    fn bundled() -> SourceLocation {
        SourceLocation::parse("maps.toml").unwrap()
    }

    #[test]
    fn parses_a_minimal_map_document() {
        let doc = parse_document(&minimal("RW", "RW", "RW_LevelSet"), &bundled()).unwrap();
        assert_eq!(doc.maps.len(), 1);
        assert_eq!(doc.maps[0].level_set_name, "RW_LevelSet");
        assert_eq!(doc.maps[0].author, "RW");
    }

    #[test]
    fn identity_is_author_plus_level_set_name_case_insensitive() {
        let doc = parse_document(&minimal("RW", "RW", "RW_LevelSet"), &bundled()).unwrap();
        assert_eq!(map_identity(&doc.maps[0]), "rw\u{1}rw_levelset");
    }

    #[test]
    fn rejects_duplicate_identity_within_one_document() {
        // 同一个 author + level_set_name 出现两次：安装时会两份抢同一个目录
        let text = format!(
            "{}{}",
            minimal("A", "RW", "Same"),
            minimal("B", "rw", "same")
        );
        let err = parse_document(&text, &bundled()).unwrap_err();
        assert!(err.to_string().contains("mapSource.duplicateEntry"));
    }

    #[test]
    fn same_set_name_under_different_authors_is_not_a_duplicate() {
        let text = format!(
            "{}{}",
            minimal("A", "Alice", "Shared"),
            minimal("B", "Bob", "Shared")
        );
        assert!(parse_document(&text, &bundled()).is_ok());
    }

    #[test]
    fn requires_level_set_name() {
        let text = minimal("M", "A", "").replace("level_set_name = \"\"", "");
        let err = parse_document(&text, &bundled()).unwrap_err();
        assert!(err.to_string().contains("mapSource.missingLevelSetName"));
    }

    #[test]
    fn requires_asset_rules() {
        let text = minimal("M", "A", "S").replace(
            "asset_rules = [{ type = \"exact\", value = \"map.zip\" }]",
            "",
        );
        let err = parse_document(&text, &bundled()).unwrap_err();
        assert!(err.to_string().contains("mapSource.missingAssetRules"));
    }

    #[test]
    fn region_filtering_matches_the_repository_semantics() {
        // 走 parse_document 而不是裸 from_str：后者绕过 serde 的字段默认值
        let mut map = parse_document(&minimal("M", "A", "S"), &bundled())
            .unwrap()
            .maps
            .into_iter()
            .next()
            .unwrap();
        assert!(map_is_available(&map, "chinese_mainland"));
        assert!(map_is_available(&map, "international"));

        map.region = Some("chinese_mainland".into());
        assert!(map_is_available(&map, "chinese_mainland"));
        assert!(!map_is_available(&map, "international"));

        map.region = Some("international".into());
        assert!(!map_is_available(&map, "chinese_mainland"));
        assert!(map_is_available(&map, "international"));

        // 禁用一律不可见，与地区无关
        map.enabled = false;
        assert!(!map_is_available(&map, "international"));
    }

    #[test]
    fn relations_require_the_minimum_framework_versions() {
        let text = format!(
            "{}\n{}",
            minimal("M", "A", "S"),
            r#"
[[maps.relations]]
kind = "requires"
guid = "dev.gua.overcooked.diylevel"
min_version = "0.10.0"

[[maps.relations]]
kind = "requires"
guid = "oc2.oc2diylevelruntimewloader"
min_version = "3.10.2"
"#
        );
        let map = parse_document(&text, &bundled())
            .unwrap()
            .maps
            .into_iter()
            .next()
            .unwrap();

        // 都没装：两条都不满足
        let unmet = unmet_relations(&map, |_| None);
        assert_eq!(unmet.len(), 2);

        // 装够了：都满足
        let unmet = unmet_relations(&map, |guid| match guid {
            "dev.gua.overcooked.diylevel" => Some("0.10.0".into()),
            "oc2.oc2diylevelruntimewloader" => Some("3.10.2".into()),
            _ => None,
        });
        assert!(unmet.is_empty());

        // 框架版本偏低：仍然不满足
        let unmet = unmet_relations(&map, |guid| match guid {
            "dev.gua.overcooked.diylevel" => Some("0.9.9".into()),
            "oc2.oc2diylevelruntimewloader" => Some("3.10.2".into()),
            _ => None,
        });
        assert_eq!(unmet.len(), 1);
    }

    /*
     * 刷新源树会整批重建地图条目。缓存必须跟着 ID 接回来，否则 10 分钟
     * TTL 形同虚设 —— 每次刷新都把整批地图的 API 重打一遍。
     *
     * 这条测试是「必须看着它红过一次」的守卫：把 224 行附近的
     * `map.cached_releases = old.cached_releases.clone()` 删掉，它会失败。
     */
    #[test]
    fn refreshing_the_tree_keeps_release_cache_by_stable_id() {
        let mut config = crate::config::default_config();
        let mut first = parse_document(&minimal("M", "A", "S"), &bundled())
            .unwrap()
            .maps
            .into_iter()
            .next()
            .unwrap();
        first.id = map_repository_id("maps-x", &first);
        first.releases_cached_at = Some(1_700_000_000_000);
        first.cached_releases = vec![crate::models::ReleaseInfo {
            release_id: None,
            tag_name: "v1.2".into(),
            version: None,
            name: String::new(),
            body: String::new(),
            published_at: None,
            prerelease: false,
            draft: false,
            assets: Vec::new(),
        }];
        config.map_repositories = vec![first.clone()];

        // 源文档重建：同样的身份 → 同样的 ID
        let mut fresh = parse_document(&minimal("M", "A", "S"), &bundled())
            .unwrap()
            .maps
            .into_iter()
            .next()
            .unwrap();
        fresh.id = map_repository_id("maps-x", &fresh);
        assert!(fresh.cached_releases.is_empty(), "重建出来的条目没有缓存");

        replace_maps_keeping_runtime_state(&mut config, vec![fresh]);
        assert_eq!(config.map_repositories.len(), 1);
        assert_eq!(
            config.map_repositories[0].cached_releases.len(),
            1,
            "刷新后必须接回 Release 缓存"
        );
        assert_eq!(
            config.map_repositories[0].releases_cached_at,
            Some(1_700_000_000_000),
            "缓存时间戳同样是命中的依据"
        );
    }

    /*
     * `enabled` 是作者的声明，不是用户的偏好：源文档里关掉某张图后，
     * 刷新不能让它在本地复活。管理器没有「用户手动隐藏地图」这个操作，
     * 所以要保留它反而会吃掉作者的意图。
     */
    #[test]
    fn refreshing_the_tree_follows_the_authors_enabled_flag() {
        let mut config = crate::config::default_config();
        let mut stale = parse_document(&minimal("M", "A", "S"), &bundled())
            .unwrap()
            .maps
            .into_iter()
            .next()
            .unwrap();
        stale.id = map_repository_id("maps-x", &stale);
        stale.enabled = true;
        config.map_repositories = vec![stale];

        let mut fresh = parse_document(
            &minimal("M", "A", "S").replace("enabled = true", "enabled = false"),
            &bundled(),
        )
        .unwrap()
        .maps
        .into_iter()
        .next()
        .unwrap();
        fresh.id = map_repository_id("maps-x", &fresh);

        replace_maps_keeping_runtime_state(&mut config, vec![fresh]);
        assert!(
            !config.map_repositories[0].enabled,
            "作者在源文档里下线的地图，刷新后不应恢复可用"
        );
    }

    #[test]
    fn map_id_is_stable_across_rebuilds_and_unique_per_identity() {
        let build = |source: &str, author: &str, set: &str| {
            let map = parse_document(&minimal("M", author, set), &bundled())
                .unwrap()
                .maps
                .into_iter()
                .next()
                .unwrap();
            map_repository_id(source, &map)
        };
        // 同一身份重复派生 → 同一个 ID（缓存与安装清单都挂在它上面）
        assert_eq!(
            build("maps-x", "RW", "RW_LevelSet"),
            build("maps-x", "RW", "RW_LevelSet")
        );
        // 换作者或换集名 → 新 ID（否则两张不同的地图会共用一份缓存）
        assert_ne!(
            build("maps-x", "RW", "RW_LevelSet"),
            build("maps-x", "RW", "RW_LevelSet2")
        );
        assert_ne!(
            build("maps-x", "RW", "RW_LevelSet"),
            build("maps-x", "Alice", "RW_LevelSet")
        );
        // 换源 → 新 ID（不同源可以各自声明同名地图）
        assert_ne!(
            build("maps-x", "RW", "RW_LevelSet"),
            build("maps-y", "RW", "RW_LevelSet")
        );
    }
}

/// 内置地图快照的回归测试。
///
/// 内置文件是用户离线时的唯一来源，解析失败会静默把地图页清空 ——
/// 所以用测试锁住「能解析出至少一张地图」与关键字段。
#[cfg(test)]
mod bundled_tests {
    use super::*;

    #[test]
    fn builtin_map_index_declares_a_child_source() {
        let doc = parse_document(
            SourceLocation::parse(BUILTIN_MAP_INDEX)
                .unwrap()
                .bundled_text()
                .unwrap(),
            &SourceLocation::parse(BUILTIN_MAP_INDEX).unwrap(),
        )
        .expect("内置 maps.toml 应能解析");
        assert!(!doc.map_sources.is_empty(), "索引里应有子源");
        // 索引本身不放地图，地图在子源里
        assert!(doc.maps.is_empty(), "索引不该直接声明地图");
    }

    #[test]
    fn builtin_rw_source_declares_the_published_map() {
        let location = SourceLocation::parse("maps-rw.toml").unwrap();
        let doc = parse_document(location.bundled_text().unwrap(), &location)
            .expect("内置 maps-rw.toml 应能解析");
        assert_eq!(doc.maps.len(), 1, "RW 子源目前只有一张地图");
        let map = &doc.maps[0];
        assert_eq!(map.level_set_name, "RW_LevelSet");
        assert_eq!(map.author, "RW");
        assert_eq!(map.provider, crate::models::Provider::Gitee);
        // 依赖必须声明：缺 DIYLevel 或 Web Runtime 时地图跑不起来
        assert!(
            map.relations
                .iter()
                .any(|relation| relation.guid == "dev.gua.overcooked.diylevel"),
            "应声明 DIYLevel 依赖"
        );
        assert!(
            map.relations
                .iter()
                .any(|relation| relation.guid == "oc2.oc2diylevelruntimewloader"),
            "应声明 Web Runtime 依赖"
        );
    }

    #[test]
    fn bundled_maps_are_discoverable_from_the_index() {
        // 索引 → 子源 → 地图，整条链路能走通
        let index = SourceLocation::parse(BUILTIN_MAP_INDEX).unwrap();
        let doc = parse_document(index.bundled_text().unwrap(), &index).unwrap();
        let mut total = doc.maps.len();
        for child in &doc.map_sources {
            let location = SourceLocation::parse(&child.url).unwrap();
            total += parse_document(location.bundled_text().unwrap(), &location)
                .unwrap()
                .maps
                .len();
        }
        assert!(total >= 1, "从索引出发应能至少找到一张地图");
    }
}

/// 把地图仓库投影成 `fetch_releases` 需要的最小 `RepositoryConfig`。
///
/// ## 为什么用投影而不是改 `fetch_releases` 的签名
///
/// `fetch_releases` 是 MOD 与地图共用的**唯一** Release 抓取入口：限流闸门、
/// 直连/代理目标顺序、403 提前停止、渠道过滤、Gitee 特例都在里面。为了地图
/// 再造一条抓取路径，等于把这些规则复制一遍 —— 迟早两边行为不一致。
///
/// 投影只填「取 Release 真正会读」的字段；其余用默认值。任何新增的
/// Release 相关字段都要同时出现在这里，否则地图侧会拿到默认行为。
pub(crate) fn release_probe(map: &MapRepositoryConfig) -> crate::models::RepositoryConfig {
    // 借 `repository_from_export` 而不是手写字段：它掌握全部默认值，
    // 手写一份会在新增字段时静默落空。
    /*
     * 用 serde 构造 `RepositoryExport`：它字段很多且多数有默认值，
     * 手写结构体会在新增字段时编译失败（好事），但也要同步维护一份字段表。
     * 走 JSON 只写这次真正关心的几项，其余取模型里的 serde 默认值。
     */
    let export: crate::models::RepositoryExport = serde_json::from_value(serde_json::json!({
        "name": map.name,
        "name_i18n": map.name_i18n,
        "group": map.group,
        "group_i18n": map.group_i18n,
        "provider": map.provider,
        "username": map.username,
        "repository": map.repository,
        "enabled": map.enabled,
        "download_mode": "release",
        "channel": map.channel,
        "max_release_count": map.max_release_count,
        "download_threads": map.download_threads,
        "asset_rules": map.asset_rules,
        "install_target": "",
    }))
    .expect("地图投影的字段形状固定，构造失败说明模型改了默认值");
    crate::config::repository_from_export(&export, map.id.clone(), map.builtin)
}

/// 刷新一个地图仓库的 Release 列表。
///
/// 与仓库侧同一条路径：`fetch_releases` + `filter_channel`。缓存由调用方
/// （命令层）读写，这里不碰配置。
pub(crate) async fn fetch_map_releases(
    client: &reqwest::Client,
    map: &MapRepositoryConfig,
    token: Option<&str>,
    official_api_only: bool,
    api_proxies: &[crate::models::GitHubProxy],
) -> Result<Vec<crate::models::ReleaseInfo>, String> {
    let probe = release_probe(map);
    crate::releases::fetch_releases(client, &probe, token, official_api_only, api_proxies)
        .await
        .map(|releases| crate::releases::filter_channel(releases, &map.channel))
        .map_err(|error| error.to_string())
}
