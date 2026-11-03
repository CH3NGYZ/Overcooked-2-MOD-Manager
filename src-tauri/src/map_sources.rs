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
//! | 绑定本地 | `plugin_guid` 等 DLL 内嵌元数据 | `author + level_set_name + level_set_name_zh` |
//! | 安装单元 | MOD 包（可含多个 DLL） | 关卡集目录（恰好一个） |
//! | 关系语义 | 插件依赖 | 框架依赖（DIYLevel / Web Runtime） |
//!
//! 关系表复用 `RepositoryRelation`：「某 GUID 的版本必须在区间内」这个语义
//! 两边没有差别。
use crate::config::ConfigError;
use crate::models::{
    AppConfig, AssetRule, AssetRuleType, MapRepositoryConfig, Provider, ReleaseAsset, ReleaseInfo,
    RepositoryConfig,
};
use crate::repository_sources::SourceLocation;
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};

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
    /// 条目的公共仓库坐标，省略时被逐条继承。
    ///
    /// 「一个 tag 装一个地图集的历史版本」意味着同一份文档里的条目几乎都是
    /// 同一个仓库（作者自己的合集仓），逐条重写 `provider/username/repository`
    /// 除了复制粘贴没有别的价值 —— 抄错一个字符就是一张装不上的地图。
    #[serde(default)]
    pub bundle: Option<MapBundleCoordinates>,
    #[serde(default)]
    pub maps: Vec<MapRepositoryConfig>,
    #[serde(default)]
    pub map_sources: Vec<MapSourceDeclaration>,
}

/// `[bundle]` 段：本文件条目的默认仓库坐标。
///
/// 只在条目**没写坐标**时继承（写了就用条目自己的）：文档里混放「同一个合集
/// 仓的不同 tag」与「别的作者的独立仓库」是允许的，后者必须能盖过 bundle。
///
/// 作用范围是**当前文档**：子源是另一份文件，要继承就得自己再写一段。
#[derive(Debug, Clone, Deserialize)]
pub struct MapBundleCoordinates {
    /// 省略时按 GitHub 处理（与 `Provider` 的默认值一致）。
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub repository: String,
}

/// 一个地图源以什么方式变成地图条目。
///
/// 三种形态对应三种真实的作者习惯，而不是三个功能开关：
///
/// - `document`：作者自己维护一份 `[[maps]]` TOML（历史形态，保留兼容）。
/// - `repository`：**整个仓库就是一张地图**，每个 Release 是它的一个版本
///   （`max_release_count` 决定显示几个版本）。
/// - `release-tags`：**一个 tag 一张地图**。作者把「该地图的元数据」写进
///   Release note 里的 `<!-- oc2-map ... -->` 块，管理器读完整仓的 Release
///   就地把条目生成出来 —— 作者不再需要维护、同步一份集中式索引。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MapSourceMode {
    #[default]
    Document,
    Repository,
    ReleaseTags,
}

/// 源文档里对子源的声明。
///
/// 昵称只影响展示，但**必须解析**：地图页按「源 → 分类 → 地图」三层展示，
/// 源标题就来自这里。地图源树本身不落盘，所以加载时要把昵称写进每个条目
/// （`MapRepositoryConfig::source_name`），否则 `get_maps` 读持久化配置时
/// 只剩一个源 ID 可显示。
///
/// `mode` 决定后面哪些字段有意义：`document` 只读 `url`；另两种读仓库坐标
/// （`provider` / `username` / `repository`），`repository` 还读 `map` 子表。
/// 字段共用一张结构而不是三种声明，是因为它们总是并列出现在同一份索引里，
/// 拆成三个 enum 变体只会让 TOML 的写法随模式变形、作者更难写对。
#[derive(Debug, Clone, Deserialize)]
pub struct MapSourceDeclaration {
    pub id: String,
    #[serde(default)]
    pub mode: MapSourceMode,
    /// 源地址：完整 URL、内置资源名，或**相对当前文档**的路径。
    /// 仅 `mode = "document"` 使用。
    #[serde(default)]
    pub url: String,
    /// 声明级仓库坐标：`repository` / `release-tags` 模式下的仓库。
    ///
    /// 单仓库地图也可以在 `[map_sources.map]` 里逐条写坐标；两处都写时
    /// **以这里为准** —— 源声明是「这个源指向哪个仓库」的地方，条目里的
    /// 坐标只是同一信息的另一种摆放方式。
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub repository: String,
    /// 单仓库地图的条目本体（`mode = "repository"`）。
    #[serde(default)]
    pub map: Option<MapRepositoryConfig>,
    /// `release-tags` 模式每次向 API 要多少条 Release（默认 50）。
    #[serde(default)]
    pub page_size: Option<u32>,
    /// `release-tags` 最多翻几页（默认 20，防止一份超大仓库拖死刷新）。
    #[serde(default)]
    pub max_pages: Option<u32>,
    /// 单语言镜像；源文档只写 `nickname_i18n` 时从 `zh-CN` 折进来。
    #[serde(default)]
    pub nickname: String,
    #[serde(default)]
    pub nickname_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub region: Option<String>,
    /// 是否属于内置树。省略时继承父源 —— 远程文档不能自行提权。
    #[serde(default)]
    pub builtin: Option<bool>,
}

/// 文档是否托管在 Release 资产里（`.../releases/download/<tag>/<asset>`）。
///
/// 资产地址没有分支结构，`SourceLocation::resolve` 拿它当基底只会拼出一个
/// 不存在的 URL，所以这种文档里声明子源必须当场报错。
fn is_release_asset_document(origin: &SourceLocation) -> bool {
    match origin {
        SourceLocation::Remote(url) => crate::releases::release_asset_reference(url).is_some(),
        _ => false,
    }
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
     * `[bundle]` 先于校验生效：条目继承坐标之后，「缺 username/repository」
     * 才真的是作者漏写，而不是被 bundle 补过一轮的假阴性。
     */
    if let Some(bundle) = document.bundle.take() {
        if bundle.username.trim().is_empty() || bundle.repository.trim().is_empty() {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "mapSource.missingCoordinates",
                &[("map", "[bundle]")],
            )));
        }
        for entry in &mut document.maps {
            if !entry.username.trim().is_empty() && !entry.repository.trim().is_empty() {
                continue;
            }
            entry.username = bundle.username.clone();
            entry.repository = bundle.repository.clone();
            if let Some(provider) = bundle.provider.clone() {
                entry.provider = provider;
            }
        }
    }
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
    // 子源同理：昵称只写多语言表就够，加载时折出单语言镜像随条目落盘
    for child in &mut document.map_sources {
        fold_i18n_mirror(&mut child.nickname, &child.nickname_i18n);
        if let Some(map) = child.map.as_mut() {
            fold_i18n_mirror(&mut map.name, &map.name_i18n);
            fold_i18n_mirror(&mut map.group, &map.group_i18n);
            fold_i18n_mirror(&mut map.note, &map.note_i18n);
        }
        validate_declaration(child, origin)?;
    }

    /*
     * 资产里的文档不许声明子源：Release 资产地址没有分支结构，相对路径
     * 拼出来的只会是坏 URL。明确报错，别让作者以为子源加载成功了。
     */
    if let Some(child) = document.map_sources.first()
        && is_release_asset_document(origin)
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "mapSource.assetDocumentSubSources",
            &[("source", &child.id.trim().to_string())],
        )));
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

/// 按声明模式校验子源：只校验该模式真正会读的字段。
///
/// 三种模式的必填项不同，合成一份「全都必填」的校验只会逼作者写无意义的
/// 占位值（例如给单仓库源补一个永远用不到的 `url`）。
fn validate_declaration(
    declaration: &MapSourceDeclaration,
    origin: &SourceLocation,
) -> Result<(), ConfigError> {
    let source = declaration.id.trim();
    match declaration.mode {
        MapSourceMode::Document => {
            if declaration.url.trim().is_empty() {
                return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                    "mapSource.declarationMissingUrl",
                    &[("source", source)],
                )));
            }
        }
        MapSourceMode::Repository => {
            let Some(map) = declaration.map.as_ref() else {
                return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                    "mapSource.declarationMissingMap",
                    &[("source", source)],
                )));
            };
            let mut probe = map.clone();
            apply_declaration_coordinates(&mut probe, declaration);
            validate_map_entry(&probe, origin)?;
        }
        MapSourceMode::ReleaseTags => {
            if declaration.username.trim().is_empty() || declaration.repository.trim().is_empty() {
                return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                    "mapSource.missingCoordinates",
                    &[("map", source)],
                )));
            }
        }
    }
    Ok(())
}

