use super::community::append_community_map_maps;
use super::context::MapSourceContext;
use super::document::MapSourceMode;
use super::document::build_declaration_map;
use super::document::parse_document;
use super::identity::map_repository_id;
use super::release_tags::load_release_tag_maps;
use super::state::SourceLabel;
use super::transport::load_document;
use crate::config::ConfigError;
use crate::models::MapRepositoryConfig;
use crate::repository_sources::SourceLocation;
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

/// 同步加载内置地图源树（索引 → 子源 → 地图）。
///
/// 只走编译期嵌入的资源，读取/解析失败跳过，不向调用方返回错误。内置索引只声明
/// `release-tags` 源时结果必然为空（`collect_bundled_maps` 会跳过联网型源）：
/// 它仍用于两件事 —— 内置索引里若出现 `repository` / `document` 形式的条目，
/// 先给地图页一个无需网络的初始列表；根索引读取失败时由调用方决定是否回退。
pub(crate) fn bundled_map_tree() -> Vec<MapRepositoryConfig> {
    let mut collected = Vec::new();
    let Ok(index) = SourceLocation::parse(BUILTIN_MAP_INDEX) else {
        return collected;
    };
    let root = SourceLabel {
        network_plan: Default::default(),
        id: DEFAULT_MAP_INDEX_ID.to_string(),
        ..SourceLabel::default()
    };
    collect_bundled_maps(&index, &root, 0, &mut collected);
    collected
}

fn collect_bundled_maps(
    location: &SourceLocation,
    label: &SourceLabel,
    depth: usize,
    collected: &mut Vec<MapRepositoryConfig>,
) {
    let Ok(text) = location.bundled_text() else {
        return;
    };
    let Ok(document) = parse_document(text, location, &label.network_plan) else {
        return;
    };
    for mut map in document.maps {
        label.attach(&mut map);
        map.builtin = true;
        map.id = map_repository_id(&label.id, &map);
        collected.push(map);
    }
    if depth + 1 > MAX_MAP_SOURCE_DEPTH {
        return;
    }
    for child in document.map_sources {
        // clone 而不是移动：下面还要把整条声明交给 `build_declaration_map`
        let child_label = SourceLabel {
            network_plan: child.network_plan.clone(),
            id: child.id.trim().to_string(),
            name: child.nickname.clone(),
            name_i18n: child.nickname_i18n.clone(),
        };
        match child.mode {
            /*
             * 单仓库地图的条目就写在这份索引里，不需要网络：内置树照样能
             * 立刻把它变成一张可用地图（首次启动、离线都有内容）。
             */
            MapSourceMode::Repository => {
                match build_declaration_map(&child, &child_label, true, location) {
                    Ok(map) => collected.push(map),
                    Err(error) => log::warn!(
                        "内置地图源里的单仓库条目无效，跳过：源={}，原因={}",
                        child_label.id,
                        error
                    ),
                }
                continue;
            }
            /*
             * release-tags 必须联网把整个仓库的 Release 读下来才知道有哪些
             * 地图；内置树的约定是「不联网也能读到东西」，所以这里只能跳过，
             * 等第一次刷新源树后条目才出现。
             */
            MapSourceMode::ReleaseTags => {
                log::debug!("内置地图源树跳过联网型源：源={}", child_label.id);
                continue;
            }
            MapSourceMode::Document => {}
        }
        let Ok(child_location) = SourceLocation::resolve(&child.url, Some(location)) else {
            continue;
        };
        collect_bundled_maps(&child_location, &child_label, depth + 1, collected);
    }
}

