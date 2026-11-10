//! 关卡集目录的外部变更监听。
//!
//! 地图管理页的列表来自磁盘扫描，但用户会直接在资源管理器里增删 `levels/`
//! 下的目录、替换 `info`；只靠页面自己的重读看不到这些变化。这里用系统的
//! 目录变更通知（Windows `ReadDirectoryChangesW`、macOS FSEvents）代替轮询：
//! 轮询要么太慢（1 秒级）要么太贵（每次都要重扫全部 `info`）。
//!
//! 事件合并路径后将变化地图集加入后台转换队列，并提醒前端重读；扫描口径（两个根的第一层）、
//! 排序与身份判定都在 `level_sets` 里，前端重读一次就能拿到完整一致的结果。

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// 关卡集列表可能已经变化。
const LEVEL_SETS_CHANGED_EVENT: &str = "level-sets-changed";

/// 静默窗口：最后一个事件之后安静这么久才通知一次。
///
/// 装一个地图集会写上千个文件；按事件逐条通知会让前端反复重读，而重读本身
/// 要解压 `info`（冷扫描实测秒级）。等写入停下来再通知一次。
const QUIET_WINDOW: Duration = Duration::from_millis(600);

/// 关卡集目录监听器。
///
/// 由 `AppState` 以 `Arc` 持有，生命周期跟应用一致。`watch` 幂等：监听目标
/// 没变就直接返回，变了（第一次调用、用户改了游戏目录、目标目录刚出现）才
/// 重建。
#[derive(Default)]
pub struct LevelWatcher {
    state: Mutex<Option<WatchState>>,
}

impl LevelWatcher {
    /// 让监听目标等于 `roots`（启用中的 `levels/` 与停用根）。
    ///
    /// 目标与当前完全一致时什么都不做；否则先停掉旧的 watcher 与线程，再按
    /// 新目标注册。游戏目录被用户改掉时 `roots` 会变，这里自动跟上。
    pub fn watch(&self, app: &AppHandle, roots: Vec<PathBuf>) {
        let plan = watch_plan(&roots);
        /*
         * 锁中毒不该让监听彻底失效：这里保存的只是句柄，没有「写了一半」的
         * 数据，直接取回内部值继续用。
         */
        let mut guard = match self.state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                log::warn!("关卡集监听锁已中毒，继续使用现有状态");
                poisoned.into_inner()
            }
        };
        if guard.as_ref().is_some_and(|state| state.plan == plan) {
            return;
        }
        if let Some(previous) = guard.take() {
            previous.stop();
        }
        // 没有可监听的位置（游戏目录整个不存在）：不建线程，等下次有目录再说
        if plan.is_empty() {
            return;
        }
        let emit_app = app.clone();
        *guard = build_watch_state(plan, move |paths| {
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
                    // A missing watched root may have appeared; rebuild off the watcher thread.
                    state.level_watch.watch(&app, roots);
                }
            });
            if let Err(error) = emit_app.emit(LEVEL_SETS_CHANGED_EVENT, ()) {
                log::debug!("发送关卡集变化事件失败：{error}");
            }
        });
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

/// 实际注册的一组监听目标。
struct WatchState {
    /// 期望的目标，用于幂等比较。存的是「算出来的计划」而不是调用方传进来的
    /// `roots`：路径不存在时会退化成祖先，那份降级也要参与比较，目标目录
    /// 出现后才能升级回递归监听。
    plan: Vec<WatchTarget>,
    /*
     * 字段本身不读，只靠 drop：watcher 内部持有事件回调，回调里握着发送端；
     * watcher 一 drop，发送端随之释放，监听线程的 `recv_timeout` 立刻返回
     * 断开，线程退出。
     */
    _watcher: RecommendedWatcher,
    thread: JoinHandle<()>,
}

impl WatchState {
    /// 停掉监听：先放掉 watcher（关通道），再等线程自己退出，不留悬空线程。
    fn stop(self) {
        let WatchState {
            _watcher, thread, ..
        } = self;
        drop(_watcher);
        if thread.join().is_err() {
            log::debug!("关卡集监听线程退出时 panic");
        }
    }
}

/// 一个监听目标：目录 + 是否递归。
#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchTarget {
    path: PathBuf,
    recursive: bool,
}