/// 把声明级的仓库坐标/地区写进条目；条目自己写了的坐标不覆盖。
///
/// provider 例外：声明级写了就一律采用 ——「这个源指向哪个仓库」是源级的事实，
/// 逐条写 provider 只会制造「同一源里有一条指向另一个站点」的怪状态。
fn apply_declaration_coordinates(
    map: &mut MapRepositoryConfig,
    declaration: &MapSourceDeclaration,
) {
    if let Some(provider) = declaration.provider.clone() {
        map.provider = provider;
    }
    if map.username.trim().is_empty() {
        map.username = declaration.username.trim().to_string();
    }
    if map.repository.trim().is_empty() {
        map.repository = declaration.repository.trim().to_string();
    }
    if map.region.is_none() {
        map.region = declaration.region.clone();
    }
}

/// 地图的身份键：`author + levelSetName`，写了中文名时再加上它。
///
/// 与本地绑定的判定**必须**是同一个函数，否则「文档里去重用的键」和
/// 「安装时比对的键」会分叉，表现为「文档里不重复，装的时候却说重复」。
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

/// tag 模式下负责「从资产名里抽版本号」的那条规则。
///
/// 必须同时满足：是正则、能编译、至少有一个捕获组（优先 `(?P<version>…)`，
/// 否则取第 1 组）。锚定由调用方另外查 —— 这里只回答「能不能抽出版本」。
fn version_rule(rules: &[AssetRule]) -> Option<&AssetRule> {
    rules.iter().find(|rule| {
        matches!(rule.rule_type, AssetRuleType::Regex)
            && regex::Regex::new(&rule.value)
                .map(|regex| regex.captures_len() > 1)
                .unwrap_or(false)
    })
}

/// 报错与日志里指代一张地图的统一写法：有名字用名字，没有用仓库坐标。
fn map_label(entry: &MapRepositoryConfig) -> String {
    if entry.name.trim().is_empty() {
        format!("{}/{}", entry.username, entry.repository)
    } else {
        entry.name.clone()
    }
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
    let label = map_label(entry);
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
    if let Some(tag) = map_tag(entry) {
        let rule = version_rule(&entry.asset_rules).ok_or_else(|| {
            ConfigError::Invalid(crate::locale_tag::with_params(
                "mapSource.assetRuleNeedsVersion",
                &[("map", &label)],
            ))
        })?;
        if !rule.value.starts_with('^') || !rule.value.ends_with('$') {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "mapSource.regexNotAnchored",
                &[("map", &label), ("asset", &rule.value)],
            )));
        }
        log::debug!(
            "地图条目使用固定 Tag：地图={}，tag={}，版本规则={}",
            label,
            tag,
            rule.value
        );
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
///
/// 唯一的加成是「文档放在 Release 资产里」这一形态：先按资产地址反查仓库与
/// tag，从 Release API 拿到 size 与 sha256，再带着这两项去竞速下载并校验。
/// 拿不到校验信息时**退回普通下载**：校验是增强，不该变成新的失败点。
pub(crate) async fn load_document(
    location: &SourceLocation,
    ctx: &MapSourceContext<'_>,
) -> Result<MapSourceDocument, ConfigError> {
    match verified_document(location, ctx).await {
        Some(verified) => {
            crate::repository_sources::load_verified_text_document(
                location,
                &verified,
                parse_document,
            )
            .await
        }
        None => crate::repository_sources::load_text_document(location, parse_document).await,
    }
}

/// 文档地址若是 Release 资产，就去 API 取它的 size/sha256 与下载候选。
///
/// 返回 `None` 表示「这份文档不需要/拿不到校验信息」，调用方按老路子读。
/// 这里**不报错**：Release API 挂了不该让整个地图源树加载失败，作者写的
/// 地址本身仍然可用 —— 只是少了摘要核对。
async fn verified_document(
    location: &SourceLocation,
    ctx: &MapSourceContext<'_>,
) -> Option<crate::repository_sources::VerifiedDocument> {
    let SourceLocation::Remote(url) = location else {
        return None;
    };
    let reference = crate::releases::release_asset_reference(url)?;
    let probe = coordinate_probe(&reference);
    let key = crate::releases::repository_cache_key(
        reference.provider.clone(),
        &reference.username,
        &reference.repository,
    );
    let token = ctx.token(&reference.provider).map(str::to_string);
    let official_api_only = ctx.config.auth.official_api_only;
    let api_proxies = ctx.api_proxies(&reference.provider).to_vec();
    let releases = match ctx
        .cache
        .list(key, ctx.pass_id, || async move {
            crate::releases::fetch_releases(
                ctx.client,
                &probe,
                token.as_deref(),
                official_api_only,
                &api_proxies,
            )
            .await
            .map_err(|error| error.to_string())
        })
        .await
    {
        Ok(releases) => releases,
        Err(error) => {
            log::warn!(
                "文档校验信息获取失败，退回普通下载：地址={}，原因={}",
                crate::repository_sources::safe_log_address(url.as_str()),
                error
            );
            return None;
        }
    };
    let Some(release) = releases
        .iter()
        .find(|release| release.tag_name.trim() == reference.tag)
    else {
        log::warn!(
            "文档所在 Release 不在仓库列表里，退回普通下载：地址={}，tag={}",
            crate::repository_sources::safe_log_address(url.as_str()),
            reference.tag
        );
        return None;
    };
    let Some(asset) = release
        .assets
        .iter()
        .find(|asset| asset.name == reference.asset)
    else {
        log::warn!(
            "文档资产不在 Release 里，退回普通下载：地址={}，资产={}",
            crate::repository_sources::safe_log_address(url.as_str()),
            reference.asset
        );
        return None;
    };
    /*
     * 候选走与普通资产下载同一条路由（直连 + 代理竞速），因为这就是一次资产
     * 下载：只是产物是文本而不是 ZIP。
     */
    let candidates = match crate::commands::plan_remote_download_candidates(
        ctx.config,
        &reference.provider,
        crate::direct_mod::RemoteUrlKind::ReleaseAsset,
        url.as_str(),
    ) {
        Ok(candidates) => candidates,
        Err(error) => {
            log::warn!(
                "文档下载候选规划失败，退回普通下载：地址={}，原因={}",
                crate::repository_sources::safe_log_address(url.as_str()),
                error
            );
            return None;
        }
    };
    log::info!(
        "文档带校验信息：地址={}，字节={}，摘要={}",
        crate::repository_sources::safe_log_address(url.as_str()),
        asset.size,
        if asset.digest.is_some() { "有" } else { "无" }
    );
    Some(crate::repository_sources::VerifiedDocument {
        // API 没给大小时不校验：`0` 会被当成「预期 0 字节」，反而拒掉正常文件
        expected_size: (asset.size > 0).then_some(asset.size),
        digest: asset.digest.clone(),
        candidates,
    })
}

/// 为「文档所在的仓库」造一个最小的 `RepositoryConfig`，只够取 Release 列表。
fn coordinate_probe(reference: &crate::releases::ReleaseAssetReference) -> RepositoryConfig {
    let export: crate::models::RepositoryExport = serde_json::from_value(serde_json::json!({
        "name": format!("{}/{}", reference.username, reference.repository),
        "name_i18n": {},
        "group": "",
        "group_i18n": {},
        "provider": reference.provider,
        "username": reference.username,
        "repository": reference.repository,
        "enabled": true,
        "download_mode": "release",
        "//": "文档可能落在很旧的 tag 里，列表要一次拿满",
        "max_release_count": 100,
        "install_target": "",
    }))
    .expect("文档校验投影的字段形状固定，构造失败说明模型改了默认值");
    crate::config::repository_from_export(&export, "release-asset-document".to_string(), false)
}

/// 用新加载的地图列表替换配置里的地图，**保留同 ID 条目的 Release 缓存**。
///
/// 每次刷新都是「源文档 → 全新条目」，直接替换会让 5 分钟缓存失效，
/// 于是每次刷新都把整批地图的 API 重打一遍（MOD 侧踩过完全相同的坑）。
///
/// 只保留缓存与失败原因，其余字段以源文档为准：
///
/// - `enabled` 必须听文档的 —— 它是作者的声明（替换或下线某张图）。管理器
///   目前没有「用户手动隐藏地图」这个操作，保留旧值只会让作者下线的图复活。
/// - 名字、资产规则、依赖版本改了就该立刻生效。
///
/// 返回是否有变化，供调用方决定要不要写盘。**「变化」必须包括只改展示字段的
/// 情况**（分组、名字、备注、资产规则……）：ID 由「源 ID + author +
/// level_set_name」派生，作者把分组从 `RW` 改成作者名时 ID 不变 —— 早先这里
/// 只比 ID 和长度，于是线上改了分组，配置里永远是旧值，刷新多少次都不生效。
/// 比较走序列化结果，以后加字段也不会漏掉。
pub(crate) fn replace_maps_keeping_runtime_state(
    config: &mut AppConfig,
    fresh: Vec<MapRepositoryConfig>,
) -> bool {
    let (merged, changed) =
        merge_maps_keeping_runtime_state(std::mem::take(&mut config.map_repositories), fresh);
    config.map_repositories = merged;
    changed
}

