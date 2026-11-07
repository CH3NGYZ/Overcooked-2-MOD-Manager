use super::relations::NoteReleaseRelations;
use super::relations::RelationSpec;
use crate::models::AssetVersionEntry;
use serde::Deserialize;
use std::collections::BTreeMap;

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
    /// 版本表（`asset_versions`）：数组顺序就是新旧顺序，末项最新。
    ///
    /// 版本号从文件名抽时排不出 `3.4.1-fix`，数字段比较也排不出「0.7 比 0.69
    /// 新」。作者排一次序，读侧就按这个顺序当成版本列表，不再猜。
    #[serde(default)]
    pub asset_versions: Vec<AssetVersionEntry>,
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
    /// 排序表非空时，表尾就是最新版（按**地图包**版本）。
    ///
    /// 表尾是作者写下的权威结论：末项为空就是「最新版未知」，不向更早的版本
    /// 或块里的旧标量借值（借来的会把旧包当成最新版）。只有完全没有版本表的老块
    /// 才读 `latest_mapset_version`。
    pub fn derived_mapset_version(&self) -> Option<String> {
        match self.asset_versions.last() {
            Some(entry) => normalize_version(Some(&entry.mapset_version)),
            None => normalize_version(self.latest_mapset_version.as_deref()),
        }
    }

    /// 排序表非空时，表尾的 `info` 版本就是最新版的 info 版本。
    pub fn derived_info_version(&self) -> Option<String> {
        match self.asset_versions.last() {
            Some(entry) => normalize_version(Some(&entry.info_version)),
            None => normalize_version(self.latest_info_version.as_deref()),
        }
    }

    /// 归一化后的最新 info 版本（手写块的首尾空白不可控）。
    pub fn normalized_info_version(&self) -> Option<String> {
        self.derived_info_version()
    }
}

/// 去掉首尾空白；空串等于没写（空版本号进到前端会变成「一直在更新」）。
fn normalize_version(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests;
