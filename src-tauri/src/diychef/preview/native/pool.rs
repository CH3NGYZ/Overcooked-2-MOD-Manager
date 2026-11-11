//! Bounded private Store leases; resource initialization and conversions run unlocked.
use super::reader::Store;
use std::{
    sync::{Arc, Condvar, Mutex, Once, OnceLock},
    time::{Duration, Instant},
};

const IDLE_TIMEOUT: Duration = Duration::from_secs(10);

pub(super) fn shared() -> &'static Pool {
    static POOL: OnceLock<Pool> = OnceLock::new();
    static RECLAIMER: Once = Once::new();
    let pool = POOL.get_or_init(|| Pool::new(crate::concurrency::workers_from(0)));
    RECLAIMER.call_once(|| {
        if let Err(error) = std::thread::Builder::new()
            .name("chef-preview-idle".into())
            .spawn(move || {
                loop {
                    pool.reclaim_after_idle(IDLE_TIMEOUT);
                }
            })
        {
            log::warn!("DIYChef 预览空闲回收线程启动失败：{error}");
        }
    });
    pool
}

pub(super) struct Pool {
    state: Mutex<State>,
    changed: Condvar,
}
struct State {
    workers: usize,
    active: usize,
    peak: usize,
    generation: u64,
    cache_epoch: u64,
    stamp: Option<String>,
    initializing: bool,
    seed: Option<Arc<Store>>,
    // Retain one useful mutable cache, irrespective of configured worker count.
    idle: Option<Store>,
    idle_since: Option<Instant>,
}

impl Pool {
    fn new(workers: usize) -> Self {
        Self {
            state: Mutex::new(State {
                workers: workers.max(1),
                active: 0,
                peak: 0,
                generation: 0,
                cache_epoch: 0,
                stamp: None,
                initializing: false,
                seed: None,
                idle: None,
                idle_since: None,
            }),
            changed: Condvar::new(),
        }
    }

    pub(super) fn set_workers(&self, workers: usize) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).workers = workers.max(1);
        self.changed.notify_all();
    }

    pub(super) fn clear(&self, epoch: u64) {
        let retired = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            state.generation = state.generation.wrapping_add(1);
            state.cache_epoch = epoch;
            state.stamp = None;
            state.initializing = false;
            state.idle_since = None;
            (state.seed.take(), state.idle.take())
        };
        self.changed.notify_all();
        // Active old leases retain only their own resources and are discarded
        // on return; shared documents and block buffers leave with that pool.
        drop(retired);
    }

    fn reclaim_after_idle(&self, timeout: Duration) {
        let retired = {
            let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
            loop {
                let Some(since) = state.idle_since.filter(|_| state.active == 0) else {
                    state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
                    continue;
                };
                let remaining = timeout.saturating_sub(since.elapsed());
                if !remaining.is_zero() {
                    state = self
                        .changed
                        .wait_timeout(state, remaining)
                        .unwrap_or_else(|e| e.into_inner())
                        .0;
                    continue;
                }
                // This is only disposal, not invalidation. Keeping the stamp
                // and generation lets any admitted/waiting call finish normally.
                state.idle_since = None;
                break (state.seed.take(), state.idle.take());
            }
        };
        self.changed.notify_all();
        // Deallocation must not block new admissions behind the pool mutex.
        drop(retired);
        log::debug!("DIYChef 预览资源池空闲回收完成");
    }

    pub(super) fn run<T>(
        &self,
        epoch: u64,
        stamp: String,
        initialize: impl FnOnce() -> Result<Store, String>,
        operation: impl FnOnce(&mut Store) -> Result<T, String>,
    ) -> Result<T, String> {
        let mut lease = self.acquire(epoch, stamp, initialize)?;
        let result = operation(lease.store.as_mut().unwrap());
        lease.reusable = result.is_ok();
        result
    }

    fn acquire(
        &self,
        epoch: u64,
        stamp: String,
        initialize: impl FnOnce() -> Result<Store, String>,
    ) -> Result<Lease<'_>, String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if epoch != state.cache_epoch {
            return Err("diychef.previewUnavailable".into());
        }
        let generation = if state.stamp.as_ref() != Some(&stamp) {
            state.generation = state.generation.wrapping_add(1);
            let generation = state.generation;
            state.stamp = Some(stamp);
            state.initializing = false;
            let retired = (state.seed.take(), state.idle.take());
            // Dropping large caches also stays outside the admission mutex.
            drop(state);
            drop(retired);
            state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            self.changed.notify_all();
            generation
        } else {
            state.generation
        };
        loop {
            // Old waiters/initializers cannot repopulate a newer resource stamp.
            if generation != state.generation {
                return Err("diychef.previewUnavailable".into());
            }
            if state.active < state.workers && (!state.initializing || state.seed.is_some()) {
                break;
            }
            state = self.changed.wait(state).unwrap_or_else(|e| e.into_inner());
        }
        state.active += 1;
        state.idle_since = None;
        self.changed.notify_all();
        if state.active > state.peak {
            state.peak = state.active;
            log::debug!(
                "DIYChef 预览并发：上限={}，实际同时执行峰值={}",
                state.workers,
                state.peak
            );
        }
        let idle = state.idle.take();
        let seed = state.seed.clone();
        let initializing = seed.is_none();
        state.initializing |= initializing;
        drop(state);

        // The guard exists before initialization/forking, so errors and panics
        // always return the global permit and release initializer waiters.
        let mut lease = Lease {
            pool: self,
            generation,
            initializing,
            reusable: false,
            store: idle,
            seed: seed.clone(),
        };
        if lease.store.is_some() {
            return Ok(lease);
        }
        let seed = if let Some(seed) = seed {
            seed
        } else {
            let seed = Arc::new(initialize()?);
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            if generation != state.generation {
                return Err("diychef.previewUnavailable".into());
            }
            state.seed = Some(seed.clone());
            state.initializing = false;
            self.changed.notify_all();
            seed
        };
        lease.store = Some(seed.fork());
        lease.seed = Some(seed);
        lease.initializing = false;
        Ok(lease)
    }
}

struct Lease<'a> {
    pool: &'a Pool,
    generation: u64,
    initializing: bool,
    reusable: bool,
    store: Option<Store>,
    seed: Option<Arc<Store>>,
}
impl Drop for Lease<'_> {
    fn drop(&mut self) {
        if self.reusable && !std::thread::panicking() {
            if let (Some(store), Some(seed)) = (&mut self.store, &self.seed) {
                // Trimming and dropping worker-only object trees stays outside
                // the admission lock and completes before releasing its permit.
                store.trim_for_idle(seed);
            }
        }
        let mut state = self.pool.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        if state.generation == self.generation {
            if self.initializing {
                state.initializing = false;
            }
            if self.reusable && !std::thread::panicking() && state.idle.is_none() {
                state.idle = self.store.take();
            }
        }
        if state.active == 0 && (state.seed.is_some() || state.idle.is_some()) {
            state.idle_since = Some(Instant::now());
        }
        self.pool.changed.notify_all();
        // Any discarded mutable Store is dropped after the admission lock.
        drop(state);
    }
}

#[cfg(test)]
#[path = "pool_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pool_memory_tests.rs"]
mod memory_tests;

#[cfg(test)]
#[path = "pool_twelve_tests.rs"]
mod twelve_tests;
