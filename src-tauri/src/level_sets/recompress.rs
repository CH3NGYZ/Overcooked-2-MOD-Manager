//! Background map-only conversion queue. Exit cancels queued work and joins active transactions.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

#[derive(Clone, Default, serde::Serialize)]
pub(crate) struct Progress {
    pub done: usize,
    pub total: usize,
    pub converted: usize,
    pub skipped: usize,
    pub failed: usize,
    pub percent: usize,
    pub current: String,
    pub running: bool,
    pub sets_done: usize,
    pub sets_total: usize,
    pub set_name: String,
    pub files_done: usize,
    pub files_total: usize,
}
mod scheduler;
use scheduler::Lifecycle;
static LIFECYCLE: OnceLock<(Mutex<Lifecycle>, std::sync::Condvar)> = OnceLock::new();
static GENERATION: AtomicU64 = AtomicU64::new(0);
static CANCEL: AtomicBool = AtomicBool::new(false);
// Unknown startup state is conservative until the initial game probe completes.
static GAME_RUNNING: AtomicBool = AtomicBool::new(true);
fn lifecycle() -> &'static (Mutex<Lifecycle>, std::sync::Condvar) {
    LIFECYCLE.get_or_init(Default::default)
}

pub(crate) fn set_game_running(running: bool) -> Option<Progress> {
    let mut queue = lifecycle().0.lock().unwrap_or_else(|e| e.into_inner());
    let changed = GAME_RUNNING.swap(running, Ordering::SeqCst) != running;
    if running && (changed || !queue.paused) {
        queue.game_started();
        GENERATION.store(queue.generation, Ordering::SeqCst);
    }
    queue.paused = running;
    queue.progress.running = queue.running && !running && !queue.rescan_requested;
    lifecycle().1.notify_all();
    changed.then(|| queue.progress.clone())
}

fn may_convert(generation: u64) -> bool {
    !CANCEL.load(Ordering::SeqCst)
        && !GAME_RUNNING.load(Ordering::SeqCst)
        && GENERATION.load(Ordering::SeqCst) == generation
}

/// Called by the existing game observer, never performs filesystem IO.
pub(crate) fn take_rescan_request() -> bool {
    lifecycle()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take_rescan()
}

pub(crate) fn has_pending_work() -> bool {
    lifecycle()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .running
        || !recovery()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
}

/// Called on a blocking exit worker, never on the window event thread.
pub(crate) fn shutdown() -> Result<(), String> {
    let (lock, wake) = lifecycle();
    let mut state = lock.lock().unwrap_or_else(|e| e.into_inner());
    state.stopping = true;
    CANCEL.store(true, Ordering::SeqCst);
    // Idle workers may be waiting for an active set's follow-up. Cancellation
    // can leave unclaimed files in that set, so completion alone cannot wake them.
    wake.notify_all();
    while state.running {
        state = wake.wait(state).unwrap_or_else(|e| e.into_inner());
    }
    let mut pending = recovery().lock().unwrap_or_else(|e| e.into_inner());
    pending.retain(|item| restore(&item.source, &item.backup, &item.hash).is_err());
    if !pending.is_empty() {
        return Err(format!(
            "{} map files still require recovery; backups retained",
            pending.len()
        ));
    }
    Ok(())
}

struct Job {
    roots: super::LevelSetRoots,
    valid_scan: Arc<AtomicBool>,
    cache: PathBuf,
    set: PathBuf,
    files: Vec<PathBuf>,
    signature: Vec<(PathBuf, u64, Option<std::time::SystemTime>)>,
    operation: Arc<Mutex<()>>,
}

fn fingerprint(
    game: &Path,
    set: &Path,
) -> Result<Vec<(PathBuf, u64, Option<std::time::SystemTime>)>, String> {
    let mut result = Vec::new();
    for relative in super::paths::collect_files(game, set)? {
        let path = game.join(relative);
        let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
        result.push((path, meta.len(), meta.modified().ok()));
    }
    result.sort();
    Ok(result)
}

pub(crate) fn snapshot() -> Progress {
    lifecycle()
        .0
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .progress
        .clone()
}