/// `replace_maps_keeping_runtime_state` 的纯逻辑部分：不碰配置，便于直接测。
fn merge_maps_keeping_runtime_state(
    previous: Vec<MapRepositoryConfig>,
    fresh: Vec<MapRepositoryConfig>,
) -> (Vec<MapRepositoryConfig>, bool) {
    let mut merged = Vec::with_capacity(fresh.len());
    for mut map in fresh {
        if let Some(old) = previous.iter().find(|old| old.id == map.id) {
            map.cached_releases = old.cached_releases.clone();
            map.releases_cached_at = old.releases_cached_at;
            map.last_error = old.last_error.clone();
        }
        merged.push(map);
    }
    // 运行态字段刚刚从旧条目接回，所以整份比较等价于「声明字段有没有变」
    let changed = match (
        serde_json::to_string(&previous),
        serde_json::to_string(&merged),
    ) {
        (Ok(old), Ok(new)) => old != new,
        // 序列化失败时保守地算作有变化：宁可多写一次配置，也不能让作者改过的
        // 声明被静默丢掉
        _ => true,
    };
    (merged, changed)
}

/// 一个源在界面上的身份：ID + 昵称（单语言镜像与多语言表）。
///
/// 地图源树不落盘，所以这三样必须跟着每个地图条目走 —— 见
/// `MapRepositoryConfig::source_name`。一份结构而不是三个散参数，是因为
/// 它们总是一起出现（`attach` 一次写全），拆开容易漏写其中一个。
#[derive(Debug, Clone, Default)]
struct SourceLabel {
    id: String,
    name: String,
    name_i18n: BTreeMap<String, String>,
}

impl SourceLabel {
    /// 把源归属与源名写进条目。
    fn attach(&self, map: &mut MapRepositoryConfig) {
        map.source_id = Some(self.id.clone());
        map.source_name = self.name.clone();
        map.source_name_i18n = self.name_i18n.clone();
    }
}

