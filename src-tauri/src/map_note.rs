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
//! level_set_name    = "GAODA"
//! level_set_name_zh = "胡闹达学"
//! author            = "GAODA"
//! name_i18n.zh-CN   = "胡闹达学"
//! name_i18n.en      = "GAODA"
//! asset             = "daxue_v${version}.zip"
//! latest_version    = "0.3.2"
//! requires          = { "dev.gua.overcooked.diylevel" = ">=0.10.0" }
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
use crate::models::{AssetRule, AssetRuleType, RelationKind, ReleaseAsset, RepositoryRelation};
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
    /// 作者声明的「这就是最新版」。
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub requires: BTreeMap<String, RelationSpec>,
    #[serde(default)]
    pub compatible: BTreeMap<String, RelationSpec>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, RelationSpec>,
}

/// 关系值的两种写法：区间字符串，或显式表。
///
/// ```toml
/// requires = { "dev.gua.overcooked.diylevel" = ">=0.10.0,<=0.11.9" }
/// conflicts = { "oc2.oc2diylevelruntimewloader" = { min_version = "4.0.0", prerelease = true } }
/// ```
///
/// 与 `oc2-info` 的 `--requires` / `--compatible` / `--conflicts` 参数同构。
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum RelationSpec {
    Range(String),
    Detailed {
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
    /// 展开成 (下限, 上限, 预发布三态, 可选)。区间写错返回 `None`。
    fn bounds(&self) -> Option<(String, String, Option<bool>, bool)> {
        match self {
            Self::Range(raw) => {
                let (min_version, max_version) = parse_range(raw)?;
                Some((min_version, max_version, None, false))
            }
            Self::Detailed {
                min_version,
                max_version,
                prerelease,
                optional,
            } => Some((
                min_version.trim().to_string(),
                max_version.trim().to_string(),
                *prerelease,
                *optional,
            )),
        }
    }
}

/// 解析区间字符串：`*`、`>=1.0`、`<=2.0`、`=1.2`，以及它们的逗号组合。
///
/// 只接受闭区间写法。`>1.0` 这种开区间在项目里没有语义（版本比较是离散的
/// 数字段），接受它只会让作者以为自己表达了「大于」，实际边界却含糊。
///
/// 返回 `(下限, 上限)`，空串表示该端不限。
pub fn parse_range(raw: &str) -> Option<(String, String)> {
    let raw = raw.trim();
    if raw.is_empty() || raw == "*" {
        return Some((String::new(), String::new()));
    }
    let mut min_version = String::new();
    let mut max_version = String::new();
    for part in raw.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return None;
        }
        let (target, value) = if let Some(value) = part.strip_prefix(">=") {
            (Bound::Min, value)
        } else if let Some(value) = part.strip_prefix("<=") {
            (Bound::Max, value)
        } else if let Some(value) = part.strip_prefix('=') {
            (Bound::Exact, value)
        } else {
            return None;
        };
        let value = value.trim();
        if value.is_empty() {
            return None;
        }
        match target {
            Bound::Min => {
                if !min_version.is_empty() {
                    return None;
                }
                min_version = value.to_string();
            }
            Bound::Max => {
                if !max_version.is_empty() {
                    return None;
                }
                max_version = value.to_string();
            }
            Bound::Exact => {
                if !min_version.is_empty() || !max_version.is_empty() {
                    return None;
                }
                min_version = value.to_string();
                max_version = value.to_string();
            }
        }
    }
    Some((min_version, max_version))
}

enum Bound {
    Min,
    Max,
    Exact,
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
        let Some((min_version, max_version, prerelease, optional)) = spec.bounds() else {
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
            min_version,
            max_version,
            // 「可选的冲突」没有含义，只有 requires 保留 optional
            optional: optional && matches!(kind, RelationKind::Requires),
            prerelease,
        });
    }
}

/// 一份 note 解析出来的东西。
///
/// 元数据块是给管理器的机器可读部分，其中 `note_i18n` 是卡片上的简短说明；
/// 块外的正文不进这里的任何字段 —— 它属于「这个版本的更新说明」，由
/// `strip_note_block` 在合成版本列表时清洗后交给详情面板显示。
#[derive(Debug, Clone)]
pub struct NoteBlock {
    pub metadata: MapNoteMetadata,
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
    Ok(Some(NoteBlock { metadata }))
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
conflicts = { "oc2.oc2diylevelruntimewloader" = { min_version = "4.0.0", prerelease = true } }
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
        assert_eq!(requires.min_version, "0.10.0");
        assert!(requires.max_version.is_empty());
        let compatible = relations
            .iter()
            .find(|relation| relation.kind == RelationKind::Compatible)
            .unwrap();
        assert_eq!(compatible.min_version, "0.10.0");
        assert_eq!(compatible.max_version, "0.11.9");
        let conflicts = relations
            .iter()
            .find(|relation| relation.kind == RelationKind::Conflicts)
            .unwrap();
        assert_eq!(conflicts.min_version, "4.0.0");
        assert_eq!(conflicts.prerelease, Some(true));
    }

    #[test]
    fn a_star_range_means_no_bounds() {
        assert_eq!(parse_range("*"), Some((String::new(), String::new())));
        assert_eq!(
            parse_range(">=1.0"),
            Some(("1.0".to_string(), String::new()))
        );
        assert_eq!(
            parse_range("=1.2"),
            Some(("1.2".to_string(), "1.2".to_string()))
        );
    }

    #[test]
    fn open_ranges_are_rejected() {
        // 项目里没有开区间语义，含糊地接受只会让作者以为边界是想要的
        assert_eq!(parse_range(">1.0"), None);
        assert_eq!(parse_range("<2.0"), None);
        assert_eq!(parse_range("1.0"), None);
        assert_eq!(parse_range(">=1.0,>=2.0"), None);
    }

    #[test]
    fn a_bad_relation_is_skipped_instead_of_dropping_the_map() {
        let block = parse_release_note(&body(
            r#"level_set_name = "A"
requires = { "com.example.badloader" = ">1.0", "com.example.loader" = ">=2.0" }
"#,
            "",
        ))
        .unwrap()
        .unwrap();
        let relations = block.metadata.relations();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].guid, "com.example.loader");
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
