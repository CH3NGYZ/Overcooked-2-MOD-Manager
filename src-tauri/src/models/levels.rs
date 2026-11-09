use super::mods::DllPluginInfo;
use super::operations::InstallConflict;
use super::relations::RepositoryRelation;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 关卡集扫描进度（事件 `level-set-scan-progress` 的载荷）。
///
/// 首次进地图页要逐份解析 `info`（真实目录里 7 个地图合计 131 MB，最大一份
/// 65 MB），期间界面只有一个转圈条 —— 用户看不出是在跑还是卡死。把「已解析
/// 几份 / 共几份 / 正在读哪个目录」发出来，转圈条下面就有真实进度可看。
///
/// `done` 是**已完成**数而不是当前下标；`current` 是刚开始解析的那份（可能
/// 与 `done` 指向同一份，因为完成与开始几乎同时），前端当「正在读取」显示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetScanProgress {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

/// ZIP 识别 / 导入进度（事件 `level-set-zip-progress` 的载荷）。
///
/// 这里和 `LevelSetScanProgress` 的区别是「读的是哪个 ZIP」：扫描读的是
/// 游戏目录里已经落盘的关卡集，而这一份读的是本地拖入 / 直链下载 /
/// 在线 Release 下载来的 ZIP —— 一个上百 MB 的合集在 debug 构建里识别要
/// 几十秒，只转圈不给进度，用户无法判断是在跑还是卡死。
///
/// `phase` 区分同一条「暂存 → 确认 → 导入」链上的两个可感知阶段：
/// - `info`：解析 ZIP 里的 `info`（暂存阶段，识别出这个包里有哪些关卡集）；
/// - `install`：把选中的关卡集写进游戏目录（导入阶段，解压落盘）。
///
/// `done` 是**已完成**数而不是当前下标；`current` 是正在处理的那个条目
/// （`info` 阶段是 ZIP 内条目路径，`install` 阶段是关卡集目录名）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetZipProgress {
    pub phase: String,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

/// 一个 DIYLevel 关卡集（玩家口中的「地图」）在磁盘上的状态。
///
/// 与 `InstalledMod` 的关系：两者都是「游戏目录里的东西 + 元数据」，但扫描源
/// 与身份都不同 —— MOD 靠 BepInPlugin 的 GUID，关卡集靠 info 里的
/// `levelSetName + levelSetNameZH + author`（中文名仅双方都有时比较）。
/// 界面复用同一套卡片样式，数据不共用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetInfo {
    /// 目录名（唯一键，也是开关时移动的目录名）。
    pub dir_name: String,
    /// `true` = 在启用根 `levels/`；`false` = 在停用根。
    pub enabled: bool,
    /// 关卡集目录的展示用绝对路径；停用根为 `BepInEx/cache/modmanager_disabled_maps`。
    ///
    /// 详情面板要把「目录名 + 位置」合成一行可直接打开的路径，所以由扫描
    /// 结果原样带回，前端不再自己拼 `BepInEx/plugins/OC2DIYLevel/...` ——
    /// 拼两份迟早漂移。
    pub path: String,
    /// info 里的英文集名；解析失败时为空。
    #[serde(default)]
    pub name: String,
    /// info 里的中文集名；老资源或解析失败时为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_zh: Option<String>,
    /// info 里的作者；官方导出常为空。
    #[serde(default)]
    pub author: String,
    /// info 里的版本号（地图自己的版本，与 Release tag 比对）。
    #[serde(default)]
    pub version: String,
    /// 安装那一刻记下的资产版本（Release tag，如 `3.4.1-fix`）。
    ///
    /// info 版本判不出「装的是不是最新资产」：作者发 `-fix` 修复版时常常忘了
    /// 改 info，两个包的 info 版本都是 `3.4.1`。空串表示没有记录（本地 ZIP
    /// 导入、老数据），前端退回比 info 版本。
    #[serde(default)]
    pub installed_mapset_version: String,
    /// info 里的 uid（= MD5(baseUID + version)，随版本变化，不能当稳定身份）。
    #[serde(default)]
    pub uid: String,
    /// info 里的 baseUID（老导出可能为空）；保留为元数据，不作为安装身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_uid: Option<String>,
    /// 关卡数量（按 info 里 `levelInfos` 的声明统计，不是对象表）。
    #[serde(default)]
    pub level_count: usize,
    /// info 中每关的名称与参数，扫描时复用已有解析与缓存。
    #[serde(default)]
    pub levels: Vec<crate::asset_bundle::LevelMetadata>,
    /// 解析失败的原因（tag）；成功时为 `None`。
    ///
    /// **解析失败不影响开关**：界面照常显示目录名，用户仍可把它移出去。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 绑定的地图仓库 id（英文名、双方非空中文名及作者匹配；未绑定为 `None`）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_id: Option<String>,
    /// 线上声明的分组多语言表（地图的分组语义就是作者）。
    ///
    /// 来源是「绑定地图的当前声明 → 安装时固化的地图依赖快照」：地图没有
    /// MOD 那样的安装清单，源文档被删或一直刷新失败时分组就无处可查，而
    /// 管理页的折叠分组与详情里的作者中英两名都要靠它稳定。两处都查不到时
    /// 留空，前端回退 info 里的本地 `author`。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub map_group_i18n: BTreeMap<String, String>,
    /// 分组的单语镜像（旧源文档只写 `group` 时用它）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub map_group: Option<String>,
    /// 依赖快照（安装时/刷新地图源后写入地图依赖清单）。
    ///
    /// 与 MOD 侧同一套回退规则：绑定地图还在配置里时前端用线上声明，
    /// 地图源被删之后退回这份快照。
    #[serde(default)]
    pub relations: Vec<RepositoryRelation>,
    /// 随这张地图集一起装进集目录的插件（作者随包的修复 DLL）。
    ///
    /// 同时涵盖集目录内 DLL 与表层包登记 DLL；后者不与 info 同处目录，
    /// 扫描归属由登记表身份决定，不能只按物理位置推导。
    #[serde(default)]
    pub plugins: Vec<LevelSetPlugin>,
}

