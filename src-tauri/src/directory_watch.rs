//! Shared OS directory notifications, bounded batches and owned thread cleanup.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Wait for a burst of copy/install operations to settle before one read-back.
pub(crate) const QUIET_WINDOW: Duration = Duration::from_millis(600);

/// Identity includes the domain key: a path can belong to a new target or scan
/// generation whose callback must replace the previous callback.
pub(crate) struct DirectoryWatcher<K> {
    state: Mutex<Option<Registration<K>>>,
}

impl<K> Default for DirectoryWatcher<K> {
    fn default() -> Self {
        Self {
            state: Mutex::new(None),
        }
    }
}

struct Registration<K> {
    key: K,
    roots: Vec<PathBuf>,
    watch: WatchState,
}

impl<K: Eq> DirectoryWatcher<K> {
    /// Failed registration is optional enhancement failure, never a read error.
    /// Callbacks must not call `watch`/`stop` synchronously on this same instance.
    pub(crate) fn watch(
        &self,
        key: K,
        roots: Vec<PathBuf>,
        accept: impl Fn(&Event) -> bool + Send + 'static,
        emit: impl FnMut(Vec<PathBuf>) + Send + 'static,
    ) {
        let plan = watch_plan(&roots);
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if guard.as_ref().is_some_and(|current| {
            current.key == key && current.roots == roots && current.watch.plan == plan
        }) {
            return;
        }
        // Drop joins the old worker before a replacement can emit a new key.
        drop(guard.take());
        if let Some(watch) = build_watch_state(plan, accept, emit) {
            *guard = Some(Registration { key, roots, watch });
        }
    }

    pub(crate) fn stop(&self) {
        let mut guard = self.state.lock().unwrap_or_else(|e| e.into_inner());
        drop(guard.take());
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WatchTarget {
    path: PathBuf,
    recursive: bool,
}

/// Missing roots use their nearest existing ancestor without creating folders.
/// A later domain read re-registers the now-existing recursive target.
fn watch_plan(roots: &[PathBuf]) -> Vec<WatchTarget> {
    let mut targets: BTreeMap<PathBuf, bool> = BTreeMap::new();
    for root in roots {
        let Some((path, recursive)) = watch_target_for(root) else {
            continue;
        };
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

struct WatchState {
    plan: Vec<WatchTarget>,
    watcher: Option<RecommendedWatcher>,
    thread: Option<JoinHandle<()>>,
    active: Arc<AtomicBool>,
}

impl Drop for WatchState {
    fn drop(&mut self) {
        self.active.store(false, Ordering::Release);
        // notify owns the last sender. Closing it wakes the worker immediately;
        // joining prevents an already-running emit escaping after stop returns.
        drop(self.watcher.take());
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                log::debug!("目录监听线程退出时 panic");
            }
        }
    }
}

fn build_watch_state(
    plan: Vec<WatchTarget>,
    accept: impl Fn(&Event) -> bool + Send + 'static,
    mut emit: impl FnMut(Vec<PathBuf>) + Send + 'static,
) -> Option<WatchState> {
    if plan.is_empty() {
        return None;
    }
    let (sender, receiver) = mpsc::channel::<Vec<PathBuf>>();
    let mut watcher =
        match notify::recommended_watcher(move |result: notify::Result<Event>| match result {
            Ok(event) if affects_contents(&event.kind) && accept(&event) => {
                let _ = sender.send(event.paths);
            }
            Ok(_) => {}
            Err(error) => log::debug!("目录监听错误：{error}"),
        }) {
            Ok(watcher) => watcher,
            Err(error) => {
                log::warn!("创建目录监听失败：{error}");
                return None;
            }
        };
    let mut registered = 0;
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
                    "目录监听：{}（{}）",
                    target.path.display(),
                    if target.recursive {
                        "递归"
                    } else {
                        "非递归"
                    }
                );
            }
            Err(error) => log::warn!("目录监听注册失败：{}，原因={error}", target.path.display()),
        }
    }
    if registered == 0 {
        return None;
    }
    let active = Arc::new(AtomicBool::new(true));
    let running = active.clone();
    let thread = match std::thread::Builder::new()
        .name("directory-watch".into())
        .spawn(move || {
            debounce_loop(receiver, QUIET_WINDOW, move |paths| {
                if running.load(Ordering::Acquire) {
                    emit(paths);
                }
            })
        }) {
        Ok(thread) => thread,
        Err(error) => {
            log::warn!("启动目录监听线程失败：{error}");
            return None;
        }
    };
    log::debug!("目录监听已启动：目标数={registered}");
    Some(WatchState {
        plan,
        watcher: Some(watcher),
        thread: Some(thread),
        active,
    })
}

/// Reads from the scanner itself must never cause another scan.
fn affects_contents(kind: &EventKind) -> bool {
    !matches!(kind, EventKind::Access(_))
}

fn debounce_loop(
    receiver: Receiver<Vec<PathBuf>>,
    window: Duration,
    mut notify: impl FnMut(Vec<PathBuf>),
) {
    let mut pending = false;
    let mut paths = BTreeSet::new();
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
}

#[cfg(test)]
mod tests;