/// Watch only directories selected by a completed local MOD scan.
pub(crate) fn watch_selected(state: &crate::app_state::AppState, app: &tauri::AppHandle) {
    // Root changes and scan invalidation hold the same lock. A late callback must
    // select and install one current watch plan without a mutation between them.
    let Ok(_operation) = state.operation.lock() else {
        log::warn!("操作锁已中毒，暂不重建关卡集监听");
        return;
    };
    let roots = crate::game::detect_game(&state.config.load_or_default())
        .path
        .and_then(|game| crate::mods::snapshot::level_roots(state, Path::new(&game)).ok())
        .map(|(_, roots)| {
            let mut paths = vec![super::disabled_root(roots.game_root())];
            if let Ok(enabled) = super::levels_root(&roots) {
                paths.insert(0, enabled);
            }
            paths
        })
        .unwrap_or_default();
    state.level_watch.watch(app, roots);
}

/// Completed local scans pass None; installation passes only successful directories.
pub(crate) fn enqueue_installed(
    state: &crate::app_state::AppState,
    app: &tauri::AppHandle,
    selected: Option<&[String]>,
) {
    let config = state.config.load_or_default();
    let info = crate::game::detect_game(&config);
    if let Some(progress) = set_game_running(info.kitchen_running) {
        use tauri::Emitter;
        let _ = app.emit("level-set-compression-progress", progress);
    }
    let generation = GENERATION.load(Ordering::SeqCst);
    if !may_convert(generation) {
        return;
    }
    let Some(game) = info.path else {
        return;
    };
    let game = PathBuf::from(game);
    let Ok((snapshot, roots)) = crate::mods::snapshot::level_roots(state, &game) else {
        return;
    };
    let Ok(valid_scan) = state
        .local_mod_snapshot
        .lock()
        .map_err(|_| ())
        .and_then(|scans| {
            scans
                .lease(snapshot.generation, roots.game_root())
                .map_err(|_| ())
        })
    else {
        return;
    };
    let game = roots.game_root();
    let mut sets = Vec::new();
    let mut map_roots = vec![super::disabled_root(game)];
    if let Ok(enabled) = super::levels_root(&roots) {
        map_roots.insert(0, enabled);
    }
    for root in map_roots {
        let Ok(root) = root.canonicalize() else {
            continue;
        };
        let Ok(entries) = fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(set) = entry.path().canonicalize() else {
                continue;
            };
            if !set.starts_with(&root)
                || !set.is_dir()
                || super::scan::locate_info_file(&set).is_none()
            {
                continue;
            }
            if selected.is_some_and(|names| {
                !names.iter().any(|name| {
                    set.file_name()
                        .is_some_and(|part| part == std::ffi::OsStr::new(name))
                })
            }) {
                continue;
            }
            sets.push(set);
        }
    }
    sets.sort();
    sets.dedup();
    // Enumerate outside the queue lock; no directory IO while workers publish progress.
    let jobs = sets
        .into_iter()
        .filter_map(|set| {
            if !may_convert(generation) || !valid_scan.load(Ordering::SeqCst) {
                return None;
            }
            let signature = fingerprint(&game, &set).ok()?;
            let files = signature
                .iter()
                .map(|(path, _, _)| path.clone())
                .filter(|path| {
                    let mut magic = [0u8; 8];
                    fs::File::open(path)
                        .and_then(|mut file| file.read_exact(&mut magic))
                        .is_ok()
                        && &magic == b"UnityFS\0"
                })
                .collect();
            Some((
                Job {
                    roots: roots.clone(),
                    valid_scan: valid_scan.clone(),
                    cache: state.config.cache_dir().join("level-compression"),
                    set,
                    files,
                    signature: signature.clone(),
                    operation: state.operation.clone(),
                },
                signature,
            ))
        })
        .collect::<Vec<_>>();
    let (lock, _) = lifecycle();
    let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
    if !queue.accepts(generation) || queue.rescan_requested || !valid_scan.load(Ordering::SeqCst) {
        return;
    }
    let workers = crate::concurrency::workers_from(config.map_compression_concurrency);
    let was_running = queue.running;
    for (job, signature) in jobs {
        queue.enqueue(job, signature);
    }
    queue.progress.running = queue.running && !queue.paused;
    lifecycle().1.notify_all();
    if was_running || queue.pending.is_empty() {
        return;
    }
    queue.running = true;
    queue.progress.running = !queue.paused;
    drop(queue);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Emitter;
        struct Finish(bool);
        impl Drop for Finish {
            fn drop(&mut self) {
                if !self.0 {
                    return;
                }
                let (lock, wake) = lifecycle();
                let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
                queue.running = false;
                queue.progress.running = false;
                wake.notify_all();
            }
        }
        let mut finish = Finish(true);
        loop {
            run_workers(workers, &|progress| {
                let _ = app.emit("level-set-compression-progress", progress);
            });
            let mut queue = lifecycle().0.lock().unwrap_or_else(|e| e.into_inner());
            // Admission and going idle share a lock: an enqueue at the boundary cannot be lost.
            if !queue.stopping && !queue.paused && !queue.pending.is_empty() {
                continue;
            }
            finish.0 = false;
            queue.running = false;
            queue.progress.running = false;
            lifecycle().1.notify_all();
            let progress = queue.progress.clone();
            drop(queue);
            let _ = app.emit("level-set-compression-progress", progress);
            break;
        }
    });
}

