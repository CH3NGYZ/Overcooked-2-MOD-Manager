use super::metadata::MapNoteMetadata;
use crate::models::RelationKind;
use crate::models::ReleaseRelationOverride;
use crate::models::RepositoryRelation;
use serde::Deserialize;
use std::collections::BTreeMap;

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
    /// 上传页展示用的区间文本。
    ///
    /// 能规范化就规范化（`=1.2` → `==1.2`），写错的区间原样返回 —— 上传页
    /// 不该因为作者上一版手抖写坏了一个区间，就把这张地图的元数据整份丢掉。
    pub(crate) fn display_range(&self) -> String {
        if let Some((range, _, _)) = self.spec() {
            return range;
        }
        match self {
            Self::Range(raw) => raw.trim().to_string(),
            Self::Detailed {
                version,
                min_version,
                max_version,
                ..
            } => {
                let version = version.trim();
                if version.is_empty() {
                    crate::version_range::fold_bounds(min_version.trim(), max_version.trim())
                } else {
                    version.to_string()
                }
            }
        }
    }

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

#[cfg(test)]
mod tests;
