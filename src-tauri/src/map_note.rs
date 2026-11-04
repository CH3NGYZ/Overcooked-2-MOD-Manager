//! Release note 里的地图元数据块。
//!
//! ## 为什么元数据放在 note 里
//!
//! 「一个仓库放多张地图」需要的不是又一份 TOML 源文档，而是**让地图信息跟着
//! Release 自己走**：作者发一个新 tag 就等于发布一张新地图，改一次 note 就是
//! 改地图的说明与依赖，不需要再维护、再同步一份集中式索引。
//!
//! 于是约定在 Release note 里写一段隐藏注释：
//!
//! ```text
//! <!-- oc2-map
//! level_set_name        = "GAODA"
//! level_set_name_zh     = "胡闹达学"
//! author                = "GAODA"
//! name_i18n.zh-CN       = "胡闹达学"
//! name_i18n.en          = "GAODA"
//! asset                 = "daxue_v${version}.zip"
//! latest_mapset_version = "0.3.2"
//! latest_info_version   = "0.3.2"
//! requires              = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }
//! -->
//! ```
//!
//! 注释在 GitHub 上不渲染，普通用户看到的仍然是正常的更新说明；块外面的文字
//! 原样成为地图卡片上的「说明」。格式由伴生仓的 `tools/oc2-info` 生成，两边
//! 必须保持同一套字段名与区间语法。
//!
//! ## 与 `[[maps]]` 条目字段的对应
//!
//! 块里的字段就是同名字段的一个子集，`requires` / `compatible` / `conflicts`
//! 三张表按 `RelationKind` 展开成 `RepositoryRelation`。剩下的字段（provider、
//! username、repository、tag、asset_rules 的形态）由解析方按「源声明 + Release
//! 实际资产」补齐 —— 那些是仓库坐标，写在每份 note 里只会重复。
use crate::models::{
    AssetRule, AssetRuleType, RelationKind, ReleaseAsset, ReleaseRelationOverride,
    RepositoryRelation,
};
use serde::Deserialize;
use std::collections::BTreeMap;

/// 元数据块的起始标记。
pub const NOTE_BLOCK_OPEN: &str = "<!-- oc2-map";

/// 元数据块的结束标记。
pub const NOTE_BLOCK_CLOSE: &str = "-->";

/// 资产名里的版本号占位符（`asset = "daxue_v${version}.zip"`）。
pub const VERSION_PLACEHOLDER: &str = "${version}";

/// 版本号的形状：数字段，点分隔。与 `oc2-info` 的区间语法共用。
///
/// 不做「必须两段」之类的限制：`0.73`、`1.0.3`、`2` 都是作者真实用过的写法。
const VERSION_PATTERN: &str = r"[0-9]+(?:\.[0-9]+)*";

/// 一条 Release note 里的地图元数据。
///
/// 未知字段一律忽略：新版本的 `oc2-info` 可能写进旧管理器不认识的键，
/// 那不该让整张地图从列表里消失。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct MapNoteMetadata {
    /// 本地关卡集的 `levelSetName`。必填 —— 没有它就无法与本地绑定。
    #[serde(default)]
    pub level_set_name: String,
    #[serde(default)]
    pub level_set_name_zh: String,
    /// 本地关卡集的 `author`（官方导出常为空串，此时绑定按空串比对）。
    #[serde(default)]
    pub author: String,
    /// 地图卡片的显示名。块里只需要写多语言表，单语言镜像由解析方折出来。
    #[serde(default)]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    /// 卡片上的**简短**说明（五语）。卡片放不下长文，长文在块外面。
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
    /// `note_i18n` 的单语言写法，折出镜像时用它兜底。
    #[serde(default)]
    pub note: String,
    /// 安装包名模板，`${version}` 由资产名里的实际版本号替换。
    #[serde(default)]
    pub asset: Option<String>,
    /// 作者声明的「这就是最新版」（**地图包**版本）。
    ///
    /// 旧名 `latest_version` 仍可反序列化：已经发布出去的 Release note 写的就是
    /// 那个名字，改名不能把那些地图的声明变成「没写」（那样它们会退回按 tag
    /// 比较，正是这个字段要消除的歧义）。
    #[serde(default, alias = "latest_version")]
    pub latest_mapset_version: Option<String>,
    /// 这次 Release 里 `info` 文件**内**的版本号（`tools/oc2-info` 自动取）。
    ///
    /// 与 `latest_mapset_version`（作者定义的地图包版本）平时相同；作者升了包版本却忘了
    /// 改 info 里的版本号时会不同，那时管理器按这一项判断本地装的是不是最新版。
    #[serde(default)]
    pub latest_info_version: Option<String>,
    /// 按地图版本覆盖下面三张表的整表（`[[release_relations]]`）。
    ///
    /// 与源文档 `[[maps.release_relations]]` 同构：命中区间的那张整体替换
    /// `requires`/`compatible`/`conflicts`，用来表达「2.0 起依赖 Web Runtime
    /// 0.20 而不是 0.10」。多数地图用不到，留空即可。
    #[serde(default)]
    pub release_relations: Vec<NoteReleaseRelations>,
    #[serde(default)]
    pub requires: BTreeMap<String, RelationSpec>,
    #[serde(default)]
    pub compatible: BTreeMap<String, RelationSpec>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, RelationSpec>,
}

