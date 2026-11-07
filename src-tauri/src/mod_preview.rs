//! MOD 安装预览：本地文件 / 在线直链在真正安装之前的「解析 + 确认」阶段。
//!
//! 流程统一为两步，本地与在线共用同一套逻辑：
//!   1. `analyze`：把源文件放到暂存目录，解析里面每个 DLL 的元数据；
//!      在线源在这一步下载，因此「下载只发生一次」。
//!   2. `confirm`：用户在预览里确认后，从暂存目录取出文件走正式安装；
//!      `cancel` 或超时则删除暂存。
//!
//! 判定规则（单 mod 还是包）只看 **ZIP 内的 DLL 数量**：
//!   - 0 个：按包处理，没有可预览的 mod；
//!   - 1 个：单 mod，展示该 DLL 的详情；
//!   - >= 2 个：包，逐个解析，能读出 BepInPlugin 的列为 mod，
//!     读不出的（纯依赖，如 LevelEditorStub）列为一类「依赖」条目。
//! 也就是说「能否解析出 BepInPlugin」只决定它在预览里显示为 mod 还是依赖，
//! 不决定这个 ZIP 是不是包 —— DIYLevel 那种「一个真插件 + 一个纯依赖」
//! 必须被认成包，而不是单 mod。

mod analysis;
pub(crate) mod commands;
mod conflicts;
mod model;
mod staging;

// Keep the pre-split module paths available; Tauri registration targets the concrete command module.
#[allow(unused_imports)]
pub use commands::{analyze_mod_source, cancel_mod_preview, confirm_mod_install};
#[allow(unused_imports)]
pub use conflicts::find_install_conflicts;
#[allow(unused_imports)]
pub use model::{ModPreview, ModPreviewEntry, ModSource, ModSourceKind, PreviewOrigin};
#[allow(unused_imports)]
pub(crate) use staging::cleanup_stale_previews;
pub(crate) use staging::stage_downloaded_file;

#[cfg(test)]
mod tests;
