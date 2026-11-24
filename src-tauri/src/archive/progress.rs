use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tokio::time::Instant;

struct ProgressObserver {
    interactive: bool,
    publish: Arc<dyn Fn(DownloadProgress) + Send + Sync>,
}

tokio::task_local! {
    /// A download's caller may need its real progress without guessing a global
    /// event's label. The scope follows this future, including the race/volume
    /// coordinators, and is released even when the future fails or is dropped.
    static PROGRESS_OBSERVER: ProgressObserver;
}

pub(crate) fn observe_download_progress<T>(
    interactive: bool,
    observer: impl Fn(DownloadProgress) + Send + Sync + 'static,
    operation: impl Future<Output = T>,
) -> impl Future<Output = T> {
    PROGRESS_OBSERVER.scope(
        ProgressObserver {
            interactive,
            publish: Arc::new(observer),
        },
        Box::pin(operation),
    )
}

pub(crate) fn interactive_download_progress() -> bool {
    PROGRESS_OBSERVER
        .try_with(|observer| observer.interactive)
        .unwrap_or(false)
}

pub(crate) fn channel_download_progress<T>(
    interactive: bool,
    channel: tauri::ipc::Channel<DownloadProgress>,
    operation: impl Future<Output = T>,
) -> impl Future<Output = T> {
    observe_download_progress(
        interactive,
        move |progress| {
            let _ = channel.send(progress);
        },
        operation,
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bytes: u64,
    pub finished: bool,
    pub phase: DownloadPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<DownloadRoute>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub race: Option<DownloadRaceProgress>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<VolumeProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadRaceProgress {
    pub id: String,
    pub primary_id: Option<usize>,
    pub locked_id: Option<usize>,
    pub can_lock: bool,
    pub waiting: usize,
    pub elapsed_ms: u64,
    pub sources: Vec<DownloadSourceProgress>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSourceProgress {
    pub id: usize,
    pub route: DownloadRoute,
    pub source: Option<String>,
    pub downloaded: u64,
    pub total: Option<u64>,
    pub speed_bytes: Option<u64>,
    pub phase: DownloadSourcePhase,
    pub history_preferred: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadSourcePhase {
    Connecting,
    Downloading,
    Verifying,
    Failed,
    Stopped,
    Finished,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeProgress {
    pub name: String,
    pub state: String,
    pub retry: usize,
    pub progress: Box<DownloadProgress>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadPhase {
    Connecting,
    Switching,
    Downloading,
    Verifying,
    LockedFailed,
    Merging,
    Finished,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DownloadRoute {
    Direct,
    Proxy,
}

impl DownloadProgress {
    pub(crate) fn set_file_name(&mut self, name: Option<&str>) {
        self.file_name = name
            .filter(|name| !name.trim().is_empty())
            .map(str::to_owned);
    }

    pub fn connecting(label: &str) -> Self {
        Self {
            label: label.into(),
            file_name: None,
            downloaded: 0,
            total: None,
            speed_bytes: 0,
            finished: false,
            phase: DownloadPhase::Connecting,
            route: None,
            source: None,
            race: None,
            volumes: Vec::new(),
        }
    }
}

pub struct DownloadReporter {
    progress: Mutex<DownloadProgress>,
    transferred: AtomicU64,
    first_byte: OnceLock<Instant>,
    transfer_end: OnceLock<Instant>,
}

impl DownloadReporter {
    pub fn new(label: &str) -> Self {
        Self {
            progress: Mutex::new(DownloadProgress::connecting(label)),
            transferred: AtomicU64::new(0),
            first_byte: OnceLock::new(),
            transfer_end: OnceLock::new(),
        }
    }

    pub fn update(&self, progress: DownloadProgress) {
        if let Ok(mut current) = self.progress.lock() {
            *current = progress;
        }
    }

    pub fn snapshot(&self) -> DownloadProgress {
        self.progress
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn received(&self, bytes: u64) {
        if bytes != 0 {
            self.first_byte.get_or_init(Instant::now);
            self.transferred.fetch_add(bytes, Ordering::Relaxed);
        }
    }

    pub fn transferred(&self) -> u64 {
        self.transferred.load(Ordering::Relaxed)
    }

    pub fn first_byte_at(&self) -> Option<Instant> {
        self.first_byte.get().copied()
    }

    pub fn transfer_end(&self) -> Option<Instant> {
        self.transfer_end.get().copied()
    }

    pub(crate) fn verifying(&self) {
        self.transfer_end.get_or_init(Instant::now);
        let mut progress = self
            .progress
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        progress.phase = DownloadPhase::Verifying;
        progress.speed_bytes = 0;
        progress.finished = false;
    }
}

pub fn emit_progress(
    reporter: &DownloadReporter,
    label: &str,
    downloaded: u64,
    total: Option<u64>,
    elapsed: std::time::Duration,
    finished: bool,
) {
    let percent = total
        .filter(|value| *value > 0)
        .map(|value| downloaded as f64 / value as f64 * 100.0);
    let speed_bytes = downloaded / elapsed.as_secs().max(1);
    log::debug!(
        "下载进度：任务={}，已下载={}，总长度={:?}，百分比={:?}%，速度={}B/s，完成={}",
        label,
        downloaded,
        total,
        percent,
        speed_bytes,
        finished
    );
    if finished {
        reporter.transfer_end.get_or_init(Instant::now);
    }
    reporter.update(DownloadProgress {
        label: label.into(),
        file_name: None,
        downloaded,
        total,
        speed_bytes,
        finished,
        phase: if finished {
            DownloadPhase::Finished
        } else {
            DownloadPhase::Downloading
        },
        // The coordinator adds the selected source's connection context.
        route: None,
        source: None,
        race: None,
        volumes: Vec::new(),
    });
}

pub fn publish_download_progress(progress: DownloadProgress) {
    let _ = PROGRESS_OBSERVER.try_with(|observer| (observer.publish)(progress));
}

#[cfg(test)]
mod tests;
