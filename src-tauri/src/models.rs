use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameInfo {
    pub path: Option<String>,
    pub steam_library: Option<String>,
    #[serde(default)]
    pub steam_root: Option<String>,
    #[serde(default)]
    pub wine_prefix: Option<String>,
    pub game_exists: bool,
    pub bepinex: BepInExInfo,
    pub kitchen_running: bool,
    pub winhttp_state: WinhttpState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BepInExInfo {
    /// 框架核心程序集（`BepInEx/core/BepInEx.dll`）存在。
    ///
    /// 只判断 `BepInEx/` 目录会把「装过 MOD 但没装框架」误认成已安装：
    /// 装任何 MOD 都会创建 `BepInEx/plugins/`。
    pub installed: bool,
    /// 目录在但核心不在：装过 MOD 留下的空壳，安装时应保留其中的 plugins/config。
    #[serde(default)]
    pub has_residue: bool,
    #[serde(default)]
    pub has_config: bool,
    pub version: Option<String>,
    pub version_source: Option<String>,
    pub backups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WinhttpState {
    ModEnabled,
    ModDisabled,
    Missing,
    Conflict,
}

/// DIYLevel 自定义关卡框架的状态。
///
/// 它本质是一个 MOD（`dev.gua.overcooked.diylevel`），但承担「框架」角色：
/// 自制地图要靠它加载。所以设置页给它一个独立管理区块，而不是让用户去
/// MOD 列表里自己找。判定依据与 MOD 扫描一致 —— 用 GUID 识别，不看文件名
/// （作者会改名，GUID 才是稳定身份）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiyLevelInfo {
    /// 扫描到了这个 GUID 的插件。
    pub installed: bool,
    /// 已启用（`.dll` 而非 `.dll.disabled`）。
    pub enabled: bool,
    /// 插件版本（读 BepInPlugin 元数据）。
    pub version: Option<String>,
    /// DLL 的游戏相对路径，用于开关与打开目录。
    pub dll_file: Option<String>,
    /// 本地地图目录（`BepInEx/plugins/OC2DIYLevel/levels`）是否存在。
    #[serde(default)]
    pub has_levels_dir: bool,
    /// 地图集数量：只看 `levels/` 下的**文件夹**个数。
    ///
    /// 一个地图集就是 `levels/` 下的一个目录，目录里的散文件（readme、
    /// 缩略图等）不属于任何地图集，不计数。目录不可读时为 0。
    #[serde(default)]
    pub level_packs: u32,
    /// 已有的 DIYLevel 目录备份（绝对路径，按名称升序）。
    ///
    /// 只备份 `BepInEx/plugins/OC2DIYLevel` 这一个文件夹 —— 它是框架本体
    /// 和自制地图所在处，而 `BepInEx/core` 等属于加载器，归 BepInEx 备份管。
    #[serde(default)]
    pub backups: Vec<String>,
}

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
/// `levelSetName + author`。界面复用同一套卡片样式，数据不共用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetInfo {
    /// 目录名（唯一键，也是开关时移动的目录名）。
    pub dir_name: String,
    /// `true` = 在 `levels/`；`false` = 在 `disabled/`。
    pub enabled: bool,
    /// 关卡集目录的绝对路径（原生分隔符；停用集在 `disabled/` 下）。
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
    /// info 里的 baseUID（稳定身份；老导出可能为空）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_uid: Option<String>,
    /// 关卡数量（按 info 里 `levelInfos` 的声明统计，不是对象表）。
    #[serde(default)]
    pub level_count: usize,
    /// 解析失败的原因（tag）；成功时为 `None`。
    ///
    /// **解析失败不影响开关**：界面照常显示目录名，用户仍可把它移出去。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// 绑定的地图仓库 id（按 `level_set_name + author` 身份匹配；未绑定为 `None`）。
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
    /// 绑定关系就是物理位置：DLL 与 `info` 同处集目录，扫描时按文件名读回，
    /// 停用/启用整目录搬家时它们跟着一起走，导出地图包时整目录打包即可。
    #[serde(default)]
    pub plugins: Vec<LevelSetPlugin>,
}

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

/// 已经装在磁盘上的随包插件（扫描关卡集目录时读出）。
///
/// 与 [`ZipLevelSetPlugin`] 是同一个东西的两个阶段：导入后插件就躺在集目录
/// 根里，扫描时按文件名读回来。`enabled` 只看文件后缀（`.dll.disabled` 是
/// 手动禁用），与关卡集自身的开关（整目录搬家）不是一回事。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LevelSetPlugin {
    pub file_name: String,
    pub enabled: bool,
    #[serde(flatten)]
    pub info: DllPluginInfo,
}

