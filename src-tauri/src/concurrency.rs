//! Shared bounded worker engine. Domain queues own admission, waiting and cancellation.
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};

/// Zero selects all available logical processors.
pub(crate) const AUTO_CONCURRENCY: u32 = 0;

/// 当前机器的可用核心数，至少 1。
pub(crate) fn cores() -> usize {
    std::thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(1)
        .max(1)
}

/// 把配置里的并发值解析成实际线程数。
///
/// `0` = 全部核心；其余夹到 `1..=cores`。上限永远按**当前机器**算，配置里
/// 存的是用户意图（例如 8），换到 4 核机器上按 4 跑，不会起 8 个线程。
pub(crate) fn resolve_concurrency(stored: u32, cores: usize) -> usize {
    let cores = cores.max(1);
    if stored == AUTO_CONCURRENCY {
        cores
    } else {
        (stored as usize).clamp(1, cores)
    }
}

/// 配置值 → 本机实际线程数（`0` = 全部核心）。
///
/// 调用方只关心「这次批次开几个线程」，不该各自重复 `resolve_concurrency(x,
/// cores())`：写两遍就有一处会忘了夹紧。
pub(crate) fn workers_from(stored: u32) -> usize {
    resolve_concurrency(stored, cores())
}

/// 保存设置时把并发值夹到本机允许的范围（`0` = 全部核心，原样保留）。
///
/// 与 [`resolve_concurrency`] 的区别：那个返回「这次开几个线程」，这个返回
/// 「写回配置文件的值」——`0` 不能在这里被展开成核心数，否则换到核心更多的
/// 机器上就永远是旧机器的上限了。
pub(crate) fn clamp_stored_concurrency(stored: u32) -> u32 {
    if stored == AUTO_CONCURRENCY {
        AUTO_CONCURRENCY
    } else {
        stored.clamp(1, cores() as u32)
    }
}

/// Dynamic queues and finite batches use the same worker loop. Each worker owns its
/// local state (for example a ZIP reader); task panics are delivered to completion
/// as None, so callers can release reservations and keep the queue progressing.
pub(crate) fn run_queue<T, R, S>(
    workers: usize,
    init: impl Fn() -> S + Sync,
    claim: impl Fn() -> Option<T> + Sync,
    run: impl Fn(&mut S, &T) -> R + Sync,
    complete: impl Fn(T, Option<R>) + Sync,
) {
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let finished = AtomicUsize::new(0);
    std::thread::scope(|scope| {
        for _ in 0..workers.max(1) {
            let (init, claim, run, complete) = (&init, &claim, &run, &complete);
            let (active, peak, finished) = (&active, &peak, &finished);
            scope.spawn(move || {
                let mut local = init();
                while let Some(task) = claim() {
                    let now = active.fetch_add(1, Ordering::Relaxed) + 1;
                    peak.fetch_max(now, Ordering::Relaxed);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        run(&mut local, &task)
                    }))
                    .ok();
                    if result.is_none() {
                        log::warn!("后台并行任务 panic，已隔离");
                    }
                    active.fetch_sub(1, Ordering::Relaxed);
                    complete(task, result);
                    finished.fetch_add(1, Ordering::Relaxed);
                }
            });
        }
    });
    log::debug!(
        "并行队列完成：线程上限={}，实际同时执行峰值={}，完成任务={}",
        workers.max(1),
        peak.load(Ordering::Relaxed),
        finished.load(Ordering::Relaxed)
    );
}

/// Finite batches preserve input order and continue filling free workers while
/// slower tasks are still running. No barrier between chunks of work.
pub(crate) fn run_bounded<T: Sync, R: Send>(
    items: &[T],
    workers: usize,
    run: impl Fn(usize, &T) -> R + Sync,
) -> Vec<(usize, Option<R>)> {
    if items.is_empty() {
        return Vec::new();
    }
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(items.len()));
    run_queue(
        workers.clamp(1, items.len()),
        || (),
        || {
            let index = next.fetch_add(1, Ordering::Relaxed);
            (index < items.len()).then_some(index)
        },
        |_, index| run(*index, &items[*index]),
        |index, value| {
            results
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((index, value));
        },
    );
    let mut results = results.into_inner().unwrap_or_else(|e| e.into_inner());
    results.sort_by_key(|(index, _)| *index);
    results
}

#[cfg(test)]
mod tests;