/// 同步加载内置地图源树（索引 → 子源 → 地图）。
///
/// 只走编译期嵌入的资源，因此不需要网络、也没有失败路径。内置索引只声明
/// `release-tags` 源时结果必然为空（`collect_bundled_maps` 会跳过联网型源）：
/// 它仍用于两件事 —— 内置索引里若出现 `repository` / `document` 形式的条目，
/// 先给地图页一个无需网络的初始列表；远程索引整体不可用时的回退（与仓库侧
/// 的官方源回退同一套条件）。
pub(crate) fn bundled_map_tree() -> Vec<MapRepositoryConfig> {
    let mut collected = Vec::new();
    let Ok(index) = SourceLocation::parse(BUILTIN_MAP_INDEX) else {
        return collected;
    };
    let root = SourceLabel {
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
    let Ok(document) = parse_document(text, location) else {
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

/// 老配置里的地图条目缺源昵称时，从内置源树补一次。
///
/// 昵称只在加载源树时写进条目，升级本身不会重新加载源树；不补的话地图页
/// 的源标题会退化成源 ID（`maps-rw`）。内置树来自编译期嵌入的资源，没有
/// 网络与失败路径，而且只有真的缺名字时才解析它；补齐后立即落盘，后续
/// 读配置命中缓存直接跳过。远程索引独有的子源在下次刷新时自然覆盖。
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
/// 加载顺序：索引 → 子源（深度优先、按声明顺序）→ 每个文档里的地图条目。
/// 前端因此可以直接按条目首次出现的顺序排出源顺序，与 `[[map_sources]]` 的
/// 声明顺序一致。
///
/// `ctx` 只为「文档放进 Release 资产」这一形态服务：那种地址要先问一次
/// Release API 拿到 size/sha256 才能校验下载结果，而 API 请求必须按仓库
/// 收敛（见 `release_cache`）。内置树与 raw 文档用不到它。
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
        id: DEFAULT_MAP_INDEX_ID.to_string(),
        ..SourceLabel::default()
    };
    load_map_document(
        index,
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
    let document = load_document(&location, ctx).await?;
    for mut map in document.maps {
        // 源归属与源名只在本进程内有效：导出配置时写出的是作者本地的源 ID
        label.attach(&mut map);
        map.builtin = builtin;
        // ID 必须跨刷新稳定：缓存、安装清单和「用户手动禁用」都挂在它上面。
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

/// `release-tags` 模式每次向 API 要多少条 Release（默认值）。
const RELEASE_TAG_PAGE_SIZE: u32 = 50;

/// `release-tags` 模式最多翻几页。
///
/// 50 × 20 = 1000 个 Release 已经是「一个人几年的产出」量级；设上限只是
/// 为了防止一份异常仓库（几千个 tag）让一次刷新迟迟不结束。
const RELEASE_TAG_MAX_PAGES: u32 = 20;

/// 把「单仓库地图」声明变成一张地图条目。
///
/// 声明里的 `[map_sources.map]` 就是条目本体，这里只补三样它不该重复写的
/// 东西：源归属、内置标记、跨刷新稳定的 ID。
fn build_declaration_map(
    declaration: &MapSourceDeclaration,
    label: &SourceLabel,
    builtin: bool,
    origin: &SourceLocation,
) -> Result<MapRepositoryConfig, ConfigError> {
    let mut map = declaration.map.clone().ok_or_else(|| {
        ConfigError::Invalid(crate::locale_tag::with_params(
            "mapSource.declarationMissingMap",
            &[("source", declaration.id.trim())],
        ))
    })?;
    apply_declaration_coordinates(&mut map, declaration);
    label.attach(&mut map);
    map.builtin = builtin;
    map.id = map_repository_id(&label.id, &map);
    validate_map_entry(&map, origin)?;
    Ok(map)
}

/// 「单仓多 Release」源要读哪个仓库。
///
/// 只为复用 `release_cache` 与 `fetch_releases` 的限流/代理/重试链路：
/// 那条链路吃的是 `RepositoryConfig`，为地图再造一条抓取路径等于把规则
/// 复制一遍，迟早两边行为不一致。
fn declaration_probe(
    declaration: &MapSourceDeclaration,
    per_page: u32,
) -> crate::models::RepositoryConfig {
    let export: crate::models::RepositoryExport = serde_json::from_value(serde_json::json!({
        "name": declaration.nickname,
        "name_i18n": declaration.nickname_i18n,
        "group": declaration.nickname,
        "group_i18n": declaration.nickname_i18n,
        "provider": declaration.provider.clone().unwrap_or(Provider::Github),
        "username": declaration.username.trim(),
        "repository": declaration.repository.trim(),
        "enabled": true,
        "download_mode": "release",
        "max_release_count": per_page,
        "install_target": "",
    }))
    .expect("地图源投影的字段形状固定，构造失败说明模型改了默认值");
    crate::config::repository_from_export(
        &export,
        declaration.id.trim().to_string(),
        declaration.builtin.unwrap_or(false),
    )
}

/// 读一个「单仓多 Release」源，把每个 Release note 里的元数据块变成一张地图。
///
/// 一个 tag 只描述一张地图：note 里的块缺 `level_set_name` 或读不出来就跳过
/// 那一条（记 warn），绝不猜。整份结果共用同一个 `label`，前端因此把它们
/// 归在同一个源下面。
async fn load_release_tag_maps(
    declaration: &MapSourceDeclaration,
    label: &SourceLabel,
    builtin: bool,
    origin: &SourceLocation,
    ctx: &MapSourceContext<'_>,
) -> Result<Vec<MapRepositoryConfig>, ConfigError> {
    let provider = declaration.provider.clone().unwrap_or(Provider::Github);
    let page_size = declaration
        .page_size
        .unwrap_or(RELEASE_TAG_PAGE_SIZE)
        .clamp(1, 100);
    let max_pages = declaration
        .max_pages
        .unwrap_or(RELEASE_TAG_MAX_PAGES)
        .clamp(1, 100);
    let probe = declaration_probe(declaration, page_size);
    /*
     * 缓存键加 `::all` 后缀：同一个仓库的「首页 10 条」与「全部分页」不是
     * 同一份数据，共用键要么让地图源读到不完整列表，要么让 MOD 侧拿到全量。
     */
    let key = format!(
        "{}::all",
        crate::releases::repository_cache_key(
            provider.clone(),
            &declaration.username,
            &declaration.repository,
        )
    );
    let token = ctx.token(&provider).map(str::to_string);
    let official_api_only = ctx.config.auth.official_api_only;
    let api_proxies = ctx.api_proxies(&provider).to_vec();
    let releases = match ctx
        .cache
        .list(key, ctx.pass_id, || async move {
            crate::releases::fetch_all_releases(
                ctx.client,
                &probe,
                page_size,
                max_pages,
                token.as_deref(),
                official_api_only,
                &api_proxies,
            )
            .await
            .map_err(|error| error.to_string())
        })
        .await
    {
        Ok(releases) => releases,
        Err(error) => {
            /*
             * 取不到就沿用上一次成功时生成的条目：第三方源不该因为一次限流
             * 或断网从地图页整块消失（用户会以为作者下架了地图）。首次运行
             * 没有旧条目时就是空，下一次刷新补上。
             */
            let previous = ctx.previous_for(&label.id);
            log::warn!(
                "按 Release note 生成地图失败，沿用上次结果：源={}，原因={}，沿用={}",
                label.id,
                error,
                previous.len()
            );
            return Ok(previous);
        }
    };

    let mut maps = Vec::new();
    let mut seen = HashSet::new();
    for release in releases.iter() {
        let tag = release.tag_name.trim();
        if tag.is_empty() {
            continue;
        }
        let note = match crate::map_note::parse_release_note(&release.body) {
            Ok(Some(note)) => note,
            Ok(None) => {
                // 没有块就是这个 Release 不是地图（例如只是普通更新说明）
                log::debug!(
                    "Release note 没有 oc2-map 块，跳过：源={}，tag={}",
                    label.id,
                    tag
                );
                continue;
            }
            Err(reason) => {
                log::warn!(
                    "Release note 的 oc2-map 块无效，跳过：源={}，tag={}，原因={}",
                    label.id,
                    tag,
                    reason
                );
                continue;
            }
        };
        let Some(rule) =
            crate::map_note::versioned_asset_rule(note.metadata.asset.as_deref(), &release.assets)
        else {
            log::warn!(
                "Release 里没有能提取版本号的地图包，跳过：源={}，tag={}",
                label.id,
                tag
            );
            continue;
        };
        match note_block_map(declaration, label, &note, tag, rule, builtin, origin) {
            Ok(map) => {
                if !seen.insert(map_identity(&map)) {
                    log::warn!(
                        "同一个源里有两张身份相同的地图，跳过后者：源={}，tag={}",
                        label.id,
                        tag
                    );
                    continue;
                }
                maps.push(map);
            }
            Err(error) => log::warn!(
                "Release note 生成的条目无效，跳过：源={}，tag={}，原因={}",
                label.id,
                tag,
                error
            ),
        }
    }
    log::info!(
        "按 Release note 生成地图完成：源={}，Release={}，地图={}",
        label.id,
        releases.len(),
        maps.len()
    );
    Ok(maps)
}

/// 把一个 Release 的 note 元数据拼成地图条目。
///
/// 走 JSON 而不是手写结构体：`MapRepositoryConfig` 字段很多、多数有默认值，
/// 只写这次真正来自 note 的字段，其余交给 serde 默认值 —— 与
/// `release_probe` 同一条理由。
#[allow(clippy::too_many_arguments)]
fn note_block_map(
    declaration: &MapSourceDeclaration,
    label: &SourceLabel,
    note: &crate::map_note::NoteBlock,
    tag: &str,
    rule: crate::models::AssetRule,
    builtin: bool,
    origin: &SourceLocation,
) -> Result<MapRepositoryConfig, ConfigError> {
    let metadata = &note.metadata;
    /*
     * 显示名兜底：作者只写了关卡集名时，卡片标题就用它。不兜的话卡片会退化
     * 成 `username/repository`，而同一个仓库里所有地图都长一个样。
     */
    let mut name_i18n = metadata.name_i18n.clone();
    if name_i18n.is_empty() {
        let fallback = if metadata.level_set_name_zh.trim().is_empty() {
            metadata.level_set_name.trim()
        } else {
            metadata.level_set_name_zh.trim()
        };
        name_i18n.insert("zh-CN".to_string(), fallback.to_string());
    }
    // 分类默认与显示名一致：一个 tag 一张图，逐张地图自成一类，
    // 前端的三层结构里「每张卡都挂在自己的分类下」比「全部挤进无分类」清楚
    let group_i18n = if metadata.group_i18n.is_empty() {
        name_i18n.clone()
    } else {
        metadata.group_i18n.clone()
    };
    /*
     * 单语言镜像必须在这里折出来：`get_maps` 只透传 `name`/`group`，前端拿
     * 到空串时会退回 `username/repository`，一个仓库多张图就全长一个样。
     */
    let name = name_i18n
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let group = group_i18n
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|| name.clone());
    /*
     * 卡片短语只来自块里的 `note_i18n`（五语）。块外正文不是卡片说明 ——
     * 它是「这个版本的更新说明」，由 `synthesize_releases` 清洗后进版本详情的
     * 提交日志。两处展示同一次 API 的数据，没有第三处要喂。
     */
    let mut note_i18n = metadata.note_i18n.clone();
    if note_i18n.is_empty() && !metadata.note.trim().is_empty() {
        note_i18n.insert("zh-CN".to_string(), metadata.note.trim().to_string());
    }
    let note_text = note_i18n
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .unwrap_or_default();
    let value = serde_json::json!({
        "provider": declaration.provider.clone().unwrap_or(Provider::Github),
        "username": declaration.username.trim(),
        "repository": declaration.repository.trim(),
        // tag 是这张地图在仓库里的唯一标识：一个 Release = 一张地图，
        // 版本列表从这个 tag 的资产里按 asset 规则拆
        "tag": tag,
        "level_set_name": metadata.level_set_name.trim(),
        "level_set_name_zh": metadata.level_set_name_zh.trim(),
        "author": metadata.author.trim(),
        "name": name,
        "group": group,
        "name_i18n": name_i18n,
        "group_i18n": group_i18n,
        "note": note_text,
        "note_i18n": note_i18n,
        "asset_rules": [rule],
        "latest_version": metadata.latest_version,
        "relations": metadata.relations(),
        "enabled": true,
    });
    let mut map: MapRepositoryConfig =
        serde_json::from_value(value).expect("note 块的字段形状与 MapRepositoryConfig 一致");
    label.attach(&mut map);
    map.builtin = builtin;
    map.id = map_repository_id(&label.id, &map);
    validate_map_entry(&map, origin)?;
    Ok(map)
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
     * ID，所以空 slug 时改用内容哈希 —— 只要中文名不同，ID 就不同。
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

    /*
     * 刷新源树会整批重建地图条目。缓存必须跟着 ID 接回来，否则 5 分钟
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

    /// 只改展示字段（分组名）也必须判定为「有变化」。
    ///
    /// 回归：ID 由「源 ID + author + level_set_name」派生，作者把 `group_i18n`
    /// 从 `RW` 改成作者名时 ID 不变。早先的实现只比 ID 与长度，于是刷新回来的
    /// 新分组被判成「没变化」、配置不写盘，界面永远显示旧分组。
    #[test]
    fn a_display_only_map_change_still_counts_as_change() {
        let fresh = parse_document(&minimal("RW", "RW", "RW_LevelSet"), &bundled())
            .unwrap()
            .maps;
        let mut stored = fresh.clone();
        stored[0].id = "maps-rw::rw::rw_levelset".to_string();
        stored[0].releases_cached_at = Some(1_700_000_000);
        stored[0].last_error = Some("release.rateLimited".to_string());
        let mut renamed = stored.clone();
        renamed[0]
            .group_i18n
            .insert("zh-CN".to_string(), "RickWang".to_string());

        // 内容相同 → 不算变化，避免每次刷新都写一次配置
        let (same, changed) = merge_maps_keeping_runtime_state(stored.clone(), stored.clone());
        assert!(!changed);
        assert_eq!(same[0].releases_cached_at, Some(1_700_000_000));

        // 只改 group_i18n（身份没变）→ 必须算变化，且保留 Release 缓存
        let (merged, changed) = merge_maps_keeping_runtime_state(stored, renamed);
        assert!(
            changed,
            "只改 group_i18n 也必须写盘，否则线上分组永远不生效"
        );
        assert_eq!(
            merged[0].group_i18n.get("zh-CN").map(String::as_str),
            Some("RickWang")
        );
        assert_eq!(merged[0].releases_cached_at, Some(1_700_000_000));
        assert_eq!(merged[0].last_error.as_deref(), Some("release.rateLimited"));
    }
}

/// 内置地图快照的回归测试。
///
/// 内置索引现在只声明第三方地图仓库（`release-tags`）：条目来自各 Release
/// note，离线解析不出任何地图，首启/断网时地图页为空（联网刷新后才有；
/// 刷新失败沿用上次成功生成的条目）。所以这里锁的不再是「至少一张地图」，
/// 而是「索引能解析、声明字段正确、内置树不偷偷联网」。
///
/// 另外 `repository_sources` 的 `NOT_REPOSITORY_DOCUMENTS` 登记了非仓库类
/// 文档，那边的 `every_non_repository_document_is_registered` 会保证登记表
/// 不漏项。
#[cfg(test)]
mod bundled_tests {
    use super::*;

    fn index_location() -> SourceLocation {
        SourceLocation::parse(BUILTIN_MAP_INDEX).expect("内置索引应该可解析")
    }

    fn index_document() -> MapSourceDocument {
        let location = index_location();
        parse_document(location.bundled_text().unwrap(), &location)
            .expect("内置 maps.toml 应能解析")
    }

    fn declaration(id: &str) -> MapSourceDeclaration {
        index_document()
            .map_sources
            .into_iter()
            .find(|source| source.id == id)
            .unwrap_or_else(|| panic!("索引里应有源 {id}"))
    }

    #[test]
    fn builtin_map_index_declares_only_sources() {
        let doc = index_document();
        assert!(doc.maps.is_empty(), "索引只声明源，条目在各自源里");
        assert_eq!(
            doc.map_sources.len(),
            1,
            "内置索引只保留第三方地图仓库（release-tags）"
        );
        assert_eq!(doc.map_sources[0].id, "community-maps");
    }

    /// 第三方源：一个 tag 一张图，元数据跟着各自 Release note 走。
    ///
    /// 单测不联网，所以这里只锁「声明写得对」；真正解析 note 块的逻辑在
    /// `map_note` 的单测里（纯函数、离线可跑）。
    #[test]
    fn builtin_community_source_declares_release_tags_mode() {
        let source = declaration("community-maps");
        assert_eq!(source.mode, MapSourceMode::ReleaseTags);
        assert_eq!(source.provider, Some(crate::models::Provider::Github));
        assert_eq!(source.username, "QingyuQingxue");
        assert_eq!(source.repository, "OC2-Map-Bundle");
        assert_eq!(source.page_size, Some(100), "一次读 100 个 Release");
        assert_eq!(source.max_pages, Some(10), "最多翻 10 页");
        assert!(source.map.is_none(), "release-tags 不需要内联条目");
    }

    /// 内置树（离线）为空：唯一的源是 `release-tags`，条目要联网读 Release
    /// note 才生成。
    ///
    /// 这条断言守住「内置树不偷偷联网、不伪造 tag 条目」；离线首启的空地图页
    /// 是这套配置的已知代价（要内置离线地图就再加一个 `repository` 源）。
    #[test]
    fn bundled_tree_is_empty_offline() {
        assert!(
            bundled_map_tree().is_empty(),
            "内置索引只有 release-tags 源：离线解析不出条目"
        );
    }
}

/// 地图源加载期间的外部依赖：缓存、HTTP 客户端、配置、令牌、刷新批次。
///
/// 源树加载本身是纯解析，但「文档放进 Release 资产」这一形态要向 API 要
/// size/sha256，而 API 请求必须按仓库收敛（一次刷新里同一个仓库只打一次，
/// 见 `release_cache`）。显式传进来而不是让解析函数去读全局单例，测试才能
/// 不联网地跑。
pub struct MapSourceContext<'a> {
    pub cache: &'a crate::release_cache::RepoReleaseCache,
    pub client: &'a reqwest::Client,
    pub config: &'a AppConfig,
    pub github_token: Option<&'a str>,
    pub gitee_token: Option<&'a str>,
    /// 一次用户动作的批次身份：同一批次里同一个仓库只请求一次 Release API。
    pub pass_id: Option<&'a str>,
    /// 上一次加载出来的条目。
    ///
    /// 只给「需要联网才能变成列表」的源兜底：那些源取不到 API 时不能整块
    /// 消失（用户会以为作者下架了），沿用旧条目比清空诚实。文档型源不用它 ——
    /// 文档取不到本来就有内置树兜底。
    pub previous: &'a [MapRepositoryConfig],
}

impl MapSourceContext<'_> {
    fn token(&self, provider: &Provider) -> Option<&str> {
        match provider {
            Provider::Github => self.github_token,
            Provider::Gitee => self.gitee_token,
        }
    }

    fn api_proxies(&self, provider: &Provider) -> &[crate::models::GitHubProxy] {
        match provider {
            Provider::Github => &self.config.github_proxies.github_api_proxies,
            Provider::Gitee => &self.config.github_proxies.gitee_api_proxies,
        }
    }

    /// 上一次成功生成、属于这个源的条目。
    fn previous_for(&self, source_id: &str) -> Vec<MapRepositoryConfig> {
        self.previous
            .iter()
            .filter(|map| map.source_id.as_deref() == Some(source_id))
            .cloned()
            .collect()
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
    /*
     * tag 模式下 `max_release_count` 的语义变了：它现在是「最多显示几个版本」，
     * 与「列表要拉多少条」无关 —— 要读的那个 tag 可能落在很旧的位置，所以
     * per_page 固定拉满。两个语义不能共用一个数，共用的后果是条目把
     * `max_release_count` 写成 3（只想显示 3 个版本）之后就再也找不到自己的 tag。
     */
    let per_page = if map_tag(map).is_some() {
        100
    } else {
        map.max_release_count
    };
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
        "max_release_count": per_page,
        "download_threads": map.download_threads,
        "asset_rules": map.asset_rules,
        "install_target": "",
    }))
    .expect("地图投影的字段形状固定，构造失败说明模型改了默认值");
    crate::config::repository_from_export(&export, map.id.clone(), map.builtin)
}

