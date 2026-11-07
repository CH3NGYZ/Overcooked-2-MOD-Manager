use super::context::MapSourceContext;
use super::document::MapSourceDeclaration;
use super::document::MapSourceMode;
use super::identity::map_identity;
use super::release_tags::load_release_tag_maps;
use super::state::SourceLabel;
use crate::models::CommunityMapRepository;
use crate::models::MapRepositoryConfig;
use crate::models::Provider;
use crate::repository_sources::SourceLocation;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// 社区地图源在界面上共用的源 ID（两个平台各一个）。
///
/// 用「每平台一个共享源」而不是「一个仓库一个源」：地图页顶层是「源」、
/// 中层分类本来就按作者聚合，把十几个社区仓库拆成十几个源只会让顶层列表
/// 铺满陌生仓库名，也看不出它们属于同一类来源。
pub const GITHUB_COMMUNITY_MAP_SOURCE_ID: &str = "github-map-community";

pub const GITEE_COMMUNITY_MAP_SOURCE_ID: &str = "gitee-map-community";

/// 一次刷新最多展开多少个社区地图仓库。
///
/// 每个仓库都要读一次 Release 列表（匿名 GitHub 只有 60 次/小时），而社区
/// 发现的仓库数量不受我们控制。上限只防「一次刷新把配额烧光」：超出部分
/// 记 warn，等发现结果变化、用户下次刷新时自然轮得到。
const MAX_COMMUNITY_MAP_REPOSITORIES: usize = 50;

/// 社区地图源标签（每平台一个，五语名称）。
pub(super) fn community_map_label(provider: &Provider) -> SourceLabel {
    let (id, product) = match provider {
        Provider::Github => (GITHUB_COMMUNITY_MAP_SOURCE_ID, "GitHub"),
        Provider::Gitee => (GITEE_COMMUNITY_MAP_SOURCE_ID, "Gitee"),
    };
    SourceLabel {
        id: id.to_string(),
        name: format!("社区地图（{product}）"),
        name_i18n: BTreeMap::from([
            ("zh-CN".to_string(), format!("社区地图（{product}）")),
            ("zh-TW".to_string(), format!("社群地圖（{product}）")),
            ("en".to_string(), format!("Community maps ({product})")),
            ("ko".to_string(), format!("커뮤니티 맵 ({product})")),
            ("ja".to_string(), format!("コミュニティマップ（{product}）")),
        ]),
    }
}

/// 把一个「社区发现的仓库」变成 `release-tags` 源声明。
///
/// 复用 `load_release_tag_maps` 而不是另写一条抓取路径：地图条目怎么从
/// Release note 里长出来只有一份实现，社区仓库与作者写在索引里的仓库
/// 走同一套规则，才不会两边行为不一致。
pub(super) fn community_map_declaration(
    repository: &CommunityMapRepository,
) -> MapSourceDeclaration {
    let username = repository.username.trim();
    let name = repository.repository.trim();
    MapSourceDeclaration {
        id: format!(
            "{}/{username}/{name}",
            crate::community::source_id(&repository.provider)
        ),
        mode: MapSourceMode::ReleaseTags,
        url: String::new(),
        provider: Some(repository.provider.clone()),
        username: username.to_string(),
        repository: name.to_string(),
        map: None,
        // 页码上限走 `release-tags` 的默认值：社区仓库通常只有几个版本
        page_size: None,
        max_pages: None,
        nickname: format!("{username}/{name}"),
        nickname_i18n: BTreeMap::new(),
        region: None,
        // 社区仓库永远是远程结果，不能靠自己声明成内置
        builtin: Some(false),
    }
}

/// 把社区发现的地图源仓库接进地图源树。
///
/// 发现结果由 `refresh_community_repositories` 写进配置（同一次搜索里就把
/// `mod` / `map` 分好了流），地图侧只负责把仓库坐标展开成条目：一个仓库
/// = 一个 `release-tags` 源，条目来自各自的 Release note。
///
/// 单个仓库失败只跳过它自己：一个仓库的 note 写错或 API 限流，不该让其他
/// 社区地图从页面上消失。`ctx.previous_for` 按源 ID 过滤，拿到的是该平台
/// 全部社区条目的超集，靠 `seen` 去重。
pub(super) async fn append_community_map_maps(
    collected: &mut Vec<MapRepositoryConfig>,
    origin: &SourceLocation,
    ctx: &MapSourceContext<'_>,
) {
    let repositories = &ctx.config.community_map_repositories;
    if repositories.is_empty() {
        return;
    }
    if repositories.len() > MAX_COMMUNITY_MAP_REPOSITORIES {
        log::warn!(
            "社区地图仓库超过单次展开上限，只处理前 {} 个：总数={}",
            MAX_COMMUNITY_MAP_REPOSITORIES,
            repositories.len()
        );
    }
    /*
     * 官方索引先加载，撞身份时保留它：社区发现只补官方没覆盖到的地图。
     * 同一个合集仓既被作者写进索引、又被发现出来的情况很常见。
     */
    let mut seen = collected.iter().map(map_identity).collect::<HashSet<_>>();
    for repository in repositories.iter().take(MAX_COMMUNITY_MAP_REPOSITORIES) {
        let label = community_map_label(&repository.provider);
        let declaration = community_map_declaration(repository);
        match load_release_tag_maps(&declaration, &label, false, origin, ctx).await {
            Ok(maps) => {
                let total = maps.len();
                let mut added = 0;
                for map in maps {
                    if seen.insert(map_identity(&map)) {
                        collected.push(map);
                        added += 1;
                    }
                }
                log::info!(
                    "社区地图源加载完成：仓库={}/{}，Release 条目={}，新增={}",
                    repository.username,
                    repository.repository,
                    total,
                    added
                );
            }
            Err(error) => log::warn!(
                "社区地图源加载失败，跳过：仓库={}/{}，原因={}",
                repository.username,
                repository.repository,
                error
            ),
        }
    }
}