/// 安装前发现的「本地已经装了同一个东西」。
///
/// 安装是「先卸旧、再装新」的整包语义：同一个 MOD（同 GUID）或同一张地图
/// （同 `levelSetName + levelSetNameZH + author`）不管版本相同、升级还是降级，
/// 都算同一个东西 —— 并存只会得到两份互相顶掉的文件。后端只负责报告，
/// 卸载动作由前端确认后逐个执行（MOD 走 `uninstall_local_mod` 的整包语义，
/// 地图走 `delete_level_set`），安装事务里不掺卸载。
///
/// 一个已装副本对应一条；已有重复副本时全部列出，`enabled=false` 说明它在
/// 停用根（地图）或已被手动禁用（MOD），关着也要卸 —— 只装新的会留下两份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstallConflict {
    /// 展示名：MOD 用包名（没有包名回落插件名），地图用关卡集英文名（空则中文名）。
    pub name: String,
    /// 本地已装版本原文；读不出时为空串。
    pub version: String,
    /// 卸载句柄：MOD 是 `InstalledMod.dll_file`，地图是关卡集目录名 `dir_name`。
    pub handle: String,
    /// 地图：这份副本是否在启用根（`levels/`）；MOD 如实反映自身的启用状态。
    pub enabled: bool,
    /// 本次要装的版本是否与本地相同（决定「重新安装」还是「版本变化」文案）。
    pub same_version: bool,
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
    /// 界面把每个插件当成一个 MOD 展示名称/版本/作者等详情，不用让用户自己猜
    /// `file_count` 里混进了什么。停用/启用整目录搬家时它们跟着一起走。
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
    pub repository_binding: Option<RepositoryBinding>,
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
pub struct ReleaseInfo {
    #[serde(default)]
    pub release_id: Option<u64>,
    pub tag_name: String,
    pub version: Option<String>,
    pub name: String,
    pub body: String,
    pub published_at: Option<String>,
    pub prerelease: bool,
    pub draft: bool,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub size: u64,
    #[serde(default)]
    pub download_count: Option<u64>,
    pub download_url: String,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitee,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
    #[default]
    All,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryDownloadMode {
    #[default]
    Release,
    RepositoryFile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryBinding {
    pub provider: Provider,
    pub owner: String,
    pub repository: String,
    pub tag: String,
    #[serde(default)]
    pub channel: Channel,
}

impl RepositoryBinding {
    pub fn from_release(repo: &RepositoryConfig, tag: &str) -> Option<Self> {
        if repo.download_mode != RepositoryDownloadMode::Release || tag.trim().is_empty() {
            return None;
        }
        let binding = Self {
            provider: repo.provider.clone(),
            owner: repo.username.clone(),
            repository: repo.repository.clone(),
            tag: tag.into(),
            channel: repo.channel,
        };
        binding.is_valid().then_some(binding)
    }

    pub fn is_valid(&self) -> bool {
        !self.tag.trim().is_empty()
            && !self.tag.chars().any(char::is_control)
            && crate::releases::repository_coordinates(&self.owner, &self.repository).is_ok()
    }

    pub fn matches_identity(&self, repo: &RepositoryConfig) -> bool {
        self.provider == repo.provider
            && self.owner.eq_ignore_ascii_case(&repo.username)
            && self.repository.eq_ignore_ascii_case(&repo.repository)
    }

    pub fn matches_repository(&self, repo: &RepositoryConfig) -> bool {
        self.matches_identity(repo)
            && repo.download_mode == RepositoryDownloadMode::Release
            && (repo.channel == self.channel || repo.channel == Channel::All)
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommunityDiscovery {
    pub repository_ids: Vec<String>,
    pub total_count: u64,
    pub incomplete_results: bool,
    pub truncated: bool,
}

/// 社区发现得到的「地图源仓库」坐标。
///
/// 与 `CommunityDiscovery` 分开存：MOD 仓库发现的结果要落成完整的
/// `RepositoryConfig` 进 `repositories`（有开关、缓存、安装绑定），
/// 地图侧的仓库只是一句「去这个仓库读 Release note」—— 地图条目要等
/// 地图源树加载时按 `release-tags` 规则现展开，不能提前固化成本地缓存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommunityMapRepository {
    pub provider: Provider,
    pub username: String,
    pub repository: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProxyMode {
    Direct,
    AutoDirect,
    AutoProxy,
    Fixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubProxy {
    pub id: String,
    pub name: String,
    pub template: String,
    #[serde(default)]
    pub regions: Vec<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    /// A managed proxy discovered in a userscript, rather than the TOML defaults.
    #[serde(default)]
    pub remote: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GitHubProxyConfig {
    #[serde(default)]
    pub release_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
    /// 头像专用代理。
    ///
    /// 单独一组而不是复用 raw：实测 raw 组全部取不到 `github.com/{owner}.png`
    /// （模板都按 `{owner}/{repo}/{branch}/{path}` 设计），能取到头像的是
    /// 「纯 {url} 包装」型。名单本身写在代理清单文件里（内置
    /// `repositories-proxies.toml` 或远程清单），代码不硬编码任何地址 ——
    /// 与另外四组走同一套「读取 → 合并 → 去重」流程。
    #[serde(default)]
    pub avatar_proxies: Vec<GitHubProxy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubProxyManifest {
    #[serde(default = "default_proxy_manifest_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub release_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
    /// 头像代理（`[[avatar_proxies]]`），与另外四组同一套清单格式。
    #[serde(default)]
    pub avatar_proxies: Vec<GitHubProxy>,
}

fn default_proxy_manifest_version() -> u32 {
    1
}

impl Default for ProxyMode {
    fn default() -> Self {
        Self::AutoDirect
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AssetRuleType {
    Exact,
    Glob,
    Regex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetRule {
    #[serde(rename = "type")]
    pub rule_type: AssetRuleType,
    pub value: String,
}

/// MOD 与另一个插件之间的关系种类。
///
/// 依赖、排斥、兼容三件事本质上是同一个问题 ——「对某个 GUID 的版本区间
/// 有什么要求」，只是满足与违反时的含义不同：
///
/// - `requires`   目标**必须存在**且版本落在区间内。缺失或版本不符 → 缺依赖。
/// - `conflicts`  目标**若存在**且版本落在区间内 → 冲突，需要卸载。
/// - `compatible` 目标**若存在**，版本必须落在区间内 → 否则不兼容，需要卸载。
///
/// 三者共用 `version` 版本区间（pip 风格 `">=1.8.0,<2.0.0"`）：不写或写 `*`
/// 就是不设限制。
/// 放在一张表里而不是三张表，是为了让作者按「跟谁有关系」组织配置，
/// 而不是按「哪种关系」拆成三处；前端也只需要一条判定管线。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    #[default]
    Requires,
    Conflicts,
    Compatible,
}

impl RelationKind {
    /// 从配置里的 `kind` 文本解析。未知值返回 `None`，由调用方决定跳过。
    pub fn from_token(token: &str) -> Option<Self> {
        match token.trim().to_ascii_lowercase().as_str() {
            "requires" | "require" | "dependency" => Some(Self::Requires),
            "conflicts" | "conflict" | "incompatible_with" => Some(Self::Conflicts),
            "compatible" | "compatibility" => Some(Self::Compatible),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Requires => "requires",
            Self::Conflicts => "conflicts",
            Self::Compatible => "compatible",
        }
    }
}

/// 仓库（MOD）与另一个插件的一条关系声明。
///
/// 用 GUID 表达，与本地 MOD 绑定用的 `plugin_guid` 是同一套标识 ——
/// 「哪个插件」这件事只有 GUID 是可靠的，插件名和文件名都会随作者改写。
///
/// 版本边界用本项目规则比较（四段 = beta 序号，见
/// `releases::compare_release_versions`），所以 `1.8.7.2` 与 `1.8.7-beta.2` 等价。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositoryRelation {
    pub kind: RelationKind,
    /// 目标插件的 GUID（比较时忽略大小写）。
    pub guid: String,
    /// 版本区间；空字符串表示不限版本。
    ///
    /// 语法见 `version_range`：逗号分隔的多个条件是 AND，支持
    /// `>=` / `>` / `<=` / `<` / `==`（`=` 同义）/ `!=` / `*`。
    /// 旧写法 `min_version` / `max_version` 仍能反序列化（由
    /// `deserialize_relations` 折成这个字段），序列化只写 `version`。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// 只对**预发布版本**成立（三态）。
    ///
    /// - 不写（`None`）：正式版与预发布版都算
    /// - `true`：只匹配预发布版（本项目的四段写法 `1.9.8.1` 即 `1.9.8-beta.1`）
    /// - `false`：只匹配正式版
    ///
    /// 为什么需要它：正式版与测试版常是**同一个 GUID** 的两条发布线
    /// （同一仓库的 stable / beta 渠道各一个仓库条目）。要表达「正式版与
    /// 测试版互斥」时，版本区间无能为力 —— 预发布版按 semver 排在**同号
    /// 正式版之下**，`[1.9.8, 1.9.9)` 会命中 `1.9.8` 却漏掉 `1.9.8.1`。
    /// 用这个标志直接按「是不是预发布」筛，两边各写一条即可。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prerelease: Option<bool>,
    /// 仅对 `requires` 有意义：缺失时不提示、不显示按钮。
    ///
    /// 其它种类写 `optional = true` 属于配置错误 —— 「可选的冲突」没有含义。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
}

/// 反序列化关系表：**未知的 `kind` 跳过而不是报错**。
///
/// 这是仓库源的前向兼容边界。新版本客户端可能写入旧版本不认识的关系种类
/// （例如以后的 `loads_after`），旧客户端若直接报错，整份源文档就不可读，
/// 用户会看到「所有仓库都失败」。跳过 + 记一条 warn 更合适：
/// 旧客户端只是不认识这条关系，其余配置仍然可用。
///
/// 缺少 `kind` 按 `requires` 处理：这是最可能的意图，也让历史写法
/// （`[[repositories.requires]]` 只写 guid 的表）继续可读。
fn deserialize_relations<'de, D>(deserializer: D) -> Result<Vec<RepositoryRelation>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    struct RelationWire {
        #[serde(default)]
        kind: String,
        #[serde(default)]
        guid: String,
        #[serde(default)]
        version: String,
        /// 旧写法，仍接受：与 `max_version` 一起折成 `version`。
        #[serde(default)]
        min_version: String,
        #[serde(default)]
        max_version: String,
        #[serde(default)]
        prerelease: Option<bool>,
        #[serde(default)]
        optional: bool,
        /*
         * 捕获「把仓库字段写到关系表里」这类错误。
         *
         * TOML 里 `[[repositories.relations]]` 之后的任何键都属于**关系条目**，
         * 不再属于仓库。作者很容易在关系块后面接着写 `install_target`、
         * `note_i18n` —— serde 默认忽略未知字段，于是这些字段凭空消失，
         * 最终报出的却是「missing field `install_target`」，作者会以为
         * 自己根本忘了写。这里显式收集，给出能对得上症状的提示。
         */
        #[serde(flatten)]
        extra: std::collections::BTreeMap<String, toml::Value>,
    }

    let raw = Vec::<RelationWire>::deserialize(deserializer)?;
    let mut relations = Vec::with_capacity(raw.len());
    for entry in raw {
        /*
         * 关系条目里出现仓库字段 → 说明作者把仓库字段写在关系块之后了。
         * 这个错误本身的后果是「仓库字段神秘消失」，直接报错点名更好排查。
         */
        if let Some(misplaced) = entry
            .extra
            .keys()
            .find(|key| REPOSITORY_FIELD_NAMES.contains(&key.as_str()))
        {
            return Err(serde::de::Error::custom(format!(
                "关系表里出现了仓库字段 `{misplaced}`：`[[repositories.relations]]` 之后的键都属于关系条目，\
                 仓库字段必须写在这些关系之前。"
            )));
        }
        let kind = if entry.kind.trim().is_empty() {
            RelationKind::Requires
        } else {
            let Some(kind) = RelationKind::from_token(&entry.kind) else {
                log::warn!(
                    "忽略不认识的关系类型：kind={}，guid={}（可能是更新版本的客户端写入）",
                    entry.kind.trim(),
                    entry.guid.trim()
                );
                continue;
            };
            kind
        };
        let version = relation_version(
            entry.version.trim(),
            entry.min_version.trim(),
            entry.max_version.trim(),
            entry.guid.trim(),
        );
        relations.push(RepositoryRelation {
            kind,
            guid: entry.guid,
            version,
            prerelease: entry.prerelease,
            optional: entry.optional,
        });
    }
    Ok(relations)
}

/// 关系的版本区间：新字段 `version` 优先，旧 `min_version`/`max_version` 折成它。
///
/// 两者同时出现按新字段处理并记一条 warn —— 那是迁移期手写了两套写法，
/// 猜「哪个更想要」不如用明确的优先级；旧字段迟早会从仓库源里消失。
fn relation_version(version: &str, min_version: &str, max_version: &str, guid: &str) -> String {
    let legacy = crate::version_range::fold_bounds(min_version, max_version);
    if version.is_empty() {
        return legacy;
    }
    if !legacy.is_empty() {
        log::warn!(
            "关系同时写了 version 与 min_version/max_version，按 version 处理：guid={guid}，version={version}，legacy={legacy}"
        );
    }
    version.to_string()
}

/// 仓库级字段名：出现在关系条目里几乎一定是位置写错了。
const REPOSITORY_FIELD_NAMES: &[&str] = &[
    "name",
    "name_i18n",
    "group",
    "group_i18n",
    "note",
    "note_i18n",
    "provider",
    "username",
    "repository",
    "download_mode",
    "branch",
    "file_path",
    "region",
    "platform",
    "channel",
    "asset_rules",
    "install_target",
    "plugin_guid",
    "local_product_name",
    "local_original_filename",
    "version_regex",
    "max_release_count",
    "download_threads",
    "enabled",
];

/// 按 Release 版本区间覆盖默认关系表。
///
/// 场景：同一个 MOD 的不同大版本依赖不同版本的基础库 ——
/// `A 1.0` 要 `B ≥ 2.0`，`A 1.1` 要 `B ≥ 3.0`。仓库级的 `relations`
/// 无法表达这种差异（它是固定值），所以额外提供这张表：
///
/// ```toml
/// [[repositories.release_relations]]
/// version = ">=1.8.0,<2.0.0"   # 只对这段版本生效
/// relations = [{ kind = "requires", guid = "...", version = ">=19.0" }]
/// ```
///
/// 匹配规则：所有**区间命中**的条目里，取区间下限最高的那条，
/// 其 `relations` 整体替换仓库级 `relations`（不是合并 —— 合并会让
/// 「新版降低要求」无法表达）。下限相同取先出现的一条。
///
/// `version` 留空（或不写）是错误配置：那样它会匹配任何版本，等同于直接改
/// `relations`，所以在校验阶段就拒绝，避免「以为写了区间其实没限」。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReleaseRelationOverride {
    /// 这张覆盖表生效的 Release 版本区间；空表示不限（校验阶段拒绝）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// 命中的区间内使用的关系表（整体替换，不合并）。
    #[serde(default, alias = "requires")]
    pub relations: Vec<RepositoryRelation>,
}

impl<'de> Deserialize<'de> for ReleaseRelationOverride {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            version: String,
            /// 旧写法，仍接受：与 `max_version` 一起折成 `version`。
            #[serde(default)]
            min_version: String,
            #[serde(default)]
            max_version: String,
            #[serde(
                default,
                alias = "requires",
                deserialize_with = "deserialize_relations"
            )]
            relations: Vec<RepositoryRelation>,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self {
            version: relation_version(
                wire.version.trim(),
                wire.min_version.trim(),
                wire.max_version.trim(),
                "<release_relations>",
            ),
            relations: wire.relations,
        })
    }
}

/// 整批刷新期间暂存的 Release 缓存。
///
/// 见 `AppConfig::staged_release_cache` 的说明：清理仓库前把缓存挪到这里，
/// 按源文档重建仓库时再取回，避免 5 分钟缓存被「重建」这件事本身作废。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StagedReleaseCache {
    #[serde(default)]
    pub releases: Vec<ReleaseInfo>,
    #[serde(default)]
    pub cached_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryConfig {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    pub provider: Provider,
    pub username: String,
    pub repository: String,
    #[serde(default)]
    pub download_mode: RepositoryDownloadMode,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// 这个 MOD 支持的桌面平台；`None` / 缺失表示全平台可用。
    ///
    /// 与 `region` 同一语义层：不适用就等于**不存在**（不展示、不刷新、
    /// 不参与绑定与更新统计），而不是「存在但不显示」——否则前端会替一个
    /// 用户看不见的仓库发请求，失败时弹出一条无从处理的错误。
    ///
    /// 为什么需要它：有些 MOD 只对特定平台有意义（例如 macOS 手柄修复），
    /// 在别的平台上装上也毫无作用，却会占满列表并参与更新检测。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default)]
    pub channel: Channel,
    #[serde(default)]
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    /// 这个 MOD 与其它插件的关系（依赖 / 排斥 / 兼容，按 GUID 声明）。
    ///
    /// 缺失依赖或出现冲突时，MOD 安装页与 MOD 管理页都会在卡片上提示；
    /// 能匹配到线上仓库且版本满足时给出快捷安装入口。
    ///
    /// `requires` 是早期只有依赖时的字段名，保留为别名：已经照那个名字
    /// 写下的源文档不需要改写。
    #[serde(
        default,
        alias = "requires",
        deserialize_with = "deserialize_relations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub relations: Vec<RepositoryRelation>,
    /// 按 Release 版本区间覆盖 `relations`（见 `ReleaseRelationOverride`）。
    #[serde(
        default,
        alias = "release_requires",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub release_relations: Vec<ReleaseRelationOverride>,
    /// Optional regular expression applied to the installed MOD version when
    /// binding a local plugin to this repository.
    #[serde(default)]
    pub version_regex: String,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub github_token: String,
    #[serde(default)]
    pub gitee_token: String,
    #[serde(default = "default_true")]
    pub official_api_only: bool,
    /// 是否在这个仓库源下展示并刷新这个仓库。
    ///
    /// `false` = 完全不参与：不在 MOD 安装页显示卡片，也不发起 Release 刷新，
    /// 不参与 MOD 绑定与更新统计。除了用户主动关闭，被官方仓库源覆盖的社区
    /// 副本也会被置成 `false`。
    ///
    /// 地区不适用（例如中国大陆下的 international 仓库）是另一条独立判定，
    /// 但语义相同：整个仓库视作不存在。统一判定见
    /// `config::repository_is_available`。
    pub enabled: bool,
    pub builtin: bool,
    #[serde(default)]
    pub pinned_version: Option<String>,
    #[serde(default)]
    pub ignored_version: Option<String>,
    #[serde(default)]
    pub etag: Option<String>,
    #[serde(default)]
    pub cached_releases: Vec<ReleaseInfo>,
    /// `cached_releases` 的获取时间（Unix 毫秒）。
    ///
    /// 用来判断缓存是否还在有效期内，避免每次打开页面或批量刷新都把整棵
    /// 仓库树的 Release API 重打一遍。`None` 表示这份缓存来自旧版本配置或
    /// 从未成功获取过 —— 两种情况都当作「已过期」，必须重新请求。
    #[serde(default)]
    pub releases_cached_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub schema_version: u32,
    #[serde(default = "crate::config::default_ui_zoom")]
    pub ui_zoom: f64,
    #[serde(default = "default_hide_unidentified_mods")]
    pub hide_unidentified_mods: bool,
    /// 删除 MOD（含覆盖安装前清理旧包）时是否先移入回收站。
    ///
    /// 默认 `true`：管理器删除的文件都能在回收站找回。
    /// 设成 `false` 后直接删除文件与安装清单，**不可恢复** —— 设置界面必须
    /// 先给出风险提示与二次确认才允许关闭。
    #[serde(default = "default_true")]
    pub recycle_deleted_mods: bool,
    /// 读取 MOD 元数据（DLL）时最多同时处理几个文件。
    ///
    /// `0` = 全部 CPU 核心（默认）。用户可存任意正整数，真正执行前按当前机器
    /// 的核心数夹紧（[`crate::metadata_read::resolve_concurrency`]），所以配置
    /// 换机器仍然安全。
    #[serde(default)]
    pub mod_metadata_concurrency: u32,
    /// 解析地图 `info` 时最多同时处理几份。
    ///
    /// `0` = 全部 CPU 核心（默认，[`crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY`]）。
    #[serde(default = "default_map_info_concurrency")]
    pub map_info_concurrency: u32,
    /// 已经看过的教程步骤 id（前端步骤表的 `id`）。
    ///
    /// 教程步骤是只增不改的清单：更新后新加的步骤 id 不在这个集合里，
    /// 启动时只播这些新步骤，老步骤不再重复。id 由前端维护、后端只做
    /// 并集与保存，所以这里不校验「是否存在这个步骤」—— 校验只会让
    /// 删改过步骤表的版本把历史悄悄抹掉。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tutorial_watched_steps: Vec<String>,
    pub game: GameConfig,
    pub network: NetworkConfig,
    pub launch: LaunchConfig,
    pub auth: AuthConfig,
    pub repositories: Vec<RepositoryConfig>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
    /// 地图仓库（DIYLevel 关卡集的远程来源）。
    ///
    /// 与 `repositories` 平行而非合并：两者的绑定身份、安装单元都不同
    /// （见 `MapRepositoryConfig` 的说明）。源树复用 `repository_sources`
    /// 的地址解析与相对路径规则。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map_repositories: Vec<MapRepositoryConfig>,
    /// 是否只用内置地图快照（与 `builtin_sources_only` 呼应）。
    ///
    /// 打开后官方地图索引的 `url` 指向编译期嵌入的 `maps.toml`，
    /// 与仓库侧「仅使用内置仓库源」完全同一套语义。
    ///
    /// 默认 `false`：先读远端索引，读不到时 `refresh_map_sources` 才回退到
    /// 内置快照。缺字段的配置按 `false` 补齐（旧版本写的 `true` 由
    /// `config::migrate_bundled_source_switches` 一次性迁走）。
    #[serde(default)]
    pub builtin_map_sources_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub community_discovery: Option<CommunityDiscovery>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gitee_community_discovery: Option<CommunityDiscovery>,
    /// 社区地图发现：带 `map` topic 的仓库（两个平台各一份，发现时整体替换）。
    ///
    /// 只存坐标而不存条目：地图条目依赖各 Release note 里的 `oc2-map` 块，
    /// 每次地图源刷新都要重新读一遍，缓存下来反而会有「作者删了版本、本地
    /// 还留着」的陈旧数据。所以这里没有缓存与开关，也不需要迁移旧配置。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub community_map_repositories: Vec<CommunityMapRepository>,
    // Identity only: no source documents, repository settings or Release cache.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_repository_ids: BTreeMap<String, String>,
    /// 整批刷新清掉仓库前，暂存的 Release 缓存（`repository_id` → 缓存）。
    ///
    /// 刷新流程是「先清空整棵源的仓库 → 再拉源文档 → 按源文档重建仓库」。
    /// 重建出来的仓库是全新的，`cached_releases` 为空，于是 5 分钟缓存
    /// **永远命中不了** —— 每次刷新都把整棵仓库树的 API 重打一遍（实测
    /// 20+ 个仓库、几十个请求，还固定撞速率限制）。
    ///
    /// 所以清理时把缓存挪到这里，`apply_document` 重建仓库时按 ID 取回。
    /// 它只是短暂的搬运中转：重建完成的仓库会把缓存接回自己的字段，这里
    /// 随即清空。真正到期与否仍由各仓库的 `releases_cached_at` 决定。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub staged_release_cache: BTreeMap<String, StagedReleaseCache>,
    #[serde(default = "crate::github_proxies::bundled_defaults")]
    pub github_proxies: GitHubProxyConfig,
    #[serde(default)]
    pub mod_notes: Vec<ModNote>,
    /// 只用管理器自带的仓库源快照，不访问 Gitee / GitHub 的 raw 入口。
    ///
    /// 打开后官方根源的地址直接落到打包资源，不再按地区选择远端直链；
    /// 这也让完全离线环境能列出仓库。自定义源不受影响 —— 那是用户明确添加的。
    ///
    /// 默认 `false`，理由与 `builtin_map_sources_only` 相同：远端索引是数据
    /// 来源，内置快照只是读不到时的回退。
    #[serde(default)]
    pub builtin_sources_only: bool,
}

impl AppConfig {
    /// 把「已经看过」的教程步骤 id 并进配置。
    ///
    /// 并集而不是覆盖：重播与跳步可能在不同次运行里交错回写，谁的 id 都
    /// 不该丢。顺序保留首次写入的先后，空白与重复值直接跳过。
    pub(crate) fn record_tutorial_steps(&mut self, ids: &[String]) {
        for id in ids {
            let id = id.trim();
            if id.is_empty() || self.tutorial_watched_steps.iter().any(|seen| seen == id) {
                continue;
            }
            self.tutorial_watched_steps.push(id.to_owned());
        }
    }
}

/// 游戏运行时。
///
/// 默认 `Native`（含 macOS）：原生版走 macOS BepInEx，不需要 CrossOver
/// 容器，是「装好游戏就能装 MOD」的那条路；CrossOver 只是给还没换原生
/// 框架的用户保留的选项，由用户自己切过去。Windows 只有原生一种，
/// 这里的取值不影响它（见 `bepinex::Package::for_platform`）。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum GameRuntime {
    Wine,
    #[default]
    Native,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GameConfig {
    /// 用户手动选定的游戏目录。空 = 没有手动设置，启动时自动探测。
    ///
    /// 自动探测没有开关：写死路径优先，没有写死路径就去程序目录与各 Steam
    /// 库里找（见 `game::find_game_path`）—— 一个「关掉就永远找不到游戏」的
    /// 开关没有实际用途，曾经有过的 `auto_detect` 字段因此被删除。
    pub path: String,
    /// `#[serde(default)]` 取 `GameRuntime::default()`：旧配置缺这个字段时
    /// 与全新配置走同一个默认值，不在两处各写一遍平台判断。
    #[serde(default)]
    pub runtime: GameRuntime,
}

impl GameConfig {
    pub(crate) fn switch_runtime(&mut self, runtime: GameRuntime) {
        if self.runtime != runtime {
            self.runtime = runtime;
            // 两种运行时的游戏目录完全不同，旧路径在新运行时下必然失效。
            self.path.clear();
        }
    }
}

/// 这次授权打开的是哪个页面。
///
/// `install` = 还没装过 App，直接去安装页：装 App 与勾仓库一步完成，回调会把
/// `installation_id` 一起带回来；`authorize` = 已经装过，只重新授权，不再经过
/// 安装页 —— GitHub 对已安装的账号会停在配置页，不会回调本机。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PublishAuthMode {
    Install,
    Authorize,
}

