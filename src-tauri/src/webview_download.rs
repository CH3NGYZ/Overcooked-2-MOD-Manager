//! Native, interactive downloads. Remote pages receive no IPC capability.
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{
    AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tokio::{io::AsyncReadExt, sync::watch};
use url::Url;

const WINDOW_PREFIX: &str = "binary-download-";
const FAILED: &str = "source.webDownloadFailed";
const CANCELLED: &str = "source.webDownloadCancelled";
const TIMED_OUT: &str = "source.webDownloadTimeout";
const TOO_LARGE: &str = "source.webDownloadTooLarge";
const INVALID: &str = "source.webDownloadAddressInvalid";
const SIZE_POLL: Duration = Duration::from_millis(100);
static MAIN_CLOSED: AtomicBool = AtomicBool::new(false);

pub(crate) struct Request {
    pub url: Url,
    pub user_agent: String,
    pub max_bytes: usize,
    pub title: String,
    pub timeout: Duration,
    /// Trusted DNS suffixes: `gitee.com` permits the apex and its subdomains.
    pub allowed_download_hosts: Vec<String>,
    /// Additional exact paths allowed only on the initial HTTPS origin.
    pub allowed_download_paths: Vec<String>,
}

struct Policy {
    initial: Url,
    hosts: Vec<String>,
    paths: Vec<String>,
}

fn safe_https(url: &Url) -> bool {
    url.scheme() == "https"
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.port_or_known_default() == Some(443)
}

fn normalize_host(host: &str) -> Result<String, String> {
    if host.contains('*') || host.ends_with('.') {
        return Err(INVALID.into());
    }
    match url::Host::parse(host).map_err(|_| INVALID)? {
        url::Host::Domain(host) if !host.is_empty() => Ok(host.to_ascii_lowercase()),
        _ => Err(INVALID.into()),
    }
}

fn host_matches(host: &str, suffix: &str) -> bool {
    host == suffix
        || host
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.len() > 1 && prefix.ends_with('.'))
}

fn normalize_download_path(initial: &Url, path: &str) -> Result<String, String> {
    if !path.starts_with('/')
        || path.starts_with("//")
        || path.contains(['?', '#', '\\'])
        || path
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(INVALID.into());
    }
    let normalized = initial.join(path).map_err(|_| INVALID)?;
    if !safe_https(&normalized)
        || normalized.origin() != initial.origin()
        || normalized.query().is_some()
        || normalized.fragment().is_some()
    {
        return Err(INVALID.into());
    }
    Ok(normalized.path().to_owned())
}

impl Policy {
    fn new(request: &Request) -> Result<Self, String> {
        if !safe_https(&request.url)
            || request.max_bytes == 0
            || request.timeout.is_zero()
            || request.user_agent.is_empty()
            || request.user_agent.contains(['\r', '\n'])
        {
            return Err(INVALID.into());
        }
        let hosts = request
            .allowed_download_hosts
            .iter()
            .map(|host| normalize_host(host))
            .collect::<Result<Vec<_>, _>>()?;
        let paths = request
            .allowed_download_paths
            .iter()
            .map(|path| normalize_download_path(&request.url, path))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            initial: request.url.clone(),
            hosts,
            paths,
        })
    }

    fn navigation(&self, url: &Url) -> bool {
        // WebView2 may navigate through the verification/download host before
        // DownloadStarting. Apply the same explicit host trust at that boundary.
        safe_https(url) && (url.origin() == self.initial.origin() || self.trusted_host(url))
    }

    fn download(&self, url: &Url) -> bool {
        if !safe_https(url) {
            return false;
        }
        if url.origin() == self.initial.origin() {
            return url.path() == self.initial.path()
                || self.paths.iter().any(|path| path == url.path());
        }
        self.trusted_host(url)
    }

    fn trusted_host(&self, url: &Url) -> bool {
        url.host_str()
            .is_some_and(|host| self.hosts.iter().any(|suffix| host_matches(host, suffix)))
    }
}

#[derive(Clone)]
enum Phase {
    Waiting,
    Downloading(Url),
    Finished(bool),
    Cancelled,
}

struct DownloadState {
    phase: watch::Sender<Phase>,
}

impl DownloadState {
    fn new() -> Self {
        Self {
            phase: watch::channel(Phase::Waiting).0,
        }
    }

    fn requested(&self, url: &Url, policy: &Policy) -> bool {
        if !policy.download(url) {
            return false;
        }
        self.phase.send_if_modified(|phase| {
            if !matches!(phase, Phase::Waiting) {
                return false;
            }
            *phase = Phase::Downloading(url.clone());
            true
        })
    }

