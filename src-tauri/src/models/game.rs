use serde::{Deserialize, Serialize};

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
    /// 库里找（见 `game/discovery.rs::find_game_path`）—— 一个「关掉就永远找不到游戏」的
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

#[cfg(test)]
mod game_runtime_tests;