/// 授权流程启动后的返回：系统浏览器已经打开，前端可以给出备用链接。
#[derive(Debug, Clone, Serialize)]
pub struct PublishAuthorization {
    pub authorize_url: String,
    pub expires_in: u64,
    /// 前端据此决定按钮是「安装 App 并选择仓库」还是「登录 GitHub」。
    pub mode: PublishAuthMode,
}

/// 授权轮询结果。
///
/// 用 `status` 而不是 `Result`：等待、过期、拒绝都是正常分支，不该在前端被当成
/// 异常弹红字。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PublishAuthPoll {
    Pending,
    Authorized { login: String },
    Expired,
    Denied,
    Failed { reason: String },
}

/// 进入发布页时检查一次登录状态的结论。
///
/// - `valid`：本地令牌可用，没有刷新；
/// - `refreshed`：access token 失效，已用 refresh token 换到新的一对并落盘；
/// - `reauthorize`：令牌失效且无法刷新（没有 / 过期 / 被拒绝），本地令牌已清掉，
///   界面要请用户重新授权；`reason` 是语言表 tag；
/// - `unavailable`：网络或 GitHub 暂时不可用，**本地令牌原样保留**，只提示；
/// - `signed_out`：本来就没登录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PublishSessionOutcome {
    SignedOut,
    Valid,
    Refreshed,
    Reauthorize { reason: String },
    Unavailable { reason: String },
}