impl MapNoteMetadata {
    /// 归一化后的 `latest_info_version`（手写块的首尾空白不可控）。
    pub fn normalized_info_version(&self) -> Option<String> {
        let value = self.latest_info_version.as_deref()?.trim();
        (!value.is_empty()).then(|| value.to_string())
    }
}

/// 关系值的两种写法：区间字符串，或显式表。
///
/// ```toml
/// requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0,<=0.11.9" }
/// conflicts = { "oc2.oc2diylevelruntimewloader" = { version = ">=4.0.0", prerelease = true } }
/// ```
///
/// 区间语法与仓库源、管理器配置完全一致（见 `version_range`）；显式表只在
/// 需要 `prerelease` / `optional` 时用，`version` 留空表示不限版本。
/// 旧写法 `min_version` / `max_version` 仍能读，由 `fold_bounds` 折成区间。
/// 与 `oc2-info` 的 `--requires` / `--compatible` / `--conflicts` 参数同构。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RelationSpec {
    Range(String),
    Detailed {
        #[serde(default)]
        version: String,
        /// 旧写法，仍接受。
        #[serde(default)]
        min_version: String,
        #[serde(default)]
        max_version: String,
        #[serde(default)]
        prerelease: Option<bool>,
        #[serde(default)]
        optional: bool,
    },
}

impl RelationSpec {
    /// 展开成 (规范化区间, 预发布三态, 可选)。区间写错返回 `None`。
    ///
    /// 规范化让两种写法落到同一段文本上（`=1.2` → `==1.2`、旧
    /// `min_version = "1.0"` → `>=1.0`），前端与 `oc2-info` 都只需认一种形态。
    fn spec(&self) -> Option<(String, Option<bool>, bool)> {
        let (raw, prerelease, optional) = match self {
            Self::Range(raw) => (raw.trim().to_string(), None, false),
            Self::Detailed {
                version,
                min_version,
                max_version,
                prerelease,
                optional,
            } => {
                let raw = if version.trim().is_empty() {
                    crate::version_range::fold_bounds(min_version.trim(), max_version.trim())
                } else {
                    version.trim().to_string()
                };
                (raw, *prerelease, *optional)
            }
        };
        let range = crate::version_range::VersionRange::parse(&raw).ok()?;
        Some((range.canonical(), prerelease, optional))
    }
}

/// Release note 里的一张按版本覆盖表（`[[release_relations]]`）。
///
/// 内层写法和块里的三张关系表一致（`requires` / `compatible` / `conflicts`
/// 各一张 GUID → 区间映射），这样作者可以直接把某一段关系从上面搬下来换掉
/// 区间，`tools/oc2-info --print` 也是按这个形态读写的；同时接受
/// `relations = [{ kind, guid, version }]`，方便从源文档的同名表整段复制。
#[derive(Debug, Clone, Deserialize)]
pub struct NoteReleaseRelations {
    /// 覆盖表生效的版本区间。
    #[serde(default)]
    pub version: String,
    /// 旧写法，仍接受。
    #[serde(default)]
    pub min_version: String,
    #[serde(default)]
    pub max_version: String,
    #[serde(default)]
    pub requires: BTreeMap<String, RelationSpec>,
    #[serde(default)]
    pub compatible: BTreeMap<String, RelationSpec>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, RelationSpec>,
    #[serde(default)]
    pub relations: Vec<RepositoryRelation>,
}