struct Recovery {
    source: PathBuf,
    backup: PathBuf,
    hash: Vec<u8>,
}
static RECOVERY: OnceLock<Mutex<Vec<Recovery>>> = OnceLock::new();
fn recovery() -> &'static Mutex<Vec<Recovery>> {
    RECOVERY.get_or_init(Default::default)
}
fn restore(path: &Path, backup: &Path, expected: &[u8]) -> Result<(), String> {
    fs::copy(backup, path).map_err(|e| e.to_string())?;
    fs::OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    if hash(path)? != expected {
        return Err("rollback hash mismatch".into());
    }
    Ok(())
}
fn hash(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        digest.update(&buf[..n]);
    }
    Ok(digest.finalize().to_vec())
}
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())
}

/// Data-root staging may be on another volume: copy under the operation lock,
/// verify destination bytes, and retain the verified backup until commit.
fn replace(
    path: &Path,
    output: &Path,
    backup: &Path,
    source_hash: &[u8],
    output_hash: &[u8],
    should_continue: &dyn Fn() -> bool,
) -> Result<bool, String> {
    if !should_continue() {
        return Ok(false);
    }
    if hash(path)? != source_hash {
        return Err("source changed before replacement".into());
    }
    if !should_continue() {
        return Ok(false);
    }
    let result: Result<(), String> = (|| {
        let mut input = fs::File::open(output).map_err(|e| e.to_string())?;
        let mut target = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        std::io::copy(&mut input, &mut target)
            .and_then(|_| target.sync_all())
            .map_err(|e| e.to_string())?;
        drop(target);
        if hash(path)? != output_hash {
            return Err("replacement hash mismatch".into());
        }
        Ok(())
    })();
    if let Err(error) = result {
        let rollback = restore(path, backup, source_hash);
        if rollback.is_err() {
            recovery()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(Recovery {
                    source: path.to_owned(),
                    backup: backup.to_owned(),
                    hash: source_hash.to_vec(),
                });
        }
        return Err(format!(
            "{error}; rollback={rollback:?}; backup={}",
            backup.display()
        ));
    }
    Ok(true)
}

