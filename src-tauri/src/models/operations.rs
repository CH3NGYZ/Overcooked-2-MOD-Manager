use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BepInExInstallResult {
    #[serde(flatten)]
    pub operation: OperationResult,
    pub wine_override_applied: bool,
    pub wine_override_error: Option<String>,
    pub macos_overlay_repaired: bool,
}