/// 关卡集 ZIP 里的一个随包插件（作者附带、跟着地图一起装的修复 DLL）。
///
/// `path` 是 ZIP 内条目路径（作者原始目录结构，仅供展示定位），`file_name`
/// 才是落盘后的文件名 —— 拍平形态导入时插件被放进关卡集目录根，只用文件名。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipLevelSetPlugin {
    pub path: String,
    pub file_name: String,
    /// 落盘位置（相对游戏目录、正斜杠归一化）。
    ///
    /// 只有 BepInEx 表层包会带：那种包按 ZIP 内路径原样解压，插件留在
    /// `BepInEx/plugins/**` 里，界面用它在安装详情里说明「将安装到 …」。
    /// 拍平形态没有这个值 —— 插件落进集目录根，位置由集目录决定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_path: Option<String>,
    #[serde(flatten)]
    pub info: DllPluginInfo,
}

/// 关卡集目录的落盘形态。
///
/// `flattened` 是历史形态：ZIP 里的每个集拍平成 `levels/<集名>/`，随包插件
/// 也搬进集目录根。`bepinex` 是「BepInEx 表层包」：作者已经把目录规划好，
/// 导入时按 ZIP 内路径原样解压，插件留在 `BepInEx/plugins/**`，靠
/// `manifests/level-set-plugins.json` 的登记表绑定到集身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LevelSetLayout {
    #[default]
    Flattened,
    Bepinex,
}

/// 「一键拍平」的一个候选目录（`dry_run` 只返回这些，不写盘）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetFlattenCandidate {
    /// `levels/` 下的直接子目录名。
    pub dir_name: String,
    /// 浅探测（≤3 层）找到的 `info` / `info_*` 文件数。
    pub nested_infos: usize,
}

/// 「一键拍平」的结果。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LevelSetFlattenReport {
    /// 探测到的候选目录；`dry_run=true` 时这就是全部结果。
    #[serde(default)]
    pub candidates: Vec<LevelSetFlattenCandidate>,
    /// 实际拍平出来的关卡集数。
    #[serde(default)]
    pub flattened: usize,
    /// 逐条失败记录（`"<目录>: <原因>"`），前端原样展示。
    #[serde(default)]
    pub failed: Vec<String>,
}

/// 本地扫描得到的随包插件（集内 DLL 或表层包登记 DLL）。
///
/// 与 [`ZipLevelSetPlugin`] 对应导入后的展示；集内 DLL 的 `enabled` 取文件后缀，
/// 当前扫描器对存在的登记 DLL 填 `true`。它不是地图自身的启停状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetPlugin {
    pub file_name: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub info: DllPluginInfo,
}

/// ZIP 里识别出的一个关卡集（导入前给用户确认用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZipLevelSet {
    /// 导入后使用的目录名（由集名推导，已清理非法字符）。
    pub dir_name: String,
    /// 落盘形态：`flattened`（历史）或 `bepinex`（表层包原样解压）。
    #[serde(default)]
    pub layout: LevelSetLayout,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_zh: Option<String>,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub level_count: usize,
    /// 该关卡集在 ZIP 里的文件数。
    #[serde(default)]
    pub file_count: usize,
    /// 本地已有的同身份副本（导入前要卸载的那些）。
    ///
    /// 同一批识别结果共用一份列表：地图冲突是「这次导入会与本地哪些副本
    /// 撞身份」，与批次里具体是哪一张无关，界面按 `handle` 去重展示。
    #[serde(default)]
    pub conflicts: Vec<InstallConflict>,
    /// 随这个关卡集一起装进集目录的插件（作者随包的修复 DLL）。
    ///
    /// 界面展示每个插件的名称/版本/作者等详情；插件按布局留在表层登记位置或
    /// 跟随集目录，具体归属由 ZIP 识别结果决定。
    #[serde(default)]
    pub plugins: Vec<ZipLevelSetPlugin>,
}

/// ZIP 导入结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetImportResult {
    pub installed: Vec<String>,
}

/// 在线暂存实际选中的版本及其主地图身份；资产版本不能广播给 ZIP 内其它地图。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInstallOrigin {
    pub level_set_name: String,
    pub level_set_name_zh: String,
    pub author: String,
    pub mapset_version: String,
}

/// 一个已解析完、可以导入的地图来源（本地 ZIP 或下载直链）。
///
/// `staged` 表示 `path` 是管理器缓存目录里的临时文件：导入完成或用户取消后
/// 要用 `discard_map_source` 清掉。本地 ZIP 只是被读取，`staged=false`，
/// 文件始终归用户所有。
#[derive(Debug, Clone, Serialize)]
pub struct StagedMapSource {
    pub path: String,
    pub staged: bool,
    pub sets: Vec<ZipLevelSet>,
    pub origin: Option<MapInstallOrigin>,
}