impl MapNoteMetadata {
    /// 三张关系表展开成统一的 `RepositoryRelation` 列表。
    ///
    /// 单条区间写错只丢这一条并记 warn：一份 note 写错一个依赖不应该让整张
    /// 地图从列表里消失 —— 那正是「元数据跟着 Release 走」要避免的脆弱性。
    pub fn relations(&self) -> Vec<RepositoryRelation> {
        let mut relations = Vec::new();
        push_relations(&mut relations, RelationKind::Requires, &self.requires);
        push_relations(&mut relations, RelationKind::Compatible, &self.compatible);
        push_relations(&mut relations, RelationKind::Conflicts, &self.conflicts);
        relations
    }

    /// 块里的按版本覆盖表，转成与源文档同名表一致的模型。
    ///
    /// 没有写区间（或不写 `*`）的覆盖表会被丢掉并记 warn：它会匹配任何版本，
    /// 效果等于直接改上面三张表，作者却以为自己限定了范围。
    pub fn release_relation_overrides(&self) -> Vec<ReleaseRelationOverride> {
        let mut overrides = Vec::new();
        for entry in &self.release_relations {
            let raw = if entry.version.trim().is_empty() {
                crate::version_range::fold_bounds(
                    entry.min_version.trim(),
                    entry.max_version.trim(),
                )
            } else {
                entry.version.trim().to_string()
            };
            let range = match crate::version_range::VersionRange::parse(&raw) {
                Ok(range) => range,
                Err(error) => {
                    log::warn!(
                        "Release note 里的覆盖表区间无法解析，已忽略：区间={raw}，原因={error}"
                    );
                    continue;
                }
            };
            if range.is_any() {
                log::warn!("Release note 里的覆盖表没有写版本区间，已忽略");
                continue;
            }
            let mut relations = Vec::new();
            push_relations(&mut relations, RelationKind::Requires, &entry.requires);
            push_relations(&mut relations, RelationKind::Compatible, &entry.compatible);
            push_relations(&mut relations, RelationKind::Conflicts, &entry.conflicts);
            // 「可选的冲突」没有含义，只有 requires 保留 optional
            relations.extend(entry.relations.iter().map(|relation| RepositoryRelation {
                optional: relation.optional && matches!(relation.kind, RelationKind::Requires),
                ..relation.clone()
            }));
            if relations.is_empty() {
                log::warn!("Release note 里的覆盖表没有任何关系，已忽略");
                continue;
            }
            overrides.push(ReleaseRelationOverride {
                version: range.canonical(),
                relations,
            });
        }
        overrides
    }
}

fn push_relations(
    relations: &mut Vec<RepositoryRelation>,
    kind: RelationKind,
    table: &BTreeMap<String, RelationSpec>,
) {
    for (guid, spec) in table {
        let guid = guid.trim();
        if guid.is_empty() {
            continue;
        }
        let Some((version, prerelease, optional)) = spec.spec() else {
            log::warn!(
                "Release note 里的关系区间无法解析，跳过这一条：关系={}，目标={}",
                kind.as_str(),
                guid
            );
            continue;
        };
        relations.push(RepositoryRelation {
            kind,
            guid: guid.to_string(),
            version,
            // 「可选的冲突」没有含义，只有 requires 保留 optional
            optional: optional && matches!(kind, RelationKind::Requires),
            prerelease,
        });
    }
}

/// 一份 note 解析出来的东西。
///
/// 元数据块是给管理器的机器可读部分，其中 `note_i18n` 是卡片上的简短说明；
/// `body` 是块外的正文（已剥掉声明块）：它主要是「这个版本的更新说明」，
/// 由 `synthesize_releases` 交给详情面板显示，作者没写 `note_i18n` 时
/// 地图卡片也拿它当说明。
#[derive(Debug, Clone)]
pub struct NoteBlock {
    pub metadata: MapNoteMetadata,
    /// 去掉元数据块之后的正文（首尾空白已裁掉）。
    pub body: String,
}