/// 刷新一个地图仓库的 Release 列表（tag 模式下返回合成的版本列表）。
///
/// 与仓库侧同一条路径：`fetch_releases`。区别只有两处 —— 请求按仓库收敛
/// （同一批里同一个仓库只打一次 API），以及 tag 模式下不「取最新那一条」，
/// 而是把那个 tag 的资产逐个拆成版本。
pub(crate) async fn fetch_map_releases(
    ctx: &MapSourceContext<'_>,
    map: &MapRepositoryConfig,
) -> Result<Vec<ReleaseInfo>, String> {
    let probe = release_probe(map);
    let key =
        crate::releases::repository_cache_key(map.provider.clone(), &map.username, &map.repository);
    let token = ctx.token(&map.provider).map(str::to_string);
    let official_api_only = ctx.config.auth.official_api_only;
    let api_proxies = ctx.api_proxies(&map.provider).to_vec();
    let releases = ctx
        .cache
        .list(key, ctx.pass_id, || async move {
            crate::releases::fetch_releases(
                ctx.client,
                &probe,
                token.as_deref(),
                official_api_only,
                &api_proxies,
            )
            .await
            .map_err(|error| error.to_string())
        })
        .await?;
    match map_tag(map) {
        /*
         * tag 模式不走 `filter_channel`：渠道过滤的是 **Release**（稳定/预览），
         * 而这里一个 Release 被拆成了「每个资产一个版本」—— 版本稳不稳定由
         * 作者在条目里声明，不再由 Release 的 prerelease 标记决定。
         * 硬过滤只会让写了 tag 的条目在 stable 渠道下整条消失。
         */
        Some(tag) => synthesize_releases(map, tag, releases.as_ref()),
        None => Ok(crate::releases::filter_channel(
            releases.as_ref().clone(),
            &map.channel,
        )),
    }
}