fn convert(
    path: &Path,
    roots: &super::LevelSetRoots,
    valid_scan: &AtomicBool,
    cache: &Path,
    operation: &Mutex<()>,
    progress: &dyn Fn(usize, usize),
    generation: u64,
) -> Result<Option<bool>, String> {
    let game = roots.game_root();
    let may_convert = || may_convert(generation) && valid_scan.load(Ordering::SeqCst);
    if !may_convert() {
        return Ok(None);
    }
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut magic = [0u8; 8];
    if file.read_exact(&mut magic).is_err() || &magic != b"UnityFS\0" {
        return Ok(Some(false));
    }
    drop(file);
    if fs::metadata(path).map_err(|e| e.to_string())?.len() > 512 * 1024 * 1024 {
        return Err("bundle size limit".into());
    }
    if !crate::asset_bundle::bundle_needs_conversion(path).map_err(|e| e.to_string())? {
        return Ok(Some(false));
    }
    if !may_convert() {
        return Ok(None);
    }
    let bytes = fs::read(path).map_err(|e| e.to_string())?;
    fs::create_dir_all(cache).map_err(|e| e.to_string())?;
    let work = cache.join(uuid::Uuid::new_v4().to_string());
    fs::create_dir(&work).map_err(|e| e.to_string())?;
    let backup = work.join("original-bundle");
    let output_path = work.join("converted-bundle");
    let source_hash = Sha256::digest(&bytes).to_vec();
    // Record recovery coordinates before any source modification.
    fs::write(work.join("source.txt"), path.to_string_lossy().as_bytes())
        .map_err(|e| e.to_string())?;
    write_new(&backup, &bytes)?;
    if hash(&backup)? != source_hash {
        return Err("backup hash mismatch".into());
    }
    let output = crate::asset_bundle::recompress::to_lz4hc3(&bytes, progress, &may_convert)
        .map_err(|e| e.to_string())?;
    let Some(output) = output else {
        fs::remove_dir_all(&work).map_err(|e| e.to_string())?;
        return Ok(None);
    };
    write_new(&output_path, &output)?;
    let output_hash = Sha256::digest(&output).to_vec();
    if hash(&output_path)? != output_hash {
        return Err("staging hash mismatch".into());
    }
    // Shutdown can discard work before replacement; once copying begins rollback must finish.
    if !may_convert() {
        fs::remove_file(&output_path).map_err(|e| e.to_string())?;
        fs::remove_file(&backup).map_err(|e| e.to_string())?;
        let _ = fs::remove_file(work.join("source.txt"));
        let _ = fs::remove_dir(&work);
        return Ok(None);
    }
    {
        let _guard = operation.lock().map_err(|_| "lock.poisoned")?;
        // Installation may have held this lock while shutdown was requested.
        if !may_convert() {
            let _ = fs::remove_file(&output_path);
            let _ = fs::remove_file(&backup);
            let _ = fs::remove_file(work.join("source.txt"));
            let _ = fs::remove_dir(&work);
            return Ok(None);
        }
        let canonical = path.canonicalize().map_err(|e| e.to_string())?;
        let mut map_roots = vec![super::disabled_root(game)];
        if let Ok(enabled) = super::levels_root(roots) {
            map_roots.insert(0, enabled);
        }
        let inside = map_roots
            .iter()
            .filter_map(|root| root.canonicalize().ok())
            .any(|root| {
                game.canonicalize().is_ok_and(|game| root.starts_with(game))
                    && canonical.starts_with(root)
            });
        if !inside {
            return Err("source moved outside map roots".into());
        }
        // Refresh the real process state at the write boundary, covering observer latency.
        if crate::game::is_game_running(game) {
            set_game_running(true);
            fs::remove_dir_all(&work).map_err(|e| e.to_string())?;
            return Ok(None);
        }
        if !replace(
            path,
            &output_path,
            &backup,
            &source_hash,
            &output_hash,
            &may_convert,
        )? {
            fs::remove_dir_all(&work).map_err(|e| e.to_string())?;
            return Ok(None);
        }
    }
    for item in [&output_path, &backup, &work.join("source.txt")] {
        fs::remove_file(item).map_err(|e| e.to_string())?;
    }
    fs::remove_dir(&work).map_err(|e| e.to_string())?;
    Ok(Some(true))
}

