use serde::{Deserialize, Serialize};
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter};

tokio::task_local! {
    /// A download's caller may need its real progress without guessing a global
    /// event's label. The scope follows this future, including the race/volume
    /// coordinators, and is released even when the future fails or is dropped.
    static PROGRESS_OBSERVER: Arc<dyn Fn(DownloadProgress) + Send + Sync>;
}

pub(crate) fn observe_download_progress<T>(
    observer: impl Fn(DownloadProgress) + Send + Sync + 'static,
    operation: impl Future<Output = T>,
) -> impl Future<Output = T> {
    PROGRESS_OBSERVER.scope(Arc::new(observer), Box::pin(operation))
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
    pub attempt: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attempt_total: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallel_sources: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub volumes: Vec<VolumeProgress>,
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
            attempt: None,
            attempt_total: None,
            parallel_sources: None,
            volumes: Vec::new(),
        }
    }
}

// Every source reports into its own snapshot. The race coordinator publishes
// one source at a time so duplicate downloads never inflate the progress bar.
pub struct DownloadReporter {
    progress: Mutex<DownloadProgress>,
    transferred: AtomicU64,
}

impl DownloadReporter {
    pub fn new(label: &str) -> Self {
        Self {
            progress: Mutex::new(DownloadProgress::connecting(label)),
            transferred: AtomicU64::new(0),
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
        self.transferred.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn transferred(&self) -> u64 {
        self.transferred.load(Ordering::Relaxed)
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
        attempt: None,
        attempt_total: None,
        parallel_sources: None,
        volumes: Vec::new(),
    });
}

pub fn emit_progress_event(app: Option<&AppHandle>, progress: DownloadProgress) {
    // An owned progress scope is the presentation for this request. Broadcasting
    // it too could overwrite another download's unscoped floating progress card.
    if PROGRESS_OBSERVER
        .try_with(|observer| observer(progress.clone()))
        .is_ok()
    {
        return;
    }
    if let Some(app) = app {
        let _ = app.emit("download-progress", progress);
    }
}

#[cfg(test)]
mod tests;