/// 会话检查的返回：结论 + 检查之后的授权状态（刷新或清除后会变）。
#[derive(Debug, Clone, Serialize)]
pub struct PublishSessionCheck {
    pub outcome: PublishSessionOutcome,
    pub auth: PublishAuthState,
}

/// 上传页的授权状态（登录名与 client_id 都是明文，Token 本身不出后端）。
///
/// `install_url` 是 GitHub App 的安装入口（安装时可以选择「Only select
/// repositories」）；`secret_configured` 为 false 说明这个构建没有注入 Client
/// Secret，根本换不到 Token，界面要直接说清楚而不是点登录才报错。
#[derive(Debug, Clone, Serialize)]
pub struct PublishAuthState {
    pub authenticated: bool,
    pub login: String,
    pub client_id: String,
    pub install_url: String,
    /// 已经记下上次安装的 installation id：有它时登录只重新授权，不再回安装页。
    pub installed: bool,
    pub secret_configured: bool,
}

/// 上传页可选的一个地图仓库。
#[derive(Debug, Clone, Serialize)]
pub struct PublishRepository {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub default_branch: String,
    pub topics: Vec<String>,
    /// 当前令牌对这个仓库有没有 push 权限（只读仓库不允许发布）。
    pub push: bool,
}

/// 发布页的仓库列表。
///
/// 光有仓库列表不够：`installations == 0` 说明 GitHub App 还没装到这个账号上，
/// 这时必须先走安装页勾选仓库，而不是让用户对着空列表「先新建一个」。
#[derive(Debug, Clone, Serialize)]
pub struct PublishRepositoryList {
    /// 授权到本 App 的安装数量；0 = 还没安装。
    pub installations: usize,
    /// 是否有安装范围是「全部仓库」：只有这时新建的仓库才自动落在授权范围内。
    pub all_repositories: bool,
    pub repositories: Vec<PublishRepository>,
}

/// 一个 Release 附件。
#[derive(Debug, Clone, Serialize)]
pub struct PublishAsset {
    pub id: u64,
    pub name: String,
    pub size: u64,
}

