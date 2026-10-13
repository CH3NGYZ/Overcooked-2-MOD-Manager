use serde::Deserialize;
use std::path::PathBuf;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tauri::{
    AppHandle, Manager, Runtime, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use url::Url;

const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(250);
const EVALUATION_TIMEOUT: Duration = Duration::from_secs(2);
const WINDOW_PREFIX: &str = "gitee-discovery-";

pub(crate) const PAGE_PROBE: &str = r#"(() => {
  try {
    const list = document.querySelector('.explore-repo__list');
    if (!list || document.readyState === 'loading') return { state: 'waiting' };
    const pagination = document.querySelectorAll('.pagination, .pager, .pagination-container, a[rel=next]');
    const html = list.outerHTML + Array.from(pagination, node => node.outerHTML).join('');
    if (html.length > 2097152) return { state: 'oversized' };
    return { state: 'ready', url: location.href, html };
  } catch (_) { return { state: 'error' }; }
})()"#;

pub(crate) struct BrowserOptions {
    pub interaction_after: Duration,
    pub page_timeout: Duration,
}

impl Default for BrowserOptions {
    fn default() -> Self {
        Self {
            interaction_after: Duration::from_secs(8),
            page_timeout: Duration::from_secs(120),
        }
    }
}

#[derive(Deserialize)]
struct Snapshot {
    state: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    html: String,
}

fn same_topic(candidate: &Url, endpoint: &Url) -> bool {
    candidate.scheme() == endpoint.scheme()
        && candidate.host_str() == endpoint.host_str()
        && candidate.port_or_known_default() == endpoint.port_or_known_default()
        && candidate.username().is_empty()
        && candidate.password().is_none()
        && candidate.path().trim_end_matches('/') == endpoint.path().trim_end_matches('/')
}

fn page_number(url: &Url) -> Option<usize> {
    let pages = url
        .query_pairs()
        .filter(|(key, _)| key == "page")
        .map(|(_, value)| value.parse::<usize>().ok())
        .collect::<Vec<_>>();
    match pages.as_slice() {
        [] => Some(1),
        [Some(page)] if *page > 0 => Some(*page),
        _ => None,
    }
}

fn snapshot_html(raw: &str, expected: &Url) -> Result<Option<String>, String> {
    if raw.len() > MAX_HTML_BYTES * 6 + 1024 {
        return Err("Gitee 社区网页响应过大".into());
    }
    // Evaluation can be interrupted by the challenge's normal page reload.
    if matches!(raw, "null" | "") {
        return Ok(None);
    }
    let snapshot: Snapshot = serde_json::from_str(raw).map_err(|_| "Gitee 社区网页读取失败")?;
    match snapshot.state.as_str() {
        "waiting" => Ok(None),
        "oversized" => Err("Gitee 社区网页响应过大".into()),
        "ready" => {
            let actual = Url::parse(&snapshot.url).map_err(|_| "Gitee 社区网页地址无效")?;
            if !same_topic(&actual, expected) {
                return Err("Gitee 社区网页地址无效".into());
            }
            if page_number(&actual) != page_number(expected) {
                return Ok(None);
            }
            if snapshot.html.len() > MAX_HTML_BYTES {
                return Err("Gitee 社区网页响应过大".into());
            }
            Ok(Some(snapshot.html))
        }
        _ => Err("Gitee 社区网页读取失败".into()),
    }
}

pub(crate) struct TopicBrowser<R: Runtime> {
    window: WebviewWindow<R>,
    closed: Arc<AtomicBool>,
    options: BrowserOptions,
}

pub(crate) fn close_with_main<R: Runtime>(window: &tauri::Window<R>, event: &WindowEvent) {
    if window.label() != "main" || !matches!(event, WindowEvent::Destroyed) {
        return;
    }
    for (label, webview) in window.app_handle().webview_windows() {
        if label.starts_with(WINDOW_PREFIX) && webview.destroy().is_err() {
            log::debug!("管理器已关闭，Gitee 验证窗口已结束或无法再次关闭");
        }
    }
}

impl<R: Runtime> TopicBrowser<R> {
    pub(crate) fn open(
        app: &AppHandle<R>,
        initial: Url,
        data_directory: PathBuf,
        options: BrowserOptions,
    ) -> Result<Self, String> {
        std::fs::create_dir_all(&data_directory).map_err(|_| "无法创建 Gitee 验证浏览器")?;
        let endpoint = initial.clone();
        // No capability or remote IPC grant is added for this window. Rust reads
        // the DOM through native callbacks, never through commands exposed to Gitee.
        let window = WebviewWindowBuilder::new(
            app,
            format!("{WINDOW_PREFIX}{}", uuid::Uuid::new_v4()),
            WebviewUrl::External(initial),
        )
        .title("Gitee")
        .inner_size(1000.0, 720.0)
        .visible(false)
        .focused(false)
        .data_directory(data_directory)
        .on_navigation(move |url| same_topic(url, &endpoint))
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
        .devtools(false)
        .build()
        .map_err(|_| "无法创建 Gitee 验证浏览器")?;
        let closed = Arc::new(AtomicBool::new(false));
        let on_close = closed.clone();
        window.on_window_event(move |event| {
            if matches!(
                event,
                WindowEvent::CloseRequested { .. } | WindowEvent::Destroyed
            ) {
                on_close.store(true, Ordering::Release);
            }
        });
        Ok(Self {
            window,
            closed,
            options,
        })
    }