/// 从 Release note 正文里解析元数据块。
///
/// - `Ok(None)`：没有块 —— 这个 Release 不是「一张地图」，调用方跳过即可；
/// - `Err(reason)`：有块但读不懂（合法 TOML 但不缺必填字段等），调用方记
///   warn 后跳过这一条；
/// - `Ok(Some(_))`：解析成功。
pub fn parse_release_note(body: &str) -> Result<Option<NoteBlock>, String> {
    let Some((start, end)) = block_span(body) else {
        return Ok(None);
    };
    let toml_text = &body[start + NOTE_BLOCK_OPEN.len()..end - NOTE_BLOCK_CLOSE.len()];
    let metadata: MapNoteMetadata =
        toml::from_str(toml_text).map_err(|error| format!("oc2-map 块不是合法 TOML：{error}"))?;
    if metadata.level_set_name.trim().is_empty() {
        return Err("oc2-map 块缺少 level_set_name".to_string());
    }
    Ok(Some(NoteBlock {
        metadata,
        body: strip_note_block(body),
    }))
}

/// 去掉元数据块、只留作者写的正文；没有块时原样返回（去首尾空白）。
///
/// 调用点是合成版本列表：`ReleaseInfo.body` 会原样进详情面板的「提交日志」，
/// 那串 HTML 注释只是给管理器的机器可读块，不该跟着作者的更新说明一起露出。
pub fn strip_note_block(body: &str) -> String {
    match block_span(body) {
        // 只删标记连同它包住的内容，块外的正文按原顺序接起来
        Some((start, end)) => format!("{}{}", &body[..start], &body[end..])
            .trim()
            .to_string(),
        None => body.trim().to_string(),
    }
}

/// 块的字符区间：`[起始标记的第一个字符, 结束标记之后的第一个字符)`。
///
/// 没有成对的起止标记时返回 `None` —— 那说明作者只是写了句 HTML 注释，或者
/// 块写了一半；两种都不该被当成元数据，也不该被从正文里删掉。
fn block_span(body: &str) -> Option<(usize, usize)> {
    let open = body.find(NOTE_BLOCK_OPEN)?;
    let after_open = open + NOTE_BLOCK_OPEN.len();
    let close = body[after_open..].find(NOTE_BLOCK_CLOSE)?;
    Some((open, after_open + close + NOTE_BLOCK_CLOSE.len()))
}

/// 由 note 声明与实际资产推导「从资产名里抽版本号」的规则。
///
/// 三条路径，按可靠性排序：
///
/// 1. 作者写了带 `${version}` 的模板 —— 模板就是唯一权威，其余部分按字面
///    转义，整串锚定；
/// 2. 作者没写模板 —— 从 Release 的第一个 `.zip` 资产名里找出版本号那一段，
///    用它两侧的字面内容拼出同样的正则（于是「这张图的包叫 daxue_v0.3.2.zip」
///    这种最常见的情况不需要作者额外声明）；
/// 3. 两者都推不出版本号 —— 返回 `None`，调用方跳过这条 Release 并记 warn。
///
/// 之所以必须有版本号：地图的「版本」既是界面上的数字，也是安装时挑哪个
/// 资产的标识（见 `map_sources::synthesize_releases`）。
pub fn versioned_asset_rule(template: Option<&str>, assets: &[ReleaseAsset]) -> Option<AssetRule> {
    if let Some(template) = template.map(str::trim).filter(|value| !value.is_empty()) {
        if template.contains(VERSION_PLACEHOLDER) {
            return Some(AssetRule {
                rule_type: AssetRuleType::Regex,
                value: template_rule(template),
            });
        }
        log::warn!(
            "oc2-map 的 asset 模板不含 {}，无法提取版本号：{}",
            VERSION_PLACEHOLDER,
            template
        );
        return None;
    }
    assets
        .iter()
        .filter(|asset| asset.name.to_ascii_lowercase().ends_with(".zip"))
        .find_map(|asset| rule_from_asset_name(asset.name.trim()))
}

