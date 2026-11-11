//! Async admission precedes spawn_blocking; waiters retain no parsed resources.
use std::sync::{Arc, Mutex, OnceLock};

struct State {
    limit: usize,
    active: usize,
}

struct Admission {
    state: Mutex<State>,
    changed: tokio::sync::Notify,
}

impl Admission {
    fn new(limit: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                limit: limit.max(1),
                active: 0,
            }),
            changed: Default::default(),
        })
    }

    fn set_limit(&self, limit: usize) {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).limit = limit.max(1);
        self.changed.notify_waiters();
    }

    async fn acquire(self: &Arc<Self>) -> Result<Permit, String> {
        loop {
            let changed = self.changed.notified();
            tokio::pin!(changed);
            // Register before inspecting active, including wakeups between the
            // mutex release and awaiting this notification.
            changed.as_mut().enable();
            {
                let mut state = self
                    .state
                    .lock()
                    .map_err(|_| "diychef.previewUnavailable")?;
                if state.active < state.limit {
                    state.active += 1;
                    return Ok(Permit(self.clone()));
                }
            }
            changed.await;
        }
    }
}

pub(crate) struct Permit(Arc<Admission>);
impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active -= 1;
        drop(state);
        self.0.changed.notify_waiters();
    }
}

fn scene_slots() -> &'static Arc<Admission> {
    static SLOTS: OnceLock<Arc<Admission>> = OnceLock::new();
    SLOTS.get_or_init(|| Admission::new(crate::concurrency::workers_from(0)))
}

pub(super) fn configure_concurrency(stored: u32) {
    scene_slots().set_limit(crate::concurrency::workers_from(stored));
}

pub(crate) async fn scene_permit() -> Result<Permit, String> {
    scene_slots().acquire().await
}

pub(crate) async fn fingerprint_permit() -> Result<Permit, String> {
    static SLOTS: OnceLock<Arc<Admission>> = OnceLock::new();
    SLOTS.get_or_init(|| Admission::new(4)).acquire().await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn lowering_limits_keeps_in_flight_jobs_and_blocks_new_work_until_the_limit_allows_it() {
        let admission = Admission::new(2);
        let one = admission.acquire().await.unwrap();
        let two = admission.acquire().await.unwrap();
        admission.set_limit(1);
        let pending = admission.acquire();
        tokio::pin!(pending);
        assert!(
            tokio::time::timeout(Duration::from_millis(5), &mut pending)
                .await
                .is_err()
        );
        drop(one);
        assert!(
            tokio::time::timeout(Duration::from_millis(5), &mut pending)
                .await
                .is_err()
        );
        drop(two);
        let third = tokio::time::timeout(Duration::from_secs(1), &mut pending)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(admission.state.lock().unwrap().active, 1);
        admission.set_limit(2);
        let fourth = admission.acquire().await.unwrap();
        drop((third, fourth));
        assert_eq!(admission.state.lock().unwrap().active, 0);
    }
}
