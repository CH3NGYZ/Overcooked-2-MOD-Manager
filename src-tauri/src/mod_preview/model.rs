use crate::models::{DllRole, InstallConflict};
use serde::{Deserialize, Serialize};

/// 预览里的一行（一个 DLL）。
#[derive(Debug, Clone, Serialize)]
pub struct ModPreviewEntry {
    /// 相对安装包根目录的路径，用于展示。
    pub path: String,
    pub role: DllRole,
    /// 展示名称：BepInPlugin 名称 → PE 产品名 → 文件名。
    pub name: Option<String>,
    pub guid: Option<String>,
    pub version: Option<String>,
    pub title: Option<String>,
    pub product_name: Option<String>,
    pub company: Option<String>,
    pub copyright: Option<String>,
    pub original_filename: Option<String>,
}

/// 解析结果。
#[derive(Debug, Clone, Serialize)]
pub struct ModPreview {
    /// 后续 `confirm` / `cancel` 用这个 id 找到暂存目录。
    pub preview_id: String,
    /// `single` 表示单 mod，`package` 表示包含多个 DLL 的包。
    pub kind: &'static str,
    /// 安装后作为 MOD 包/单个 MOD 的名称。
    pub package_name: String,
    /// 源文件名（本地路径的文件名，或直链 URL 的最后一段）。
    pub file_name: String,
    /// 包内 DLL 总数。
    pub total_dlls: usize,
    /// 能读出 BepInPlugin 的数量。
    pub mod_count: usize,
    /// 纯依赖数量。
    pub dependency_count: usize,
    /// 逐 DLL 的解析结果（按包内顺序）。
    pub entries: Vec<ModPreviewEntry>,
    /// 本地已有的同身份 MOD（安装前要卸载的那些）。
    ///
    /// 只在 `analyze_mod_source` / `stage_downloaded_file` 里填；安装预览模型
    /// 直接复用，前端据此在确认界面先提示「将卸载本地旧副本」。
    #[serde(default)]
    pub conflicts: Vec<InstallConflict>,
    /// 来源：`repository` 表示从在线仓库资产下载，`manual` 表示本地文件或直链。
    /// 界面据此决定是否展示 Release 日志区块。
    pub origin: PreviewOrigin,
    /// 仓库安装时本次 Release 的版本标签（如 `v1.2.3`）；手动来源为空。
    pub release_tag: String,
    /// 仓库安装时本次 Release 的提交日志（Markdown）；手动来源为空。
    pub release_body: String,
}

/// 预览内容的来源，决定界面展示哪些来源专属信息。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PreviewOrigin {
    Repository,
    Manual,
}

/// 源类型：本地路径或在线直链。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModSourceKind {
    Local,
    Url,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModSource {
    pub kind: ModSourceKind,
    pub value: String,
}