    pub(crate) async fn read_page(&self, url: Url, navigate: bool) -> Result<String, String> {
        if navigate {
            self.window
                .navigate(url.clone())
                .map_err(|_| "Gitee 社区网页读取失败")?;
        }
        let started = Instant::now();
        let mut shown = false;
        loop {
            if self.closed.load(Ordering::Acquire) {
                return Err("社区仓库搜索已取消".into());
            }
            if started.elapsed() >= self.options.page_timeout {
                return Err("Gitee 网页验证超时，请稍后刷新".into());
            }
            let (sender, receiver) = tokio::sync::oneshot::channel();
            let sender = Mutex::new(Some(sender));
            self.window
                .eval_with_callback(PAGE_PROBE, move |result| {
                    if let Ok(mut sender) = sender.lock() {
                        if let Some(sender) = sender.take() {
                            let _ = sender.send(result);
                        }
                    }
                })
                .map_err(|_| "Gitee 社区网页读取失败")?;
            if let Ok(Ok(raw)) = tokio::time::timeout(EVALUATION_TIMEOUT, receiver).await {
                if let Some(html) = snapshot_html(&raw, &url)? {
                    return Ok(html);
                }
            }
            if !shown && started.elapsed() >= self.options.interaction_after {
                self.window
                    .show()
                    .map_err(|_| "无法显示 Gitee 验证浏览器")?;
                let _ = self.window.set_focus();
                log::info!("Gitee 网页验证需要交互，已显示独立验证窗口；关闭窗口可取消搜索");
                shown = true;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }
}

impl<R: Runtime> Drop for TopicBrowser<R> {
    fn drop(&mut self) {
        if !self.closed.load(Ordering::Acquire) {
            if self.window.destroy().is_err() {
                log::debug!("Gitee 验证窗口已结束或无法再次关闭");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn navigation_and_results_reject_other_origins_credentials_and_paths() {
        let expected = Url::parse("https://gitee.com/explore/topic/overcooked-2?page=1").unwrap();
        for candidate in [
            "https://example.com/explore/topic/overcooked-2",
            "http://gitee.com/explore/topic/overcooked-2",
            "https://user@gitee.com/explore/topic/overcooked-2",
            "https://gitee.com:444/explore/topic/overcooked-2",
            "https://gitee.com/api/v5/repos",
            "https://gitee.com/login",
        ] {
            let url = Url::parse(candidate).unwrap();
            assert!(!same_topic(&url, &expected));
            assert!(
                snapshot_html(
                    &serde_json::json!({"state":"ready", "url":candidate, "html":"x"}).to_string(),
                    &expected
                )
                .is_err()
            );
        }
    }

    #[test]
    fn challenge_and_previous_page_never_become_successful_empty_results() {
        let expected = Url::parse("https://gitee.com/explore/topic/overcooked-2?page=2").unwrap();
        assert!(snapshot_html("null", &expected).unwrap().is_none());
        assert!(
            snapshot_html(r#"{"state":"waiting"}"#, &expected)
                .unwrap()
                .is_none()
        );
        let ready = |url| {
            serde_json::json!({"state":"ready", "url":url, "html":"<div class='explore-repo__list'></div>"}).to_string()
        };
        assert!(
            snapshot_html(
                &ready("https://gitee.com/explore/topic/overcooked-2"),
                &expected
            )
            .unwrap()
            .is_none()
        );
        assert!(
            snapshot_html(&ready(expected.as_str()), &expected)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            page_number(&Url::parse("https://gitee.com/?page=1&page=2").unwrap()),
            None
        );
    }

    #[test]
    fn snapshot_limits_count_utf8_bytes_and_reject_invalid_or_failed_evaluation() {
        let expected = Url::parse("https://gitee.com/explore/topic/overcooked-2").unwrap();
        for raw in [
            "not JSON",
            r#"{"state":"error"}"#,
            r#"{"state":"oversized"}"#,
        ] {
            assert!(snapshot_html(raw, &expected).is_err());
        }
        let raw = serde_json::json!({"state":"ready", "url":expected, "html":"界".repeat(MAX_HTML_BYTES / 3 + 1)}).to_string();
        assert!(
            snapshot_html(
                &raw,
                &Url::parse("https://gitee.com/explore/topic/overcooked-2").unwrap()
            )
            .is_err()
        );
    }
}