fn run_workers(workers: usize, emit: &(dyn Fn(Progress) + Sync)) {
    crate::concurrency::run_queue(
        workers,
        || (),
        || {
            let (lock, wake) = lifecycle();
            let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(work) = queue.claim() {
                    return Some(work);
                }
                if queue.stopping || queue.paused || !queue.has_active() {
                    return None;
                }
                queue = wake.wait(queue).unwrap_or_else(|e| e.into_inner());
            }
        },
        |_, work| {
            let job = &work.job;
            work.index.map(|index| {
                let path = &job.files[index];
                convert(
                    path,
                    &job.roots,
                    &job.valid_scan,
                    &job.cache,
                    &job.operation,
                    &|done, total| {
                        let progress = {
                            let mut queue = lifecycle().0.lock().unwrap_or_else(|e| e.into_inner());
                            if !queue.accepts(work.generation) {
                                return;
                            }
                            queue.progress.percent = done.saturating_mul(100) / total.max(1);
                            queue.progress.current = crate::display_path::display_path(path);
                            queue.progress.clone()
                        };
                        emit(progress);
                    },
                    work.generation,
                )
            })
        },
        |work, result| {
            let job = &work.job;
            let mut finished = work.index.is_none();
            if let Some(index) = work.index {
                let path = &job.files[index];
                let result = result.flatten().unwrap_or_else(|| {
                    Err("conversion worker panicked; recovery materials retained".into())
                });
                if matches!(result, Ok(None)) {
                    let queue = lifecycle().0.lock().unwrap_or_else(|e| e.into_inner());
                    if !queue.accepts(work.generation) {
                        lifecycle().1.notify_all();
                        let progress = queue.progress.clone();
                        drop(queue);
                        emit(progress);
                        return;
                    }
                }
                let stamp = result
                    .as_ref()
                    .ok()
                    .filter(|result| result.is_some())
                    .and_then(|_| fs::metadata(path).ok())
                    .map(|meta| (meta.len(), meta.modified().ok()));
                let progress = {
                    let mut queue = lifecycle().0.lock().unwrap_or_else(|e| e.into_inner());
                    if !queue.accepts(work.generation) {
                        return;
                    }
                    match result {
                        Ok(Some(true)) => queue.progress.converted += 1,
                        Ok(Some(false)) => queue.progress.skipped += 1,
                        Ok(None) => queue.progress.skipped += 1,
                        Err(error) => {
                            queue.progress.failed += 1;
                            log::warn!("地图转换失败，保留恢复材料：{}: {error}", path.display());
                        }
                    }
                    queue.progress.done += 1;
                    queue.progress.current = crate::display_path::display_path(path);
                    queue.progress.percent = 100;
                    finished = queue.file_finished(job, path.clone(), stamp);
                    queue.progress.clone()
                };
                emit(progress);
            }
            if finished {
                let fingerprint = job
                    .valid_scan
                    .load(Ordering::SeqCst)
                    .then(|| fingerprint(job.roots.game_root(), &job.set).ok())
                    .flatten();
                let progress = {
                    let mut queue = lifecycle().0.lock().unwrap_or_else(|e| e.into_inner());
                    if !queue.accepts(work.generation) {
                        return;
                    }
                    queue.set_finished(job, fingerprint);
                    lifecycle().1.notify_all();
                    queue.progress.clone()
                };
                emit(progress);
            }
        },
    );
}
#[cfg(test)]
mod tests {
    use super::*;
    fn workspace() -> PathBuf {
        let path = std::env::temp_dir().join(format!("oc2-compress-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        path
    }
    #[test]
    fn busy_queue_keeps_new_sets_and_replaces_only_pending_duplicates() {
        fn job(set: &str, count: usize) -> Job {
            Job {
                roots: super::super::LevelSetRoots::new("game".into(), None),
                valid_scan: Arc::new(AtomicBool::new(true)),
                cache: PathBuf::from("cache"),
                set: PathBuf::from(set),
                files: (0..count).map(|i| PathBuf::from(i.to_string())).collect(),
                signature: vec![],
                operation: Arc::new(Mutex::new(())),
            }
        }
        let mut queue = Lifecycle::default();
        queue.enqueue(job("first", 2), vec![]);
        queue.running = true;
        let active = queue.pending.pop_front().unwrap();
        queue.enqueue(job("second", 3), vec![]);
        queue.enqueue(job("second", 5), vec![]);
        queue.enqueue(job("first", 4), vec![]);
        assert_eq!(queue.pending.len(), 2);
        assert_eq!(queue.progress.total, 11); // active 2 + pending 5 + follow-up 4
        assert_eq!(queue.progress.sets_total, 3);
        assert_eq!(active.files.len(), 2);
        assert_eq!(queue.pending[0].set, PathBuf::from("second"));
        queue.completed.insert(PathBuf::from("unchanged"), vec![]);
        queue.enqueue(job("unchanged", 9), vec![]);
        assert_eq!(queue.progress.total, 11);
        queue.stopping = true;
        queue.enqueue(job("after-exit", 10), vec![]);
        assert_eq!(queue.pending.len(), 2);
    }
    #[test]
    fn replacement_verifies_hash_and_rolls_back_corrupt_output() {
        let dir = workspace();
        let path = dir.join("source");
        let backup = dir.join("backup");
        let output = dir.join("output");
        fs::write(&path, b"original").unwrap();
        fs::write(&backup, b"original").unwrap();
        fs::write(&output, b"corrupt").unwrap();
        let old = hash(&path).unwrap();
        let expected = Sha256::digest(b"expected").to_vec();
        assert!(replace(&path, &output, &backup, &old, &expected, &|| true).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        assert!(backup.exists());
        fs::write(&output, b"expected").unwrap();
        assert!(!replace(&path, &output, &backup, &old, &expected, &|| false).unwrap());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        replace(&path, &output, &backup, &old, &expected, &|| true).unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"expected");
        fs::remove_file(path).unwrap();
        fs::remove_file(backup).unwrap();
        fs::remove_file(output).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    fn changed_source_is_never_overwritten() {
        let dir = workspace();
        let path = dir.join("source");
        let backup = dir.join("backup");
        let output = dir.join("output");
        fs::write(&path, b"new user data").unwrap();
        fs::write(&backup, b"old").unwrap();
        fs::write(&output, b"converted").unwrap();
        assert!(
            replace(
                &path,
                &output,
                &backup,
                &Sha256::digest(b"old"),
                &hash(&output).unwrap(),
                &|| true
            )
            .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), b"new user data");
        fs::remove_file(path).unwrap();
        fs::remove_file(backup).unwrap();
        fs::remove_file(output).unwrap();
        fs::remove_dir(dir).unwrap();
    }
    #[test]
    #[ignore = "requires installed map fixtures; writes only a temporary copy"]
    fn real_map_resource_conversion_transaction() {
        set_game_running(false);
        let source = std::env::var_os("OC2_LZMA_SCENE_FIXTURE").map(PathBuf::from).unwrap_or_else(||
            "E:/SteamLibrary/steamapps/common/Overcooked! 2/BepInEx/plugins/OC2DIYLevel/levels/blni_v2.1(小NO)/s_blni_1".into()
        );
        let bytes = fs::read(&source).expect("real LZMA resource");
        let dir = workspace();
        let roots = super::super::paths::test_roots(&dir);
        let set = super::super::levels_root(&roots).unwrap().join("fixture");
        fs::create_dir_all(&set).unwrap();
        let path = set.join("scene");
        fs::write(&path, &bytes).unwrap();
        let cache = dir.join("cache");
        assert_eq!(
            convert(
                &path,
                &roots,
                &AtomicBool::new(true),
                &cache,
                &Mutex::new(()),
                &|_, _| {
                    set_game_running(true);
                    // Even a rapid exit must not revive this old conversion.
                    set_game_running(false);
                },
                GENERATION.load(Ordering::SeqCst)
            )
            .unwrap(),
            None
        );
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 0);
        set_game_running(false);
        assert_eq!(
            convert(
                &path,
                &roots,
                &AtomicBool::new(true),
                &cache,
                &Mutex::new(()),
                &|_, _| {},
                GENERATION.load(Ordering::SeqCst)
            )
            .unwrap(),
            Some(true)
        );
        let result = fs::read(&path).unwrap();
        assert_eq!(
            crate::asset_bundle::read_bundle_entries(&bytes).unwrap(),
            crate::asset_bundle::read_bundle_entries(&result).unwrap()
        );
        assert_eq!(
            convert(
                &path,
                &roots,
                &AtomicBool::new(true),
                &cache,
                &Mutex::new(()),
                &|_, _| {},
                GENERATION.load(Ordering::SeqCst)
            )
            .unwrap(),
            Some(false)
        );
        assert_eq!(fs::read(&source).unwrap(), bytes);
        assert_eq!(fs::read_dir(cache).unwrap().count(), 0);
    }
}