/// 老配置里的地图条目缺源昵称时，从内置源树补一次。
///
/// 昵称只在加载源树时写进条目，升级本身不会重新加载源树；不补的话地图页
/// 的源标题会退化成源 ID（`maps-rw`）。内置树来自编译期嵌入的资源，没有
/// 网络，读取失败跳过；只有缺名字时才查内置条目。调用方负责保存，后续
/// 配置缓存命中不再执行同步。远程索引独有的子源在刷新时覆盖。
pub(crate) fn fill_missing_source_names(maps: &mut [MapRepositoryConfig]) -> bool {
    fn missing(map: &MapRepositoryConfig) -> bool {
        map.source_id
            .as_deref()
            .is_some_and(|id| !id.trim().is_empty())
            && map.source_name.trim().is_empty()
    }
    if !maps.iter().any(missing) {
        return false;
    }
    let mut names = std::collections::HashMap::new();
    for map in bundled_map_tree() {
        let Some(id) = map.source_id else {
            continue;
        };
        names
            .entry(id)
            .or_insert((map.source_name, map.source_name_i18n));
    }
    let mut changed = false;
    for map in maps.iter_mut() {
        if !missing(map) {
            continue;
        }
        let found = map.source_id.as_ref().and_then(|id| names.get(id).cloned());
        if let Some((name, name_i18n)) = found {
            map.source_name = name;
            map.source_name_i18n = name_i18n;
            changed = true;
        }
    }
    changed
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
/// 每个源有独立的错误状态与刷新按钮。地图侧仍然不维护这棵树：源在界面上
/// 只是「源 → 分类 → 地图」三层折叠结构的第一层，因此每个条目自带
/// 「源 ID + 源昵称」（`source_id` / `source_name` / `source_name_i18n`），
/// 不额外造一棵需要同步、清理、迁移的源树。
///
/// 每份文档先收自身地图，再按声明顺序深度优先展开子源。
/// 前端因此可以直接按条目首次出现的顺序排出源顺序，与 `[[map_sources]]` 的
/// 声明顺序一致。
///
/// `ctx` 同时服务资产文档校验、release-tags 源和社区源。builtin_only 只选
/// 内置索引，不代表离线；严格离线初始化使用 bundled_map_tree。
pub async fn load_map_tree(
    region: &str,
    builtin_only: bool,
    ctx: &MapSourceContext<'_>,
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
    let root = SourceLabel {
        network_plan: Default::default(),
        id: DEFAULT_MAP_INDEX_ID.to_string(),
        ..SourceLabel::default()
    };
    /*
     * `index` 留一份给社区仓库使用：它们的条目也用索引位置做「来源文档」
     * 校验基底（`release-tags` 的条目校验只看条目自身字段）。
     */
    load_map_document(
        index.clone(),
        &root,
        None,
        root_builtin,
        region,
        0,
        &mut visited,
        &mut collected,
        ctx,
    )
    .await?;
    /*
     * 社区发现的地图源排在官方源之后：官方索引是人工维护的权威条目，
     * 身份撞车时保留官方那份（见 `append_community_map_maps` 的去重）。
     */
    append_community_map_maps(&mut collected, &index, ctx).await;
    log::info!("地图源树加载完成：地图={}", collected.len());
    Ok(collected)
}

/// 读取一个地图源文档，并把它的地图与子源递归收进 `collected`。
#[allow(clippy::too_many_arguments)]
async fn load_map_document(
    location: SourceLocation,
    label: &SourceLabel,
    parent_id: Option<&str>,
    builtin: bool,
    region: &str,
    depth: usize,
    visited: &mut HashSet<(String, String)>,
    collected: &mut Vec<MapRepositoryConfig>,
    ctx: &MapSourceContext<'_>,
) -> Result<(), ConfigError> {
    let document = load_document(&location, ctx, &label.network_plan).await?;
    for mut map in document.maps {
        // 把源标签附到条目，持久化配置时不需要另存一棵源树。
        label.attach(&mut map);
        map.builtin = builtin;
        // ID 必须跨刷新稳定，否则重建条目会丢失缓存与关系快照的关联。
        // 用「源 ID + 身份」派生，重建出来的地图能接回上一次的缓存。
        map.id = map_repository_id(&label.id, &map);
        collected.push(map);
    }

    for child in document.map_sources {
        // 地区不匹配的子源整个跳过：它的地址对当前用户没有意义，
        // 请求它只会换来一次 404 和一条无从处理的错误
        if !child_matches_region(child.region.as_deref(), region) {
            log::debug!("跳过地区不匹配的地图子源：源={}", child.id);
            continue;
        }
        // 远程文档不能自行提权：省略时继承父源
        let child_builtin = builtin && child.builtin.unwrap_or(true);
        let child_label = SourceLabel {
            network_plan: child.network_plan.clone(),
            id: child.id.trim().to_string(),
            name: child.nickname.clone(),
            name_i18n: child.nickname_i18n.clone(),
        };
        match child.mode {
            /*
             * 单仓库地图：整仓 = 一张图，条目本体写在声明里，这里只是补齐
             * 源归属与 ID。不递归，因此不受层级上限约束。
             */
            MapSourceMode::Repository => {
                match build_declaration_map(&child, &child_label, child_builtin, &location) {
                    Ok(map) => collected.push(map),
                    Err(error) => log::warn!(
                        "单仓库地图源声明无效，跳过：源={}，原因={}",
                        child_label.id,
                        error
                    ),
                }
                continue;
            }
            /*
             * 单仓多 Release：一个 tag 一张图，元数据来自各自的 Release note。
             * 一次读完整仓的 Release（按 page_size 翻页），把条目拼出来；
             * 同一份地图源的全部条目共用 child_label，前端因此把它们归到
             * 同一个源下面。
             */
            MapSourceMode::ReleaseTags => {
                match load_release_tag_maps(&child, &child_label, child_builtin, &location, ctx)
                    .await
                {
                    Ok(maps) => collected.extend(maps),
                    Err(error) => log::warn!(
                        "按 Release note 生成地图失败，跳过：源={}，原因={}",
                        child_label.id,
                        error
                    ),
                }
                continue;
            }
            MapSourceMode::Document => {}
        }
        /*
         * 子源失败仍然继续加载其余子源 —— 一个作者的 TOML 404 不该让
         * 其他作者的地图从页面上消失。失败只记日志，索引层不因此报错。
         */
        if depth + 1 > MAX_MAP_SOURCE_DEPTH {
            log::warn!(
                "地图源层级超过上限，丢弃更深层的子源：源={}，上限={}",
                child_label.id,
                MAX_MAP_SOURCE_DEPTH
            );
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
        let boxed = Box::pin(load_map_document(
            child_location,
            &child_label,
            Some(parent_id.unwrap_or(&label.id)),
            child_builtin,
            region,
            depth + 1,
            visited,
            collected,
            ctx,
        ))
        .await;
        if let Err(error) = boxed {
            log::warn!(
                "地图子源加载失败，跳过：源={}，原因={}",
                child_label.id,
                error
            );
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