/// 本地 ZIP 识别摘要。发布前从 info 取版本，原文件名版本仅供核对。
#[derive(Debug, Clone, Default, Serialize)]
pub struct PublishZipInspection {
    /// ZIP 文件名（不含目录）。
    pub file_name: String,
    /// 本地 ZIP 的字节数，用于上传前确认。
    pub file_size: u64,
    /// 原 ZIP 文件名里的版本；仅供核对，不能作为上传版本。
    pub mapset_version: String,
    /// `info` 里的版本号。
    pub info_version: String,
    pub level_set_name: String,
    pub level_set_name_zh: String,
    pub author: String,
    pub level_count: usize,
    pub file_count: usize,
    /// 发布只接受单地图，数量不为 1 时拒绝上传。
    pub set_count: usize,
    /// `requires.txt` 声明的最低 runtime 版本（GUID → 版本区间）。
    pub requires: BTreeMap<String, String>,
}

/// 仓库标签检查结果：`required` 是发布页要求的三件套，`missing` 为空表示齐全。
#[derive(Debug, Clone, Default, Serialize)]
pub struct PublishRepositoryTopics {
    pub topics: Vec<String>,
    pub required: Vec<String>,
    pub missing: Vec<String>,
}

/// 上传页看到的「一版地图」，就是仓库里的一个 Release。
#[derive(Debug, Clone, Serialize)]
pub struct PublishRelease {
    pub id: u64,
    /// tag 是这张地图在仓库里的唯一标识（一个 tag = 一张地图集），不是版本号。
    pub tag: String,
    pub title: String,
    pub draft: bool,
    pub prerelease: bool,
    pub created_at: String,
    pub published_at: Option<String>,
    /// 块外面的说明文字（`oc2-map` 块已剥掉）。
    pub body: String,
    pub assets: Vec<PublishAsset>,
    /// 解析出的元数据；没有 `oc2-map` 块（不是地图 Release）时为 None。
    pub metadata: Option<MapPublishMetadata>,
}

/// 版本表里的一项：某一版地图的「资产版本 + info 版本」。
///
/// 用数组而不是 map：顺序本身就是信息（末项最新），拖动排序就是移动元素。
/// 附件名不存进表里 —— 它由作者的 asset 规则渲染 `mapset_version` 得到，
/// 再存一份副本只会在改名之后与规则对不上。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetVersionEntry {
    /// 作者定义的地图包版本号；允许 `3.4.1-fix` 这种带后缀的写法。
    #[serde(default)]
    pub mapset_version: String,
    /// 这一版 ZIP 里 `info` 自报的版本号，判「装的是不是最新」用它。
    #[serde(default)]
    pub info_version: String,
}

/// 前端编辑并提交的元数据。
///
/// 与 `map_note::MapNoteMetadata` 的差别只有关系表：这里用「GUID → 区间字符串」，
/// 默认关系用简写表；按资产版本的覆盖关系复用 `ReleaseRelationOverride`，
/// 读写时保留其中的 `prerelease` / `optional`，避免编辑版本区间丢掉原声明。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MapPublishMetadata {
    #[serde(default)]
    pub level_set_name: String,
    #[serde(default)]
    pub level_set_name_zh: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub asset: String,
    #[serde(default)]
    pub latest_mapset_version: String,
    #[serde(default)]
    pub latest_info_version: String,
    #[serde(default)]
    pub requires: BTreeMap<String, String>,
    #[serde(default)]
    pub compatible: BTreeMap<String, String>,
    #[serde(default)]
    pub conflicts: BTreeMap<String, String>,
    /// 版本表（末项最新）；空表 = 退回按附件名里的数字段排。
    #[serde(default)]
    pub asset_versions: Vec<AssetVersionEntry>,
    #[serde(default)]
    pub release_relations: Vec<ReleaseRelationOverride>,
}

/// 新建或修改一版地图的请求。
#[derive(Debug, Clone, Deserialize)]
pub struct PublishSaveRequest {
    pub owner: String,
    pub repository: String,
    /// 有值 = 改这一版；空 = 新建一版（Release 会自动建 tag）。
    #[serde(default)]
    pub release_id: Option<u64>,
    /// 地图集的地图 ID：一个 tag 一张图，版本号写在附件名里。
    pub tag: String,
    /// 块外面的说明文字。
    #[serde(default)]
    pub body: String,
    /// 三步式发布里只有第三步带它；省略时保留远端已有的 `oc2-map` 块。
    #[serde(default)]
    pub metadata: Option<MapPublishMetadata>,
}

/// 上传附件请求。
#[derive(Debug, Clone, Deserialize)]
pub struct PublishUploadRequest {
    pub owner: String,
    pub repository: String,
    pub release_id: u64,
    /// 本地 ZIP 的绝对路径；附件名由后端从 ZIP 版本和 Release 规则生成。
    pub path: String,
}

/// 上传进度：已读入 HTTP 请求流的字节数；done 表示服务端确认成功。
#[derive(Debug, Clone, Serialize)]
pub struct PublishUploadProgress {
    pub sent: u64,
    pub total: u64,
    pub phase: &'static str,
}

/// 删除一版地图的请求。
#[derive(Debug, Clone, Deserialize)]
pub struct PublishDeleteReleaseRequest {
    pub owner: String,
    pub repository: String,
    pub release_id: u64,
    /// 删 Release 之后要不要顺手删同名 tag（标签模式下留孤儿 tag 会让
    /// 作者以为自己还发布了这一版）。
    pub tag: String,
    pub delete_tag: bool,
}

#[cfg(test)]
mod game_runtime_tests {
    use super::*;

    /// 旧配置里没有 `runtime` 字段时必须落到原生：macOS 新装用户默认就是
    /// 原生框架，而缺字段说明这份配置来自更早的版本，不该反过来把它
    /// 推进 CrossOver 分支（那会去 Wine 容器里找游戏与 BepInEx）。
    #[test]
    fn missing_and_fresh_game_settings_both_default_to_native() {
        let missing: GameConfig = serde_json::from_str(r#"{"path":""}"#).unwrap();
        assert_eq!(missing.runtime, GameRuntime::Native);
        // 旧配置里的 `auto_detect` 字段已删除：多出来的键被忽略，不影响迁移。
        let legacy: GameConfig =
            serde_json::from_str(r#"{"path":"","auto_detect":false}"#).unwrap();
        assert_eq!(legacy.runtime, GameRuntime::Native);
        let fresh = GameConfig {
            path: String::new(),
            runtime: GameRuntime::default(),
        };
        assert_eq!(fresh.runtime, GameRuntime::Native);
    }

    #[test]
    fn switching_runtime_clears_the_selected_path() {
        let mut settings = GameConfig {
            path: "old Windows game directory".into(),
            runtime: GameRuntime::Wine,
        };
        settings.switch_runtime(GameRuntime::Wine);
        assert_eq!(settings.path, "old Windows game directory");
        settings.switch_runtime(GameRuntime::Native);
        assert!(settings.path.is_empty());
        let persisted = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<GameConfig>(&persisted).unwrap(),
            settings
        );
        settings.path = "native game directory".into();
        settings.switch_runtime(GameRuntime::Wine);
        assert!(settings.path.is_empty());
    }
}

#[cfg(test)]
mod tutorial_progress_tests {
    /// 前端每走一步都会整批发一次；并集去重是唯一合并规则。
    #[test]
    fn watched_step_ids_are_merged_as_a_deduped_union() {
        let mut config = crate::config::default_config();
        config.record_tutorial_steps(&["intro".into(), "sidebar".into()]);
        // 重播时又发了一遍老 id，并带上新 id：老的不能重复、顺序不动。
        config.record_tutorial_steps(&[
            " sidebar ".into(),
            "sidebar-mods-online".into(),
            String::new(),
        ]);
        assert_eq!(
            config.tutorial_watched_steps,
            vec!["intro", "sidebar", "sidebar-mods-online"]
        );
    }