    fn finished(&self, url: &Url, success: bool) {
        let matched = self.phase.send_if_modified(|phase| {
            if !matches!(&*phase, Phase::Downloading(expected) if expected == url) {
                return false;
            }
            *phase = Phase::Finished(success);
            true
        });
        log::info!(
            "网页下载结束：域名={:?}，路径={}，成功={success}，匹配当前任务={matched}",
            url.host_str(),
            url.path()
        );
    }

    fn cancel(&self) {
        self.phase.send_replace(Phase::Cancelled);
    }
}

struct DownloadGuard<R: Runtime> {
    window: Option<WebviewWindow<R>>,
    directory: Option<tempfile::TempDir>,
    state: Arc<DownloadState>,
}

impl<R: Runtime> Drop for DownloadGuard<R> {
    fn drop(&mut self) {
        self.state.cancel();
        let had_window = self.window.is_some();
        if let Some(window) = self.window.take() {
            let _ = window.destroy();
        }
        if let Some(directory) = self.directory.take() {
            if !had_window {
                return;
            } // TempDir cleans up before any native writer exists.
            // Native destruction is queued. Windows may briefly keep the file open.
            // Only the absolute, freshly-owned TempDir is ever removed.
            tauri::async_runtime::spawn_blocking(move || {
                for _ in 0..100 {
                    match std::fs::remove_dir_all(directory.path()) {
                        Ok(()) => return,
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                        Err(_) => std::thread::sleep(Duration::from_millis(50)),
                    }
                }
                // TempDir makes a final cleanup attempt; no URL or filesystem path is logged.
                let _ = directory.close();
            });
        }
    }
}

/// Called by the application's existing global window-event hook.
pub(crate) fn close_with_main<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if window.label() != "main" || !matches!(event, WindowEvent::Destroyed) {
        return;
    }
    MAIN_CLOSED.store(true, Ordering::Release);
    for (label, webview) in window.app_handle().webview_windows() {
        if label.starts_with(WINDOW_PREFIX) {
            let _ = webview.destroy();
        }
    }
}

async fn cancelled(mut changes: watch::Receiver<Phase>) {
    loop {
        if MAIN_CLOSED.load(Ordering::Acquire)
            || matches!(*changes.borrow_and_update(), Phase::Cancelled)
        {
            return;
        }
        tokio::select! {
            result = changes.changed() => if result.is_err() { return; },
            _ = tokio::time::sleep(SIZE_POLL) => (),
        }
    }
}

fn extend_bounded(bytes: &mut Vec<u8>, chunk: &[u8], maximum: usize) -> Result<(), String> {
    if bytes
        .len()
        .checked_add(chunk.len())
        .is_none_or(|size| size > maximum)
    {
        return Err(TOO_LARGE.into());
    }
    bytes.extend_from_slice(chunk);
    Ok(())
}

async fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>, String> {
    let mut file = tokio::fs::File::open(path).await.map_err(|_| FAILED)?;
    let metadata = file.metadata().await.map_err(|_| FAILED)?;
    if !metadata.is_file() {
        return Err(FAILED.into());
    }
    if metadata.len() > maximum as u64 {
        return Err(TOO_LARGE.into());
    }
    let mut bytes = Vec::new();
    let mut chunk = vec![0; 16 * 1024];
    loop {
        let size = file.read(&mut chunk).await.map_err(|_| FAILED)?;
        if size == 0 {
            return Ok(bytes);
        }
        extend_bounded(&mut bytes, &chunk[..size], maximum)?;
    }
}

// Chromium may write a temporary sibling (for example .crdownload) before
// renaming the final path. Bound all files in this freshly-owned directory.
async fn check_download_size(directory: &Path, maximum: usize) -> Result<(), String> {
    let mut entries = tokio::fs::read_dir(directory).await.map_err(|_| FAILED)?;
    let mut total = 0u64;
    while let Some(entry) = entries.next_entry().await.map_err(|_| FAILED)? {
        let metadata = match tokio::fs::symlink_metadata(entry.path()).await {
            Ok(metadata) => metadata,
            // Native downloads can rename a partial file between enumeration
            // and metadata. The final path is checked separately before reading.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(FAILED.into()),
        };
        if !metadata.is_file() {
            return Err(FAILED.into());
        }
        total = total.checked_add(metadata.len()).ok_or(TOO_LARGE)?;
        if total > maximum as u64 {
            return Err(TOO_LARGE.into());
        }
    }
    Ok(())
}

