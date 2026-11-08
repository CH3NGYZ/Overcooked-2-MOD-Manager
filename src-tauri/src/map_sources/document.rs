use super::assets::version_rule;
use super::identity::map_identity;
use super::identity::map_label;
use super::identity::map_repository_id;
use super::identity::map_tag;
use super::state::SourceLabel;
use crate::config::ConfigError;
use crate::models::MapRepositoryConfig;
use crate::models::Provider;
use crate::repository_sources::SourceLocation;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::collections::HashSet;

/// 地图源文档。
///
/// 结构与 `RepositorySourceDocument` 对称：一个源文件既可以直接声明地图，
/// 也可以声明下一级子源 —— 于是「一个作者一个 TOML、里面放他的多个地图」
/// 和「按主题拆成多份子源」两种组织方式都支持。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MapSourceDocument {
    #[serde(rename = "type", default)]
    pub kind: Option<crate::models::RepositoryKind>,
    #[serde(default)]
    pub network_plan: crate::models::NetworkPlanSelection,
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
/// 条目的 username/repository 都非空时保留整组坐标；任一缺失时两者一起
/// 从 bundle 替换，并采用 bundle 显式给出的 provider，不逐字段拼接两个仓库。
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
    #[serde(default)]
    pub network_plan: crate::models::NetworkPlanSelection,
    pub id: String,
    #[serde(default)]
    pub mode: MapSourceMode,
    /// 源地址：完整 URL、内置资源名，或**相对当前文档**的路径。
    /// 仅 `mode = "document"` 使用。
    #[serde(default)]
    pub url: String,
    /// 声明级仓库坐标：`repository` / `release-tags` 模式下的仓库。
    ///
    /// 单仓库地图的 username/repository 非空时保留条目值，缺失才从声明补齐；
    /// provider 则在声明显式给出时覆盖条目，见 apply_declaration_coordinates。
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub repository: String,
    /// 单仓库地图的条目本体（`mode = "repository"`）。
    #[serde(default)]
    pub map: Option<MapRepositoryConfig>,
    /// 兼容读取旧配置；分页已统一为 50，不再接受源级覆盖。
    #[allow(dead_code)]
    #[serde(default)]
    pub page_size: Option<u32>,
    /// 兼容读取旧配置；完整读取到末页，不再用旧页数截断。
    #[allow(dead_code)]
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
/// 先展开当前文档的公共坐标，再校验身份名、仓库坐标和资产规则；展示字段
/// 可为空。源格式校验不代替安装时对真实 ZIP 身份的检查。
pub fn parse_document(
    text: &str,
    origin: &SourceLocation,
    inherited: &crate::models::NetworkPlanSelection,
) -> Result<MapSourceDocument, ConfigError> {
    let document: MapSourceDocument = toml::from_str(text).map_err(|error| {
        ConfigError::Invalid(crate::locale_tag::with_params(
            "source.tomlInvalid",
            &[("reason", &error.to_string())],
        ))
    })?;

    let mut document = document;
    document.network_plan = document.network_plan.inherit(inherited);
    if document
        .kind
        .is_some_and(|kind| kind != crate::models::RepositoryKind::Map)
    {
        return Err(ConfigError::Invalid("proxy.validationFailed".into()));
    }
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
        entry.network_plan = entry.network_plan.inherit(&document.network_plan);
        fold_i18n_mirror(&mut entry.name, &entry.name_i18n);
        fold_i18n_mirror(&mut entry.group, &entry.group_i18n);
        fold_i18n_mirror(&mut entry.note, &entry.note_i18n);
    }
    // 子源同理：昵称只写多语言表就够，加载时折出单语言镜像随条目落盘
    for child in &mut document.map_sources {
        child.network_plan = child.network_plan.inherit(&document.network_plan);
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
            crate::network_policy::NetworkPolicy::shared()
                .and_then(|policy| {
                    policy.validate_selection(
                        &declaration.provider.clone().unwrap_or(Provider::Github),
                        crate::models::RepositoryKind::Map,
                        &declaration.network_plan,
                    )
                })
                .map_err(ConfigError::Invalid)?;
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
    map.network_plan = map.network_plan.inherit(&declaration.network_plan);
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

/// 单条地图的必填校验。
///
/// 只查「解析后一定会用到、缺失就没有意义」的字段：
///
/// - `level_set_name`：必须非空；author 允许为空以兼容已有地图；
/// - `username` / `repository`：取 Release 所需，provider 使用模型默认值；
/// - `asset_rules`：没有规则就选不出要下载哪个文件。
///
/// 名字与备注缺失**不算错**：界面会退回仓库坐标显示，总比整份源解析失败好。
pub(super) fn validate_map_entry(
    entry: &MapRepositoryConfig,
    origin: &SourceLocation,
) -> Result<(), ConfigError> {
    crate::network_policy::NetworkPolicy::shared()
        .and_then(|policy| {
            policy.validate_selection(
                &entry.provider,
                crate::models::RepositoryKind::Map,
                &entry.network_plan,
            )
        })
        .map_err(ConfigError::Invalid)?;
    if entry.kind != crate::models::RepositoryKind::Map {
        return Err(ConfigError::Invalid("proxy.validationFailed".into()));
    }
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

/// 把「单仓库地图」声明变成一张地图条目。
///
/// 声明里的 `[map_sources.map]` 就是条目本体，这里只补三样它不该重复写的
/// 东西：源归属、内置标记、跨刷新稳定的 ID。
pub(super) fn build_declaration_map(
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