/// 把 `roots` 翻译成真正能注册的目标。
///
/// 两个根都可能还不存在，而直接监听不存在的路径会失败。这里**不**
/// `create_dir_all` 再监听：`get_level_sets` 是只读命令（不创建 `levels/`），
/// 替用户造出 `BepInEx/plugins/OC2DIYLevel/levels` 会把一个没装框架的游戏目录
/// 改成半成品。所以目标不存在时退到最近一个存在的祖先、用非递归模式只盯
/// 「下一层出现」；目标一旦出现就会产生事件，前端重读时再次调用 `watch`，
/// 计划随之升级成对真实目录的递归监听（见 `appearing_target_upgrades_the_plan`）。
fn watch_plan(roots: &[PathBuf]) -> Vec<WatchTarget> {
    let mut targets: BTreeMap<PathBuf, bool> = BTreeMap::new();
    for root in roots {
        let Some((path, recursive)) = watch_target_for(root) else {
            continue;
        };
        // 同一个路径可能既是某个根、又是另一个根缺失时的祖先：递归优先
        let entry = targets.entry(path).or_insert(false);
        *entry = *entry || recursive;
    }
    targets
        .into_iter()
        .map(|(path, recursive)| WatchTarget { path, recursive })
        .collect()
}

fn watch_target_for(root: &Path) -> Option<(PathBuf, bool)> {
    if root.is_dir() {
        return Some((root.to_path_buf(), true));
    }
    let mut ancestor = root.parent();
    while let Some(path) = ancestor {
        if path.is_dir() {
            return Some((path.to_path_buf(), false));
        }
        ancestor = path.parent();
    }
    None
}

/// 建 watcher、注册目标、起监听线程。
///
/// 任何一步失败都只记日志并返回 `None`：监听是增强能力，坏了只是列表不再
/// 自动刷新，不能影响扫描本身；下次 `watch` 调用会重试。
fn build_watch_state(
    plan: Vec<WatchTarget>,
    emit: impl FnMut(Vec<PathBuf>) + Send + 'static,
) -> Option<WatchState> {
    let (sender, receiver) = mpsc::channel::<Vec<PathBuf>>();
    let mut watcher =
        match notify::recommended_watcher(move |result: notify::Result<notify::Event>| match result
        {
            Ok(event) => {
                if affects_level_list(&event.kind) {
                    // 线程已经退出（watcher 被 drop）时发送失败，忽略即可
                    let _ = sender.send(event.paths);
                }
            }
            Err(error) => log::debug!("关卡集目录监听错误：{error}"),
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                log::warn!("创建关卡集目录监听失败：{error}");
                return None;
            }
        };
    let mut registered = 0_usize;
    for target in &plan {
        let mode = if target.recursive {
            RecursiveMode::Recursive
        } else {
            RecursiveMode::NonRecursive
        };
        match watcher.watch(&target.path, mode) {
            Ok(()) => {
                registered += 1;
                log::debug!(
                    "关卡集目录监听：{}（{}）",
                    target.path.display(),
                    if target.recursive {
                        "递归"
                    } else {
                        "非递归"
                    }
                );
            }
            Err(error) => log::warn!(
                "关卡集目录监听注册失败：{}，原因={error}",
                target.path.display()
            ),
        }
    }
    if registered == 0 {
        log::warn!("关卡集目录监听没有可用目标，跳过");
        return None;
    }
    let thread = match std::thread::Builder::new()
        .name("level-set-watch".into())
        .spawn(move || debounce_loop(receiver, QUIET_WINDOW, emit))
    {
        Ok(thread) => thread,
        Err(error) => {
            log::warn!("启动关卡集目录监听线程失败：{error}");
            return None;
        }
    };
    log::info!("关卡集目录监听已启动：目标数={registered}");
    Some(WatchState {
        plan,
        _watcher: watcher,
        thread,
    })
}

/// 只关心会改变列表的事件。
///
/// `Access` 是「有人读了文件」，与列表无关，必须忽略（扫描自己就会读 `info`，
/// 不忽略会自我触发）。其余（Create/Remove/Rename/Modify，以及平台给的
/// Any/Other）都可能改变第一层目录或 `info` 内容，一律当作需要重读。
fn affects_level_list(kind: &EventKind) -> bool {
    !matches!(kind, EventKind::Access(_))
}

/// 防抖循环：把事件吃干净，安静 `window` 后通知一次。
///
/// 发送端全部释放（watcher 已 drop）时 `recv_timeout` 返回断开，循环退出，
/// 线程不残留。
fn debounce_loop(
    receiver: Receiver<Vec<PathBuf>>,
    window: Duration,
    mut notify: impl FnMut(Vec<PathBuf>),
) {
    let mut pending = false;
    let mut paths = std::collections::BTreeSet::new();
    loop {
        match receiver.recv_timeout(window) {
            Ok(changed) => {
                pending = true;
                paths.extend(changed);
            }
            Err(RecvTimeoutError::Timeout) => {
                if pending {
                    pending = false;
                    notify(std::mem::take(&mut paths).into_iter().collect());
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    log::debug!("关卡集目录监听线程退出");
}

#[cfg(test)]
mod tests;
