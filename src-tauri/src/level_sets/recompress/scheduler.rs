//! One file queue across sets. A follow-up for an active set waits until its files finish.
use super::{Job, Progress};
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    sync::Arc,
    time::SystemTime,
};

pub(super) type Signature = Vec<(PathBuf, u64, Option<SystemTime>)>;

struct Active {
    job: Arc<Job>,
    next: usize,
    done: usize,
    processed: BTreeMap<PathBuf, (u64, Option<SystemTime>)>,
}

pub(super) struct Work {
    pub job: Arc<Job>,
    pub generation: u64,
    /// Empty sets still need their completion fingerprint recorded.
    pub index: Option<usize>,
}

#[derive(Default)]
pub(super) struct Lifecycle {
    pub running: bool,
    pub stopping: bool,
    pub paused: bool,
    pub generation: u64,
    pub rescan_requested: bool,
    pub pending: VecDeque<Job>,
    pub progress: Progress,
    pub completed: BTreeMap<PathBuf, Signature>,
    active: BTreeMap<PathBuf, Active>,
}

impl Lifecycle {
    pub fn enqueue(&mut self, job: Job, signature: Signature) {
        // Recheck under the queue lock: enumeration can outlive its MOD scan and
        // must never replace pending work admitted by the newer scan.
        if self.stopping
            || self.paused
            || !job.valid_scan.load(std::sync::atomic::Ordering::SeqCst)
            || self.completed.get(&job.set) == Some(&signature)
        {
            return;
        }
        if let Some(index) = self.pending.iter().position(|item| item.set == job.set) {
            self.progress.total = self
                .progress
                .total
                .saturating_sub(self.pending[index].files.len())
                + job.files.len();
            self.pending[index] = job;
            return;
        }
        if !self.running && self.pending.is_empty() {
            self.progress = Progress {
                running: true,
                ..Default::default()
            };
        }
        self.progress.total += job.files.len();
        self.progress.sets_total += 1;
        self.pending.push_back(job);
    }

    /// Called under the queue lock, with no filesystem IO. Workers never wait for a set barrier.
    pub fn claim(&mut self) -> Option<Work> {
        if self.stopping {
            self.pending.clear();
            return None;
        }
        if self.paused {
            return None;
        }
        for active in self.active.values_mut() {
            if active.next < active.job.files.len() {
                let index = active.next;
                active.next += 1;
                return Some(Work {
                    generation: self.generation,
                    job: active.job.clone(),
                    index: Some(index),
                });
            }
        }
        // A notification may enqueue the same set while its conversion is still active.
        // Skip only that follow-up, rather than blocking unrelated sets behind it.
        while let Some(index) = self
            .pending
            .iter()
            .position(|job| !self.active.contains_key(&job.set))
        {
            let job = self.pending.remove(index).unwrap();
            if self.completed.get(&job.set) == Some(&job.signature) {
                self.progress.total = self.progress.total.saturating_sub(job.files.len());
                self.progress.sets_total = self.progress.sets_total.saturating_sub(1);
                continue;
            }
            let job = Arc::new(job);
            let index = (!job.files.is_empty()).then_some(0);
            self.active.insert(
                job.set.clone(),
                Active {
                    job: job.clone(),
                    next: usize::from(index.is_some()),
                    done: 0,
                    processed: BTreeMap::new(),
                },
            );
            return Some(Work {
                job,
                index,
                generation: self.generation,
            });
        }
        None
    }

    pub fn has_active(&self) -> bool {
        !self.active.is_empty()
    }

    /// Discard the entire old batch; in-flight transactions still finish safely.
    pub fn game_started(&mut self) {
        self.paused = true;
        self.generation += 1;
        self.rescan_requested = true;
        self.pending.clear();
        self.active.clear();
        self.completed.clear();
        self.progress = Progress::default();
    }

    pub fn accepts(&self, generation: u64) -> bool {
        !self.stopping && !self.paused && self.generation == generation
    }

    /// Only the observer consumes this after old workers have joined.
    pub fn take_rescan(&mut self) -> bool {
        if self.stopping || self.paused || self.running {
            return false;
        }
        std::mem::take(&mut self.rescan_requested)
    }

    pub fn file_finished(
        &mut self,
        job: &Job,
        path: PathBuf,
        stamp: Option<(u64, Option<SystemTime>)>,
    ) -> bool {
        let active = self.active.get_mut(&job.set).expect("claimed set");
        active.done += 1;
        if let Some(stamp) = stamp {
            active.processed.insert(path, stamp);
        }
        self.progress.set_name = job
            .set
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        self.progress.files_done = active.done;
        self.progress.files_total = job.files.len();
        active.done == job.files.len()
    }

