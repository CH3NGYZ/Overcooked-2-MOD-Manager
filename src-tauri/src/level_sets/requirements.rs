//! 本地 requires.txt 到统一关系模型的适配。

use crate::mods::plugin_target::RUNTIME_WLOADER_GUID;
use std::fs;
use std::path::Path;

/// 关卡集目录里声明「外部 runtime 最低版本」的文件。
///
/// 由关卡集导出器写出，内容是**纯版本号**（实测 `3.10.2`、`3.8.8`）。
pub(super) const REQUIRES_FILE: &str = "requires.txt";

/// 读 `requires.txt`：第一行有效内容就是要求的外部 runtime 最低版本。
///
/// 允许空行与 `#` 注释行（同目录的 `stub_levels.txt` 就带说明行）。文件缺失、
/// 全是空行注释、或读不出来时返回 `None` —— 那种情况下界面不显示关系卡片。
pub(super) fn read_required_runtime(directory: &Path) -> Option<String> {
    let text = fs::read_to_string(directory.join(REQUIRES_FILE)).ok()?;
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
}

/// 把 `requires.txt` 的版本号做成一条 `requires` 关系。
///
/// guid 与地图源里写的完全一致，所以依赖表判定、缺依赖提示、「去安装」跳转
/// 全部复用已有管线，不需要为本地包另写一套。
pub fn runtime_relation(min_version: &str) -> Option<crate::models::RepositoryRelation> {
    let min_version = min_version.trim();
    if min_version.is_empty() {
        return None;
    }
    Some(crate::models::RepositoryRelation {
        kind: crate::models::RelationKind::Requires,
        guid: RUNTIME_WLOADER_GUID.to_string(),
        version: if min_version.trim().is_empty() {
            String::new()
        } else {
            format!(">={}", min_version.trim())
        },
        prerelease: None,
        optional: false,
    })
}
