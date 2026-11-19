//! 关卡集目录的外部变更监听。
//!
//! 地图管理页的列表来自磁盘扫描，但用户会直接在资源管理器里增删 `levels/`
//! 下的目录、替换 `info`；只靠页面自己的重读看不到这些变化。这里用系统的
//! 目录变更通知（Windows `ReadDirectoryChangesW`、macOS FSEvents）代替轮询：
//! 轮询要么太慢（1 秒级）要么太贵（每次都要重扫全部 `info`）。
//!
//! 事件合并路径后将变化地图集加入后台转换队列，并提醒前端重读；扫描口径（两个根的第一层）、
//! 排序与身份判定都在 `level_sets` 里，前端重读一次就能拿到完整一致的结果。

use crate::directory_watch::DirectoryWatcher;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

const LEVEL_SETS_CHANGED_EVENT: &str = "level-sets-changed";

/// AppState owns the same shared OS watcher used by DIYChef files.
#[derive(Default)]
pub struct LevelWatcher {
    directory: DirectoryWatcher<()>,
}

impl LevelWatcher {
    pub fn watch(&self, app: &AppHandle, roots: Vec<PathBuf>) {
        if roots.is_empty() {
            self.directory.stop();
            return;
        }
        let emit_app = app.clone();
        self.directory.watch(
            (),
            roots.clone(),
            &[],
            |_| true,
            move |paths| {
                let app = emit_app.clone();
                let roots = roots.clone();
                tauri::async_runtime::spawn_blocking(move || {
                    if let Some(state) = app.try_state::<crate::app_state::AppState>() {
                        let names = changed_sets(&roots, &paths);
                        crate::level_sets::recompress::enqueue_installed(
                            &state,
                            &app,
                            names.as_deref(),
                        );
                        // Rebuild away from the owned notification thread if a root appeared.
                        crate::level_sets::recompress::watch_selected(&state, &app);
                    }
                });
                if let Err(error) = emit_app.emit(LEVEL_SETS_CHANGED_EVENT, ()) {
                    log::debug!("发送关卡集变化事件失败：{error}");
                }
            },
        );
    }
}

/// Only changed top-level sets are queued; ancestor notifications recheck all candidates.
fn changed_sets(roots: &[PathBuf], paths: &[PathBuf]) -> Option<Vec<String>> {
    let mut names = std::collections::BTreeSet::new();
    for path in paths {
        let mut matched = false;
        for root in roots {
            if let Ok(relative) = path.strip_prefix(root) {
                if let Some(std::path::Component::Normal(name)) = relative.components().next() {
                    names.insert(name.to_string_lossy().into_owned());
                    matched = true;
                } else {
                    return None;
                }
            }
        }
        if !matched {
            return None;
        }
    }
    Some(names.into_iter().collect())
}

#[cfg(test)]
mod tests;