async fn receive(path: &Path, state: &DownloadState, maximum: usize) -> Result<Vec<u8>, String> {
    let mut changes = state.phase.subscribe();
    let mut poll = tokio::time::interval(SIZE_POLL);
    loop {
        let phase = changes.borrow_and_update().clone();
        match phase {
            Phase::Cancelled => return Err(CANCELLED.into()),
            Phase::Finished(false) => return Err(FAILED.into()),
            _ => (),
        }
        check_download_size(path.parent().ok_or(FAILED)?, maximum).await?;
        match tokio::fs::symlink_metadata(path).await {
            Ok(metadata) => {
                if !metadata.is_file() {
                    return Err(FAILED.into());
                }
                if metadata.len() > maximum as u64 {
                    return Err(TOO_LARGE.into());
                }
            }
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && !matches!(phase, Phase::Finished(true)) =>
            {
                ()
            }
            Err(_) => return Err(FAILED.into()),
        }
        if matches!(phase, Phase::Finished(true)) {
            return read_bounded(path, maximum).await;
        }
        tokio::select! {
            _ = changes.changed() => (),
            _ = poll.tick() => (),
        }
    }
}

pub(crate) async fn download<R: Runtime>(
    app: &AppHandle<R>,
    request: Request,
) -> Result<Vec<u8>, String> {
    if MAIN_CLOSED.load(Ordering::Acquire) || app.get_webview_window("main").is_none() {
        return Err(CANCELLED.into());
    }
    let policy = Arc::new(Policy::new(&request)?);
    let directory = tempfile::Builder::new()
        .prefix("oc2-binary-download-")
        .tempdir()
        .map_err(|_| FAILED)?;
    if !directory.path().is_absolute() {
        return Err(FAILED.into());
    }
    let destination = directory.path().join("archive.zip");
    let state = Arc::new(DownloadState::new());
    let mut guard = DownloadGuard {
        window: None,
        directory: Some(directory),
        state: state.clone(),
    };
    let navigation = policy.clone();
    let bootstrap = Arc::new(AtomicBool::new(true));
    let initial_blank = bootstrap.clone();
    let events = state.clone();
    // Both WebView2 and WKDownload honor destination; the native session retains
    // verification cookies and one-use URLs. There is no HTTP request replay.
    let window = WebviewWindowBuilder::new(
        app,
        format!("{WINDOW_PREFIX}{}", uuid::Uuid::new_v4()),
        WebviewUrl::External(Url::parse("about:blank").map_err(|_| INVALID)?),
    )
    .title(&request.title)
    .user_agent(&request.user_agent)
    .inner_size(1000.0, 720.0)
    .visible(false)
    .focused(false)
    .on_navigation(move |url| {
        let allowed = (initial_blank.load(Ordering::Acquire) && url.as_str() == "about:blank")
            || navigation.navigation(url);
        if !allowed {
            log::warn!(
                "网页下载导航被拒绝：域名={:?}，HTTPS={}",
                url.host_str(),
                safe_https(url)
            );
        }
        allowed
    })
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
    .on_download(move |_, event| {
        match event {
            tauri::webview::DownloadEvent::Requested {
                url,
                destination: actual,
            } => {
                if !MAIN_CLOSED.load(Ordering::Acquire) && events.requested(&url, &policy) {
                    *actual = destination.clone();
                    log::info!(
                        "已接管网页下载：域名={:?}，路径={}",
                        url.host_str(),
                        url.path()
                    );
                    return true;
                }
                log::warn!(
                    "网页下载请求被拒绝：域名={:?}，路径={}，地址匹配={}，主窗口已关闭={}",
                    url.host_str(),
                    url.path(),
                    policy.download(&url),
                    MAIN_CLOSED.load(Ordering::Acquire)
                );
            }
            tauri::webview::DownloadEvent::Finished { url, success, .. } => {
                events.finished(&url, success)
            }
            _ => (),
        }
        false
    })
    .devtools(false)
    .build()
    .map_err(|_| FAILED)?;
    guard.window = Some(window);
    if MAIN_CLOSED.load(Ordering::Acquire) || app.get_webview_window("main").is_none() {
        return Err(CANCELLED.into());
    }
    let closed = state.clone();
    let window = guard.window.as_ref().ok_or(FAILED)?;
    window.on_window_event(move |event| {
        if matches!(
            event,
            WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
        ) {
            closed.cancel();
        }
    });
    bootstrap.store(false, Ordering::Release);
    window.show().map_err(|_| FAILED)?;
    let _ = window.set_focus();
    window.navigate(request.url).map_err(|_| FAILED)?;
    let path = guard
        .directory
        .as_ref()
        .ok_or(FAILED)?
        .path()
        .join("archive.zip");
    // This outer race covers the entire operation, including reading a completed file.
    tokio::select! {
        biased;
        _ = cancelled(state.phase.subscribe()) => Err(CANCELLED.into()),
        result = tokio::time::timeout(request.timeout, receive(&path, &state, request.max_bytes)) => match result {
            Ok(result) => result,
            Err(_) => {
                log::warn!("网页下载超时：仍在等待下载请求={}", matches!(*state.phase.borrow(), Phase::Waiting));
                Err(TIMED_OUT.into())
            }
        },
    }
}

#[cfg(test)]
#[path = "webview_download/tests.rs"]
mod tests;