/// 把一个 tag 的资产拆成「一个资产 = 一个版本」的 Release 列表。
///
/// ## 为什么是合成而不是新模型
///
/// 前端、安装流程（`stage_map_release`）、缓存字段、依赖快照全都按
/// `Vec<ReleaseInfo>` 走。合成一份形状相同的列表，等于让「一个 tag 装一张图的
/// 历史版本」在整条链路上零改动 —— 安装时按 `tag_name` 找到的那条 Release 里
/// 就只有一个资产，正是它自己。
///
/// ## 取哪个数字当版本
///
/// 从资产名里用作者写的正则抽（`version_rule`）。这比解析 tag 名可靠：
/// 一个 tag 里放的是多个文件，版本只在文件名里。抽出来的号同时充当
/// `tag_name` —— 前端拿它当「版本号」显示、当「回滚到这一版」的标识。
pub(crate) fn synthesize_releases(
    map: &MapRepositoryConfig,
    tag: &str,
    releases: &[ReleaseInfo],
) -> Result<Vec<ReleaseInfo>, String> {
    let label = map_label(map);
    let Some(release) = releases
        .iter()
        .find(|release| release.tag_name.trim() == tag)
    else {
        return Err(crate::locale_tag::with_params(
            "mapSource.releaseTagNotFound",
            &[("map", &label), ("tag", tag)],
        ));
    };
    let Some(rule) = version_rule(&map.asset_rules) else {
        return Err(crate::locale_tag::with_params(
            "mapSource.assetRuleNeedsVersion",
            &[("map", &label)],
        ));
    };
    let regex = regex::Regex::new(&rule.value).map_err(|error| {
        log::error!("地图版本规则无法编译：地图={}，原因={}", label, error);
        crate::locale_tag::with_params("mapSource.assetRuleNeedsVersion", &[("map", &label)])
    })?;
    let mut versions: Vec<(String, ReleaseAsset)> = Vec::new();
    for asset in &release.assets {
        let Some(captures) = regex.captures(&asset.name) else {
            continue;
        };
        /*
         * 整串匹配才算：`asset_matches` 用的是 `is_match`（子串命中即真），
         * 不锚定就会出现「同一个文件同时匹配出两个版本号」。解析阶段已经
         * 要求规则写成 `^…$`，这里再判一次是因为配置里的旧条目可能没经过
         * 这道校验。
         */
        let Some(whole) = captures.get(0) else {
            continue;
        };
        if whole.start() != 0 || whole.end() != asset.name.len() {
            log::debug!(
                "资产名不是整串匹配，忽略：地图={}，资产={}",
                label,
                asset.name
            );
            continue;
        }
        let version = captures
            .name("version")
            .or_else(|| captures.get(1))
            .map(|matched| matched.as_str().trim().to_string())
            .unwrap_or_default();
        if version.is_empty() {
            continue;
        }
        if versions.iter().any(|(existing, _)| existing == &version) {
            return Err(crate::locale_tag::with_params(
                "mapSource.duplicateVersion",
                &[("map", &label), ("version", &version)],
            ));
        }
        versions.push((version, asset.clone()));
    }
    if versions.is_empty() {
        return Err(crate::locale_tag::with_params(
            "mapSource.noAssetInTag",
            &[("map", &label), ("tag", tag)],
        ));
    }
    /* 作者声明的「这就是最新版」必须真的存在，否则界面会把一张不存在的版本
     * 标成最新，用户点安装却找不到资产。 */
    let forced = map
        .latest_version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty());
    if let Some(forced) = forced
        && !versions.iter().any(|(version, _)| version == forced)
    {
        return Err(crate::locale_tag::with_params(
            "mapSource.latestVersionMissing",
            &[("map", &label), ("version", forced)],
        ));
    }
    versions.sort_by(|(left, _), (right, _)| newest_first(left, right));
    if let Some(forced) = forced
        && let Some(index) = versions.iter().position(|(version, _)| version == forced)
    {
        // 置顶而不是重排：其余版本仍按数字顺序，作者只说「它是最新」。
        let entry = versions.remove(index);
        versions.insert(0, entry);
    }
    // tag 模式下这个字段是「最多暴露几个版本」，见 `release_probe` 的说明
    versions.truncate(map.max_release_count.clamp(1, 100) as usize);
    Ok(versions
        .into_iter()
        .map(|(version, asset)| ReleaseInfo {
            // 多个版本共用一个真实 Release，ID 只能留空：留着会让它们撞键
            release_id: None,
            tag_name: version.clone(),
            version: Some(version.clone()),
            name: format!("v{version}"),
            // 版本详情里显示的是作者写给人看的说明，元数据块是给管理器的，
            // 不该跟着 body 一起流到界面上（它是 HTML 注释，但别指望渲染器）
            body: crate::map_note::strip_note_block(&release.body),
            published_at: release.published_at.clone(),
            // 渠道过滤已按作者声明跳过，这两个标记不能再参与判定
            prerelease: false,
            draft: false,
            assets: vec![asset],
        })
        .collect())
}

/// 版本号里的数字段，用于排序。
///
/// 只按数字段比：`0.10` > `0.9`，`1.7.0` > `1.7`。段数不一致是常态（作者手写
/// 的文件名），纯字符串比较会把 `0.10` 排到 `0.9` 前面，更错。
fn version_parts(version: &str) -> Vec<u64> {
    version
        .split(|character: char| !character.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse::<u64>().ok())
        .collect()
}

/// 新的排前面（降序）。
fn newest_first(left: &str, right: &str) -> std::cmp::Ordering {
    version_parts(right)
        .cmp(&version_parts(left))
        .then_with(|| right.cmp(left))
}

/*
 * 「源怎么变成地图条目」三种模式的回归测试。
 *
 * 全部不联网：`build_declaration_map` 与 `note_block_map` 都是纯函数，
 * 输入是声明与 note 文本，API 与缓存都在它们之外。
 */
#[cfg(test)]
mod declaration_tests {
    use super::*;

    fn origin() -> SourceLocation {
        SourceLocation::parse(BUILTIN_MAP_INDEX).expect("内置索引应该可解析")
    }

    fn parse(text: &str) -> Result<MapSourceDocument, ConfigError> {
        parse_document(text, &origin())
    }