    /// 旧配置缺字段时反序列化成空集合，序列化时整个键都不写。
    #[test]
    fn missing_field_round_trips_as_empty_and_stays_out_of_json() {
        let raw = serde_json::to_value(crate::config::default_config()).unwrap();
        assert!(raw.get("tutorial_watched_steps").is_none());
        let mut config = crate::config::default_config();
        config.record_tutorial_steps(&["intro".into()]);
        let raw = serde_json::to_value(&config).unwrap();
        assert_eq!(raw["tutorial_watched_steps"], serde_json::json!(["intro"]));
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub timeout_seconds: u64,
    pub download_threads: u32,
    pub max_retry_count: u32,
}
fn default_max_release_count() -> u32 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchConfig {
    pub ask_user_to_start_from_steam: bool,
    pub auto_restore_winhttp: bool,
}

/// 一组 GitHub App 用户令牌（首次授权或刷新的结果），时刻都是 Unix 秒。
///
/// 只在后端流转，不序列化给前端：令牌本身不出后端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishTokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    pub github_token: String,
    pub gitee_token: String,
    pub official_api_only: bool,
    /// 地图发布页经 GitHub App 用户授权取得的 access token（DPAPI 保护）。
    ///
    /// 单独存一份而不是复用 `github_token`：App 令牌只覆盖安装时勾选的仓库，
    /// 直接覆盖用户手填的 PAT 会把读 API 的权限悄悄降下来；两份并存时读路径
    /// 继续用用户的 PAT，发布路径只用这一份。
    #[serde(default)]
    pub github_publish_token: String,
    /// 与 access token 配对的 refresh token（DPAPI 保护）；App 关闭了令牌过期时为空。
    ///
    /// GitHub 每次刷新都会**同时轮换**两者：旧 refresh token 立即作废，所以新的一对
    /// 必须一起落盘，不能只换 access token。
    #[serde(default)]
    pub github_publish_refresh_token: String,
    /// access token 的过期时刻（Unix 秒）；`None` = 令牌不过期或旧配置没记。
    #[serde(default)]
    pub github_publish_token_expires_at: Option<i64>,
    /// refresh token 的过期时刻（Unix 秒）；过了就只能重新授权。
    #[serde(default)]
    pub github_publish_refresh_expires_at: Option<i64>,
    /// 授权到的 GitHub 登录名，只用于界面显示「以谁的身份发布」。
    #[serde(default)]
    pub github_publish_login: String,
    /// 上次安装 App 时回调带回来的 installation id；0 = 还不知道。
    ///
    /// 存它只是为了绕开一件事：`/user/installations` 是最终一致的，刚在 GitHub
    /// 上勾完仓库时它能空几十秒（实测退出重登后有 25 秒空窗），而按 id 直查
    /// `/user/installations/{id}/repositories` 立刻就是用户刚勾的那一份。
    /// 退出登录不清它：用户在 GitHub 上改仓库范围时这个 id 不会变。
    #[serde(default)]
    pub github_publish_installation_id: u64,
}

fn default_hide_unidentified_mods() -> bool {
    true
}

/// 解析地图 `info` 的默认并发上限。
///
/// 值是 `metadata_read` 里那一个常量：配置默认值与运行时上限必须同源，
/// 否则「默认 4」和「最多 4」会各说一套。
fn default_map_info_concurrency() -> u32 {
    crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicConfig {
    pub schema_version: u32,
    pub platform: String,
    pub region: String,
    pub ui_zoom: f64,
    pub hide_unidentified_mods: bool,
    /// 删除 MOD 时先移入回收站；false = 直接删除（不可恢复）。
    #[serde(default = "default_true")]
    pub recycle_deleted_mods: bool,
    /// MOD 元数据并行读取上限；`0` = 全部 CPU 核心。
    #[serde(default)]
    pub mod_metadata_concurrency: u32,
    /// 地图 `info` 并行解析上限；`0` = 全部 CPU 核心。
    #[serde(default = "default_map_info_concurrency")]
    pub map_info_concurrency: u32,
    /// 已经看过的教程步骤 id；启动教程时只播不在里面的步骤。
    #[serde(default)]
    pub tutorial_watched_steps: Vec<String>,
    pub game: GameConfig,
    pub network: NetworkConfig,
    pub launch: LaunchConfig,
    pub official_api_only: bool,
    pub github_token_set: bool,
    pub gitee_token_set: bool,
    #[serde(default)]
    pub github_publish_token_set: bool,
    #[serde(default)]
    pub github_publish_login: String,
    pub repositories: Vec<RepositoryConfig>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
    #[serde(default)]
    pub community_discovery: Option<CommunityDiscovery>,
    #[serde(default)]
    pub gitee_community_discovery: Option<CommunityDiscovery>,
    #[serde(default)]
    pub github_proxies: GitHubProxyConfig,
    /// 只用内置仓库源（不访问远端 raw 入口）。
    #[serde(default)]
    pub builtin_sources_only: bool,
    /// 只用内置地图源。与仓库源开关相互独立：地图索引和仓库索引是两个文件，
    /// 一个连不上不代表另一个也连不上。
    #[serde(default)]
    pub builtin_map_sources_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RepositorySource {
    pub id: String,
    pub nickname: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub nickname_i18n: BTreeMap<String, String>,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryExport {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub name_i18n: BTreeMap<String, String>,
    pub provider: Provider,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub group_i18n: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    /// 支持的桌面平台（`windows` / `macos`）；`None` 表示全平台可用。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    pub username: String,
    pub repository: String,
    pub enabled: bool,
    #[serde(default)]
    pub source_id: Option<String>,
    #[serde(default)]
    pub download_mode: RepositoryDownloadMode,
    #[serde(default)]
    pub channel: Channel,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    /// 与其它插件的关系（依赖 / 排斥 / 兼容）；与 `RepositoryConfig` 同名字段对应。
    ///
    /// `requires` 是早期只有依赖时的字段名，保留为别名：已经照那个名字
    /// 写下的源文档不需要改写。
    #[serde(
        default,
        alias = "requires",
        deserialize_with = "deserialize_relations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub relations: Vec<RepositoryRelation>,
    /// 按 Release 版本覆盖关系表的规则（与 `RepositoryConfig` 同名字段对应）。
    #[serde(
        default,
        alias = "release_requires",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub release_relations: Vec<ReleaseRelationOverride>,
    #[serde(default)]
    pub version_regex: String,
    #[serde(default)]
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_i18n: BTreeMap<String, String>,
}

/// 一个地图仓库：地图的分发单元。
///
/// 与 `RepositoryConfig` 对称但**刻意不合并**：
///
/// - MOD 仓库绑定本地插件靠 `plugin_guid`（DLL 内嵌元数据）；
/// - 地图仓库绑定本地关卡集靠 `author + level_set_name + level_set_name_zh`（info 里的字段）。
///
/// 两者字段几乎没有交集（GUID / 产品名 / 关系表 vs 关卡集名 / 作者），
/// 合并后每个使用点都要判断「这是哪种」。共用的是源树、Release 刷新与下载
/// 竞速这些**流水线**，不是模型。
fn default_map_provider() -> Provider {
    Provider::Github
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapRepositoryConfig {
    /// 稳定 ID。
    ///
    /// 源文档可以不写：首次导入时按「源 ID + 身份」生成并持久化，
    /// 之后一直复用（安装清单与缓存都挂在它上面）。手写 UUID 只会制造
    /// 「复制一份配置忘了改 ID」这类问题。
    #[serde(default)]
    pub id: String,
    /// 单语言镜像。源文档**只需要写 `name_i18n`**，这里是加载后从
    /// `zh-CN` 折进 来的兼容字段，不要求作者手写两遍。
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_i18n: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    /// 条目可以省略：源文档顶部写了 `[bundle]` 时由它统一提供坐标，
    /// 单条目自己写就覆盖。缺省按 GitHub —— 存量条目全在 GitHub 上，
    /// 而「缺 provider 直接解析失败」会让只是漏写一行的作者整份源都发不出去。
    #[serde(default = "default_map_provider")]
    pub provider: Provider,
    /// 与 `provider` 同理：可以整组留给 `[bundle]` 提供，也可以自己写覆盖。
    /// 校验（`validate_map_entry`）在继承之后才跑，所以「两条都空」仍然报错。
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub repository: String,
    #[serde(default)]
    pub channel: Channel,
    /// 只读这一个 Release。
    ///
    /// 空 = 老行为（列表里最新的那个）。非空时**不再按时间取最新**：直接把
    /// 该 tag 的资产按 `asset_rules` 里的版本正则拆成多个「版本」，一个资产
    /// 一条 Release（见 `map_sources::synthesize_releases`）。
    ///
    /// 这是「一个仓库装多个地图集」的落地方式：作者每张图只往自己的固定 tag
    /// 里追加版本文件，条目靠 tag 区分，不必给每张图各开一个仓库。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    /// 强制最新**地图包版本**，空 = 按数字段比较取最大。
    ///
    /// 只有 tag 模式有意义：资产名里的版本号是作者手写的，`0.7` 在数字比较
    /// 里小于 `0.69`，作者却可能认为前者更新。写了它就等于「按这个号发最新
    /// 版」——前端判定也改成等值才算最新（见 `src/levelSetStatus.ts`）。
    ///
    /// 旧名 `latest_version` 依旧可反序列化：已发布出去的地图源文档用的就是它。
    #[serde(
        default,
        alias = "latest_version",
        skip_serializing_if = "Option::is_none"
    )]
    pub latest_mapset_version: Option<String>,
    /// 最新 Release 里 `info` 文件**内**的版本号（Release note 块的
    /// `latest_info_version`，由 `tools/oc2-info` 自动取）。
    ///
    /// `latest_mapset_version` 是作者定义的地图包版本，这一项是包内 info 自报的版本。
    /// 两者平时相同；作者升了包版本却忘了改 info 版本号时会不同，那时按包版本
    /// 比较会让用户永远看到「可更新」，前端改用这一项确认「装的就是最新版」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_info_version: Option<String>,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    /// 选哪个资产：地图只分发 ZIP。
    #[serde(default)]
    pub asset_rules: Vec<AssetRule>,
    /// 版本表（来自 Release note 的 `asset_versions`，末项最新）。
    ///
    /// 决定「谁是最新版」和版本列表的展示顺序；空表表示这份源没排过序，
    /// 退回按附件名里的数字段降序。
    #[serde(default)]
    pub asset_versions: Vec<AssetVersionEntry>,
    /// 本地关卡集的 `levelSetName`（info 里的英文名）。
    ///
    /// 与 `author` 一起构成绑定身份。不能用 `uid`：它是
    /// `MD5(baseUID + version)`，每发一版就变。
    #[serde(default)]
    pub level_set_name: String,
    /// 本地关卡集的 `levelSetNameZH`（info 里的中文名），可选。
    ///
    /// 英文集名与作者都相同的两张地图只靠前两者分不开（实测「胡闹糕中」与
    /// 「胡闹达学」是同一个作者的两张图，都写 `GAODA / GAODA`），中文名是
    /// 唯一能区分、又不随版本变化的字段。老源不写它时退回前两者比较，
    /// 行为与加入该字段之前完全一致。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level_set_name_zh: String,
    /// 本地关卡集的 `author`（info 里的作者，官方导出常为空）。
    #[serde(default)]
    pub author: String,
    /// 依赖声明：地图需要 DIYLevel 与 Web Runtime 在特定版本以上才跑得起来。
    ///
    /// 复用 `RepositoryRelation`——「某 GUID 的版本必须在区间内」这个语义
    /// 两边完全一样，没有再造一套的理由。
    #[serde(
        default,
        alias = "requires",
        deserialize_with = "deserialize_relations",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub relations: Vec<RepositoryRelation>,
    /// 按地图版本覆盖上面的关系表（与仓库源 `release_relations` 同语义）。
    ///
    /// 同一个地图集的大版本可能依赖不同版本的 DIYLevel / Web Runtime，
    /// 一张固定表表达不了；命中区间的那张整体替换，见
    /// `mods::effective_map_relations`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_relations: Vec<ReleaseRelationOverride>,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub note_i18n: BTreeMap<String, String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    /// 所属源的昵称：单语言镜像 + 多语言表。
    ///
    /// 后端**不持久化地图源树**（见 `map_sources::load_map_tree` 的说明），源
    /// 只是归类方式；但界面要按「源 → 分类 → 地图」三层展示，源名必须能读到。
    /// 所以加载源树时把昵称写进条目本身 —— `get_maps` 读的是持久化配置，
    /// 只靠 `source_id` 拿不到作者写在 `[[map_sources]]` 里的昵称。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_name_i18n: BTreeMap<String, String>,
    /// Release 缓存，与 `RepositoryConfig` 同一套字段与 TTL 语义。
    #[serde(default)]
    pub cached_releases: Vec<ReleaseInfo>,
    #[serde(default)]
    pub releases_cached_at: Option<i64>,
    #[serde(default)]
    pub last_error: Option<String>,
}

/// 一张地图仓库的依赖快照。管理器的清单，不写进游戏目录。
///
/// 关卡集没有安装清单：`level_sets::install_from_zip` 只把目录写进
/// `BepInEx/plugins/OC2DIYLevel/levels|disabled`，`manifests/` 只在回收时用。
/// 所以地图的依赖声明需要这份管理器侧的清单来固化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapRelationSnapshot {
    pub repository_id: String,
    pub level_set_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level_set_name_zh: String,
    pub author: String,
    /// 安装/同步那一刻线上声明的分组（分组语义就是作者）。
    ///
    /// 地图源被删掉之后这张图不再有线上条目，分组只剩这份快照记着；
    /// 没有它，管理页只能显示 info 里的裸作者名，作者声明的中英两名全丢。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_i18n: BTreeMap<String, String>,
    /// 分组的单语镜像（旧源文档只写 `group` 时用它）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RepositoryRelation>,
    /// 按版本覆盖的依赖表：快照必须带上它，否则「已经装了 1.x、线上声明的是
    /// 2.x 的依赖」时，依赖表会显示成 2.x 那一套。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_relations: Vec<ReleaseRelationOverride>,
    /// 安装/更新那一刻的资产版本（Release 的 tag_name，如 `3.4.1-fix`）。
    ///
    /// 本地 info 里的版本号可能与资产版本不一致（作者发修复版时常忘了改 info），
    /// 只靠 info 版本无法判断装的是不是最新资产，所以安装时记下来。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub installed_mapset_version: String,
}

/// `<data_root>/manifests/map-relations.json` 的整份内容。
///
/// 为什么不用一张清单一个文件：关卡集不是以仓库 id 落地在磁盘上的（身份是
/// `info` 里的 `levelSetName + author`），读取时只做一次「身份→关系」查表，
/// 单文件比几十个小文件省一次目录扫描。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MapRelationSnapshots {
    #[serde(default = "default_snapshot_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub entries: BTreeMap<String, MapRelationSnapshot>,
}

pub(crate) fn default_snapshot_schema_version() -> u32 {
    1
}

/// `sync_dependency_snapshots` 的结果：本次真正改写了多少份清单。
#[derive(Debug, Clone, Serialize)]
pub struct DependencySyncResult {
    pub mods_updated: usize,
    pub maps_updated: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryBundle {
    pub format: String,
    pub schema_version: u32,
    pub exported_at: String,
    pub repositories: Vec<RepositoryExport>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySource>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportPreview {
    pub valid: bool,
    pub errors: Vec<String>,
    pub additions: Vec<String>,
    pub duplicates: Vec<String>,
    pub conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositorySourceImportResult {
    pub preview: Option<ImportPreview>,
    pub access_error: Option<String>,
    pub builtin_fallback_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateStatus {
    pub repository_id: String,
    pub local_version: Option<String>,
    pub remote_version: Option<String>,
    pub state: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub ok: bool,
    pub message: String,
}

/// 头像请求：一次批量读取时要哪个平台、哪个 owner 的头像。
///
/// 头像由 owner（用户/组织）决定，不是仓库级信息 —— GitHub/Gitee 都没有
/// 独立的仓库头像。同一 owner 下的多个仓库共用一张图，前端按
/// `{provider}:{owner}` 去重后再传上来。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AvatarRequest {
    pub provider: Provider,
    pub owner: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BepInExInstallResult {
    #[serde(flatten)]
    pub operation: OperationResult,
    pub wine_override_applied: bool,
    pub wine_override_error: Option<String>,
    pub macos_overlay_repaired: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RustLog {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateInfo {
    pub version: String,
    pub current_version: String,
    pub body: Option<String>,
    pub date: Option<String>,
    pub download_size: Option<u64>,
    #[serde(default)]
    pub manifest_url: String,
    #[serde(default)]
    pub installer_url: Option<String>,
    #[serde(default)]
    pub installer_signature: String,
    #[serde(default)]
    pub installer_name: String,
    #[serde(default)]
    pub installer_type: String,
    #[serde(default)]
    pub provider: Option<Provider>,
    #[serde(default)]
    pub source_channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateChannelStatus {
    pub id: String,
    pub provider: Provider,
    pub mode: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateChannelEvent {
    pub region: String,
    pub channel: AppUpdateChannelStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdatePhaseEvent {
    pub region: String,
    pub phase: String,
    pub message: Option<String>,
    pub update: Option<AppUpdateInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppUpdateCheckResult {
    pub update: Option<AppUpdateInfo>,
    pub channels: Vec<AppUpdateChannelStatus>,
    pub all_failed: bool,
    pub region: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardData {
    pub config: PublicConfig,
    pub game: GameInfo,
    pub mods: Vec<InstalledMod>,
    pub updates: Vec<UpdateStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingsPatch {
    pub ui_zoom: Option<f64>,
    pub hide_unidentified_mods: Option<bool>,
    /// 删除 MOD 时先移入回收站（默认 true）。关闭即直接删除，不可恢复。
    pub recycle_deleted_mods: Option<bool>,
    pub game_path: Option<String>,
    pub game_runtime: Option<GameRuntime>,
    pub official_api_only: Option<bool>,
    pub timeout_seconds: Option<u64>,
    pub download_threads: Option<u32>,
    /// MOD 元数据并行读取上限；`0` = 全部 CPU 核心。超出核心数由后端夹紧。
    pub mod_metadata_concurrency: Option<u32>,
    /// 地图 `info` 并行解析上限；`0` = 全部 CPU 核心。
    pub map_info_concurrency: Option<u32>,
    /// 这次「已经看过」的教程步骤 id；后端把它并进配置里的集合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutorial_watched_steps: Option<Vec<String>>,
    /// 只用内置仓库源（不访问远端 raw 入口）。
    pub builtin_sources_only: Option<bool>,
    /// 只用内置地图源（不影响仓库源，反之亦然）。
    pub builtin_map_sources_only: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubProxyPatch {
    pub release_proxies: Vec<GitHubProxy>,
    pub raw_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub github_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub gitee_api_proxies: Vec<GitHubProxy>,
    #[serde(default)]
    pub avatar_proxies: Vec<GitHubProxy>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenPatch {
    pub github_token: String,
    pub gitee_token: String,
    /// 显式清除已保存的 Token（留空字符串表示「不修改」）。
    #[serde(default)]
    pub clear: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepositoryPatch {
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub name_i18n: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub group_i18n: Option<BTreeMap<String, String>>,
    pub provider: Provider,
    pub username: String,
    pub repository: String,
    #[serde(default)]
    pub download_mode: RepositoryDownloadMode,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub region: Option<String>,
    /// 支持的桌面平台（`windows` / `macos`）；`None` 表示全平台可用。
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(default)]
    pub channel: Channel,
    pub asset_rules: Vec<AssetRule>,
    pub install_target: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub note_i18n: Option<BTreeMap<String, String>>,
    pub plugin_guid: String,
    #[serde(default)]
    pub local_product_name: String,
    #[serde(default)]
    pub local_original_filename: String,
    /// 与其它插件的关系；`None` 表示本次编辑不修改关系表。
    #[serde(default)]
    pub relations: Option<Vec<RepositoryRelation>>,
    /// 按 Release 覆盖关系表的规则；`None` 表示本次编辑不修改。
    #[serde(default)]
    pub release_relations: Option<Vec<ReleaseRelationOverride>>,
    #[serde(default)]
    pub version_regex: String,
    #[serde(default = "default_max_release_count")]
    pub max_release_count: u32,
    #[serde(default = "default_download_threads")]
    pub download_threads: u32,
    #[serde(default)]
    pub github_token: String,
    #[serde(default)]
    pub gitee_token: String,
    #[serde(default = "default_true")]
    pub official_api_only: bool,
    /// 是否展示并刷新这个仓库（语义同 RepositoryConfig::enabled）。
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}
fn default_download_threads() -> u32 {
    0
}

/// 一个地图仓库的界面模型。
///
/// 与 `MapRepositoryConfig` 的区别：只带界面要显示的内容（含最新 tag 与缓存
/// 状态）和 `asset_rules`，不带 `download_threads` 这类纯内部参数。
///
/// `asset_rules` 必须暴露：详情面板要标出「安装时会选哪个资产」，而自动匹配
/// 由后端 `select_install_asset` 完成，前端只有拿到同一份规则才能镜像结论。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapInfo {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub name_i18n: BTreeMap<String, String>,
    #[serde(default)]
    pub group: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub group_i18n: BTreeMap<String, String>,
    pub provider: Provider,
    pub username: String,
    pub repository: String,
    /// 绑定的本地关卡集名（info 里的 `levelSetName`）。
    pub level_set_name: String,
    /// 绑定的本地中文名（info 里的 `levelSetNameZH`），可选。
    ///
    /// 英文集名与作者都相同的两张地图靠前两者分不开（实测「胡闹糕中」与
    /// 「胡闹达学」都写 GAODA / GAODA），前端判「已安装 / 有新版」时必须一起看。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub level_set_name_zh: String,
    /// 绑定的本地作者（info 里的 `author`）。
    pub author: String,
    /// 下载资产规则（与 `RepositoryConfig.asset_rules` 同语义）。
    ///
    /// 地图只分发 ZIP，资产却是作者随意命名的，所以必须把规则交给前端，
    /// 让详情面板用同一套 exact/glob/regex 判定标出会被下载的那个资产。
    #[serde(default)]
    pub asset_rules: Vec<AssetRule>,
    #[serde(default)]
    pub note: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub note_i18n: BTreeMap<String, String>,
    pub enabled: bool,
    pub builtin: bool,
    /// 所属地图源：ID 与昵称（多语言表优先，单语言镜像兜底）。
    ///
    /// 地图源树不落盘（源只是归类方式，见 `map_sources`），归属与源名随每次
    /// 加载写进条目，这里原样透传 —— 否则前端只能把「哪张图属于哪个源」
    /// 重新猜一遍，页面的三层结构也就无从建立。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source_name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_name_i18n: BTreeMap<String, String>,
    /// 作者指定的「这就是最新版」的**地图包版本**；没写时为 `None`。
    ///
    /// 前端判定改的是**语义**而不是比较方式：写了它就不再比大小，只比等值
    /// （资产名里的 `0.7` 与 `0.69` 用数字比较会得出反的结论）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_mapset_version: Option<String>,
    /// 最新 Release 里 `info` 文件内的版本号；源没声明时为 `None`。
    ///
    /// 前端拿它与本地扫出的 `LevelSetInfo.version` 比：相等即「装的就是这一版」，
    /// 作者忘没忘改 info 里的版本号都不影响结论。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_info_version: Option<String>,
    /// 缓存里最新的 Release tag；没刷新过时为 `None`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_tag: Option<String>,
    /// 缓存里最新的那一条 Release（卡片复用 MOD 卡片时直接喂给版本/下载量）。
    ///
    /// 只给界面要显示的字段，和 `latest_tag` 冗余但语义不同：卡片需要
    /// `assets` 算下载量、需要 `prerelease` 决定版本标记样式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub latest_release: Option<ReleaseInfo>,
    /// 缓存里的全部 Release，供详情抽屉选版本（装旧版本回滚）。
    ///
    /// 与 `latest_release` 冗余但用途不同：那个回答「最新是哪一版」，这里回答
    /// 「能装哪几版」。只给最新一条时，详情里的版本下拉没有内容可列。
    ///
    /// 不 skip：前端类型是必填数组（`cached_releases: ReleaseInfo[]`），没刷新过
    /// 时也要拿到 `[]` 而不是 `undefined`。
    #[serde(default)]
    pub cached_releases: Vec<ReleaseInfo>,
    /// 当前地区可安装的 Release 数（与 `cached_release_count` 同源）。
    #[serde(default)]
    pub cached_release_count: usize,
    /// Release 缓存时间（Unix 毫秒），用于界面提示「几分钟前」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// 仓库声明的全部关系（依赖 / 排斥 / 兼容，原始形态）。
    ///
    /// `required_frameworks` 只投影「依赖」给框架提示用；依赖表要按原样展示，
    /// 所以冲突/兼容、发布线、可选标记也必须给前端。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub relations: Vec<RepositoryRelation>,
    /// 按地图版本覆盖 `relations` 的表（前端按「要装/已装版本」选一张）。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub release_relations: Vec<ReleaseRelationOverride>,
    /// 需要的框架与版本要求（来自 `relations`），供界面提示升级框架。
    ///
    /// 只有**声明**：`installed_version` 恒为 `None`，后端不再为此扫描本地插件。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required_frameworks: Vec<RequiredFramework>,
    /// 兼容字段，当前恒为空。
    ///
    /// 依赖判定已统一到前端 `modRelations.ts`（输入是 Dashboard 已扫好的本地
    /// MOD 列表），后端不再保留第二份实现。字段留着只为旧前端读取时不崩。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unmet_frameworks: Vec<RequiredFramework>,
}

/// 地图依赖的一个框架组件。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequiredFramework {
    pub guid: String,
    /// 版本区间表达式（与 `RepositoryRelation.version` 同一语法）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// 本地已装版本（未装时为空），让界面能说清「已装 0.9.9，需要 >=0.10.0」。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
}

/// 前端 `ModPreviewRole = 'mod' | 'dependency'` 与 `ZipLevelSetPlugin` 的字段名
/// 都是 IPC 契约：这里直接钉住序列化结果，改枚举或字段名时先被这条用例拦下。
#[cfg(test)]
mod dll_role_tests {
    use super::*;

    #[test]
    fn dll_role_and_plugin_fields_keep_their_json_names() {
        assert_eq!(serde_json::to_string(&DllRole::Mod).unwrap(), "\"mod\"");
        assert_eq!(
            serde_json::to_string(&DllRole::Dependency).unwrap(),
            "\"dependency\""
        );

        let plugin = ZipLevelSetPlugin {
            path: "pkg/plugins/fix/Fix.dll".into(),
            file_name: "Fix.dll".into(),
            install_path: None,
            info: DllPluginInfo {
                role: DllRole::Dependency,
                name: Some("Fix".into()),
                guid: None,
                version: None,
                title: None,
                product_name: None,
                company: None,
                original_filename: None,
            },
        };
        let json = serde_json::to_value(&plugin).unwrap();
        assert_eq!(json["role"], "dependency");
        assert_eq!(json["path"], "pkg/plugins/fix/Fix.dll");
        assert_eq!(json["file_name"], "Fix.dll");
        assert_eq!(json["name"], "Fix");
        // 读不出的字段直接省略，前端按可选字段处理（不是空串/null）
        assert!(json.get("guid").is_none());
        assert!(json.get("title").is_none());
    }
}
