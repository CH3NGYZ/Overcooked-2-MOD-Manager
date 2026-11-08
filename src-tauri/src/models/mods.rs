use super::ModRepositoryBinding;
use super::relations::RepositoryRelation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一个 DLL 在安装预览里的角色：真正的 BepInEx 插件，还是纯依赖。
///
/// 这个枚举是角色判定的唯一权威：MOD 预览与地图包里的随包插件都按同一份
/// 规则（有没有 BepInPlugin 的 GUID/版本）得出角色，序列化值固定为
/// `mod` / `dependency`，前端 `ModPreviewRole` 依赖这两个字面量。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DllRole {
    Mod,
    Dependency,
}

/// 一个随地图发布的 DLL 的元数据（ZIP 导入预览与本地扫描共用）。
///
/// 两个场景读到的是同一份 [`crate::mods::read_dll_details`] 结果，所以字段
/// 定义只留这一处：ZIP 里的随包插件与磁盘上的随包插件各自 `#[serde(flatten)]`
/// 它，前端拿到的仍是平铺 JSON，字段名与 MOD 预览完全一致。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DllPluginInfo {
    pub role: DllRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub guid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub product_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_filename: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledModStatusLabel {
    UpdateAvailable,
    Latest,
    NotReleaseAsset,
    Unbound,
    BoundRepositoryMissing,
    RepositoryUnavailable,
    Unknown,
    Duplicate,
    DuplicateLowerVersion,
    LoadWarning,
    Disabled,
    /// 包内的纯依赖 DLL：读不出 BepInPlugin 元数据（如 LevelEditorStub）。
    Dependency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub repository_id: Option<String>,
    #[serde(default)]
    pub repository_ids: Vec<String>,
    /// 该 MOD 的安装清单是否由手动安装（本地文件 / 在线直链）创建。
    ///
    /// 手动安装的清单 id 是 `manual-<uuid>`，没有仓库也没有 Release 绑定。
    /// 扫描时如果它能按 GUID/产品名 匹配到在线仓库，仍会绑定该仓库并归入
    /// 仓库分组；匹配不到时才用这个标记归到「本地安装」分类。
    #[serde(default)]
    pub manual_install: bool,
    /// 该 DLL 读不出 BepInPlugin 元数据，只是随包安装的纯依赖（如 LevelEditorStub）。
    ///
    /// 这类文件仍然会出现在扫描结果里（它确实占用了 plugins 目录，
    /// 删除/开关也需要能操作），但界面上标为「依赖」而不是当作一个 MOD。
    #[serde(default)]
    pub is_dependency: bool,
    #[serde(default)]
    pub repository_binding: Option<ModRepositoryBinding>,
    #[serde(default)]
    pub package_id: Option<String>,
    #[serde(default)]
    pub package_name: Option<String>,
    /// 包名的多语言快照（与安装清单里的同名，供前端按语言现选）。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub package_name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub package_mod_count: usize,
    /// 安装时仓库的分组快照（多语言）。
    ///
    /// 分组语义是「安装时绑定的仓库分类」，所以在安装那一刻固化下来最稳定：
    /// 刚启动或仓库源还没刷新完时，仓库配置可能为空/仍旧，前端据此分组会乱。
    /// 扫描时用这份快照；仓库分组真的变了才在下一次刷新写到清单里。
    #[serde(default)]
    pub group_i18n: std::collections::BTreeMap<String, String>,
    /// 安装时的单语分组名（快照缺失时兜底）。
    #[serde(default)]
    pub group: String,
    /// 安装的 Release 是否为预发布版本（来自安装清单快照）。
    #[serde(default)]
    pub prerelease: bool,
    /// 依赖快照（安装时写入安装清单，刷新仓库后由 `sync_dependency_snapshots` 同步为线上声明）。
    ///
    /// 前端优先用在线仓库的 `relations`；仓库不在当前配置里（源下线/自定义源被删）
    /// 时退回这份快照，否则 MOD 管理页的依赖表会凭空消失。
    #[serde(default)]
    pub relations: Vec<RepositoryRelation>,
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    pub product_name: Option<String>,
    pub original_filename: Option<String>,
    pub guid: Option<String>,
    #[serde(default)]
    pub has_config: bool,
    /// `BepInEx/logs/<guid>` 是否存在：前端据此决定是否显示「查看日志」按钮。
    #[serde(default)]
    pub has_logs: bool,
    pub plugin_name: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    pub dll_file: String,
    pub enabled: bool,
    pub version: Option<String>,
    pub version_source: String,
    pub update_available: bool,
    #[serde(default)]
    pub labels: Vec<InstalledModStatusLabel>,
    pub duplicate_guid: bool,
    #[serde(default)]
    pub duplicate_version: bool,
    pub load_warning: Option<String>,
    pub note: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModNote {
    pub key: String,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModRecycleEntry {
    pub id: String,
    pub name: String,
    /// 删除时仓库名的多语言快照，前端按当前语言现选。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
    pub deleted_at: String,
    pub file_count: usize,
    pub files: Vec<String>,
    pub game_path: String,
    pub package_id: Option<String>,
    pub release_tag: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallLocalModResult {
    pub ok: bool,
    pub message: String,
    pub requires_confirmation: bool,
    #[serde(default)]
    pub requires_package_confirmation: bool,
    #[serde(default)]
    pub package_id: Option<String>,
    #[serde(default)]
    pub package_mod_names: Vec<String>,
}

#[cfg(test)]
mod dll_role_tests;