    fn error_tag(result: Result<MapSourceDocument, ConfigError>) -> String {
        match result {
            Ok(_) => panic!("应当报错"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn a_repository_source_builds_one_inline_map() {
        let doc = parse(
            r#"
[[map_sources]]
id = "solo"
mode = "repository"
provider = "gitee"
username = "u"
repository = "r"
[map_sources.map]
name_i18n.zh-CN = "单仓地图"
level_set_name = "Solo"
author = "A"
asset_rules = [{ type = "glob", value = "solo_v*.zip" }]
[[map_sources.map.relations]]
kind = "requires"
guid = "dev.gua.overcooked.diylevel"
min_version = "0.10.0"
"#,
        )
        .unwrap();
        let source = &doc.map_sources[0];
        assert_eq!(source.mode, MapSourceMode::Repository);
        assert_eq!(
            source.map.as_ref().unwrap().name,
            "单仓地图",
            "单语言镜像应从 zh-CN 折出来"
        );
        let label = SourceLabel {
            id: source.id.clone(),
            ..SourceLabel::default()
        };
        let map = build_declaration_map(source, &label, true, &origin()).unwrap();
        // 声明级坐标落到条目上，否则取 Release 时 404
        assert_eq!(map.provider, crate::models::Provider::Gitee);
        assert_eq!(map.username, "u");
        assert_eq!(map.repository, "r");
        assert_eq!(map.id, map_repository_id("solo", &map));
        assert_eq!(map.relations.len(), 1);
        assert!(map.builtin);
    }

    #[test]
    fn a_document_source_must_carry_a_url() {
        let tag = error_tag(parse("[[map_sources]]\nid = \"x\"\n"));
        assert!(tag.contains("mapSource.declarationMissingUrl"), "{tag}");
    }

    #[test]
    fn a_repository_source_must_carry_its_map_entry() {
        let tag = error_tag(parse(
            "[[map_sources]]\nid = \"x\"\nmode = \"repository\"\nusername = \"u\"\nrepository = \"r\"\n",
        ));
        assert!(tag.contains("mapSource.declarationMissingMap"), "{tag}");
    }

    #[test]
    fn a_release_tags_source_must_name_its_repository() {
        let tag = error_tag(parse(
            "[[map_sources]]\nid = \"x\"\nmode = \"release-tags\"\n",
        ));
        assert!(tag.contains("mapSource.missingCoordinates"), "{tag}");
    }

    #[test]
    fn a_release_note_block_becomes_a_tag_mode_entry() {
        let body = r#"共 2 张关卡。

<!-- oc2-map
level_set_name = "GAODA"
level_set_name_zh = "胡闹达学"
author = "GAODA"
name_i18n.zh-CN = "胡闹达学"
name_i18n.en = "GAODA"
note_i18n.zh-CN = "两张关卡，需要 DIYLevel。"
asset = "daxue_v${version}.zip"
latest_version = "0.3.2"
requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }
-->
"#;
        let doc = parse(
            "[[map_sources]]\nid = \"community\"\nmode = \"release-tags\"\nprovider = \"github\"\nusername = \"CH3NGYZ\"\nrepository = \"oc2\"\n",
        )
        .unwrap();
        let declaration = &doc.map_sources[0];
        let note = crate::map_note::parse_release_note(body).unwrap().unwrap();
        let rule = crate::map_note::versioned_asset_rule(
            note.metadata.asset.as_deref(),
            &[crate::models::ReleaseAsset {
                name: "daxue_v0.3.2.zip".to_string(),
                size: 1,
                download_count: None,
                download_url: String::new(),
                digest: None,
            }],
        )
        .unwrap();
        let label = SourceLabel {
            id: declaration.id.clone(),
            name: "社区地图仓库".to_string(),
            ..SourceLabel::default()
        };
        let map =
            note_block_map(declaration, &label, &note, "daxue", rule, true, &origin()).unwrap();

        // 一个 tag 一张地图：tag 来自 Release，身份来自 note 里的块
        assert_eq!(map_tag(&map), Some("daxue"));
        assert_eq!(map.level_set_name, "GAODA");
        assert_eq!(map.level_set_name_zh, "胡闹达学");
        assert_eq!(map.author, "GAODA");
        // 卡片短语只来自块里的 note_i18n，单语言镜像跟着折出来
        assert_eq!(
            map.note_i18n.get("zh-CN").map(String::as_str),
            Some("两张关卡，需要 DIYLevel。")
        );
        assert_eq!(map.note, "两张关卡，需要 DIYLevel。");
        assert_eq!(map.name, "胡闹达学", "显示名单语言镜像必须折出来");
        assert!(map.id.starts_with("community::"), "{}", map.id);
        assert_eq!(map.source_id.as_deref(), Some("community"));
        assert_eq!(map.relations.len(), 1);
        assert_eq!(map.latest_version.as_deref(), Some("0.3.2"));
        // 版本号从资产名里按模板抽，规则必须整串锚定
        assert_eq!(map.asset_rules.len(), 1);
        assert!(map.asset_rules[0].value.starts_with('^'));
        assert!(map.asset_rules[0].value.ends_with('$'));
    }

    /// 真机联网回归：内置索引 → 单仓库地图 + release-tags 源 → 真实条目。
    ///
    /// 默认 `#[ignore]`：单测不联网。改完 tag 源或 `map_note` 的字段后手动跑一次：
    ///
    /// ```text
    /// cargo test --manifest-path src-tauri/Cargo.toml --lib \
    ///   real_release_tag_source_builds_maps -- --ignored --nocapture
    /// ```
    ///
    /// 它验证的是整条真实链路：`maps.toml` → `load_map_tree` → 分页读完全部
    /// Release → 解析每份 note 的 oc2-map 块 → 生成带版本规则的地图条目。
    #[ignore]
    #[tokio::test]
    async fn real_release_tag_source_builds_maps() {
        let config = crate::config::default_config();
        let cache = crate::release_cache::RepoReleaseCache::default();
        let client = crate::releases::build_client(config.network.timeout_seconds)
            .expect("HTTP 客户端应可创建");
        let ctx = MapSourceContext {
            cache: &cache,
            client: &client,
            config: &config,
            github_token: None,
            gitee_token: None,
            pass_id: Some("real-release-tags"),
            previous: &[],
        };
        let maps = load_map_tree(crate::config::current_user_region(), true, &ctx)
            .await
            .expect("内置索引应能加载");

        // 单仓库地图（RW）离线就有；三张 tag 模式地图来自真实 Release note
        assert!(
            maps.iter().any(|map| map.level_set_name == "RW_LevelSet"),
            "单仓库地图应来自内联条目：{}",
            maps.len()
        );
        let mut tags: Vec<&str> = maps
            .iter()
            .filter(|map| map.source_id.as_deref() == Some("community-maps"))
            .filter_map(|map| map.tag.as_deref())
            .collect();
        tags.sort_unstable();
        assert_eq!(
            tags,
            ["blni", "chan", "daxue"],
            "三份 Release note 应各生成一张图"
        );

        let gaoda = maps
            .iter()
            .find(|map| map.level_set_name == "GAODA")
            .expect("胡闹达学应在列表里");
        assert_eq!(gaoda.author, "GAODA");
        assert_eq!(gaoda.level_set_name_zh, "胡闹达学");
        assert_eq!(gaoda.provider, crate::models::Provider::Github);
        assert_eq!(gaoda.username, "CH3NGYZ");
        assert_eq!(gaoda.repository, "oc2");
        assert!(
            !gaoda.note_i18n.get("zh-CN").is_none_or(String::is_empty),
            "卡片说明应来自 note_i18n：{:?}",
            gaoda.note_i18n.keys().collect::<Vec<_>>()
        );
        assert!(gaoda.name_i18n.contains_key("en"));
        assert!(
            gaoda
                .asset_rules
                .iter()
                .any(|rule| rule.value.starts_with('^') && rule.value.ends_with('$')),
            "版本规则必须整串锚定"
        );
        assert_eq!(gaoda.latest_version.as_deref(), Some("0.3.2"));
        assert!(
            gaoda
                .relations
                .iter()
                .any(|relation| relation.guid == "dev.gua.overcooked.diylevel")
        );

        // 没有元数据块的 Release（Dmm）不该混进来
        assert!(
            maps.iter().all(|map| map.tag.as_deref() != Some("Dmm")),
            "没有 oc2-map 块的 Release 必须被跳过"
        );
    }
}

/*
 * tag 模式（一个 tag 装一张图的历史版本）的纯函数测试。
 *
 * 全部不联网：`synthesize_releases` 只做「给定真实 Release + 条目声明 →
 * 合成版本列表」，API 与缓存都在它之外。
 */
#[cfg(test)]
mod tag_mode_tests {
    use super::*;

    const VERSION_RULE: &str = r#"{ type = "regex", value = '^rw_v(?P<version>[0-9.]+)\.zip$' }"#;

    fn entry(rules: &str, extra: &str) -> MapRepositoryConfig {
        let text = format!(
            r#"
[[maps]]
name = "RW"
provider = "github"
username = "u"
repository = "r"
level_set_name = "RW_LevelSet"
author = "RW"
{extra}
asset_rules = [{rules}]
"#
        );
        parse_document(&text, &SourceLocation::parse("maps.toml").unwrap())
            .unwrap()
            .maps
            .into_iter()
            .next()
            .unwrap()
    }

    fn tag_entry(extra: &str) -> MapRepositoryConfig {
        entry(VERSION_RULE, extra)
    }

    fn release(tag: &str, assets: &[&str]) -> ReleaseInfo {
        ReleaseInfo {
            release_id: Some(7),
            tag_name: tag.to_string(),
            version: None,
            name: "地图合集".to_string(),
            body: "更新说明".to_string(),
            published_at: Some("2026-01-01T00:00:00Z".to_string()),
            prerelease: true,
            draft: false,
            assets: assets
                .iter()
                .map(|name| ReleaseAsset {
                    name: name.to_string(),
                    size: 1024,
                    download_count: None,
                    download_url: format!("https://github.com/u/r/releases/download/{tag}/{name}"),
                    digest: None,
                })
                .collect(),
        }
    }

    fn tag_names(releases: &[ReleaseInfo]) -> Vec<&str> {
        releases
            .iter()
            .map(|release| release.tag_name.as_str())
            .collect()
    }

    #[test]
    fn one_tag_becomes_one_release_per_asset() {
        // 现实形态：一个 tag 里放同一张图的多个版本，文件名带版本号
        let map = tag_entry("tag = \"maps\"");
        let releases = synthesize_releases(
            &map,
            "maps",
            &[release("maps", &["rw_v0.22.zip", "rw_v0.78.zip"])],
        )
        .unwrap();
        assert_eq!(tag_names(&releases), ["0.78", "0.22"]);
        assert_eq!(releases[0].version.as_deref(), Some("0.78"));
        assert_eq!(releases[0].name, "v0.78");
        assert_eq!(releases[0].assets.len(), 1);
        assert_eq!(releases[0].assets[0].name, "rw_v0.78.zip");
        /*
         * 两条版本共用同一个真实 Release，`release_id` 留空是刻意的：
         * 前端拿它当列表键，重复的 ID 会让两条版本互相覆盖。
         */
        assert_eq!(releases[0].release_id, None);
        // 渠道由作者在条目里声明，不再由 Release 的预发布标记决定
        assert!(!releases[0].prerelease);
        // 说明与时间仍来自真实 Release：版本列表要显示「这次更新了什么」
        assert_eq!(releases[0].body, "更新说明");
        assert_eq!(
            releases[0].published_at.as_deref(),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn assets_that_do_not_match_the_rule_are_skipped() {
        let map = tag_entry("tag = \"maps\"");
        let releases = synthesize_releases(
            &map,
            "maps",
            &[release(
                "maps",
                &["rw_v0.22.zip", "readme.md", "source.zip"],
            )],
        )
        .unwrap();
        assert_eq!(tag_names(&releases), ["0.22"]);
    }

    #[test]
    fn the_declared_latest_version_goes_first_without_reordering_the_rest() {
        /*
         * 作者认为 0.45 才是最新（0.7 与 0.69 那种「按数字比会排错」的现实问题）。
         * 置顶只影响第一条，其余仍按版本号降序 —— 作者只说「它是最新」，没说
         * 其余版本怎么排。
         */
        let map = tag_entry("tag = \"maps\"\nlatest_version = \"0.45\"");
        let releases = synthesize_releases(
            &map,
            "maps",
            &[release(
                "maps",
                &["rw_v0.22.zip", "rw_v0.45.zip", "rw_v0.78.zip"],
            )],
        )
        .unwrap();
        assert_eq!(tag_names(&releases), ["0.45", "0.78", "0.22"]);
    }

    #[test]
    fn versions_sort_by_numeric_segments() {
        // 0.10 比 0.9 新：纯字符串比较会反着排
        let map = tag_entry("tag = \"maps\"");
        let releases = synthesize_releases(
            &map,
            "maps",
            &[release("maps", &["rw_v0.9.zip", "rw_v0.10.zip"])],
        )
        .unwrap();
        assert_eq!(tag_names(&releases), ["0.10", "0.9"]);
    }

    #[test]
    fn an_asset_hosted_document_may_not_declare_sub_sources() {
        let text = r#"
[[maps]]
name = "M"
provider = "github"
username = "u"
repository = "r"
level_set_name = "S"
author = "A"
enabled = true
asset_rules = [{ type = "exact", value = "map.zip" }]

[[map_sources]]
id = "child"
nickname = "Child"
url = "child.toml"
"#;
        let asset = SourceLocation::parse(
            "https://github.com/RW/RW/releases/download/maps/map-sources.toml",
        )
        .unwrap();
        let error = parse_document(text, &asset).unwrap_err();
        assert!(
            format!("{error:?}").contains("mapSource.assetDocumentSubSources"),
            "{error:?}"
        );
        // 放在普通地址上的同一份文档仍然可以声明子源
        let plain = SourceLocation::parse("maps.toml").unwrap();
        assert!(parse_document(text, &plain).is_ok());
    }

    #[test]
    fn max_release_count_limits_how_many_versions_are_exposed() {
        let map = tag_entry("tag = \"maps\"\nmax_release_count = 2");
        let releases = synthesize_releases(
            &map,
            "maps",
            &[release(
                "maps",
                &["rw_v0.22.zip", "rw_v0.45.zip", "rw_v0.78.zip"],
            )],
        )
        .unwrap();
        // 截断留最新的两个，而不是列表里的前两个
        assert_eq!(tag_names(&releases), ["0.78", "0.45"]);
    }

    #[test]
    fn a_tag_that_is_not_in_the_repository_is_reported() {
        let map = tag_entry("tag = \"maps\"");
        let err =
            synthesize_releases(&map, "typo", &[release("maps", &["rw_v0.22.zip"])]).unwrap_err();
        assert!(err.contains("mapSource.releaseTagNotFound"), "{err}");
    }

    #[test]
    fn a_tag_without_any_matching_asset_is_reported() {
        let map = tag_entry("tag = \"maps\"");
        let err =
            synthesize_releases(&map, "maps", &[release("maps", &["notes.txt"])]).unwrap_err();
        assert!(err.contains("mapSource.noAssetInTag"), "{err}");
    }

    #[test]
    fn two_assets_that_yield_the_same_version_are_rejected() {
        /*
         * 版本号同时是「安装哪个资产」的标识。两个资产抽出同一个号，
         * 安装时就只能瞎选一个 —— 宁可在刷新时明确报错。
         */
        let map = entry(
            r#"{ type = "regex", value = '^rw_v(?P<version>[0-9.]+)(?:_alt)?\.zip$' }"#,
            "tag = \"maps\"",
        );
        let err = synthesize_releases(
            &map,
            "maps",
            &[release("maps", &["rw_v0.22.zip", "rw_v0.22_alt.zip"])],
        )
        .unwrap_err();
        assert!(err.contains("mapSource.duplicateVersion"), "{err}");
    }

    #[test]
    fn a_declared_latest_version_must_exist_in_the_tag() {
        // 声明了一个不存在的版本：界面会把幽灵版本标成最新，点安装找不到资产
        let map = tag_entry("tag = \"maps\"\nlatest_version = \"9.9\"");
        let err =
            synthesize_releases(&map, "maps", &[release("maps", &["rw_v0.22.zip"])]).unwrap_err();
        assert!(err.contains("mapSource.latestVersionMissing"), "{err}");
    }

    #[test]
    fn a_version_rule_without_anchors_is_rejected_at_parse_time() {
        let text = format!(
            r#"
[[maps]]
name = "RW"
provider = "github"
username = "u"
repository = "r"
level_set_name = "RW_LevelSet"
author = "RW"
tag = "maps"
asset_rules = [{{ type = "regex", value = 'rw_v(?P<version>[0-9.]+)\.zip' }}]
"#
        );
        let err = parse_document(&text, &SourceLocation::parse("maps.toml").unwrap()).unwrap_err();
        // 不锚定就会「同一个文件匹配出两个版本号」，必须在加载时就拒绝
        assert!(err.to_string().contains("mapSource.regexNotAnchored"));
    }

    #[test]
    fn a_rule_without_a_version_group_is_rejected_at_parse_time() {
        let text = format!(
            r#"
[[maps]]
name = "RW"
provider = "github"
username = "u"
repository = "r"
level_set_name = "RW_LevelSet"
author = "RW"
tag = "maps"
asset_rules = [{{ type = "regex", value = '^rw_v[0-9.]+\.zip$' }}]
"#
        );
        let err = parse_document(&text, &SourceLocation::parse("maps.toml").unwrap()).unwrap_err();
        assert!(err.to_string().contains("mapSource.assetRuleNeedsVersion"));
    }

    #[test]
    fn an_exact_rule_is_enough_for_the_legacy_single_release_mode() {
        // 没写 tag 的条目还是老语义：一条 Release 一个资产，不需要抽版本号
        let map = entry(r#"{ type = "exact", value = "rw.zip" }"#, "");
        assert_eq!(map_tag(&map), None);
    }

    #[test]
    fn bundle_coordinates_are_inherited_only_by_entries_that_omit_them() {
        let text = r#"
[bundle]
provider = "gitee"
username = "QingyuQingxue"
repository = "OC2-Map-Bundle"

[[maps]]
name = "A"
level_set_name = "A_Set"
author = "Qingyu"
asset_rules = [{ type = "exact", value = "a.zip" }]

[[maps]]
name = "B"
provider = "github"
username = "other"
repository = "repo"
level_set_name = "B_Set"
author = "Other"
asset_rules = [{ type = "exact", value = "b.zip" }]
"#;
        let doc = parse_document(text, &SourceLocation::parse("maps.toml").unwrap()).unwrap();
        assert_eq!(doc.maps[0].provider, Provider::Gitee);
        assert_eq!(doc.maps[0].username, "QingyuQingxue");
        assert_eq!(doc.maps[0].repository, "OC2-Map-Bundle");
        // 显式写了坐标的条目不受 bundle 影响
        assert_eq!(doc.maps[1].provider, Provider::Github);
        assert_eq!(doc.maps[1].username, "other");
        assert!(doc.bundle.is_none(), "bundle 已展开，不再留在文档上");
    }

    #[test]
    fn a_bundle_without_coordinates_is_rejected() {
        let text = r#"
[bundle]
username = ""
repository = "OC2-Map-Bundle"

[[maps]]
name = "A"
level_set_name = "A_Set"
author = "Qingyu"
asset_rules = [{ type = "exact", value = "a.zip" }]
"#;
        let err = parse_document(text, &SourceLocation::parse("maps.toml").unwrap()).unwrap_err();
        assert!(err.to_string().contains("mapSource.missingCoordinates"));
    }
}