/// `daxue_v${version}.zip` → `^daxue_v(?P<version>[0-9]+(?:\.[0-9]+)*)\.zip$`。
fn template_rule(template: &str) -> String {
    let mut pattern = String::from("^");
    let mut rest = template;
    while let Some(index) = rest.find(VERSION_PLACEHOLDER) {
        pattern.push_str(&regex::escape(&rest[..index]));
        pattern.push_str("(?P<version>");
        pattern.push_str(VERSION_PATTERN);
        pattern.push(')');
        rest = &rest[index + VERSION_PLACEHOLDER.len()..];
    }
    pattern.push_str(&regex::escape(rest));
    pattern.push('$');
    pattern
}

/// 从资产名里认出第一段版本号，拼出只匹配这一个文件的规则。
///
/// 名字里只有一段数字序列时这是「模板」最粗的还原；两段数字（`daxue_v0.3.2`）
/// 靠 `VERSION_PATTERN` 的贪婪点号段吃下整段。
fn rule_from_asset_name(name: &str) -> Option<AssetRule> {
    let pattern = regex::Regex::new(VERSION_PATTERN).expect("版本号正则固定且可编译");
    let found = pattern.find(name)?;
    Some(AssetRule {
        rule_type: AssetRuleType::Regex,
        value: format!(
            "^{}(?P<version>{}){}$",
            regex::escape(&name[..found.start()]),
            VERSION_PATTERN,
            regex::escape(&name[found.end()..])
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(block: &str, note: &str) -> String {
        format!("{note}\n\n<!-- oc2-map\n{block}\n-->\n")
    }

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.to_string(),
            size: 1024,
            download_count: None,
            download_url: format!("https://github.com/u/r/releases/download/t/{name}"),
            digest: None,
        }
    }

    #[test]
    fn a_release_without_the_block_is_not_a_map() {
        assert!(parse_release_note("只是普通的更新说明").unwrap().is_none());
    }

    #[test]
    fn the_legacy_latest_version_field_still_parses() {
        // 改名不能作废已发布的 Release note：旧名必须继续认得，否则那些地图
        // 的「这就是最新版」直接变成没写，退回按 tag 比较。
        let text = body(
            r#"level_set_name = "GAODA"
author = "GAODA"
latest_version = "0.42"
"#,
            "",
        );
        let note = parse_release_note(&text).unwrap().unwrap();
        assert_eq!(note.metadata.latest_mapset_version.as_deref(), Some("0.42"));
    }

    #[test]
    fn latest_info_version_is_trimmed_and_empty_dropped() {
        let mut metadata = MapNoteMetadata::default();
        assert_eq!(metadata.normalized_info_version(), None);
        // 只有空白等于没写：空串版本号进到前端会变成「一直在更新」
        metadata.latest_info_version = Some("   ".to_string());
        assert_eq!(metadata.normalized_info_version(), None);
        metadata.latest_info_version = Some(" 0.69 ".to_string());
        assert_eq!(metadata.normalized_info_version().as_deref(), Some("0.69"));
    }

    #[test]
    fn the_card_note_comes_from_the_block_not_from_the_body() {
        let text = body(
            r#"level_set_name = "GAODA"
author = "GAODA"
note_i18n.zh-CN = "共 2 张关卡。"
note_i18n.en = "Two levels."
"#,
            "## 玩法\n\n需要 DIYLevel。",
        );
        let block = parse_release_note(&text).unwrap().unwrap();
        assert_eq!(block.metadata.level_set_name, "GAODA");
        // 卡片短语来自块里的 note_i18n（五语），块外正文不属于卡片
        assert_eq!(
            block.metadata.note_i18n.get("zh-CN").map(String::as_str),
            Some("共 2 张关卡。")
        );
        assert_eq!(
            block.metadata.note_i18n.get("en").map(String::as_str),
            Some("Two levels.")
        );
        // 块外正文是「这个版本的更新说明」，清洗后交给版本详情显示
        assert_eq!(strip_note_block(&text), "## 玩法\n\n需要 DIYLevel。");
        // 解析结果也带一份同样的正文，作者没写 note_i18n 时卡片拿它当说明
        assert_eq!(block.body, "## 玩法\n\n需要 DIYLevel。");
    }

    #[test]
    fn the_metadata_block_never_reaches_the_visible_note() {
        let text = body("level_set_name = \"A\"\n", "共 2 张关卡。");
        let note = strip_note_block(&text);
        assert_eq!(note, "共 2 张关卡。");
        assert!(!note.contains("oc2-map"), "{note}");
        // 没有块时只去首尾空白，正文不动
        assert_eq!(strip_note_block("  普通说明  "), "普通说明");
    }

    #[test]
    fn dotted_i18n_keys_are_read_as_tables() {
        let block = parse_release_note(&body(
            r#"level_set_name = "nono"
name_i18n.zh-CN = "小no"
name_i18n.en = "nono"
"#,
            "",
        ))
        .unwrap()
        .unwrap();
        assert_eq!(
            block.metadata.name_i18n.get("zh-CN").map(String::as_str),
            Some("小no")
        );
        assert_eq!(
            block.metadata.name_i18n.get("en").map(String::as_str),
            Some("nono")
        );
    }

    #[test]
    fn a_block_without_a_level_set_name_is_rejected() {
        let error = parse_release_note(&body("author = \"A\"\n", "")).unwrap_err();
        assert!(error.contains("level_set_name"), "{error}");
    }

    #[test]
    fn the_three_relation_tables_become_relations() {
        let block = parse_release_note(&body(
            r#"level_set_name = "A"
requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }
compatible = { "dev.gua.overcooked.diylevel" = ">=0.10.0,<=0.11.9" }
conflicts = { "oc2.oc2diylevelruntimewloader" = { version = ">=4.0.0", prerelease = true } }
"#,
            "",
        ))
        .unwrap()
        .unwrap();
        let relations = block.metadata.relations();
        assert_eq!(relations.len(), 3);
        let requires = relations
            .iter()
            .find(|relation| relation.kind == RelationKind::Requires)
            .unwrap();
        assert_eq!(requires.version, ">=0.10.0");
        let compatible = relations
            .iter()
            .find(|relation| relation.kind == RelationKind::Compatible)
            .unwrap();
        assert_eq!(compatible.version, ">=0.10.0,<=0.11.9");
        let conflicts = relations
            .iter()
            .find(|relation| relation.kind == RelationKind::Conflicts)
            .unwrap();
        assert_eq!(conflicts.version, ">=4.0.0");
        assert_eq!(conflicts.prerelease, Some(true));
    }

    #[derive(serde::Deserialize)]
    struct Holder {
        value: RelationSpec,
    }

    /// 关系值规范化到 `version` 文本：两种写法落到同一段话上。
    fn normalized(toml_value: &str) -> Option<String> {
        let holder: Holder = toml::from_str(&format!("value = {toml_value}")).ok()?;
        holder.value.spec().map(|(version, _, _)| version)
    }

    #[test]
    fn relation_specs_are_normalized_to_one_version_string() {
        assert_eq!(normalized("\"*\""), Some(String::new()));
        assert_eq!(normalized("\"\""), Some(String::new()));
        assert_eq!(normalized("\">=1.0\""), Some(">=1.0".into()));
        assert_eq!(normalized("\">=1.0,<2.0\""), Some(">=1.0,<2.0".into()));
        // 单个 `=` 统一成 `==`，开区间保留（版本比较是离散的，边界明确）
        assert_eq!(normalized("\"=1.2\""), Some("==1.2".into()));
        assert_eq!(normalized("\">1.0\""), Some(">1.0".into()));
        // 显式表同样接受，旧 min/max 折成区间
        assert_eq!(
            normalized("{ min_version = \"1.0\", max_version = \"2.0\" }"),
            Some(">=1.0,<=2.0".into())
        );
    }

    #[test]
    fn malformed_relation_specs_are_rejected() {
        // 缺比较符、`~=`、空条件都不是合法区间，绝不能静默当成「不限」
        assert_eq!(normalized("\"1.0\""), None);
        assert_eq!(normalized("\"~=1.0\""), None);
        assert_eq!(normalized("\">=1.0,\""), None);
    }

    #[test]
    fn a_bad_relation_is_skipped_instead_of_dropping_the_map() {
        let block = parse_release_note(&body(
            r#"level_set_name = "A"
requires = { "com.example.badloader" = "1.0", "com.example.loader" = ">=2.0" }
"#,
            "",
        ))
        .unwrap()
        .unwrap();
        let relations = block.metadata.relations();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].guid, "com.example.loader");
    }

    /// 块里的 `[[release_relations]]` 按版本覆盖依赖，内层是块自己的三张表写法。
    #[test]
    fn release_relations_override_the_relations_for_that_version() {
        let block = parse_release_note(&body(
            r#"level_set_name = "A"
requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }

[[release_relations]]
version = ">=2.0.0"
requires = { "dev.gua.overcooked.diylevel" = ">=0.12.0" }
compatible = { "com.example.sorting" = "*" }

[[release_relations]]
version = ">=2.1.0,<3.0.0"
relations = [
  { kind = "requires", guid = "dev.gua.overcooked.diylevel", version = ">=0.13.0", optional = true },
]

[[release_relations]]
version = "*"
requires = { "dev.gua.overcooked.diylevel" = ">=9.9.9" }

[[release_relations]]
requires = { "dev.gua.overcooked.diylevel" = ">=9.9.9" }
"#,
            "",
        ))
        .unwrap()
        .unwrap();
        let overrides = block.metadata.release_relation_overrides();
        // 不限版本与没写区间的那两张被丢掉，只留真正限定了版本的两张
        assert_eq!(overrides.len(), 2, "{overrides:?}");
        assert_eq!(overrides[0].version, ">=2.0.0");
        assert_eq!(overrides[0].relations.len(), 2);
        assert!(
            overrides[0]
                .relations
                .iter()
                .any(|relation| relation.kind == RelationKind::Compatible)
        );
        assert_eq!(overrides[1].version, ">=2.1.0,<3.0.0");
        assert_eq!(overrides[1].relations.len(), 1);
        assert_eq!(overrides[1].relations[0].version, ">=0.13.0");
        // 覆盖表里的 optional 只对依赖有意义
        assert!(overrides[1].relations[0].optional);
    }

    #[test]
    fn the_asset_template_becomes_an_anchored_regex() {
        let rule = versioned_asset_rule(Some("daxue_v${version}.zip"), &[]).unwrap();
        assert_eq!(rule.rule_type, AssetRuleType::Regex);
        assert_eq!(
            rule.value,
            r"^daxue_v(?P<version>[0-9]+(?:\.[0-9]+)*)\.zip$"
        );
        let regex = regex::Regex::new(&rule.value).unwrap();
        let captures = regex.captures("daxue_v0.3.2.zip").unwrap();
        assert_eq!(
            captures.name("version").map(|found| found.as_str()),
            Some("0.3.2")
        );
    }

    #[test]
    fn a_template_without_a_version_placeholder_is_reported() {
        assert!(versioned_asset_rule(Some("daxue.zip"), &[]).is_none());
    }

    #[test]
    fn without_a_template_the_first_zip_asset_gives_the_rule() {
        let rule = versioned_asset_rule(None, &[asset("blni_v2.1.zip")]).unwrap();
        let regex = regex::Regex::new(&rule.value).unwrap();
        let captures = regex.captures("blni_v2.1.zip").expect("同一命名形状应命中");
        assert_eq!(
            captures.name("version").map(|found| found.as_str()),
            Some("2.1")
        );
        // 派生规则的版本段是通配的：同一个仓库里换版本号仍然命中（那正是要的）
        assert!(regex.captures("blni_v2.2.zip").is_some());
        // 但别的地图包不该命中
        assert!(regex.captures("daxue_v0.3.2.zip").is_none());
    }

    #[test]
    fn a_release_without_any_versioned_zip_is_skipped() {
        assert!(versioned_asset_rule(None, &[asset("levels.zip")]).is_none());
        assert!(versioned_asset_rule(None, &[asset("readme.md")]).is_none());
    }
}