    /// Keep the set active while fingerprinting outside the lock, preventing overlapping rewrites.
    pub fn set_finished(&mut self, job: &Job, fingerprint: Option<Signature>) {
        let active = self.active.remove(&job.set).expect("finishing set");
        if let Some(after) = fingerprint {
            let complete = after.len() == job.signature.len()
                && active.processed.len() == job.files.len()
                && job.signature.iter().zip(&after).all(|(old, new)| {
                    old.0 == new.0
                        && active
                            .processed
                            .get(&new.0)
                            .map_or(old == new, |stamp| *stamp == (new.1, new.2))
                });
            if complete {
                self.completed.insert(job.set.clone(), after);
            }
        }
        self.progress.sets_done += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Barrier, Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    fn job(name: &str, files: usize) -> Job {
        Job {
            roots: crate::level_sets::LevelSetRoots::new("game".into(), None),
            valid_scan: Arc::new(std::sync::atomic::AtomicBool::new(true)),
            cache: "cache".into(),
            set: name.into(),
            files: (0..files)
                .map(|i| PathBuf::from(format!("{name}/{i}")))
                .collect(),
            signature: vec![],
            operation: Arc::new(Mutex::new(())),
        }
    }

    #[test]
    fn late_invalidated_scan_cannot_replace_fresh_pending_work() {
        let root = std::path::Path::new("game");
        let mut scans = crate::mods::snapshot::LocalModSnapshots::default();
        let generation = scans.begin(Some(root));
        scans.publish(generation, Some(root), vec![]).unwrap();
        let mut late = job("same-set", 3);
        late.valid_scan = scans.lease(generation, root).unwrap();

        let generation = scans.begin(Some(root));
        scans.publish(generation, Some(root), vec![]).unwrap();
        let mut fresh = job("same-set", 1);
        fresh.valid_scan = scans.lease(generation, root).unwrap();
        let current = fresh.valid_scan.clone();
        let queue = Mutex::new(Lifecycle::default());
        let mut state = queue.lock().unwrap();
        state.enqueue(fresh, vec![]);
        // The old enumerator reaches this lock after a new scan already queued work.
        state.enqueue(late, vec![]);

        assert_eq!(state.pending.len(), 1);
        assert!(Arc::ptr_eq(&state.pending[0].valid_scan, &current));
        assert_eq!(state.progress.total, 1);
        assert_eq!(state.progress.sets_total, 1);
        let next = state.claim().unwrap();
        assert_eq!(next.job.files.len(), 1);
        assert!(next.job.valid_scan.load(Ordering::SeqCst));
    }

    #[test]
    fn invalidated_scan_cannot_start_a_new_batch() {
        let mut state = Lifecycle::default();
        let late = job("old-root", 2);
        late.valid_scan.store(false, Ordering::SeqCst);
        state.enqueue(late, vec![]);
        assert!(state.pending.is_empty());
        assert_eq!(state.progress.total, 0);
        assert_eq!(state.progress.sets_total, 0);
        assert!(!state.progress.running);
        assert!(state.claim().is_none());
    }

    #[test]
    fn game_start_discards_batch_and_exit_requests_one_fresh_scan() {
        let mut state = Lifecycle::default();
        state.enqueue(job("first", 2), vec![]);
        state.running = true;
        let old = state.claim().unwrap();
        state.enqueue(job("pending", 1), vec![]);
        state.completed.insert("previous".into(), vec![]);
        state.game_started();
        assert!(!state.accepts(old.generation));
        assert!(state.pending.is_empty());
        assert!(!state.has_active());
        assert!(state.completed.is_empty());
        state.enqueue(job("during-game", 1), vec![]);
        assert!(state.pending.is_empty());
        assert!(state.claim().is_none());
        assert!(!state.take_rescan());
        state.paused = false;
        assert!(!state.take_rescan()); // Replacement from the old batch is still joining.
        state.running = false;
        assert!(state.take_rescan());
        assert!(!state.take_rescan());
        state.enqueue(job("first", 2), vec![]);
        let fresh = state.claim().unwrap();
        assert_eq!(fresh.index, Some(0));
        assert!(state.accepts(fresh.generation));
        assert!(!state.accepts(old.generation)); // Also rejects old scan admission/completions.
        assert_eq!(state.progress.done, 0);
        assert_eq!(state.progress.sets_total, 1);
    }
    #[test]
    fn four_single_file_sets_run_together_and_accept_new_work_while_busy() {
        let queue = (Mutex::new(Lifecycle::default()), Condvar::new());
        {
            let mut state = queue.0.lock().unwrap();
            for i in 0..4 {
                state.enqueue(job(&format!("set-{i}"), 1), vec![]);
            }
            state.running = true;
        }
        let barrier = Barrier::new(4);
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let processed = AtomicUsize::new(0);
        crate::concurrency::run_queue(
            4,
            || (),
            || {
                let mut state = queue.0.lock().unwrap();
                loop {
                    if let Some(work) = state.claim() {
                        return Some(work);
                    }
                    if !state.has_active() {
                        return None;
                    }
                    state = queue.1.wait(state).unwrap();
                }
            },
            |_, work| {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                if work.job.set != PathBuf::from("added-later") {
                    barrier.wait();
                }
                if work.job.set == PathBuf::from("set-0") {
                    queue
                        .0
                        .lock()
                        .unwrap()
                        .enqueue(job("added-later", 8), vec![]);
                    queue.1.notify_all();
                }
                active.fetch_sub(1, Ordering::SeqCst);
            },
            |work, outcome| {
                assert!(outcome.is_some());
                processed.fetch_add(1, Ordering::SeqCst);
                let mut state = queue.0.lock().unwrap();
                let index = work.index.unwrap();
                if state.file_finished(&work.job, work.job.files[index].clone(), None) {
                    state.set_finished(&work.job, None);
                    queue.1.notify_all();
                }
            },
        );
        assert_eq!(peak.load(Ordering::SeqCst), 4);
        assert_eq!(processed.load(Ordering::SeqCst), 12);
        assert_eq!(queue.0.lock().unwrap().progress.sets_done, 5);
    }

    #[test]
    fn same_set_followup_waits_without_blocking_other_sets() {
        let mut state = Lifecycle::default();
        state.enqueue(job("first", 1), vec![]);
        state.running = true;
        let first = state.claim().unwrap();
        state.enqueue(job("first", 2), vec![]);
        state.enqueue(job("other", 1), vec![]);
        let other = state.claim().unwrap();
        assert_eq!(other.job.set, PathBuf::from("other"));
        assert!(state.claim().is_none());
        state.file_finished(&first.job, first.job.files[0].clone(), None);
        state.set_finished(&first.job, None);
        let followup = state.claim().unwrap();
        assert_eq!(followup.job.set, first.job.set);
        assert_eq!(followup.job.files.len(), 2);
    }
}
