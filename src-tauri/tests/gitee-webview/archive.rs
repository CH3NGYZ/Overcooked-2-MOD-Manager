//! Opt-in native acceptance only; the download and cleanup are production code.
use crate::webview_download::{self, Request};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Cursor;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use url::Url;

const ADDRESS: &str =
    "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/repository/archive/main.zip";
const DOWNLOAD_PATHS: [&str; 2] = [
    "/ch3ngyz/Overcooked-2-MOD-Manager-Repos/repository/download",
    "/ch3ngyz/Overcooked-2-MOD-Manager-Repos/repository/blazearchive/main.zip",
];
const MAX_BYTES: usize = 32 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(120);
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/157.0.0.0 Safari/537.36 Edg/157.0.0.0";
static EVENTS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LOGGER: DownloadLogger = DownloadLogger;

struct DownloadLogger;

impl log::Log for DownloadLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.target().ends_with("::webview_download")
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            // Production records only the hostname and path, never query tickets.
            EVENTS.lock().unwrap().push(record.args().to_string());
        }
    }

    fn flush(&self) {}
}

pub(super) fn install_logger() {
    log::set_logger(&LOGGER).expect("the isolated test owns its logger");
    log::set_max_level(log::LevelFilter::Info);
}

pub(super) fn events() -> Vec<String> {
    EVENTS.lock().unwrap().clone()
}

pub(super) fn inputs() -> Value {
    json!({
        "url": ADDRESS, "user_agent": USER_AGENT, "max_bytes": MAX_BYTES,
        "timeout_seconds": TIMEOUT.as_secs(), "allowed_download_hosts": ["gitee.com"],
        "allowed_download_paths": DOWNLOAD_PATHS,
        "title": "OC2 源码 ZIP 下载验收（独立窗口，仅此一轮）",
    })
}

pub(super) async fn verify(app: &AppHandle, directory: &Path) -> Result<Value, String> {
    let started = Instant::now();
    let download = webview_download::download(
        app,
        Request {
            url: Url::parse(ADDRESS).unwrap(),
            user_agent: USER_AGENT.to_owned(),
            max_bytes: MAX_BYTES,
            timeout: TIMEOUT,
            title: "OC2 源码 ZIP 下载验收（独立窗口，仅此一轮）".to_owned(),
            allowed_download_hosts: vec!["gitee.com".to_owned()],
            allowed_download_paths: DOWNLOAD_PATHS.into_iter().map(str::to_owned).collect(),
        },
    );
    tokio::pin!(download);
    let mut observed = Value::Null;
    let bytes = loop {
        tokio::select! {
            result = &mut download => break result?,
            _ = tokio::time::sleep(Duration::from_millis(50)) => {
                if observed.is_null() {
                    if let Some((label, window)) = app.webview_windows().into_iter()
                        .find(|(label, _)| label.starts_with("binary-download-")) {
                        if !window.is_visible().unwrap_or(false) { continue; }
                        let size = window.inner_size().map_err(|error| error.to_string())?;
                        observed = json!({"label":label, "title":window.title().ok(),
                            "visible":window.is_visible().ok(), "physical_width":size.width,
                            "physical_height":size.height, "scale_factor":window.scale_factor().ok()});
                        std::fs::write(directory.join("window.json"), serde_json::to_vec_pretty(&observed).unwrap())
                            .map_err(|error| error.to_string())?;
                    }
                }
            }
        }
    };
    let elapsed = started.elapsed().as_millis();
    let mut closed = false;
    for _ in 0..100 {
        closed = app
            .webview_windows()
            .keys()
            .all(|label| !label.starts_with("binary-download-"));
        if closed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    if !closed {
        return Err("the production download window did not close after completion".to_owned());
    }
    let callbacks = events();
    if !callbacks
        .iter()
        .any(|message| message.contains("已接管网页下载"))
        || !callbacks.iter().any(|message| {
            message.contains("网页下载结束")
                && message.contains("成功=true")
                && message.contains("匹配当前任务=true")
        })
    {
        return Err(
            "production Requested and successful Finished callbacks were not observed".to_owned(),
        );
    }
    let zip = zip::ZipArchive::new(Cursor::new(&bytes))
        .map_err(|_| "downloaded bytes are not a readable ZIP")?;
    let names = zip
        .file_names()
        .map(|name| name.map(|name| name.into_owned()))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "source ZIP member names are not readable")?;
    let root = names
        .iter()
        .filter(|name| name.rsplit('/').next() == Some("mods.toml"))
        .filter_map(|name| name.strip_suffix("mods.toml"))
        .collect::<Vec<_>>();
    if root.len() != 1 {
        return Err("the source ZIP must contain one mods.toml root".to_owned());
    }
    let required = [
        "mods.toml",
        "maps.toml",
        "proxies.toml",
        "network-plans.toml",
    ];
    if required
        .iter()
        .any(|name| !names.contains(&format!("{}{name}", root[0])))
    {
        return Err("the downloaded source ZIP is missing a required root document".to_owned());
    }
    std::fs::write(directory.join("source.zip"), &bytes).map_err(|error| error.to_string())?;
    Ok(json!({
        "runtime":"Tauri/Wry Windows WebView2", "production_download":true,
        "bytes":bytes.len(), "sha256":format!("{:x}", Sha256::digest(&bytes)),
        "elapsed_ms":elapsed, "zip_entries":zip.len(), "required_root_documents":required,
        "requested_callback":true, "finished_success_callback":true, "window_autoclosed":closed,
        "observed_window":observed, "remote_ipc_grants_added":false,
        "manager_configuration_loaded":false, "single_instance_plugin_loaded":false,
    }))
}
