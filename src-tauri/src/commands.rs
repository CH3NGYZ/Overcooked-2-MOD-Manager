use crate::archive;
use crate::archive::DownloadRoute as AssetDownloadRoute;
use crate::config::ConfigService;
use crate::display_path::display_path;
use crate::download::{
    DownloadCandidate as AssetDownloadCandidate, DownloadContext,
    try_download_candidates as try_asset_download_candidates,
};
use crate::game;
use crate::models::*;
use crate::mods;
use crate::recycle_bin;
use crate::releases;
use crate::repository_sources::{self, SourceLocation, source_address_key};
use chrono::Utc;
use futures_util::StreamExt;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, State};
use url::Url;
use uuid::Uuid;

#[cfg(test)]
use crate::download::candidate_progress as asset_candidate_progress;

#[cfg(test)]
#[path = "installer_update_tests.rs"]
mod installer_update_tests;

const INSTALLER_GITEE_OWNER: &str = "ch3ngyz";
const INSTALLER_GITEE_REPO: &str = "Overcooked-2-MOD-Manager";
const INSTALLER_GITHUB_OWNER: &str = "CH3NGYZ";
const INSTALLER_GITHUB_REPO: &str = "Overcooked-2-MOD-Manager";
#[derive(Debug, Clone)]
struct InstallerChannel {
    id: String,
    provider: Provider,
    endpoint: String,
    proxy: Option<GitHubProxy>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallerRegion {
    MainlandChina,
    International,
}

impl InstallerRegion {
    fn as_str(self) -> &'static str {
        match self {
            Self::MainlandChina => "mainland-china",
            Self::International => "international",
        }
    }
}

fn detect_installer_region() -> InstallerRegion {
    if crate::config::current_user_region() == crate::config::REGION_CHINESE_MAINLAND {
        InstallerRegion::MainlandChina
    } else {
        InstallerRegion::International
    }
}

fn installer_manifest_url(provider: &Provider) -> String {
    match provider {
        Provider::Gitee => format!(
            "https://gitee.com/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases/download/latest/latest.json"
        ),
        Provider::Github => format!(
            "https://github.com/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases/latest/download/latest.json"
        ),
    }
}

fn installer_sources_for_url(provider: &Provider, url: &str) -> Vec<InstallerChannel> {
    let mut sources = vec![InstallerChannel {
        id: if *provider == Provider::Gitee {
            "gitee-direct"
        } else {
            "github-direct"
        }
        .into(),
        provider: provider.clone(),
        endpoint: url.into(),
        proxy: None,
    }];
    if *provider == Provider::Gitee {
        return sources;
    }
    // Startup updates use only compiled-in Release templates, independently
    // of the user/remote proxy lists used by MOD downloads.
    let mut visited = HashSet::from([url.to_string()]);
    for proxy in crate::github_proxies::bundled_defaults().release_proxies {
        let Some(endpoint) =
            crate::github_proxies::proxy_urls(std::slice::from_ref(&proxy), url, false, None)
                .into_iter()
                .next()
        else {
            continue;
        };
        if visited.insert(endpoint.clone()) {
            sources.push(InstallerChannel {
                id: format!("github-proxy:{}", proxy.id),
                provider: Provider::Github,
                endpoint,
                proxy: Some(proxy),
            });
        }
    }
    sources
}

fn installer_manifest_sources(region: InstallerRegion) -> Vec<InstallerChannel> {
    let mut sources = Vec::new();
    if region == InstallerRegion::MainlandChina {
        sources.extend(installer_sources_for_url(
            &Provider::Gitee,
            &installer_manifest_url(&Provider::Gitee),
        ));
    }
    sources.extend(installer_sources_for_url(
        &Provider::Github,
        &installer_manifest_url(&Provider::Github),
    ));
    sources
}

fn installer_candidate(channel: &InstallerChannel) -> AssetDownloadCandidate {
    AssetDownloadCandidate {
        route: if channel.proxy.is_some() {
            AssetDownloadRoute::Proxy
        } else {
            AssetDownloadRoute::Direct
        },
        url: channel.endpoint.clone(),
    }
}

fn installer_download_candidates(info: &AppUpdateInfo) -> Vec<AssetDownloadCandidate> {
    match (&info.provider, &info.installer_url) {
        (Some(provider), Some(url)) => installer_sources_for_url(provider, url)
            .iter()
            .map(installer_candidate)
            .collect(),
        _ => Vec::new(),
    }
}

fn inspect_installer_manifest(
    channel: &InstallerChannel,
    document: &serde_json::Value,
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
) -> Result<Option<AppUpdateInfo>, String> {
    let manifest: tauri_plugin_updater::RemoteRelease = serde_json::from_value(document.clone())
        .map_err(|_| "update.manifestInvalid".to_string())?;
    if !manifest.version.pre.is_empty() {
        return Err("update.manifestNotStable".into());
    }
    let key = target.manifest_key();
    let url = manifest
        .download_url(&key)
        .map_err(|_| "update.noMatchingPackage")?;
    let signature = manifest
        .signature(&key)
        .map_err(|_| "update.manifestOrSignatureUnavailable")?
        .clone();
    let name = url
        .path_segments()
        .and_then(|mut parts| parts.next_back())
        .unwrap_or_default()
        .to_string();
    let (host, owner, repo) = match channel.provider {
        Provider::Github => ("github.com", INSTALLER_GITHUB_OWNER, INSTALLER_GITHUB_REPO),
        Provider::Gitee => ("gitee.com", INSTALLER_GITEE_OWNER, INSTALLER_GITEE_REPO),
    };
    let path_prefix = format!("/{owner}/{repo}/releases/download/");
    if url.scheme() != "https"
        || url.host_str() != Some(host)
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || !url.path().starts_with(&path_prefix)
        || !target.matches_name(&name, &manifest.version)
        || signature.trim().is_empty()
    {
        return Err("update.manifestPlatformMismatch".into());
    }
    if manifest.version <= *current_version {
        return Ok(None);
    }
    let direct_url = url.to_string();
    Ok(Some(AppUpdateInfo {
        version: manifest.version.to_string(),
        current_version: current_version.to_string(),
        body: manifest.notes,
        date: document["pub_date"].as_str().map(str::to_string),
        download_size: None,
        manifest_url: channel.endpoint.clone(),
        installer_url: Some(direct_url),
        installer_signature: signature.clone(),
        installer_name: name,
        installer_type: target.kind.into(),
        provider: Some(channel.provider.clone()),
        source_channel: Some(channel.id.clone()),
    }))
}

async fn fetch_installer_manifest(
    client: &reqwest::Client,
    channel: &InstallerChannel,
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
    is_cancelled: impl Fn() -> bool,
    reporter: Option<&archive::DownloadReporter>,
) -> Result<Option<AppUpdateInfo>, String> {
    const MAX_MANIFEST_BYTES: usize = 1024 * 1024;
    log::info!(
        "正在请求程序更新清单：渠道={}，地址={}",
        channel.id,
        repository_sources::safe_log_address(&channel.endpoint)
    );
    let request = async {
        let response = client
            .get(&channel.endpoint)
            .header("User-Agent", releases::APP_USER_AGENT)
            .header("Cache-Control", "no-cache, no-store")
            .header("Pragma", "no-cache")
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
            .map_err(|_| "update.manifestRequestFailed")?;
        if !response.status().is_success() {
            return Err(crate::locale_tag::with_params(
                "update.manifestHttpStatus",
                &[("status", &response.status().as_u16().to_string())],
            ));
        }
        let total = response.content_length();
        if total.is_some_and(|size| size > MAX_MANIFEST_BYTES as u64) {
            return Err("update.manifestTooLarge".into());
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "update.manifestReadFailed")?;
            if bytes.len().saturating_add(chunk.len()) > MAX_MANIFEST_BYTES {
                return Err("update.manifestTooLarge".into());
            }
            bytes.extend_from_slice(&chunk);
            if let Some(reporter) = reporter {
                reporter.received(chunk.len() as u64);
                let mut progress = reporter.snapshot();
                progress.downloaded = bytes.len() as u64;
                progress.total = total;
                reporter.update(progress);
            }
        }
        let document = serde_json::from_slice(&bytes).map_err(|_| "update.manifestInvalid")?;
        inspect_installer_manifest(channel, &document, target, current_version)
    };
    let cancellation = async {
        loop {
            if is_cancelled() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    tokio::select! {
        biased;
        _ = cancellation => Err("download.cancelled".into()),
        result = request => result,
    }
}

fn installer_channel_status(
    channel: &InstallerChannel,
    status: &str,
    error: Option<String>,
) -> AppUpdateChannelStatus {
    AppUpdateChannelStatus {
        id: channel.id.clone(),
        provider: channel.provider.clone(),
        mode: if channel.proxy.is_some() {
            "proxy"
        } else {
            "direct"
        }
        .into(),
        status: status.into(),
        error,
    }
}

fn emit_installer_channel_status(
    app: &AppHandle,
    region: InstallerRegion,
    status: AppUpdateChannelStatus,
) {
    match status.status.as_str() {
        "failed" => log::warn!(
            "安装器更新渠道失败: id={} mode={} error={}",
            status.id,
            status.mode,
            status.error.as_deref().unwrap_or_default()
        ),
        _ => log::info!(
            "安装器更新渠道状态: id={} mode={} status={}",
            status.id,
            status.mode,
            status.status
        ),
    }
    let _ = app.emit(
        "app-update-channel",
        AppUpdateChannelEvent {
            region: region.as_str().into(),
            channel: status,
        },
    );
}

fn emit_installer_phase(
    app: &AppHandle,
    region: InstallerRegion,
    phase: &str,
    message: Option<String>,
    update: Option<AppUpdateInfo>,
) {
    let _ = app.emit(
        "app-update-phase",
        AppUpdatePhaseEvent {
            region: region.as_str().into(),
            phase: phase.into(),
            message,
            update,
        },
    );
}

enum InstallerRouteResult<T> {
    Updated(T),
    Latest,
    Failed,
}

struct SelectedInstallerManifest {
    channel: InstallerChannel,
    info: Option<AppUpdateInfo>,
}

impl crate::download::DownloadOutput for SelectedInstallerManifest {
    fn discard(&self) {}
}

impl crate::download::DownloadOutput for tauri_plugin_updater::Update {
    fn discard(&self) {}
}

async fn race_installer_channels<T, F, Fut>(
    channels: &[InstallerChannel],
    is_cancelled: impl Fn() -> bool,
    mut fetch: F,
) -> Result<T, String>
where
    T: crate::download::DownloadOutput,
    F: FnMut(InstallerChannel, std::sync::Arc<archive::DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    if is_cancelled() {
        return Err("download.cancelled".into());
    }
    // Gitee never enters a parallel pool or uses proxy/range requests.
    if channels.len() == 1 && channels[0].provider == Provider::Gitee {
        return fetch(
            channels[0].clone(),
            std::sync::Arc::new(archive::DownloadReporter::new("")),
        )
        .await;
    }
    try_asset_download_candidates(
        channels.iter().map(installer_candidate).collect(),
        is_cancelled,
        |candidate, reporter| {
            let channel = channels
                .iter()
                .find(|channel| channel.endpoint == candidate.url)
                .unwrap();
            fetch(channel.clone(), reporter)
        },
        None,
    )
    .await
}

async fn run_installer_manifest_routes<T, F, Fut>(
    client: &reqwest::Client,
    channels: &[InstallerChannel],
    target: crate::app_update::UpdateTarget,
    current_version: &semver::Version,
    is_cancelled: impl Fn() -> bool,
    on_status: impl FnMut(AppUpdateChannelStatus),
    mut on_update: F,
) -> Result<InstallerRouteResult<T>, String>
where
    F: FnMut(AppUpdateInfo) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
{
    let on_status = Mutex::new(on_status);
    for provider in [Provider::Gitee, Provider::Github] {
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        let group = channels
            .iter()
            .filter(|channel| channel.provider == provider)
            .cloned()
            .collect::<Vec<_>>();
        if group.is_empty() {
            continue;
        }
        let started = Mutex::new(HashSet::new());
        let finished = Mutex::new(HashSet::new());
        let selected = race_installer_channels(&group, &is_cancelled, |channel, reporter| {
            started.lock().unwrap().insert(channel.id.clone());
            on_status.lock().unwrap()(installer_channel_status(&channel, "checking", None));
            let (on_status, finished, is_cancelled) = (&on_status, &finished, &is_cancelled);
            async move {
                let result = fetch_installer_manifest(
                    client,
                    &channel,
                    target,
                    current_version,
                    is_cancelled,
                    Some(&reporter),
                )
                .await;
                finished.lock().unwrap().insert(channel.id.clone());
                match result {
                    Ok(info) => {
                        on_status.lock().unwrap()(installer_channel_status(
                            &channel,
                            if info.is_some() {
                                "update_available"
                            } else {
                                "latest"
                            },
                            None,
                        ));
                        Ok(SelectedInstallerManifest { channel, info })
                    }
                    Err(error) => {
                        on_status.lock().unwrap()(installer_channel_status(
                            &channel,
                            "failed",
                            Some(error.clone()),
                        ));
                        Err(error)
                    }
                }
            }
        })
        .await;
        // The coordinator has dropped losers before their final UI state.
        for channel in &group {
            if !finished.lock().unwrap().contains(&channel.id) {
                let status = if started.lock().unwrap().contains(&channel.id) {
                    "cancelled"
                } else {
                    "skipped"
                };
                on_status.lock().unwrap()(installer_channel_status(channel, status, None));
            }
        }
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        let selected = match selected {
            Ok(selected) => selected,
            Err(error) if error == "download.cancelled" => return Err(error),
            Err(_) => continue,
        };
        let result = match selected.info {
            Some(info) => on_update(info).await.map(InstallerRouteResult::Updated),
            None => Ok(InstallerRouteResult::Latest),
        };
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        match result {
            Ok(result) => {
                for skipped in channels.iter().filter(|channel| {
                    provider == Provider::Gitee && channel.provider == Provider::Github
                }) {
                    on_status.lock().unwrap()(installer_channel_status(skipped, "skipped", None));
                }
                return Ok(result);
            }
            Err(error) if error == "download.cancelled" => return Err(error),
            Err(error) => on_status.lock().unwrap()(installer_channel_status(
                &selected.channel,
                "failed",
                Some(error),
            )),
        }
    }
    Ok(InstallerRouteResult::Failed)
}

async fn discover_installer_update(
    state: &AppState,
    app: &AppHandle,
) -> Result<AppUpdateCheckResult, String> {
    let region = detect_installer_region();
    emit_installer_phase(app, region, "manifest", None, None);
    let config = state.config.load_or_default();
    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let target = crate::app_update::UpdateTarget::current()?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|e| {
        crate::locale_tag::with_params(
            "update.currentVersionInvalid",
            &[("reason", &e.to_string())],
        )
    })?;
    let mut statuses: Vec<AppUpdateChannelStatus> = Vec::new();
    let result = run_installer_manifest_routes(
        &client,
        &installer_manifest_sources(region),
        target,
        &current_version,
        || false,
        |status| {
            emit_installer_channel_status(app, region, status.clone());
            if let Some(previous) = statuses
                .iter_mut()
                .find(|previous| previous.id == status.id)
            {
                *previous = status;
            } else {
                statuses.push(status);
            }
        },
        |info| std::future::ready(Ok(info)),
    )
    .await?;
    let all_failed = matches!(result, InstallerRouteResult::Failed);
    let update = match result {
        InstallerRouteResult::Updated(info) => Some(info),
        _ => None,
    };
    Ok(AppUpdateCheckResult {
        update,
        channels: statuses,
        all_failed,
        region: region.as_str().into(),
    })
}

/// 应用状态：配置、串行化锁与几个进程内缓存。
///
/// `Clone` 是给重活用的：命令层把 `State` 克隆一份带进 `spawn_blocking`，
/// 闭包里既能拿配置也能拿操作锁，不必为每个命令重写一遍参数列表。克隆的
/// 都是句柄（配置服务与各 `Arc`），共享同一份数据，不是拷贝。
#[derive(Clone)]
pub struct AppState {
    pub config: ConfigService,
    /*
     * 串行化「写游戏目录」的操作（安装、卸载、开关、回收站）。
     *
     * `Arc` 而不是裸 `Mutex`：重活（解压落盘、全量扫描）必须搬到
     * `spawn_blocking` 里跑，锁的所有权也要跟着进闭包；同步命令在 IPC 线程
     * 上内联执行，几秒的写盘就是几秒的窗口冻结。
     */
    pub operation: Arc<Mutex<()>>,
    pub repository_cache: Arc<tokio::sync::Mutex<()>>,
    pub community: Arc<crate::community::CommunitySearch>,
    pub readme: Arc<crate::repository_readme::ReadmeDownloads>,
    // Display-only snapshot; file operations always inspect the live process.
    pub game_state: Arc<Mutex<Option<GameInfo>>>,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        log::debug!("创建应用状态");
        Ok(Self {
            config: ConfigService::new().map_err(|e| e.to_string())?,
            operation: Arc::new(Mutex::new(())),
            repository_cache: Arc::new(tokio::sync::Mutex::new(())),
            community: Arc::new(crate::community::CommunitySearch::default()),
            readme: Arc::new(crate::repository_readme::ReadmeDownloads::default()),
            game_state: Arc::new(Mutex::new(None)),
        })
    }
}

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardData, String> {
    /*
     * 第一次扫描要读全部 DLL 的元数据（之后走进程内缓存），debug 构建下能到
     * 1 s 以上；同步命令在 IPC 线程上内联执行，这一段就是窗口卡死。搬到
     * 阻塞线程池后耗时不变，但界面还能转圈、拖窗口。
     */
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || get_dashboard_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`get_dashboard`] 的同步实现（在阻塞线程池里跑）。
fn get_dashboard_blocking(state: &AppState) -> Result<DashboardData, String> {
    log::debug!("读取状态总览");
    let config = state.config.load_or_default();
    let game_info = game::detect_game(&config);
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(game_info.clone());
    let mods = game_info
        .path
        .as_ref()
        .map(|path| {
            mods::scan_mods_with_manifests(
                Path::new(path),
                &config,
                Some(&state.config.manifests_dir()),
            )
        })
        .unwrap_or_default();
    let updates = collect_update_status(&config, &mods);
    log::info!(
        "状态总览读取完成：已安装 MOD={}，更新状态={}，游戏已检测={}",
        mods.len(),
        updates.len(),
        game_info.path.is_some()
    );
    Ok(DashboardData {
        config: state.config.public_config(&config),
        game: game_info,
        mods,
        updates,
    })
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<PublicConfig, String> {
    log::debug!("读取应用配置");
    // Source imports and proxy sync can commit concurrently with this read.
    // Config loading may migrate and save, so share their short commit lock.
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn get_install_language() -> Option<String> {
    log::trace!("读取安装语言标记");
    let path = crate::user_paths::root().ok()?.join("install-language.txt");
    let value = fs::read_to_string(path).ok()?.trim().to_string();
    match value.as_str() {
        "zh-CN" | "zh-TW" | "en" | "ko" | "ja" => Some(value),
        _ => None,
    }
}

#[tauri::command]
pub fn read_rust_log() -> Result<RustLog, String> {
    read_log_file("rust.log")
}

#[tauri::command]
pub fn read_frontend_log() -> Result<RustLog, String> {
    read_log_file("frontend.log")
}

#[tauri::command]
pub fn read_crash_log() -> Result<RustLog, String> {
    read_log_file("crash.log")
}

/// 清空一份日志，并把清空后的内容回读给前端。
///
/// 回读而不是让前端自己再调一次 read：两次 IPC 之间日志插件可能又写进新行，
/// 界面就会显示「点了清空但内容还在」。一次调用拿到的就是那一刻的文件内容。
#[tauri::command]
pub fn clear_log(kind: String) -> Result<RustLog, String> {
    let file_name = log_file_name(&kind)?;
    let path = crate::user_paths::logs_dir()?.join(file_name);
    clear_log_at_path(&path)?;
    // 先回读再记这次清空：回读要给前端一个真正的空内容，而「日志被清空过」
    // 这件事必须留在文件里 —— 否则诊断时只看到日志从中间开始，却没有任何
    // 线索说明是谁在什么时候清的。
    let log = read_log_at_path(&path)?;
    log::info!("清空日志：文件={file_name}");
    Ok(log)
}

/// kind → 文件名。白名单，不接受调用方直接给文件名：
/// `logs_dir().join(任意字符串)` 里的 `..` 能逃出日志目录，而清空不可逆。
fn log_file_name(kind: &str) -> Result<&'static str, String> {
    match kind {
        "rust" => Ok("rust.log"),
        "frontend" => Ok("frontend.log"),
        "crash" => Ok("crash.log"),
        _ => Err("log.unknownKind".to_string()),
    }
}

fn read_log_file(file_name: &str) -> Result<RustLog, String> {
    let path = crate::user_paths::logs_dir()?.join(file_name);
    read_log_at_path(&path)
}

/// Reading diagnostics must never append diagnostics about the read itself.
/// Seek to the tail before reading so large logs do not allocate the full file.
fn read_log_at_path(path: &Path) -> Result<RustLog, String> {
    let content = match fs::File::open(path) {
        Ok(mut file) => {
            const MAX_LOG_BYTES: u64 = 512 * 1024;
            let length = file.metadata().map_err(|error| error.to_string())?.len();
            let start = length.saturating_sub(MAX_LOG_BYTES);
            file.seek(SeekFrom::Start(start))
                .map_err(|error| error.to_string())?;
            let mut bytes = Vec::with_capacity(length.min(MAX_LOG_BYTES) as usize);
            file.take(MAX_LOG_BYTES)
                .read_to_end(&mut bytes)
                .map_err(|error| error.to_string())?;
            String::from_utf8_lossy(&bytes).into_owned()
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.to_string()),
    };
    Ok(RustLog {
        path: display_path(path),
        content,
    })
}

/// 把日志文件截断到 0 字节。
///
/// rust.log 正被日志插件以 append 模式持有：外部截断是安全的，插件的写入走
/// O_APPEND 语义，截断后自然从头写。唯一的副作用是插件内部的字节计数不会
/// 跟着重置，可能让下一次轮转提前发生一次 —— 不丢日志，只多一个归档文件。
/// crash.log / frontend.log 每次写入才打开，没有长期句柄。
fn clear_log_at_path(path: &Path) -> Result<(), String> {
    match OpenOptions::new().write(true).truncate(true).open(path) {
        Ok(_) => Ok(()),
        // 文件本来就不存在，等价于已经是空的
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

#[tauri::command]
pub fn write_frontend_log(level: String, message: String) -> Result<(), String> {
    log::trace!("写入前端日志：等级={}，长度={}", level, message.len());
    const MAX_FRONTEND_LOG_BYTES: usize = 64 * 1024;
    let message = if message.len() > MAX_FRONTEND_LOG_BYTES {
        crate::locale_tag::with_params(
            "log.frontendTruncated",
            &[(
                "text",
                &&message[..message.floor_char_boundary(MAX_FRONTEND_LOG_BYTES)].to_string(),
            )],
        )
    } else {
        message
    };
    let level = level.to_ascii_lowercase();
    let path = crate::user_paths::logs_dir()?;
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path.join("frontend.log"))
        .map_err(|error| error.to_string())?;
    writeln!(
        file,
        "[{}][{}] {}",
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        level,
        message
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn save_mod_note(
    state: State<'_, AppState>,
    dll_file: String,
    note: String,
) -> Result<OperationResult, String> {
    log::info!("保存 MOD 备注：文件={}", dll_file);
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let installed = mods::scan_mods_with_manifests(
        Path::new(&game_root),
        &config,
        Some(&state.config.manifests_dir()),
    );
    let selected = installed
        .iter()
        .find(|item| item.dll_file == dll_file)
        .ok_or("mods.installedNotFound")?;
    let note_key = mods::note_key(selected);
    // Scan outside the configuration transaction, then modify the latest
    // snapshot so a concurrent source/proxy refresh is preserved.
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    state.config.set_mod_note(&mut config, &note_key, &note);
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!(
        "MOD 备注保存完成：文件={}，已清空={}",
        dll_file,
        note.trim().is_empty()
    );
    Ok(OperationResult {
        ok: true,
        message: if note.trim().is_empty() {
            "mods.noteCleared".into()
        } else {
            "mods.noteSaved".into()
        },
    })
}

#[tauri::command]
pub async fn install_local_mod(
    state: State<'_, AppState>,
    source_path: String,
) -> Result<OperationResult, String> {
    // 解压 ZIP 落盘是重活：同步命令在 IPC 线程上跑，界面会整段冻住
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        install_manual_mod(&state, &PathBuf::from(source_path), None)
    })
    .await
    .map_err(|error| error.to_string())?
}

pub(crate) fn install_manual_mod(
    state: &AppState,
    source: &Path,
    expected_game_root: Option<&Path>,
) -> Result<OperationResult, String> {
    log::info!("开始安装手动 MOD");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if expected_game_root.is_some_and(|expected| expected != game_root) {
        return Err("game.directoryChanged".into());
    }
    if !source.is_file() {
        return Err("mods.fileMissing".into());
    }
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("mods.invalidFileName")?;
    let extension = source
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    log::debug!(
        "本地 MOD 文件已确认：名称={}，类型={}",
        file_name,
        extension
    );
    let repository_id = format!("manual-{}", Uuid::new_v4());
    let (pending, display_name) = if extension.eq_ignore_ascii_case("dll") {
        log::debug!("按单 DLL 规则安装到 BepInEx/plugins：名称={}", file_name);
        /*
         * 显示名优先用 DLL 里的 BepInPlugin 名称 —— 用户认的是插件名，
         * 不是 `XXX.dll` 这样的文件名。读不出元数据时退回文件名去扩展名。
         * 仓库安装那边用仓库名，两条路径的提示体验保持一致。
         */
        let display =
            mods::plugin_name_from_file(&source).unwrap_or_else(|| mods::file_stem_name(file_name));
        let pending =
            archive::copy_asset_pending(&source, &game_root, "BepInEx/plugins", file_name)?;
        (pending, display)
    } else if extension.eq_ignore_ascii_case("zip") {
        log::debug!("按本地 ZIP 布局规则解压：名称={}", file_name);
        let folder = source
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("archive.invalidZipName")?;
        if folder.is_empty()
            || folder.contains('/')
            || folder.contains('\\')
            || folder.contains(':')
        {
            return Err("archive.invalidZipName".into());
        }
        let pending = archive::extract_local_mod_zip_pending(&source, &game_root, file_name)?;
        /*
         * 显示名：优先包内第一个 DLL 的 BepInPlugin 名称，退回 ZIP 文件名。
         * pending 的文件此时已落盘，可以直接读元数据。
         */
        let display = pending
            .files()
            .iter()
            .filter(|relative| relative.to_ascii_lowercase().ends_with(".dll"))
            .find_map(|relative| mods::plugin_name_from_file(&game_root.join(relative)))
            .unwrap_or_else(|| folder.to_string());
        (pending, display)
    } else {
        return Err("mods.unsupportedFileType".into());
    };
    if pending.files().is_empty() {
        log::warn!("本地 MOD 没有可安装内容：名称={}", file_name);
        return Err("archive.noInstallableContent".into());
    }
    let manifest = archive::new_manifest(
        &repository_id,
        // release_tag 是「Release 版本标签」，手动安装没有 Release，保持空字符串。
        // 之前这里塞了文件名，语义是错的，还会被更新判定相关的逻辑当成版本号尝试解析。
        "",
        // 手动安装没有仓库，包名与备注只有当前语言这一种写法
        archive::single_language(&display_name),
        archive::single_language(file_name),
        pending.files().to_vec(),
        Vec::new(),
    );
    if let Err(error) = archive::write_manifest(&state.config.manifests_dir(), &manifest) {
        log::error!(
            "本地 MOD 安装清单写入失败，已尝试回滚：名称={}，原因={}",
            display_name,
            error
        );
        return Err(error);
    }
    let files = pending.commit();
    log::info!(
        "本地 MOD 安装完成：名称={}，文件数={}",
        display_name,
        files.len()
    );
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "mods.installCompleted",
            &[("name", &display_name.to_string())],
        ),
    })
}

/// 是否需要先让用户确认「连整个 MOD 包一起删」。
///
/// 三个条件同时成立才需要：包内还有其他 MOD、没有同 GUID 的其他启用副本
/// （那种情况走单 DLL 回收分支），并且用户点的**不是**整包删除
/// （`allow_unmanaged = false`）。
///
/// 抽成函数是为了能被单测直接覆盖：命令本身要真实游戏目录和锁，测不到。
fn needs_package_confirmation(
    package_mod_names: &[String],
    has_other_enabled_same_guid: bool,
    allow_unmanaged: bool,
) -> bool {
    !package_mod_names.is_empty() && !has_other_enabled_same_guid && !allow_unmanaged
}

#[tauri::command]
pub async fn uninstall_local_mod(
    state: State<'_, AppState>,
    dll_file: String,
    allow_unmanaged: bool,
) -> Result<UninstallLocalModResult, String> {
    // 判定「要不要整包确认」要先全量扫描已安装 MOD，再加上回收站的移动：重活
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall_local_mod_blocking(&state, &dll_file, allow_unmanaged)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`uninstall_local_mod`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_local_mod_blocking(
    state: &AppState,
    dll_file: &str,
    allow_unmanaged: bool,
) -> Result<UninstallLocalModResult, String> {
    log::info!(
        "开始卸载本地 MOD：文件={}，允许非管理器文件={}",
        dll_file,
        allow_unmanaged
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest_dir = state.config.manifests_dir();
    let manifests = archive::read_manifests(&manifest_dir);
    let relative = Path::new(&dll_file);
    let safe_relative = mods::canonical_dll_name(&relative.to_string_lossy().replace('\\', "/"));
    let manifest = manifests
        .into_iter()
        .filter(|manifest| {
            manifest
                .files
                .iter()
                .any(|file| mods::canonical_dll_name(file).eq_ignore_ascii_case(&safe_relative))
        })
        .max_by(|left, right| left.installed_at.cmp(&right.installed_at));
    if let Some(manifest) = manifest {
        let installed = mods::scan_mods_with_manifests(&game_root, &config, Some(&manifest_dir));
        let target_mod = installed.iter().find(|item| {
            mods::canonical_dll_name(&item.dll_file).eq_ignore_ascii_case(&safe_relative)
        });
        let has_other_enabled_same_guid = target_mod
            .and_then(|target| target.guid.as_deref())
            .map(|guid| {
                installed.iter().any(|item| {
                    item.enabled
                        && !item.dll_file.eq_ignore_ascii_case(&safe_relative)
                        && item.guid.as_deref().is_some_and(|peer_guid| {
                            peer_guid.trim().eq_ignore_ascii_case(guid.trim())
                        })
                })
            })
            .unwrap_or(false);
        let package_mod_names = installed
            .iter()
            .filter(|item| item.package_id.as_deref() == Some(manifest.repository_id.as_str()))
            .filter(|item| !item.dll_file.eq_ignore_ascii_case(&safe_relative))
            .map(|item| item.name.clone())
            .collect::<Vec<_>>();
        /*
         * 包确认只在「用户点的是单个 MOD」时需要。
         *
         * `allow_unmanaged = true` 有两个来源：一是用户已在「整包删除」二级
         * 确认里点过确定；二是冲突卸载/安装计划里本来就按整包语义处理。
         * 两种情况都已经表达过「我要连包一起删」，再返回一次
         * requires_package_confirmation 只会让包标题上的「删除」按钮
         * 又弹一遍「当前 MOD 与其他 MOD 一并安装」——真实 bug。
         */
        if needs_package_confirmation(
            &package_mod_names,
            has_other_enabled_same_guid,
            allow_unmanaged,
        ) {
            log::warn!(
                "卸载本地 MOD 需要确认整个 MOD 包：文件={}，包={}，同包项目数={}",
                safe_relative,
                manifest.repository_id,
                package_mod_names.len()
            );
            return Ok(UninstallLocalModResult {
                ok: false,
                message: "mods.packageConfirmation".into(),
                requires_confirmation: false,
                requires_package_confirmation: true,
                package_id: Some(manifest.repository_id),
                package_mod_names,
            });
        }
        if has_other_enabled_same_guid {
            /*
             * 用户关闭「删除 MOD 时优先放入回收站」后走 purge_* 系列：
             * 同一套定位与共享文件保护逻辑，只是不留回收记录。
             */
            if config.recycle_deleted_mods {
                recycle_bin::recycle_mod_dll(
                    &state.config.mod_recycle_bin_dir(),
                    &manifest_dir,
                    &game_root,
                    &manifest.repository_id,
                    &safe_relative,
                    &installed,
                )?;
            } else {
                recycle_bin::purge_mod_dll(
                    &state.config.mod_recycle_bin_dir(),
                    &manifest_dir,
                    &game_root,
                    &manifest.repository_id,
                    &safe_relative,
                    &installed,
                )?;
            }
            log::info!(
                "已回收 MOD 包中的单个 DLL：文件={}，包={}，进回收站={}",
                safe_relative,
                manifest.repository_id,
                config.recycle_deleted_mods
            );
            return Ok(UninstallLocalModResult {
                ok: true,
                message: if config.recycle_deleted_mods {
                    "message.mods.movedToRecycleBin".into()
                } else {
                    "message.mods.deleted".into()
                },
                requires_confirmation: false,
                requires_package_confirmation: false,
                package_id: Some(manifest.repository_id),
                package_mod_names,
            });
        }
        if config.recycle_deleted_mods {
            recycle_bin::recycle_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &manifest.repository_id,
                &safe_relative,
                true,
            )?;
        } else {
            recycle_bin::purge_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &manifest.repository_id,
                &safe_relative,
                true,
            )?;
        }
        log::info!(
            "已卸载 MOD 包：包={}，文件={}，进回收站={}",
            manifest.repository_id,
            safe_relative,
            config.recycle_deleted_mods
        );
        return Ok(UninstallLocalModResult {
            ok: true,
            message: if config.recycle_deleted_mods {
                "message.mods.packageMovedToRecycleBin".into()
            } else {
                "message.mods.packageDeleted".into()
            },
            requires_confirmation: false,
            requires_package_confirmation: false,
            package_id: None,
            package_mod_names: Vec::new(),
        });
    }
    if !allow_unmanaged {
        log::warn!(
            "发现未由管理器安装的 MOD，等待用户确认：文件={}",
            safe_relative
        );
        return Ok(UninstallLocalModResult {
            ok: false,
            message: "mods.unmanagedConfirmation".into(),
            requires_confirmation: true,
            requires_package_confirmation: false,
            package_id: None,
            package_mod_names: Vec::new(),
        });
    }
    if config.recycle_deleted_mods {
        recycle_bin::recycle_unmanaged(
            &state.config.mod_recycle_bin_dir(),
            &game_root,
            &safe_relative,
        )?;
    } else {
        recycle_bin::purge_unmanaged(
            &state.config.mod_recycle_bin_dir(),
            &game_root,
            &safe_relative,
        )?;
    }
    log::info!(
        "已删除未托管 MOD DLL：文件={}，进回收站={}",
        safe_relative,
        config.recycle_deleted_mods
    );
    Ok(UninstallLocalModResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.mods.movedToRecycleBin".into()
        } else {
            "message.mods.deleted".into()
        },
        requires_confirmation: false,
        requires_package_confirmation: false,
        package_id: None,
        package_mod_names: Vec::new(),
    })
}

#[tauri::command]
pub async fn uninstall_mod_dll(
    state: State<'_, AppState>,
    dll_file: String,
    package_id: String,
) -> Result<OperationResult, String> {
    // 与 `uninstall_local_mod` 同一条规则：扫描 + 回收是重活，别占 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall_mod_dll_blocking(&state, &dll_file, &package_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`uninstall_mod_dll`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_mod_dll_blocking(
    state: &AppState,
    dll_file: &str,
    package_id: &str,
) -> Result<OperationResult, String> {
    log::info!(
        "开始删除 MOD 包内 DLL：包={}，文件={}",
        package_id,
        dll_file
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest_dir = state.config.manifests_dir();
    let manifest = archive::read_manifest(&manifest_dir, &package_id).ok_or("mods.unknownFiles")?;
    let safe_relative =
        mods::canonical_dll_name(&Path::new(&dll_file).to_string_lossy().replace('\\', "/"));
    if !manifest
        .files
        .iter()
        .any(|file| mods::canonical_dll_name(file).eq_ignore_ascii_case(&safe_relative))
    {
        return Err("mods.dllNotInPackage".into());
    }
    let installed = mods::scan_mods_with_manifests(&game_root, &config, Some(&manifest_dir));
    let package_mod_count = installed
        .iter()
        .filter(|item| item.package_id.as_deref() == Some(package_id))
        .count();
    let safe_duplicate = installed
        .iter()
        .find(|item| mods::canonical_dll_name(&item.dll_file).eq_ignore_ascii_case(&safe_relative))
        .is_some_and(|target| {
            installed
                .iter()
                .any(|peer| mods::is_safe_replacement_mod(target, peer))
        });
    if package_mod_count <= 1 && !safe_duplicate {
        if config.recycle_deleted_mods {
            recycle_bin::recycle_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &package_id,
                &safe_relative,
                true,
            )?;
        } else {
            recycle_bin::purge_selected_mod(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &package_id,
                &safe_relative,
                true,
            )?;
        }
        log::info!(
            "已删除 MOD 包最后一个 DLL：包={}，进回收站={}",
            package_id,
            config.recycle_deleted_mods
        );
        return Ok(OperationResult {
            ok: true,
            message: if config.recycle_deleted_mods {
                "message.mods.packageMovedToRecycleBin".into()
            } else {
                "message.mods.packageDeleted".into()
            },
        });
    }
    if config.recycle_deleted_mods {
        recycle_bin::recycle_selected_mod(
            &state.config.mod_recycle_bin_dir(),
            &manifest_dir,
            &game_root,
            &package_id,
            &safe_relative,
            false,
        )?;
    } else {
        recycle_bin::purge_selected_mod(
            &state.config.mod_recycle_bin_dir(),
            &manifest_dir,
            &game_root,
            &package_id,
            &safe_relative,
            false,
        )?;
    }
    log::info!(
        "已删除 MOD 包内 DLL：包={}，进回收站={}",
        package_id,
        config.recycle_deleted_mods
    );
    Ok(OperationResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.mods.movedToRecycleBin".into()
        } else {
            "message.mods.deleted".into()
        },
    })
}

#[tauri::command]
pub fn toggle_local_mod(
    state: State<'_, AppState>,
    dll_file: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换本地 MOD：文件={}，启用={}", dll_file, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let relative = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    toggle_local_mod_file(&game_root, &relative, enabled)?;
    log::info!("本地 MOD 状态切换完成：文件={}，启用={}", relative, enabled);
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "mods.enabled".into()
        } else {
            "mods.disabled".into()
        },
    })
}

fn set_local_mod_file_state(
    game_root: &Path,
    relative: &str,
    enabled: bool,
) -> Result<bool, String> {
    if !Path::new(&relative)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
    {
        return Err("mods.notInPlugins".into());
    }
    let plugins_root = game::safe_game_path(&game_root, "BepInEx/plugins")?;
    let enabled_path = game::safe_game_path(&game_root, &relative)?;
    let disabled_path = game::safe_game_path(&game_root, &mods::disabled_dll_file(&relative))?;
    if !enabled_path.starts_with(&plugins_root) || !disabled_path.starts_with(&plugins_root) {
        return Err("mods.notInPlugins".into());
    }
    let (source, target) = if enabled {
        (&disabled_path, &enabled_path)
    } else {
        (&enabled_path, &disabled_path)
    };
    let source_exists = source.is_file();
    let target_exists = target.is_file();
    match (source_exists, target_exists) {
        (false, true) => return Ok(false),
        (false, false) => {
            return Err(if enabled {
                "mods.disabledDllMissing"
            } else {
                "mods.enabledDllMissing"
            }
            .into());
        }
        (true, true) => return Err("mods.toggleTargetExists".into()),
        (true, false) => {}
    }
    archive::clear_readonly(source)?;
    fs::rename(source, target).map_err(|error| error.to_string())?;
    Ok(true)
}

fn toggle_local_mod_file(game_root: &Path, relative: &str, enabled: bool) -> Result<(), String> {
    set_local_mod_file_state(game_root, relative, enabled).map(|_| ())
}

#[tauri::command]
pub fn toggle_local_mod_package(
    state: State<'_, AppState>,
    package_id: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换 MOD 包：包={}，启用={}", package_id, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &package_id)
        .ok_or("mods.packageManifestMissing")?;
    let mut dll_files = manifest
        .files
        .iter()
        .map(|file| mods::canonical_dll_name(file))
        .filter(|file| {
            file.to_ascii_lowercase().starts_with("bepinex/plugins/")
                && Path::new(file)
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
        })
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    dll_files.sort_by_key(|file| file.to_ascii_lowercase());
    if dll_files.is_empty() {
        return Err("mods.packageNoToggleableDll".into());
    }
    let mut changed = Vec::new();
    for dll_file in dll_files {
        match set_local_mod_file_state(&game_root, &dll_file, enabled) {
            Ok(true) => changed.push(dll_file),
            Ok(false) => {}
            Err(error) => {
                log::error!(
                    "切换 MOD 包失败，开始回滚：包={}，原因={}",
                    package_id,
                    error
                );
                for previous in changed.into_iter().rev() {
                    let _ = toggle_local_mod_file(&game_root, &previous, !enabled);
                }
                return Err(error);
            }
        }
    }
    log::info!(
        "MOD 包状态切换完成：包={}，启用={}，变更文件数={}",
        package_id,
        enabled,
        changed.len()
    );
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "mods.packageEnabled".into()
        } else {
            "mods.packageDisabled".into()
        },
    })
}

/// 一次性启用或禁用**全部**已安装的插件 DLL。
///
/// 与单包切换共用 `set_local_mod_file_state`（负责路径归属校验、
/// `.disabled` 成对检查与只读位清理），因此安全边界完全一致。
///
/// 失败时回滚已经改过的文件：批量操作必须要么全成、要么全不动。
/// 不回滚会留下「一半启用一半禁用」的中间态，用户看到的是随机结果。
///
/// 不切换 `BepInEx/plugins` 之外的 DLL（`patchers`、`core` 里的管理器自身
/// 组件等）：那些不是 MOD，改了会破坏框架。
#[tauri::command]
pub fn set_all_local_mods_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("批量切换全部本地 MOD：启用={}", enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    /*
     * 直接从磁盘扫描，不依赖 Dashboard 快照：
     * 快照可能过期，而用户点「全部启用」时期望覆盖磁盘上真实存在的每一个插件。
     */
    let dll_files = mods::plugin_dll_files(&game_root)?;
    if dll_files.is_empty() {
        return Err("mods.noPluginDll".into());
    }

    let mut changed = Vec::new();
    for dll_file in dll_files {
        // 统一成启用态路径再交给状态机：它自己会按 `.disabled` 找源文件
        let relative = dll_file.trim_end_matches(".disabled").to_string();
        match set_local_mod_file_state(&game_root, &relative, enabled) {
            Ok(true) => changed.push(relative),
            Ok(false) => {}
            Err(error) => {
                log::error!("批量切换失败，开始回滚：文件={}，原因={}", relative, error);
                for previous in changed.into_iter().rev() {
                    let _ = toggle_local_mod_file(&game_root, &previous, !enabled);
                }
                return Err(crate::locale_tag::with_params(
                    "mods.toggleFailed",
                    &[
                        ("relative", &relative.to_string()),
                        ("error", &error.to_string()),
                    ],
                ));
            }
        }
    }
    log::info!(
        "批量切换完成：启用={}，变更文件数={}",
        enabled,
        changed.len()
    );
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            if enabled {
                "message.mods.bulkToggleEnabled"
            } else {
                "message.mods.bulkToggleDisabled"
            },
            &[("count", &changed.len().to_string())],
        ),
    })
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<PublicConfig, String> {
    log::info!("保存应用设置");
    let previous_game = if let Some(runtime) = patch.game_runtime {
        if cfg!(windows) && runtime != GameRuntime::Native {
            return Err("game.windowsOnly".into());
        }
        let previous = state.config.load_or_default();
        if previous.game.runtime != runtime {
            if let Some(path) = game::detect_game(&previous).path {
                game::ensure_game_stopped(Path::new(&path))?;
            }
        }
        Some(previous.game)
    } else {
        None
    };
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    if previous_game
        .as_ref()
        .is_some_and(|previous| *previous != config.game)
    {
        return Err("game.settingsChanged".into());
    }
    if let Some(runtime) = patch.game_runtime {
        config.game.switch_runtime(runtime);
    }
    if let Some(value) = patch.ui_zoom {
        config.ui_zoom = value.clamp(1.0, 2.0);
    }
    if let Some(steps) = patch.onboarding_seen_steps {
        config.onboarding_seen_steps = steps
            .into_iter()
            .map(|step| step.trim().to_string())
            .filter(|step| !step.is_empty())
            .collect();
    }
    if let Some(value) = patch.hide_unidentified_mods {
        config.hide_unidentified_mods = value;
    }
    if let Some(value) = patch.recycle_deleted_mods {
        if !value {
            // 关闭前留一条记录：这是不可逆行为的开关，排查「文件怎么没了」时要用
            log::warn!("用户关闭「删除 MOD 时优先放入回收站」：此后删除将直接移除文件");
        }
        config.recycle_deleted_mods = value;
    }
    if let Some(steps) = patch.tutorial_steps {
        config.tutorial_steps = steps
            .into_iter()
            .filter_map(|mut step| {
                step.id = step.id.trim().to_string();
                step.selector = step.selector.trim().to_string();
                step.title = step.title.trim().to_string();
                step.description = step.description.trim().to_string();
                step.title_i18n.retain(|locale, text| {
                    matches!(locale.as_str(), "zh-CN" | "zh-TW" | "en" | "ko" | "ja")
                        && !text.trim().is_empty()
                });
                for text in step.title_i18n.values_mut() {
                    *text = text.trim().to_string();
                }
                step.description_i18n.retain(|locale, text| {
                    matches!(locale.as_str(), "zh-CN" | "zh-TW" | "en" | "ko" | "ja")
                        && !text.trim().is_empty()
                });
                for text in step.description_i18n.values_mut() {
                    *text = text.trim().to_string();
                }
                if step.title.is_empty() {
                    step.title = step.title_i18n.get("zh-CN").cloned().unwrap_or_default();
                }
                if step.description.is_empty() {
                    step.description = step
                        .description_i18n
                        .get("zh-CN")
                        .cloned()
                        .unwrap_or_default();
                }
                step.view = step
                    .view
                    .take()
                    .map(|view| view.trim().to_string())
                    .filter(|view| !view.is_empty());
                if step.id.is_empty()
                    || step.selector.is_empty()
                    || step.title.is_empty()
                    || step.description.is_empty()
                {
                    None
                } else {
                    Some(step)
                }
            })
            .collect();
    }
    if let Some(path) = patch.game_path {
        config.game.path = path;
    }
    if let Some(value) = patch.auto_detect {
        config.game.auto_detect = value;
    }
    if let Some(value) = patch.check_updates_on_start {
        config.network.check_updates_on_start = value;
    }
    if let Some(value) = patch.official_api_only {
        config.auth.official_api_only = value;
    }
    if let Some(value) = patch.timeout_seconds {
        config.network.timeout_seconds = value.clamp(5, 300);
    }
    if let Some(value) = patch.download_threads {
        config.network.download_threads = value.clamp(1, 64);
    }
    /*
     * 元数据并行度：`0` 原样保存（= 全部核心，换机器后按新机器核心数跑），
     * 其余夹到本机核心数以内 —— 存一个超过本机的值只会让下次扫描白等。
     */
    if let Some(value) = patch.mod_metadata_concurrency {
        config.mod_metadata_concurrency = crate::metadata_read::clamp_stored_concurrency(value);
    }
    if let Some(value) = patch.map_info_concurrency {
        config.map_info_concurrency = crate::metadata_read::clamp_stored_concurrency(value);
    }
    if let Some(value) = patch.builtin_sources_only {
        config.builtin_sources_only = value;
        /*
         * 官方根源的地址必须同步改写，否则保存下去的配置仍指向远端。
         *
         * 读取配置时 `synchronize_builtin_repository_sources` 会按开关重建
         * 内置根，但那是**下次读取**才生效；这里先重建一次，保证本次返回的
         * 配置就已经指向正确入口（否则界面上的源地址要等重启才变）。
         * 只替换「内置根」，子源与自定义源原样保留。
         */
        let mut rebuilt = crate::config::builtin_repository_sources_for_region(
            crate::config::current_user_region(),
            value,
        );
        rebuilt.extend(
            config
                .repository_sources
                .into_iter()
                .filter(|source| !(source.builtin && source.parent_id.is_none())),
        );
        config.repository_sources = rebuilt;
        log::info!(
            "「仅使用内置仓库源」已{}：官方根入口={}",
            if value { "开启" } else { "关闭" },
            if value {
                crate::repository_sources::BUILTIN_INDEX
            } else {
                "地区默认远端地址"
            }
        );
    }
    if let Some(value) = patch.builtin_map_sources_only {
        config.builtin_map_sources_only = value;
        /*
         * 地图源不持久化源树，开关只影响「下次刷新从哪里读索引」，
         * 所以这里不需要重建任何条目 —— 立刻用新入口加载一次，让开关的
         * 效果当次可见，也让界面拿到与新入口一致的地图列表。
         */
        log::info!(
            "「仅使用内置地图源」已{}：索引={}",
            if value { "开启" } else { "关闭" },
            if value {
                crate::map_sources::BUILTIN_MAP_INDEX
            } else {
                "地区默认远端地址"
            }
        );
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!(
        "应用设置保存完成：界面缩放={}，自动检测游戏={}，启动检查更新={}",
        config.ui_zoom,
        config.game.auto_detect,
        config.network.check_updates_on_start
    );
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn save_github_proxies(
    state: State<'_, AppState>,
    patch: GitHubProxyPatch,
) -> Result<PublicConfig, String> {
    log::info!("保存 GitHub/Gitee 代理配置");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut proxies = GitHubProxyConfig {
        release_proxies: patch.release_proxies,
        raw_proxies: patch.raw_proxies,
        github_api_proxies: patch.github_api_proxies,
        gitee_api_proxies: patch.gitee_api_proxies,
        avatar_proxies: patch.avatar_proxies,
    };
    let mut config = state.config.load_or_default();
    proxies = crate::github_proxies::validate_saved_config(proxies, &config.github_proxies)?;
    config.github_proxies = proxies;
    state
        .config
        .save(&config)
        .map_err(|error| error.to_string())?;
    log::info!(
        "代理配置保存完成：Release={}，Raw={}，GitHub API={}，Gitee API={}",
        config.github_proxies.release_proxies.len(),
        config.github_proxies.raw_proxies.len(),
        config.github_proxies.github_api_proxies.len(),
        config.github_proxies.gitee_api_proxies.len()
    );
    Ok(state.config.public_config(&config))
}

#[derive(Debug)]
struct ProxySourceReadError {
    stage: &'static str,
    reason: String,
}

impl ProxySourceReadError {
    fn new(stage: &'static str, reason: impl Into<String>) -> Self {
        Self {
            stage,
            reason: reason.into(),
        }
    }
}

async fn read_github_proxy_source(
    client: &reqwest::Client,
    source: &crate::github_proxies::ProxySource,
) -> Result<GitHubProxyConfig, ProxySourceReadError> {
    use crate::github_proxies::ProxySourceFormat;
    let started_at = std::time::Instant::now();
    let safe_url = repository_sources::safe_log_address(&source.url);
    let (format, max_bytes) = match source.format {
        ProxySourceFormat::Toml => ("toml", 1024 * 1024),
        ProxySourceFormat::UserScript => ("userscript", 4 * 1024 * 1024),
    };
    log::info!(
        "GitHub 代理来源读取开始：地址={}，内置文件={}，格式={}，大小限制={}字节",
        safe_url,
        source.bundled,
        format,
        max_bytes
    );
    let text = if source.bundled {
        let location = SourceLocation::parse(&source.url).map_err(|error| {
            ProxySourceReadError::new("locate", repository_sources::error_category(&error))
        })?;
        let text = location.bundled_text().map_err(|error| {
            ProxySourceReadError::new("read", repository_sources::error_category(&error))
        })?;
        log::info!(
            "GitHub 代理内置文件读取成功：地址={}，格式={}，字节数={}",
            safe_url,
            format,
            text.len()
        );
        text.to_string()
    } else {
        if source.format == ProxySourceFormat::UserScript {
            log::info!("正在请求 GitHub 代理 JS：地址={}，超时=5秒", safe_url);
        }
        log::info!(
            "GitHub 代理来源下载开始：地址={}，格式={}，超时=5秒",
            safe_url,
            format
        );
        let mut response = client.get(&source.url).send().await.map_err(|error| {
            ProxySourceReadError::new(
                "request",
                crate::locale_tag::with_params(
                    "proxy.downloadFailureDetail",
                    &[
                        ("timeout", &error.is_timeout().to_string()),
                        ("connect", &error.is_connect().to_string()),
                        ("reason", &error.without_url().to_string()),
                    ],
                ),
            )
        })?;
        let status = response.status();
        log::info!(
            "GitHub 代理来源 HTTP 响应：地址={}，最终地址={}，格式={}，状态码={}，声明字节数={:?}",
            safe_url,
            repository_sources::safe_log_address(response.url().as_str()),
            format,
            status.as_u16(),
            response.content_length()
        );
        if !status.is_success() {
            return Err(ProxySourceReadError::new("http", format!("HTTP {status}")));
        }
        let size_error = || {
            ProxySourceReadError::new(
                "size",
                crate::locale_tag::with_params(
                    "proxy.sourceTooLargeByMib",
                    &[("mib", &(max_bytes / (1024 * 1024)).to_string())],
                ),
            )
        };
        if response
            .content_length()
            .is_some_and(|length| length > max_bytes as u64)
        {
            return Err(size_error());
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|error| {
            ProxySourceReadError::new(
                "body",
                crate::locale_tag::with_params(
                    "proxy.readFailureDetail",
                    &[
                        ("bytes", &bytes.len().to_string()),
                        ("timeout", &error.is_timeout().to_string()),
                        ("reason", &error.without_url().to_string()),
                    ],
                ),
            )
        })? {
            if bytes.len().saturating_add(chunk.len()) > max_bytes {
                return Err(size_error());
            }
            bytes.extend_from_slice(&chunk);
        }
        log::info!(
            "GitHub 代理来源下载成功：地址={}，格式={}，状态码={}，字节数={}，耗时={}ms",
            safe_url,
            format,
            status.as_u16(),
            bytes.len(),
            started_at.elapsed().as_millis()
        );
        String::from_utf8(bytes)
            .map_err(|_| ProxySourceReadError::new("utf8", "proxy.invalidUtf8"))?
    };
    if text.len() > max_bytes {
        return Err(ProxySourceReadError::new(
            "size",
            crate::locale_tag::with_params(
                "proxy.sourceTooLargeByMib",
                &[("mib", &(max_bytes / (1024 * 1024)).to_string())],
            ),
        ));
    }
    log::debug!(
        "GitHub 代理来源解析开始：地址={}，格式={}，字节数={}",
        safe_url,
        format,
        text.len()
    );
    let config = match source.format {
        ProxySourceFormat::Toml => crate::github_proxies::parse_manifest(&text),
        ProxySourceFormat::UserScript => crate::github_proxies::parse_userscript(&text),
    }
    // Parser errors may quote a remote document or a proxy URL containing a
    // secret. Log the source/stage rather than untrusted document contents.
    .map_err(|error| {
        let reason = match error.as_str() {
            "proxy.noUsableEntries" => "proxy.noUsableEntriesJs",
            "proxy.missingDownloadList" | "proxy.missingRawList" => "proxy.missingDownloadListJs",
            _ => "proxy.validationFailed",
        };
        ProxySourceReadError::new("parse", reason)
    })?;
    log::info!(
        "GitHub 代理来源解析成功：地址={}，格式={}，Release={}，Raw={}，GitHub API={}，Gitee API={}，耗时={}ms",
        safe_url,
        format,
        config.release_proxies.len(),
        config.raw_proxies.len(),
        config.github_api_proxies.len(),
        config.gitee_api_proxies.len(),
        started_at.elapsed().as_millis()
    );
    Ok(config)
}

#[tauri::command]
pub async fn refresh_github_proxies(state: State<'_, AppState>) -> Result<PublicConfig, String> {
    let region = crate::config::current_user_region();
    log::info!("开始刷新 GitHub 代理来源：地区={}", region);
    let sources = repository_sources::github_proxy_sources(region).map_err(|error| {
        log::error!(
            "GitHub 代理来源配置读取失败：地区={}，阶段=index，错误类别={}",
            region,
            repository_sources::error_category(&error)
        );
        error.to_string()
    })?;
    let script_sources = sources
        .iter()
        .filter(|source| source.format == crate::github_proxies::ProxySourceFormat::UserScript)
        .count();
    log::info!(
        "GitHub 代理来源配置已读取：地区={}，来源数={}，JS 来源数={}",
        region,
        sources.len(),
        script_sources
    );
    if script_sources == 0 {
        log::warn!(
            "当前地区没有匹配的 GitHub 代理 JS 来源，不会发送 JS 请求：地区={}，索引=repositories.toml",
            region
        );
    }
    for (index, source) in sources.iter().enumerate() {
        log::info!(
            "GitHub 代理来源已排队：序号={}，地区={}，格式={:?}，内置文件={}，地址={}",
            index + 1,
            region,
            source.format,
            source.bundled,
            repository_sources::safe_log_address(&source.url)
        );
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .user_agent(releases::APP_USER_AGENT)
        .build()
        .map_err(|error| {
            log::error!("GitHub 代理 HTTP 客户端创建失败：阶段=client");
            error.without_url().to_string()
        })?;
    let mut failed = 0usize;
    let mut downloaded = GitHubProxyConfig::default();
    let mut succeeded = 0usize;
    let mut scripts_succeeded = 0usize;
    for source in sources {
        match read_github_proxy_source(&client, &source).await {
            Ok(config) => {
                downloaded = crate::github_proxies::merge_source_configs(&downloaded, config);
                succeeded += 1;
                if source.format == crate::github_proxies::ProxySourceFormat::UserScript {
                    scripts_succeeded += 1;
                }
            }
            Err(error) => {
                log::warn!(
                    "GitHub 代理来源读取失败：地址={}，格式={:?}，阶段={}，原因={}，保留内置后备",
                    repository_sources::safe_log_address(&source.url),
                    source.format,
                    error.stage,
                    error.reason
                );
                failed += 1;
            }
        }
    }
    if succeeded == 0 || (script_sources > 0 && scripts_succeeded == 0) {
        log::warn!(
            "GitHub 远程代理来源不可用，保留已有配置与完整内置后备：JS 成功数={}，失败来源数={}",
            scripts_succeeded,
            failed
        );
    }
    // Only the final read/merge/save transaction holds the cache/operation
    // locks; network requests must not block edits or repository operations.
    let _cache_guard = state.repository_cache.lock().await;
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    // merge_downloaded always supplies the complete bundled asset/API fallback
    // set, including when a userscript failed or contained no usable proxies.
    let mut config = state.config.load_or_default();
    config.github_proxies =
        crate::github_proxies::merge_downloaded(&config.github_proxies, downloaded);
    crate::github_proxies::validate_unique_ids(&config.github_proxies).map_err(|error| {
        log::error!("GitHub 代理同步提交失败：阶段=validate");
        error
    })?;
    state.config.save(&config).map_err(|error| {
        log::error!(
            "GitHub 代理同步提交失败：阶段=save，错误类别={}",
            repository_sources::error_category(&error)
        );
        error.to_string()
    })?;
    let remote_release = config
        .github_proxies
        .release_proxies
        .iter()
        .filter(|proxy| proxy.remote)
        .count();
    let remote_raw = config
        .github_proxies
        .raw_proxies
        .iter()
        .filter(|proxy| proxy.remote)
        .count();
    let builtin_release = config
        .github_proxies
        .release_proxies
        .iter()
        .filter(|proxy| proxy.builtin && !proxy.remote)
        .count();
    let builtin_raw = config
        .github_proxies
        .raw_proxies
        .iter()
        .filter(|proxy| proxy.builtin && !proxy.remote)
        .count();
    log::info!(
        "GitHub 代理来源刷新完成：成功来源={}，失败来源={}，JS 成功数={}，远程 Release={}，远程 Raw={}，内置 Release={}，内置 Raw={}，内置后备已保留",
        succeeded,
        failed,
        scripts_succeeded,
        remote_release,
        remote_raw,
        builtin_release,
        builtin_raw
    );
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn save_tokens(state: State<'_, AppState>, patch: TokenPatch) -> Result<PublicConfig, String> {
    log::info!(
        "保存 API Token 配置：GitHub token 已提供={}，Gitee token 已提供={}",
        !patch.github_token.trim().is_empty(),
        !patch.gitee_token.trim().is_empty()
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    if patch.clear {
        // 显式清除：把两个 Token 都恢复成空
        config.auth.github_token.clear();
        config.auth.gitee_token.clear();
        log::info!("已清除全部 API Token");
    } else {
        state
            .config
            .set_tokens(&mut config, &patch.github_token, &patch.gitee_token);
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("API Token 配置保存完成");
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn detect_game(state: State<'_, AppState>) -> Result<GameInfo, String> {
    log::debug!("检测游戏目录和 BepInEx 状态");
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    log::info!(
        "游戏检测完成：已找到游戏={}，BepInEx 已安装={}",
        info.path.is_some(),
        info.bepinex.installed
    );
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(info.clone());
    Ok(info)
}

#[tauri::command]
pub fn check_crossover_running() -> bool {
    game::is_crossover_running()
}

#[tauri::command]
pub fn set_game_path(state: State<'_, AppState>, path: String) -> Result<GameInfo, String> {
    log::info!("设置游戏目录");
    let previous = state.config.load_or_default();
    let root = game::selected_game_root(Path::new(&path), &previous.game)?;
    if let Some(path) = game::detect_game(&previous).path {
        game::ensure_game_stopped(Path::new(&path))?;
    }
    let config = {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let mut config = state.config.load_or_default();
        if config.game != previous.game {
            return Err("game.settingsChanged".into());
        }
        config.game.path = root.to_string_lossy().to_string();
        config.game.auto_detect = false;
        state.config.save(&config).map_err(|e| e.to_string())?;
        config
    };
    log::info!("游戏目录设置完成");
    let info = game::detect_game(&config);
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(info.clone());
    Ok(info)
}

#[tauri::command]
pub fn reset_game_path(state: State<'_, AppState>) -> Result<GameInfo, String> {
    log::info!("重置游戏目录并启用自动检测");
    let config = {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let mut config = state.config.load_or_default();
        config.game.path.clear();
        config.game.auto_detect = true;
        state.config.save(&config).map_err(|e| e.to_string())?;
        config
    };
    log::info!("游戏目录已重置");
    let info = game::detect_game(&config);
    *state.game_state.lock().map_err(|error| error.to_string())? = Some(info.clone());
    Ok(info)
}

/// 仓库 Release 列表的缓存有效期。
///
/// 一次「刷新全部仓库源」会把整棵仓库树挨个打一遍 API：几十个仓库 × 每个
/// 若干次请求，光是打开页面就要等几十秒，而且极容易撞上 GitHub/Gitee 的
/// 速率限制。缓存 5 分钟后，重复刷新、切页再回来、重启管理器都直接读
/// 已有结果，只有真正过期或用户**主动**点刷新时才重新请求。
const RELEASE_CACHE_TTL_MS: i64 = 5 * 60 * 1000;

/// 当前 Unix 毫秒时间戳。
fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

/// 这份 Release 缓存是否仍在有效期内。
///
/// `None` 时间戳（旧配置、从未成功获取）一律视为过期 —— 宁可多请求一次，
/// 也不要拿一份来源不明的列表去判断「有没有更新」。
fn release_cache_is_fresh(cached_at: Option<i64>, now: i64) -> bool {
    match cached_at {
        // 时钟回拨（now < cached_at）也算过期：说明系统时间被改过，
        // 此时的「新鲜度」不可信。
        Some(at) => now >= at && now - at < RELEASE_CACHE_TTL_MS,
        None => false,
    }
}

/// 刷新单个仓库的 Release 列表。
///
/// `force = true` 跳过 5 分钟缓存强制请求 —— 用户手动点「刷新」时传，
/// 因为那个动作的语义就是「我要最新的」。批量刷新、自动刷新、切页回来
/// 都传 `false`（或缺省）走缓存。
#[tauri::command]
pub async fn refresh_repository(
    state: State<'_, AppState>,
    repository_id: String,
    force: Option<bool>,
) -> Result<Vec<ReleaseInfo>, String> {
    let force = force.unwrap_or(false);
    log::info!(
        "开始刷新仓库 Release：仓库={}，跳过缓存={}",
        repository_id,
        force
    );
    let config = state.config.load_or_default();
    let index = config
        .repositories
        .iter()
        .position(|repo| repo.id == repository_id)
        .ok_or("source.repositoryMissing")?;
    /*
     * 不可用的仓库不刷新：`enabled=false`（用户关闭，或被官方仓库源覆盖的
     * 社区副本）与「地区不适用」（例如中国大陆下的 international 仓库）
     * 都按「当前不存在」处理 —— 界面上根本没有这张卡片，
     * 刷新入口也就无从触发。
     */
    if !crate::config::repository_is_available(
        &config,
        &config.repositories[index],
        crate::config::current_user_region(),
    ) {
        return Err("source.repositoryUnavailableInRegion".into());
    }
    if config.repositories[index].download_mode == RepositoryDownloadMode::RepositoryFile {
        // 这条消息会直接展示在「刷新全部仓库源」的结果里，写成用户能看懂的话
        log::debug!(
            "跳过仓库文件模式的 Release 刷新：仓库={}",
            config.repositories[index].name
        );
        return Err(crate::locale_tag::with_params(
            "source.fileModeNoVersion",
            &[("repo", &config.repositories[index].name.to_string())],
        ));
    }
    let repo = config.repositories[index].clone();

    /*
     * 缓存命中就直接返回已有列表，一个网络请求都不发。
     *
     * 为什么放在这里、而不是在命令入口：上面几条「不可用 / 仓库文件模式」
     * 的拒绝要先执行 —— 那些是配置层面的「不该刷新」，与缓存无关。
     *
     * `last_error` 不清：上次失败留下的错误要保留到下一次真正成功为止，
     * 否则界面会出现「有缓存列表但没有错误」的假象，用户以为已经恢复。
     */
    if !force
        && !repo.cached_releases.is_empty()
        && release_cache_is_fresh(repo.releases_cached_at, now_millis())
    {
        log::info!(
            "仓库 Release 命中缓存：仓库={}，数量={}，缓存时间={:?}",
            repo.name,
            repo.cached_releases.len(),
            repo.releases_cached_at
        );
        return Ok(repo.cached_releases.clone());
    }

    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let token = state.config.repository_api_token(&config, &repo);
    let api_proxies = match repo.provider {
        Provider::Github => &config.github_proxies.github_api_proxies,
        Provider::Gitee => &config.github_proxies.gitee_api_proxies,
    };
    log::debug!(
        "请求仓库 Release API：仓库={}，平台={:?}，代理数={}，最大 Release 数={}",
        repo.name,
        repo.provider,
        api_proxies.len(),
        repo.max_release_count
    );
    let fetched = releases::fetch_releases(
        &client,
        &repo,
        token.as_deref(),
        config.auth.official_api_only,
        api_proxies,
    )
    .await
    .map(|releases| releases::filter_channel(releases, &repo.channel))
    .map_err(|error| error.to_string());

    let _cache_guard = state.repository_cache.lock().await;
    let _operation_guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut latest_config = state.config.load_or_default();
    let latest_index = latest_config
        .repositories
        .iter()
        .position(|item| item.id == repository_id)
        .ok_or("source.repositoryRemoved")?;
    let latest_repo = &mut latest_config.repositories[latest_index];
    if latest_repo.username != repo.username
        || latest_repo.repository != repo.repository
        || latest_repo.provider != repo.provider
        || latest_repo.channel != repo.channel
        || latest_repo.download_mode != repo.download_mode
    {
        return Err("source.repositoryConfigChanged".into());
    }

    latest_repo.etag = None;
    match fetched {
        Ok(filtered) => {
            latest_repo.cached_releases = filtered.clone();
            // 只有真正拿到结果才刷新时间戳：失败后必须让下一次请求重新打 API
            latest_repo.releases_cached_at = Some(now_millis());
            latest_repo.last_error = None;
            state
                .config
                .save(&latest_config)
                .map_err(|error| error.to_string())?;
            log::info!(
                "仓库 Release 刷新完成：仓库={}，数量={}",
                repo.name,
                filtered.len()
            );
            Ok(filtered)
        }
        Err(error) => {
            log::error!("仓库 Release 刷新失败：仓库={}，原因={}", repo.name, error);
            latest_repo.cached_releases.clear();
            // 清掉时间戳，避免「空列表 + 新鲜时间」被误判成命中缓存
            latest_repo.releases_cached_at = None;
            latest_repo.last_error = Some(error.clone());
            state
                .config
                .save(&latest_config)
                .map_err(|save_error| save_error.to_string())?;
            Err(error)
        }
    }
}

#[tauri::command]
pub fn get_repository_releases(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<Vec<ReleaseInfo>, String> {
    let config = state.config.load_or_default();
    Ok(config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("source.repositoryMissing")?
        .cached_releases
        .clone())
}

#[tauri::command]
pub async fn install_bepinex(
    app: AppHandle,
    state: State<'_, AppState>,
    reinstall: bool,
    provider: Provider,
) -> Result<BepInExInstallResult, String> {
    archive::start_download();
    log::info!(
        "开始安装 BepInEx：平台={:?}，重新安装={}",
        provider,
        reinstall
    );
    let config = state.config.load_or_default();
    let package = crate::bepinex::Package::current(&config.game);
    let info = game::detect_game(&config);
    if !info.game_exists {
        return Err("game.executableMissing".into());
    }
    let game_root = info.path.as_deref().ok_or("game.notFound")?;
    let game_root = PathBuf::from(game_root);
    game::ensure_game_stopped(&game_root)?;
    if info.bepinex.installed && !reinstall {
        return Err("framework.alreadyInstalled".into());
    }
    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|e| e.to_string())?;
    let download_url = package.url(&provider);
    log::debug!(
        "开始下载 BepInEx 安装包：平台={:?}，URL={}",
        provider,
        download_url
    );
    let proxy_urls = if provider == Provider::Github {
        asset_proxy_urls(
            &config.github_proxies.release_proxies,
            download_url,
            false,
            "",
            "",
            ProxyMode::AutoProxy,
            None,
        )
    } else {
        Vec::new()
    };
    let candidates =
        plan_asset_download_candidates(ProxyMode::AutoDirect, download_url, proxy_urls)?;
    let cache_dir = state.config.cache_dir();
    let temp = try_asset_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let client = &client;
            let cache_dir = &cache_dir;
            async move {
                let temp = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache_dir,
                    "BepInEx",
                    &reporter,
                    1,
                    Some(std::time::Duration::from_secs(5)),
                )
                .await?;
                if let Err(error) =
                    archive::validate_download_file(&temp, None, Some(package.digest()))
                        .and_then(|_| archive::validate_zip(&temp))
                {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                Ok(temp)
            }
        },
        Some(DownloadContext {
            app: Some(&app),
            label: "BepInEx",
            expected_size: None,
        }),
    )
    .await?;
    let installation = (|| {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        if state.config.load_or_default().game != config.game {
            return Err("game.settingsChanged".into());
        }
        if archive::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        game::ensure_game_stopped(&game_root)?;
        install_framework_archive(
            &temp,
            &game_root,
            &state.config.backups_dir(),
            &state.config.manifests_dir(),
            package,
            reinstall,
            crate::bepinex::PACKAGE_VERSION,
        )
    })();
    let _ = fs::remove_file(&temp);
    match installation {
        Ok(()) => {
            log::info!(
                "BepInEx 安装完成：渠道={:?}，游戏包={:?}",
                provider,
                package
            );
            #[cfg(unix)]
            let override_result = if !package.native() {
                let prefix = info.wine_prefix.map(PathBuf::from);
                Some(
                    tauri::async_runtime::spawn_blocking(move || {
                        let prefix = prefix.ok_or("wine.prefixUnknown")?;
                        crate::wine::configure_winhttp(&prefix)
                    })
                    .await
                    .unwrap_or_else(|_| Err("wine.winhttpFailed".into())),
                )
            } else {
                None
            };
            #[cfg(not(unix))]
            let override_result: Option<Result<(), String>> = None;
            let wine_override_applied = override_result.as_ref().is_some_and(Result::is_ok);
            let wine_override_error = override_result.and_then(Result::err);
            if let Some(error) = &wine_override_error {
                log::warn!("BepInEx 已安装，Wine 配置需手动完成：{error}");
            }
            Ok(BepInExInstallResult {
                operation: OperationResult {
                    ok: true,
                    message: "framework.bepInExInstalled".into(),
                },
                wine_override_applied,
                wine_override_error,
                macos_overlay_repaired: package == crate::bepinex::Package::MacOS,
            })
        }
        Err(error) => {
            log::error!("BepInEx 安装失败：原因={}", error);
            Err(crate::locale_tag::with_params(
                "framework.installFailed",
                &[("reason", &error)],
            ))
        }
    }
}

/// 框架安装清单的固定 id。
///
/// 框架不是 MOD，但它的 ZIP 解压结果同样需要一份清单：
/// 卸载时要按清单把**所有**解压出来的文件回收（`BepInEx/` 目录之外还有
/// `winhttp.dll`、`doorstop_config.ini`、`run_bepinex.sh` 等散落在游戏根目录的
/// 文件），只删 `BepInEx` 目录会留下这些孤儿文件。
const BEPINEX_MANIFEST_ID: &str = "bepinex-framework";

fn install_framework_archive(
    archive_path: &Path,
    game_root: &Path,
    backup_root: &Path,
    manifests_dir: &Path,
    package: crate::bepinex::Package,
    reinstall: bool,
    release_tag: &str,
) -> Result<(), String> {
    let framework = game::safe_game_path(game_root, "BepInEx")?;
    /*
     * 「已安装」必须看核心程序集，不能看 `BepInEx/` 目录是否存在。
     *
     * 装过 MOD 的游戏目录里必定有 `BepInEx/plugins/`，那个目录从来没有
     * 框架核心。用目录判断会得到「已安装」的结论，于是首次安装被拒绝，
     * 用户只能点「重新安装」——而重装路径又会把整个 BepInEx 删掉重建。
     */
    let installed = crate::bepinex::installed(game_root);
    if installed && !reinstall {
        return Err("framework.alreadyInstalled".into());
    }
    /*
     * 只有真装过框架才做「整目录备份 → 删除重建 → 恢复 plugins/config」。
     *
     * 残留空壳（目录在、核心不在）直接就地解压：ZIP 只写框架自己的文件，
     * 已有的 plugins/config 原样保留，不必先拷一份几个 GB 的备份再恢复。
     */
    let backup = if installed {
        Some(backup_bepinex_dir(game_root, backup_root)?)
    } else {
        None
    };
    let result = (|| {
        if installed {
            archive::clear_readonly(&framework)?;
            fs::remove_dir_all(&framework).map_err(|e| {
                format!(
                    "error.framework.cleanupFailed?reason={}",
                    crate::locale_tag::encode(&e.to_string())
                )
            })?;
        }
        let pending = archive::extract_framework_zip_pending(archive_path, game_root)?;
        if package.native() {
            crate::bepinex::prepare_native(game_root, package)?;
        }
        if let Some(backup) = &backup {
            restore_preserved_dirs(backup, game_root)?;
        }
        /*
         * 先落清单再提交文件：两者顺序反过来的话，commit 之后写清单失败会
         * 留下「文件已就位但没记录」的状态，卸载时就回收不到这些文件。
         * 清单内容取 `pending.files()` —— 那是本次实际解压出的游戏相对路径，
         * 覆盖 `BepInEx/` 与散落在根目录的 winhttp.dll / doorstop_config.ini 等。
         */
        let manifest = archive::new_manifest(
            BEPINEX_MANIFEST_ID,
            release_tag,
            archive::single_language("BepInEx"),
            Default::default(),
            pending.files().to_vec(),
            Vec::new(),
        );
        archive::write_manifest(manifests_dir, &manifest)?;
        pending.commit();
        Ok(())
    })();
    if let Err(error) = result {
        if let Some(backup) = backup {
            restore_backup_dir(&backup, game_root).map_err(|restore| {
                log::error!(
                    "BepInEx 失败回滚未完成：备份={}，原因={restore}",
                    backup.display()
                );
                "framework.installFailedRollbackIncomplete".to_string()
            })?;
        }
        return Err(error);
    }
    Ok(())
}

#[tauri::command]
pub fn repair_native_bepinex_overlay(
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    if std::env::consts::OS != "macos" || config.game.runtime != GameRuntime::Native {
        return Err("framework.overlayMacNativeOnly".into());
    }
    let info = game::detect_game(&config);
    if !info.game_exists {
        return Err("game.executableMissing".into());
    }
    let game_root = PathBuf::from(info.path.ok_or("game.directoryMissing")?);
    game::ensure_game_stopped(&game_root)?;
    crate::bepinex::repair_macos_overlay(&game_root)?;
    log::info!("macOS Steam Overlay 注入修复完成");
    Ok(OperationResult {
        ok: true,
        message: "framework.overlayRepaired".into(),
    })
}

#[tauri::command]
pub async fn uninstall_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    // 卸载前要把整个 BepInEx 目录快照到备份：几百 MB 的复制，不能占 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || uninstall_bepinex_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`uninstall_bepinex`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_bepinex_blocking(state: &AppState) -> Result<OperationResult, String> {
    log::info!("开始卸载 BepInEx");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let source = game_root.join("BepInEx");
    /*
     * 必须按「核心程序集存在」判断，不能按目录存在。
     *
     * 目录在、核心不在 = 只有 MOD 的空壳。此时如果继续走删除流程，
     * 没有框架清单就会删掉整个 `BepInEx` —— 把用户所有 MOD 一起删掉，
     * 而用户只是点了一下「卸载框架」。
     */
    if !crate::bepinex::installed(&game_root) {
        return Err(if source.exists() {
            "framework.bepinexOnlyResidue".into()
        } else {
            "framework.bepinexMissing".to_string()
        });
    }
    ensure_copy_paths_disjoint(&source, &state.config.backups_dir())?;

    /*
     * 卸载前先备份框架目录。
     *
     * 回收站保留的是「按清单移走的文件」，而备份是**整目录快照**：
     * 用户自己往 BepInEx 里放的东西（手工装的插件、改过的 config、日志）
     * 不在清单里，回收站不会带走它们 —— 只有备份能保证还能找回。
     * 两者职责不同，都要保留。
     */
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &BEPINEX_BACKUP,
        &state.config.manifests_dir(),
        BEPINEX_MANIFEST_ID,
    )?;
    log::info!("BepInEx 卸载前备份完成：路径={}", backup.display());

    let manifests_dir = state.config.manifests_dir();
    let recycle_root = state.config.mod_recycle_bin_dir();
    /*
     * 优先按安装清单回收。
     *
     * 框架 ZIP 不只包含 `BepInEx/`：`winhttp.dll`、`doorstop_config.ini`、
     * `run_bepinex.sh` 都直接解压在游戏根目录。只 `remove_dir_all(BepInEx)`
     * 会把这些文件留在原地 —— 用户以为卸载干净了，实际游戏仍被 doorstop 接管。
     *
     * 清单里记的就是当初解压出的全部游戏相对路径，所以直接复用回收站的
     * `recycle_package`（它内部走 `stage`，负责共享文件保护、清单快照、
     * scratch 目录与回滚），不另写一套移动逻辑。
     */
    if archive::read_manifest(&manifests_dir, BEPINEX_MANIFEST_ID).is_some() {
        match recycle_bin::recycle_package(
            &recycle_root,
            &manifests_dir,
            &game_root,
            BEPINEX_MANIFEST_ID,
            None,
        ) {
            Ok(entry) => {
                log::info!(
                    "BepInEx 卸载完成（已回收）：文件数={}，回收记录={}，备份={}",
                    entry.file_count,
                    entry.id,
                    backup.display()
                );
                return Ok(OperationResult {
                    ok: true,
                    message: crate::locale_tag::with_params(
                        "message.framework.movedToRecycleBin",
                        &[
                            ("count", &entry.file_count.to_string()),
                            ("path", &backup.display().to_string()),
                        ],
                    ),
                });
            }
            Err(error) => {
                /*
                 * 回收失败就中止，不退回「直接删目录」：
                 * 那种做法只能删掉一部分，留下散落的 doorstop/winhttp 文件，
                 * 用户会看到一个既装不上又卸不掉的半残状态。
                 */
                log::error!(
                    "BepInEx 回收失败，已中止卸载：原因={}，备份={}",
                    error,
                    backup.display()
                );
                return Err(crate::locale_tag::with_params(
                    "backup.recycleFailed",
                    &[
                        ("reason", &error.to_string()),
                        ("path", &backup.display().to_string()),
                    ],
                ));
            }
        }
    }

    /*
     * 没有清单（旧版本装的框架）：按「框架已知文件」现造一份清单再回收。
     *
     * 以前这里直接 `remove_dir_all(BepInEx)`，会把 `plugins/`、`config/`
     * 里的用户 MOD 和配置一起删掉 —— 而用户点的是「卸载框架」。
     * 现在只收框架自己的东西（core、doorstop 库、根目录的加载器文件），
     * 装 MOD 产生的内容原地保留；回收记录仍可在回收站恢复。
     */
    log::warn!("未找到 BepInEx 安装清单，按框架已知文件重建清单后回收");
    let files = framework_manifest_files(&game_root);
    if files.is_empty() {
        return Err(crate::locale_tag::with_params(
            "backup.noReclaimableFiles",
            &[("path", &backup.display().to_string())],
        ));
    }
    let fallback = archive::new_manifest(
        BEPINEX_MANIFEST_ID,
        crate::bepinex::PACKAGE_VERSION,
        archive::single_language("BepInEx"),
        Default::default(),
        files,
        Vec::new(),
    );
    archive::write_manifest(&manifests_dir, &fallback)?;
    match recycle_bin::recycle_package(
        &recycle_root,
        &manifests_dir,
        &game_root,
        BEPINEX_MANIFEST_ID,
        None,
    ) {
        Ok(entry) => {
            log::info!(
                "BepInEx 卸载完成（旧安装按已知文件回收）：文件数={}，回收记录={}，备份={}",
                entry.file_count,
                entry.id,
                backup.display()
            );
            Ok(OperationResult {
                ok: true,
                message: crate::locale_tag::with_params(
                    "message.framework.movedToRecycleBin",
                    &[
                        ("count", &entry.file_count.to_string()),
                        ("path", &backup.display().to_string()),
                    ],
                ),
            })
        }
        Err(error) => Err(crate::locale_tag::with_params(
            "backup.recycleFailed",
            &[
                ("reason", &error.to_string()),
                ("path", &backup.display().to_string()),
            ],
        )),
    }
}

/// 列出框架自身在游戏目录里写入的文件（游戏相对路径）。
///
/// 只包含框架随 ZIP 解压出的东西：`BepInEx/core`、`BepInEx/doorstop_libs`
/// 以及散落在游戏根目录的加载器文件。`plugins/`、`config/`、`cache/`
/// 这些目录里是用户或 MOD 产生的内容，绝不能出现在这份清单里。
fn framework_manifest_files(game_root: &Path) -> Vec<String> {
    let root = game_root.join("BepInEx");
    let mut files = Vec::new();
    for directory in ["core", "doorstop_libs"] {
        for entry in walkdir::WalkDir::new(root.join(directory))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if let Ok(relative) = entry.path().strip_prefix(game_root) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for name in [
        "winhttp.dll",
        "winhttp.dll.disabled",
        "doorstop_config.ini",
        "run_bepinex.sh",
        "libdoorstop.so",
        "libdoorstop.dylib",
        "doorstop_libs/libdoorstop_x64.so",
        "doorstop_libs/libdoorstop_x64.dylib",
    ] {
        if game_root.join(name).is_file() {
            files.push(name.to_string());
        }
    }
    files
}

#[tauri::command]
pub async fn backup_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    // 整目录快照是重活：同步命令会把窗口冻在「备份中」那一刻
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || backup_bepinex_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`backup_bepinex`] 的同步实现（在阻塞线程池里跑）。
fn backup_bepinex_blocking(state: &AppState) -> Result<OperationResult, String> {
    log::info!("开始备份 BepInEx");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &BEPINEX_BACKUP,
        &state.config.manifests_dir(),
        BEPINEX_MANIFEST_ID,
    )?;
    log::info!("BepInEx 备份完成：路径={}", backup.display());
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "message.backup.created",
            &[("path", &backup.display().to_string())],
        ),
    })
}

#[tauri::command]
pub async fn restore_bepinex(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    // 把整份备份写回游戏目录同样是重活：不能占 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || restore_bepinex_blocking(&state, &backup_path))
        .await
        .map_err(|error| error.to_string())?
}

/// [`restore_bepinex`] 的同步实现（在阻塞线程池里跑）。
fn restore_bepinex_blocking(
    state: &AppState,
    backup_path: &str,
) -> Result<OperationResult, String> {
    log::info!("开始恢复 BepInEx 备份");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let backup = validate_framework_backup_in(
        Path::new(&backup_path),
        &state.config.backups_dir(),
        &BEPINEX_BACKUP,
    )?;
    restore_framework_backup(
        &game_root,
        &backup,
        &BEPINEX_BACKUP,
        &state.config.manifests_dir(),
        BEPINEX_MANIFEST_ID,
    )?;
    log::info!("BepInEx 备份恢复完成");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.bepInExRestored".into(),
    })
}

#[tauri::command]
pub async fn delete_bepinex_backup(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    // 删整目录按文件数计：大备份能到几秒，搬到阻塞线程池
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        delete_bepinex_backup_blocking(&state, &backup_path)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`delete_bepinex_backup`] 的同步实现（在阻塞线程池里跑）。
fn delete_bepinex_backup_blocking(
    state: &AppState,
    backup_path: &str,
) -> Result<OperationResult, String> {
    log::info!("开始删除 BepInEx 备份");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let target = fs::canonicalize(&backup_path).map_err(|_| "backup.notFound".to_string())?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !name.starts_with("BepInEx.backup-") || !target.is_dir() {
        return Err("backup.wrongKindBepInEx".into());
    }

    let game_root = fs::canonicalize(&game_root).map_err(|e| e.to_string())?;
    let manager_backup_path = state.config.backups_dir();
    fs::create_dir_all(&manager_backup_path).map_err(|e| e.to_string())?;
    let manager_backup_root = fs::canonicalize(manager_backup_path).map_err(|e| e.to_string())?;
    let allowed = target.parent() == Some(game_root.as_path())
        || target.parent() == Some(manager_backup_root.as_path());
    if !allowed {
        return Err("backup.outsideRoot".into());
    }

    archive::clear_readonly(&target)?;
    fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    log::info!("BepInEx 备份删除完成");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.deleted".into(),
    })
}

pub(crate) fn selected_asset_proxy_mode(
    requested: Option<ProxyMode>,
    provider: &Provider,
    region: &str,
) -> ProxyMode {
    if *provider == Provider::Gitee {
        return ProxyMode::Direct;
    }
    requested.unwrap_or(if region == crate::config::REGION_CHINESE_MAINLAND {
        ProxyMode::AutoProxy
    } else {
        ProxyMode::AutoDirect
    })
}

pub(crate) fn asset_proxy_urls(
    entries: &[GitHubProxy],
    direct_url: &str,
    is_raw_file: bool,
    branch: &str,
    file_path: &str,
    mode: ProxyMode,
    selected_id: Option<&str>,
) -> Vec<String> {
    if mode == ProxyMode::Direct {
        return Vec::new();
    }
    let mut entries = entries
        .iter()
        .filter(|proxy| {
            mode != ProxyMode::Fixed
                || selected_id.is_some_and(|id| !id.trim().is_empty() && proxy.id == id)
        })
        .cloned()
        .collect::<Vec<_>>();
    if mode != ProxyMode::Fixed {
        // Keep the user's order inside each source. A failed userscript fetch
        // leaves only the bundled entries, so those remain an automatic fallback.
        entries.sort_by_key(|proxy| !proxy.remote);
    }
    crate::github_proxies::proxy_urls(
        &entries,
        direct_url,
        is_raw_file,
        is_raw_file.then_some(crate::github_proxies::RawProxyContext { branch, file_path }),
    )
}

/// 按平台与文件类型选代理组。
///
/// Gitee 没有代理（只有 API 代理，资产直连）；GitHub 的 Release 资产与
/// raw 文件各用一组。MOD 仓库安装、地图 Release 安装与通用直链下载共用
/// 这份选择，避免每个调用点各写一遍「哪些字段是 Release 需要的」。
pub(crate) fn provider_proxy_entries<'a>(
    config: &'a AppConfig,
    provider: &Provider,
    is_raw_file: bool,
) -> &'a [GitHubProxy] {
    if *provider == Provider::Gitee {
        return &[];
    }
    if is_raw_file {
        &config.github_proxies.raw_proxies
    } else {
        &config.github_proxies.release_proxies
    }
}

/// 为一个远程 URL 规划完整文件下载候选（含代理展开与去重）。
///
/// 仓库/地图安装与直链下载共用同一套候选规则；代理模式按平台与地区取
/// [`selected_asset_proxy_mode`] 的默认值（Gitee 恒直连）。返回值按实际
/// 启用情况排序，长度 > 1 即表示调用方应当竞速。
pub(crate) fn plan_remote_download_candidates(
    config: &AppConfig,
    provider: &Provider,
    kind: crate::direct_mod::RemoteUrlKind,
    url: &str,
) -> Result<Vec<AssetDownloadCandidate>, String> {
    let mode = selected_asset_proxy_mode(None, provider, crate::config::current_user_region());
    let is_raw_file = kind == crate::direct_mod::RemoteUrlKind::Raw;
    let entries = provider_proxy_entries(config, provider, is_raw_file);
    // Raw 的代理模板要用 `{branch}` / `{path}`：仓库配置里已知的由调用方给出，
    // 只有 URL 时由这条唯一实现推导。
    let (branch, file_path) = if is_raw_file {
        crate::github_proxies::raw_branch_and_path(url)
    } else {
        (String::new(), String::new())
    };
    let proxy_urls = asset_proxy_urls(entries, url, is_raw_file, &branch, &file_path, mode, None);
    plan_asset_download_candidates(mode, url, proxy_urls)
}

fn plan_asset_download_candidates(
    mode: ProxyMode,
    direct_url: &str,
    proxy_urls: Vec<String>,
) -> Result<Vec<AssetDownloadCandidate>, String> {
    let candidates = ordered_asset_download_candidates(mode, direct_url, proxy_urls);
    if mode == ProxyMode::Fixed && candidates.is_empty() {
        return Err("proxy.noGlobalProxy".into());
    }
    Ok(candidates)
}

// Update and MOD downloads share candidate ordering, routing and deduplication.
// Callers supply the proxy snapshot available at their point in startup.
pub(crate) fn ordered_asset_download_candidates(
    mode: ProxyMode,
    direct_url: &str,
    proxy_urls: Vec<String>,
) -> Vec<AssetDownloadCandidate> {
    let direct = || AssetDownloadCandidate {
        route: AssetDownloadRoute::Direct,
        url: direct_url.to_string(),
    };
    let proxies = proxy_urls
        .into_iter()
        .filter(|url| url != direct_url)
        .map(|url| AssetDownloadCandidate {
            route: AssetDownloadRoute::Proxy,
            url,
        })
        .collect::<Vec<_>>();
    let candidates = match mode {
        ProxyMode::Direct => vec![direct()],
        ProxyMode::Fixed => proxies.into_iter().take(1).collect(),
        ProxyMode::AutoDirect => std::iter::once(direct()).chain(proxies).collect(),
        ProxyMode::AutoProxy => proxies
            .into_iter()
            .chain(std::iter::once(direct()))
            .collect(),
    };
    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|candidate| seen.insert(candidate.url.clone()))
        .collect()
}

/// 仓库安装的结果。
///
/// `preview_only` 为真时只下载并解析，不写入游戏目录，`preview` 字段
/// 携带可供安装弹窗展示的解析结果；正常安装时 `preview` 为空。
#[derive(serde::Serialize)]
pub struct RepositoryInstallOutcome {
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<crate::mod_preview::ModPreview>,
}

#[tauri::command]
pub async fn install_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    repository_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
    asset_proxy_mode: Option<ProxyMode>,
    asset_proxy_id: Option<String>,
    preview_only: Option<bool>,
    // 界面语言（`zh-CN` / `en` / …）。用来在下载进度标签与安装预览标题里
    // 显示仓库的本地化名称；缺省时回退到仓库中文名。
    locale: Option<String>,
) -> Result<RepositoryInstallOutcome, String> {
    // The UI can cancel while we prepare metadata. Reset once at command
    // entry, never after that wait or while switching download candidates.
    archive::start_download();
    log::info!(
        "开始安装在线 MOD：仓库={}，指定版本={}，指定资产={}，代理模式={:?}",
        repository_id,
        tag.as_deref().unwrap_or("自动选择"),
        asset_name.as_deref().unwrap_or("自动匹配"),
        asset_proxy_mode
    );
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if info.bepinex.version.is_none() {
        return Err("framework.bepInExRequiredBeforeMods".into());
    }
    let repo = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("source.repositoryMissing")?
        .clone();
    // 安装要求仓库在当前地区可用；隐藏/地区不匹配的仓库不能作为安装入口
    if !crate::config::repository_is_available(&config, &repo, crate::config::current_user_region())
    {
        return Err("source.repositoryUnavailableForInstall".into());
    }
    /*
     * 用于下载进度标签与安装预览标题的显示名。
     *
     * 必须在下载开始前算好：进度事件在下载过程中持续发出，label 是其中
     * 一个字段；等下载完再改就晚了。跟随界面语言，而不是固定用中文原名。
     */
    let display_name = crate::config::localized_repository_name(&repo, locale.as_deref());
    let selected_proxy_mode = selected_asset_proxy_mode(
        asset_proxy_mode,
        &repo.provider,
        crate::config::current_user_region(),
    );
    let (
        download_url,
        download_name,
        release_tag,
        release_body,
        release_prerelease,
        digest,
        is_raw_file,
        expected_size,
    ) = match repo.download_mode {
        RepositoryDownloadMode::RepositoryFile => {
            log::debug!(
                "按仓库文件模式安装：仓库={}，分支={}，文件={}",
                repo.name,
                repo.branch,
                repo.file_path
            );
            let download_url = releases::repository_file_url(
                &repo.provider,
                &repo.username,
                &repo.repository,
                &repo.branch,
                &repo.file_path,
            )
            .map_err(|error| error.to_string())?;
            let download_name = releases::repository_file_name(&repo.file_path)
                .map_err(|error| error.to_string())?;
            (
                download_url,
                download_name,
                String::new(),
                String::new(),
                false,
                None,
                true,
                None,
            )
        }
        RepositoryDownloadMode::Release => {
            log::debug!(
                "按 Release 模式安装：仓库={}，缓存 Release 数={}",
                repo.name,
                repo.cached_releases.len()
            );
            let fetched_releases = if repo.cached_releases.is_empty() {
                let client = releases::build_client(config.network.timeout_seconds)
                    .map_err(|e| e.to_string())?;
                let token = state.config.repository_api_token(&config, &repo);
                let api_proxies = match repo.provider {
                    Provider::Github => &config.github_proxies.github_api_proxies,
                    Provider::Gitee => &config.github_proxies.gitee_api_proxies,
                };
                releases::filter_channel(
                    releases::fetch_releases(
                        &client,
                        &repo,
                        token.as_deref(),
                        config.auth.official_api_only,
                        api_proxies,
                    )
                    .await
                    .map_err(|e| e.to_string())?,
                    &repo.channel,
                )
            } else {
                repo.cached_releases.clone()
            };
            let release = tag
                .as_deref()
                .and_then(|value| {
                    fetched_releases
                        .iter()
                        .find(|release| release.tag_name == value)
                })
                .or_else(|| {
                    repo.pinned_version.as_deref().and_then(|value| {
                        fetched_releases.iter().find(|release| {
                            release.tag_name == value || release.version.as_deref() == Some(value)
                        })
                    })
                })
                .or_else(|| fetched_releases.first())
                .ok_or("source.noInstallableRelease")?;
            let asset = releases::select_install_asset(&repo, release, asset_name.as_deref())
                .ok_or("source.noMatchingAsset")?;
            log::info!(
                "已选择 Release 资产：仓库={}，版本={}，资产={}",
                repo.name,
                release.tag_name,
                asset.name
            );
            (
                asset.download_url.clone(),
                asset.name.clone(),
                release.tag_name.clone(),
                release.body.clone(),
                release.prerelease,
                asset.digest.clone(),
                false,
                (asset.size > 0).then_some(asset.size),
            )
        }
    };
    let proxy_entries = provider_proxy_entries(&config, &repo.provider, is_raw_file);
    let proxy_urls = asset_proxy_urls(
        proxy_entries,
        &download_url,
        is_raw_file,
        &repo.branch,
        &repo.file_path,
        selected_proxy_mode,
        asset_proxy_id.as_deref(),
    );
    let candidates = plan_asset_download_candidates(selected_proxy_mode, &download_url, proxy_urls);
    let candidates = candidates.map_err(|error| {
        log::warn!("指定代理模式但没有可用的 GitHub 代理：仓库={}", repo.name);
        error
    })?;
    log::debug!(
        "下载地址准备完成：仓库={}，模式={}，下载候选数={}，原始文件={}",
        repo.name,
        if is_raw_file { "raw" } else { "release" },
        candidates.len(),
        is_raw_file
    );
    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|e| e.to_string())?;
    let download_threads = match &repo.provider {
        Provider::Github => repo.download_threads.clamp(1, 32),
        Provider::Gitee => 1,
    };
    log::info!(
        "开始下载仓库文件：仓库={}，平台={:?}，线程数={}，代理策略={:?}",
        repo.name,
        repo.provider,
        download_threads,
        selected_proxy_mode
    );
    let cache_dir = state.config.cache_dir();
    let racing = candidates.len() > 1;
    let temp = try_asset_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let client = &client;
            let cache_dir = &cache_dir;
            let label = &repo.name;
            let digest = if repo.provider == Provider::Github {
                digest.as_deref()
            } else {
                None
            };
            let is_zip = download_name.to_ascii_lowercase().ends_with(".zip");
            async move {
                let temp = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache_dir,
                    label,
                    &reporter,
                    if racing { 1 } else { download_threads },
                    Some(std::time::Duration::from_secs(5)),
                )
                .await?;
                let validation = archive::validate_download_file(&temp, expected_size, digest)
                    .and_then(|_| {
                        if is_zip {
                            archive::validate_zip(&temp)
                        } else {
                            Ok(())
                        }
                    });
                if let Err(error) = validation {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                Ok(temp)
            }
        },
        Some(DownloadContext {
            app: Some(&app),
            // 下载进度里显示的名字同样跟随界面语言
            label: &display_name,
            expected_size,
        }),
    )
    .await
    .map_err(|error| {
        log::error!("在线 MOD 资产下载失败：仓库={}，原因={}", repo.name, error);
        error
    })?;
    log::info!(
        "仓库文件下载完成：仓库={}，文件名={}",
        repo.name,
        download_name
    );
    /*
     * 预览模式：只下载并解析，不写入游戏目录。
     *
     * 复用这里已经完成的下载（含代理与竞速），把临时文件移进预览暂存，
     * 之后由 confirm_mod_install 正式安装。这样仓库卡片的「安装 / 重装 /
     * 更新」与手动安装、在线直链看到的是同一个预览弹窗。
     */
    if preview_only.unwrap_or(false) {
        let preview = crate::mod_preview::stage_downloaded_file(
            &state,
            &temp,
            &download_name,
            &repo.id,
            &display_name,
            &release_tag,
            &release_body,
            release_prerelease,
        )?;
        let _ = fs::remove_file(&temp);
        log::info!(
            "仓库资产已下载并解析（预览模式）：仓库={}，DLL 数={}",
            repo.name,
            preview.total_dlls
        );
        return Ok(RepositoryInstallOutcome {
            ok: true,
            message: crate::locale_tag::with_params(
                "mods.ready",
                &[("name", &repo.name.to_string())],
            ),
            preview: Some(preview),
        });
    }
    // No network awaits while the operation lock protects recycling and install.
    let files_result = (|| {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        if archive::is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        if game::is_game_running(&game_root) {
            log::warn!("下载期间检测到厨房重新启动，取消安装：仓库={}", repo.name);
            return Err("mods.gameRestartedDuringDownload".into());
        }
        let current_config = state.config.load_or_default();
        let install_repo =
            repository_install_identity(&repo, &current_config, &state.config.manifests_dir())?;
        install_downloaded_repository(
            &temp,
            &download_name,
            &game_root,
            &state.config.manifests_dir(),
            &state.config.mod_recycle_bin_dir(),
            &install_repo,
            &release_tag,
            release_prerelease,
            current_config.recycle_deleted_mods,
        )
    })();
    let _ = fs::remove_file(&temp);
    match files_result {
        Ok(file_count) => {
            log::info!(
                "在线 MOD 安装完成：仓库={}，版本={}，文件数={}",
                repo.name,
                release_tag,
                file_count
            );
            Ok(RepositoryInstallOutcome {
                ok: true,
                message: if release_tag.is_empty() {
                    crate::locale_tag::with_params(
                        "mods.installCompletedSimple",
                        &[("name", &repo.name.to_string())],
                    )
                } else {
                    crate::locale_tag::with_params(
                        "mods.installCompletedWithVersion",
                        &[
                            ("name", &repo.name.to_string()),
                            ("version", &release_tag.to_string()),
                        ],
                    )
                },
                preview: None,
            })
        }
        Err(error) => {
            log::error!("在线 MOD 安装失败：仓库={}，原因={}", repo.name, error);
            Err(error)
        }
    }
}

#[tauri::command]
pub fn cancel_download() -> Result<OperationResult, String> {
    log::info!("收到取消下载请求");
    archive::cancel_download();
    Ok(OperationResult {
        ok: true,
        message: "download.cancelling".into(),
    })
}

#[tauri::command]
pub async fn uninstall_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<OperationResult, String> {
    // 回收整包要解析安装清单并移动文件：重活，搬到阻塞线程池
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        uninstall_repository_blocking(&state, &repository_id)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`uninstall_repository`] 的同步实现（在阻塞线程池里跑）。
fn uninstall_repository_blocking(
    state: &AppState,
    repository_id: &str,
) -> Result<OperationResult, String> {
    log::info!("开始卸载在线仓库 MOD：仓库={}", repository_id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let package_id = if let Some(repo) = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
    {
        repository_install_identity(repo, &config, &state.config.manifests_dir())?.id
    } else {
        repository_id.to_string()
    };
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &package_id)
        .ok_or("mods.unknownFiles")?;
    if config.recycle_deleted_mods {
        let entry = recycle_bin::recycle_package(
            &state.config.mod_recycle_bin_dir(),
            &state.config.manifests_dir(),
            &game_root,
            &manifest.repository_id,
            None,
        )?;
        log::info!(
            "在线仓库 MOD 卸载完成（已回收）：仓库={}，文件数={}",
            repository_id,
            entry.file_count
        );
    } else {
        recycle_bin::purge_package(
            &state.config.mod_recycle_bin_dir(),
            &state.config.manifests_dir(),
            &game_root,
            &manifest.repository_id,
            None,
        )?;
        log::info!(
            "在线仓库 MOD 卸载完成（已直接删除）：仓库={}",
            repository_id
        );
    }
    Ok(OperationResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.mods.packageMovedToRecycleBin".into()
        } else {
            "message.mods.packageDeleted".into()
        },
    })
}

#[tauri::command]
pub fn get_mod_recycle_bin(state: State<'_, AppState>) -> Result<Vec<ModRecycleEntry>, String> {
    log::debug!("读取 MOD 回收站列表请求");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    recycle_bin::list(&state.config.mod_recycle_bin_dir())
        .map(|entries| {
            // 记录里存的是删除时的 canonical 路径，只在送给前端时换成展示形态。
            entries
                .into_iter()
                .map(|mut entry| {
                    entry.game_path = display_path(&entry.game_path);
                    entry
                })
                .collect()
        })
        .map_err(|error| {
            log::warn!("读取 MOD 回收站失败：{}", error);
            error
        })
}

#[tauri::command]
pub async fn restore_recycled_mod(
    state: State<'_, AppState>,
    id: String,
) -> Result<OperationResult, String> {
    // 回收站在数据根、游戏目录在别的盘时，恢复就是复制 + 删除：按 MB 计的重活
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || restore_recycled_mod_blocking(&state, &id))
        .await
        .map_err(|error| error.to_string())?
}

/// [`restore_recycled_mod`] 的同步实现（在阻塞线程池里跑）。
fn restore_recycled_mod_blocking(state: &AppState, id: &str) -> Result<OperationResult, String> {
    log::info!("请求恢复回收站 MOD：ID={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let original = recycle_bin::original_game_path(&state.config.mod_recycle_bin_dir(), &id)?;
    game::ensure_game_stopped(&game_root)?;
    game::ensure_game_stopped(&original)?;
    recycle_bin::restore(
        &state.config.mod_recycle_bin_dir(),
        &state.config.manifests_dir(),
        &game_root,
        &id,
    )
    .map_err(|error| {
        log::warn!("恢复回收站 MOD 失败：ID={}，原因={}", id, error);
        error
    })?;
    Ok(OperationResult {
        ok: true,
        message: "recycle.modRestored".into(),
    })
}

#[tauri::command]
pub async fn delete_recycled_mod(
    state: State<'_, AppState>,
    id: String,
) -> Result<OperationResult, String> {
    // 彻底删除是磁盘删除，量大的包要几秒：不能占着 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delete_recycled_mod_blocking(&state, &id))
        .await
        .map_err(|error| error.to_string())?
}

/// [`delete_recycled_mod`] 的同步实现（在阻塞线程池里跑）。
fn delete_recycled_mod_blocking(state: &AppState, id: &str) -> Result<OperationResult, String> {
    log::info!("请求彻底删除回收站 MOD：ID={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    recycle_bin::delete(&state.config.mod_recycle_bin_dir(), &id).map_err(|error| {
        log::warn!("彻底删除回收站 MOD 失败：ID={}，原因={}", id, error);
        error
    })?;
    Ok(OperationResult {
        ok: true,
        message: "recycle.modPermanentlyDeleted".into(),
    })
}

#[tauri::command]
pub async fn delete_all_recycled_mods(
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    // 清空回收站是整目录删除：重活，搬到阻塞线程池
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || delete_all_recycled_mods_blocking(&state))
        .await
        .map_err(|error| error.to_string())?
}

/// [`delete_all_recycled_mods`] 的同步实现（在阻塞线程池里跑）。
fn delete_all_recycled_mods_blocking(state: &AppState) -> Result<OperationResult, String> {
    log::info!("请求清空回收站");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let removed =
        recycle_bin::delete_all(&state.config.mod_recycle_bin_dir()).map_err(|error| {
            log::warn!("清空回收站失败：原因={}", error);
            error
        })?;
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "recycle.emptied",
            &[("count", &removed.to_string())],
        ),
    })
}

#[tauri::command]
pub fn export_repositories(
    state: State<'_, AppState>,
    ids: Option<Vec<String>>,
) -> Result<String, String> {
    log::debug!(
        "导出仓库配置：指定数量={}",
        ids.as_ref().map_or(0, Vec::len)
    );
    let config = state.config.load_or_default();
    state
        .config
        .export_bundle(&config, ids)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_repositories_to_file(
    state: State<'_, AppState>,
    path: String,
    ids: Option<Vec<String>>,
) -> Result<OperationResult, String> {
    log::info!("导出仓库配置到文件");
    let text = export_repositories(state, ids)?;
    if !path.to_ascii_lowercase().ends_with(".oc2repo.toml")
        && !path.to_ascii_lowercase().ends_with(".toml")
    {
        return Err("config.exportExtensionInvalid".into());
    }
    fs::write(&path, text).map_err(|e| e.to_string())?;
    log::info!("仓库配置导出完成");
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params("config.exported", &[("path", &path.to_string())]),
    })
}

#[tauri::command]
pub fn copy_repositories(
    state: State<'_, AppState>,
    ids: Option<Vec<String>>,
) -> Result<OperationResult, String> {
    log::info!("复制仓库配置到剪贴板");
    let text = export_repositories(state, ids)?;
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "config.tomlCopied".into(),
    })
}

#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    log::debug!("读取剪贴板文本");
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn copy_text(text: String) -> Result<OperationResult, String> {
    log::debug!("复制文本到剪贴板：长度={}", text.len());
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "tutorial.formatCopied".into(),
    })
}

/// 街机MOD 里「多彩名字」功能的本地监听端口范围（含两端）。
const COLORFUL_NAME_PORTS: std::ops::RangeInclusive<u16> = 28780..=28790;
/// 街机MOD 的显示名上限：UTF-16LE 238 字节，即 119 个 UTF-16 代码单元。
const COLORFUL_NAME_MAX_UNITS: usize = 119;
/// 官方改名网页的来源，仅用于**旧版**街机MOD 的兜底通道。
///
/// 街机MOD v1.9.9 起 `IsValidToken` 只认 `X-OC2-Token`，Origin 已被明确
/// 移除（网页同样改发请求头）。这里的常量只服务还没写出握手文件的旧版，
/// 不要把它当常规通道，也不要据此放宽校验。
const COLORFUL_NAME_ORIGIN: &str = "https://oc2-change-colorful-name.ch3ng.top";

/// 街机MOD 为本地管理器写出的握手文件（相对游戏目录）。
///
/// MOD 已经把自己的业务数据放在 `BepInEx/config/OC2HostUtilities/` 下，
/// 握手文件沿用同一目录。管理器读到 token 就走 token 鉴权，读不到（旧版
/// MOD 还没写这个文件）才退回官方 Origin。
const COLORFUL_NAME_HANDSHAKE: &str = "BepInEx/config/OC2HostUtilities/web-bridge.json";

/// 握手文件的内容。字段全部可缺省，缺失即视为「没有可用握手」。
#[derive(serde::Deserialize)]
struct ColorfulNameHandshake {
    port: u16,
    token: String,
    /// MOD 自己声明的协议版本；不认识的版本按旧行为处理。
    #[serde(default)]
    version: u32,
}

/// 本次推送使用的凭据。
struct ColorfulNameCredential {
    port: u16,
    token: String,
}

/// 读取街机MOD 写出的握手文件。
///
/// 只接受落在端口段内、32 位十六进制的 token，以及已知的协议版本 —— 这个
/// 文件在游戏目录里，任何能写游戏目录的程序都能伪造它，所以校验的作用是
/// 不把畸形/过期内容当成有效凭据，而不是把它当安全边界。
fn read_colorful_name_handshake(game_root: &Path) -> Option<ColorfulNameCredential> {
    let path = crate::game::safe_game_path(game_root, COLORFUL_NAME_HANDSHAKE).ok()?;
    let text = std::fs::read_to_string(&path).ok()?;
    let handshake: ColorfulNameHandshake = serde_json::from_str(&text).ok()?;
    if handshake.version != 1 {
        log::warn!(
            "街机MOD 握手文件版本未知，按旧方式推送：版本={}",
            handshake.version
        );
        return None;
    }
    if !COLORFUL_NAME_PORTS.contains(&handshake.port) {
        log::warn!("街机MOD 握手端口越界，忽略：端口={}", handshake.port);
        return None;
    }
    let token = handshake.token.trim();
    if token.len() != 32 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        log::warn!("街机MOD 握手 token 格式非法，忽略");
        return None;
    }
    Some(ColorfulNameCredential {
        port: handshake.port,
        token: token.to_ascii_lowercase(),
    })
}

/// 单个端口的推送结果。
///
/// 「连上了但被拒绝」必须和「端口没人监听」区分开：前者说明街机MOD 正在
/// 运行，报错方向是鉴权或版本，后者才是「请打开游戏」。
enum ColorfulNameOutcome {
    Delivered(u16),
    Rejected(u16, String),
    Unreachable(u16),
}

/// 向街机MOD 的 `/set-name` 发一次改名请求。
///
/// `token` 为 `None` 时带官方 Origin（只有旧版街机MOD 还认它）；有 token 时
/// 改用 `X-OC2-Token`，这是当前唯一的正规通道。
async fn send_colorful_name(
    client: &reqwest::Client,
    url: &str,
    name: &str,
    token: Option<&str>,
) -> Result<reqwest::Response, reqwest::Error> {
    // 凭据是常量或握手文件里的值，不含控制字符，不需要额外转义。
    let request = client
        .post(url)
        .header("Content-Type", "application/json;charset=utf-8");
    let request = match token {
        Some(token) => request.header("X-OC2-Token", token),
        None => request.header("Origin", COLORFUL_NAME_ORIGIN),
    };
    request
        .body(serde_json::json!({ "name": name }).to_string())
        .send()
        .await
}

/// 把一次请求的结果归类。
///
/// 与发请求分开，测试才能直接对着任意本地地址跑完整的 2xx / 非 2xx /
/// 连不上三条分支，而不必去占用真实的 28780-28790。
async fn classify_colorful_name(
    port: u16,
    response: Result<reqwest::Response, reqwest::Error>,
) -> ColorfulNameOutcome {
    match response {
        Ok(response) if response.status().is_success() => ColorfulNameOutcome::Delivered(port),
        Ok(response) => {
            let status = response.status();
            // 街机MOD 的 403 正文就是「Token 不匹配」，写进日志才能直接定位鉴权问题。
            let body = response.text().await.unwrap_or_default();
            let body = body.chars().take(80).collect::<String>();
            let detail = format!("{status} {}", body.trim()).trim().to_string();
            ColorfulNameOutcome::Rejected(port, detail)
        }
        Err(_) => ColorfulNameOutcome::Unreachable(port),
    }
}

async fn post_colorful_name(
    client: &reqwest::Client,
    port: u16,
    name: &str,
    token: Option<&str>,
) -> ColorfulNameOutcome {
    let url = colorful_name_url(port);
    let response = send_colorful_name(client, &url, name, token).await;
    classify_colorful_name(port, response).await
}

fn colorful_name_url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/set-name")
}

/// 把生成好的名字推送给街机MOD 的「多彩名字」功能。
///
/// 前端 CSP 只允许 `self` 与 IPC，无法直接访问 `http://127.0.0.1:287xx`，
/// 所以由后端尝试 POST；任一端口返回 2xx 即视为成功。
#[tauri::command]
pub async fn push_colorful_name(
    state: State<'_, AppState>,
    name: String,
) -> Result<OperationResult, String> {
    if name.trim().is_empty() {
        return Err("rename.nameEmpty".into());
    }
    if name.chars().any(|character| character.is_control()) {
        return Err("rename.nameHasControlChars".into());
    }
    let units = name.encode_utf16().count();
    if units > COLORFUL_NAME_MAX_UNITS {
        // tag + 内联参数：前端按 tag 翻译并插值，后端不持有任何界面文案
        return Err(format!(
            "error.rename.nameTooLong?bytes={}&limit={}",
            units * 2,
            COLORFUL_NAME_MAX_UNITS * 2
        ));
    }
    log::info!("推送多彩名字到街机MOD：UTF-16 单元={}", units);

    /*
     * 握手文件必须按**探测到的**游戏目录去找，不能读 `config.game.path`。
     *
     * 真实事故：配置里 `game.path` 为空（目录是每次运行时探测出来的，
     * 用户从没手动填过），于是这里判定「没配目录」直接走 Origin 兜底；
     * 而新街机MOD 已改成只认 `X-OC2-Token`，Origin 必然 403，用户看到
     * 的是「MOD 拒绝了请求」。与其它命令保持一致，用 `detect_game` 的
     * 结果。
     */
    let credential = {
        let config = state.config.load_or_default();
        game::detect_game(&config)
            .path
            .as_deref()
            .and_then(|game_root| read_colorful_name_handshake(Path::new(game_root)))
    };
    let token = credential
        .as_ref()
        .map(|credential| credential.token.as_str());
    let ports: Vec<u16> = match &credential {
        Some(credential) => vec![credential.port],
        None => COLORFUL_NAME_PORTS.collect(),
    };
    if token.is_some() {
        log::info!("街机MOD 握手可用：端口={ports:?}，使用 token 鉴权");
    } else {
        log::warn!(
            "未找到街机MOD 握手文件（{COLORFUL_NAME_HANDSHAKE}），改用 Origin 兜底；新版本街机MOD 只认 token，这一步很可能被拒绝"
        );
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(1500))
        .build()
        .map_err(|e| {
            crate::locale_tag::with_params(
                "network.clientCreateFailed",
                &[("reason", &e.to_string())],
            )
        })?;
    let attempts = ports.into_iter().map(|port| {
        let client = client.clone();
        let name = &name;
        async move { post_colorful_name(&client, port, name, token).await }
    });
    let outcomes = futures_util::future::join_all(attempts).await;
    let mut rejected = Vec::new();
    let mut unreachable = Vec::new();
    for outcome in &outcomes {
        match outcome {
            ColorfulNameOutcome::Delivered(port) => {
                log::info!("街机MOD 已接收多彩名字：端口={port}");
                return Ok(OperationResult {
                    ok: true,
                    message: "rename.pushed".into(),
                });
            }
            ColorfulNameOutcome::Rejected(port, detail) => {
                log::warn!("多彩名字端口 {port} 拒绝了请求：{detail}");
                rejected.push(*port);
            }
            ColorfulNameOutcome::Unreachable(port) => unreachable.push(*port),
        }
    }
    if !rejected.is_empty() {
        log::warn!(
            "多彩名字推送失败：街机MOD 在端口 {rejected:?} 上拒绝请求，无响应的端口={unreachable:?}，检查街机MOD 是否已写出握手文件"
        );
        // 走到这里说明端口有人听、但没通过鉴权：对用户只有一种可执行的建议。
        return Err("diyLevel.rejected".into());
    }
    log::warn!("多彩名字推送失败：{unreachable:?} 端口均无响应");
    Err("diyLevel.notRunning".into())
}

fn parse_tutorial_document(text: &str) -> Result<TutorialDocument, String> {
    if text.len() > 1024 * 1024 {
        return Err("tutorial.configTooLarge".into());
    }
    let trimmed = text.trim();
    let content = if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let _ = lines.next();
        let body = lines.collect::<Vec<_>>();
        let end = body
            .iter()
            .rposition(|line| line.trim() == "```")
            .ok_or_else(|| "tutorial.unclosedCodeBlock".to_string())?;
        body[..end].join("\n")
    } else {
        trimmed.to_string()
    };
    let document = toml::from_str::<TutorialDocument>(&content).map_err(|error| {
        crate::locale_tag::with_params(
            "tutorial.tomlInvalidReason",
            &[("reason", &error.to_string())],
        )
    })?;
    if document.version != 1 {
        return Err(crate::locale_tag::with_params(
            "tutorial.unsupportedVersion",
            &[("version", &document.version.to_string())],
        ));
    }
    Ok(document)
}

#[tauri::command]
pub fn get_builtin_tutorial() -> Result<Vec<TutorialStepConfig>, String> {
    log::debug!("读取内置教程配置");
    parse_tutorial_document(include_str!("../resources/builtin-tutorial.toml"))
        .map(|document| document.steps)
}

#[tauri::command]
pub fn parse_tutorial_config(text: String) -> Result<Vec<TutorialStepConfig>, String> {
    log::debug!("解析教程配置：长度={}", text.len());
    parse_tutorial_document(&text).map(|document| document.steps)
}

#[tauri::command]
pub fn serialize_tutorial_config(steps: Vec<TutorialStepConfig>) -> Result<String, String> {
    log::debug!("序列化教程配置：步骤数={}", steps.len());
    toml::to_string_pretty(&TutorialDocument { version: 1, steps }).map_err(|error| {
        crate::locale_tag::with_params("tutorial.exportFailed", &[("reason", &error.to_string())])
    })
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    log::debug!("读取文本文件");
    let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 {
        return Err("tutorial.importTooLarge".into());
    }
    fs::read_to_string(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn preview_repository_import(
    state: State<'_, AppState>,
    text: String,
) -> Result<ImportPreview, String> {
    log::info!("预览仓库配置导入：文本长度={}", text.len());
    let config = state.config.load_or_default();
    let bundle = state
        .config
        .validate_bundle(&text)
        .map_err(|e| e.to_string())?;
    Ok(state.config.preview_bundle(&config, &bundle))
}

#[tauri::command]
pub fn import_repositories(
    state: State<'_, AppState>,
    text: String,
    conflict: String,
) -> Result<ImportPreview, String> {
    log::info!(
        "导入仓库配置：冲突策略={}，文本长度={}",
        conflict,
        text.len()
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let bundle = state
        .config
        .validate_bundle(&text)
        .map_err(|e| e.to_string())?;
    let preview = state.config.import_bundle(&mut config, &bundle, &conflict);
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!(
        "仓库配置导入完成：新增={}，重复={}，冲突={}",
        preview.additions.len(),
        preview.duplicates.len(),
        preview.conflicts.len()
    );
    Ok(preview)
}

#[tauri::command]
pub async fn clear_repository_source_cache(
    state: State<'_, AppState>,
    source_ids: Vec<String>,
) -> Result<PublicConfig, String> {
    if source_ids.iter().any(|id| crate::community::is_source(id)) {
        return Err("source.communityManaged".into());
    }
    let _cache_guard = state.repository_cache.lock().await;
    let _operation_guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    if source_ids.iter().any(|id| {
        !config
            .repository_sources
            .iter()
            .any(|source| source.id == *id)
    }) {
        return Err("source.sourceMissing".into());
    }
    for source in config
        .repository_sources
        .iter()
        .filter(|source| source_ids.contains(&source.id))
    {
        log::info!(
            "仓库源刷新计划：源={}，地址={}，父源={}",
            source.id,
            repository_sources::safe_log_address(&source.url),
            source.parent_id.as_deref().unwrap_or("无")
        );
    }
    let (repository_count, child_source_count) =
        repository_sources::clear_source_cache(&mut config, &source_ids);
    state
        .config
        .save(&config)
        .map_err(|error| error.to_string())?;
    log::info!(
        "整批刷新前已清理仓库源缓存：源={}，子源={}，仓库={}",
        source_ids.len(),
        child_source_count,
        repository_count
    );
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub async fn import_repository_source(
    state: State<'_, AppState>,
    source_id: String,
    source_url: String,
    builtin_fallback: Option<bool>,
) -> Result<RepositorySourceImportResult, String> {
    import_repository_source_into_config(
        &state,
        source_id,
        source_url,
        builtin_fallback.unwrap_or(false),
    )
    .await
}

async fn import_repository_source_into_config(
    state: &AppState,
    source_id: String,
    source_url: String,
    builtin_fallback: bool,
) -> Result<RepositorySourceImportResult, String> {
    if crate::community::is_source(&source_id) {
        return Err("source.communityManaged".into());
    }
    log::info!("开始刷新仓库源：源={}", source_id);
    // Clear this source and all descendants before loading. The lock is
    // deliberately released during the network request so other sources can
    // download and apply their own files concurrently. Keep the source itself
    // so the UI can show its error state, but remove stale descendants and
    // repositories instead of restoring them after a failed request.
    let (source_url, builtin_fallback_available) = {
        let _cache_guard = state.repository_cache.lock().await;
        let _operation_guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        let original_config = state.config.load_or_default();
        let Some(source) = original_config
            .repository_sources
            .iter()
            .find(|source| source.id == source_id)
        else {
            log::warn!("刷新仓库源失败：源不存在，源={}", source_id);
            return Err("source.sourceMissing".into());
        };
        if !source.url.trim().eq_ignore_ascii_case(source_url.trim()) {
            log::warn!("刷新仓库源失败：请求地址与当前配置不一致，源={}", source_id);
            return Err("source.addressMismatch".into());
        }
        let builtin_fallback_available = source.id == repository_sources::DEFAULT_INDEX_ID
            && [
                repository_sources::DEFAULT_INDEX_GITEE_URL,
                repository_sources::DEFAULT_INDEX_GITHUB_URL,
            ]
            .contains(&source.url.as_str())
            && source.builtin
            && source.parent_id.is_none();
        if builtin_fallback && !builtin_fallback_available {
            return Err("source.noBuiltinFallback".into());
        }
        let source_url = source.url.clone();
        log::info!(
            "仓库源开始读取：源={}，地址={}，父源={}",
            source_id,
            repository_sources::safe_log_address(&source_url),
            source.parent_id.as_deref().unwrap_or("无")
        );
        let mut cleared_config = original_config.clone();
        let (removed_repository_count, removed_child_source_count) =
            repository_sources::clear_source_cache(
                &mut cleared_config,
                std::slice::from_ref(&source_id),
            );
        state.config.save(&cleared_config).map_err(|error| {
            log::error!(
                "刷新仓库源前清理旧内容失败：源={}，原因={}",
                source_id,
                error
            );
            error.to_string()
        })?;
        log::info!(
            "刷新仓库源前已清理旧内容：源={}，子源={}，仓库={}",
            source_id,
            removed_child_source_count,
            removed_repository_count
        );
        (source_url, builtin_fallback_available && !builtin_fallback)
    };
    let document_url = if builtin_fallback {
        log::warn!(
            "官方远程源不可用，开始加载内置仓库源：源={}，文件={}",
            source_id,
            repository_sources::BUILTIN_INDEX
        );
        repository_sources::BUILTIN_INDEX
    } else {
        &source_url
    };
    let location = SourceLocation::parse(document_url).map_err(|error| {
        log::error!(
            "仓库源地址解析失败：源={}，错误类别={}",
            source_id,
            repository_sources::error_category(&error)
        );
        error.to_string()
    })?;
    // 超时由 load_document 内部固定为 SOURCE_DOCUMENT_TIMEOUT（30 秒）
    let document = match repository_sources::load_document(&location).await {
        Ok(document) => document,
        Err(error) => {
            log::error!(
                "仓库源读取失败：源={}，错误类别={}",
                source_id,
                repository_sources::error_category(&error)
            );
            // HTTP/connectivity failures can trigger the caller's fallback
            // decision. Invalid TOML, UTF-8 and unsafe content cannot.
            if matches!(error, crate::config::ConfigError::Read(_)) {
                return Ok(RepositorySourceImportResult {
                    preview: None,
                    access_error: Some(error.to_string()),
                    builtin_fallback_available,
                });
            }
            return Err(error.to_string());
        }
    };
    let repositories = document
        .repositories
        .into_iter()
        .map(|mut repository| {
            // source_id in an exported configuration describes the author's
            // local manager state, so it is deliberately ignored here.
            repository.source_id = None;
            repository
        })
        .collect::<Vec<_>>();
    let source_repository_keys = repositories
        .iter()
        .map(crate::config::export_key)
        .collect::<std::collections::HashSet<_>>();
    let child_sources = document.repository_sources;
    let child_source_count = child_sources.len();
    let bundle = crate::models::RepositoryBundle {
        format: "oc2-mod-repository-bundle".into(),
        schema_version: 1,
        exported_at: Utc::now().to_rfc3339(),
        repositories,
        repository_sources: Vec::new(),
    };
    let _cache_guard = state.repository_cache.lock().await;
    let _operation_guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    config
        .repositories
        .retain(|repo| repo.source_id.as_deref() != Some(source_id.as_str()));
    let parent_url = config
        .repository_sources
        .iter()
        .find(|source| source.id == source_id)
        .map(|source| source.url.trim().to_ascii_lowercase());
    let parent_builtin = config
        .repository_sources
        .iter()
        .find(|source| source.id == source_id)
        .is_some_and(|source| source.builtin);
    let declared_child_ids = child_sources
        .iter()
        .filter(|source| {
            parent_url
                .as_deref()
                .map(|url| !source.url.trim().eq_ignore_ascii_case(url))
                .unwrap_or(true)
        })
        .map(|source| source.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let stale_child_ids = config
        .repository_sources
        .iter()
        .filter(|source| source.parent_id.as_deref() == Some(source_id.as_str()))
        .filter(|source| !declared_child_ids.contains(source.id.as_str()))
        .map(|source| source.id.clone())
        .collect::<Vec<_>>();
    let mut removed_child_ids = stale_child_ids;
    let mut changed = true;
    while changed {
        changed = false;
        for source in &config.repository_sources {
            if let Some(parent_id) = source.parent_id.as_deref()
                && removed_child_ids.iter().any(|removed| removed == parent_id)
                && !removed_child_ids
                    .iter()
                    .any(|removed| removed == &source.id)
            {
                removed_child_ids.push(source.id.clone());
                changed = true;
            }
        }
    }
    if !removed_child_ids.is_empty() {
        config
            .repository_sources
            .retain(|source| !removed_child_ids.iter().any(|id| id == &source.id));
        config.repositories.retain(|repo| {
            repo.source_id
                .as_ref()
                .map(|id| !removed_child_ids.iter().any(|child_id| child_id == id))
                .unwrap_or(true)
        });
    }
    for child in child_sources {
        if crate::community::is_source(&child.id) {
            log::warn!("忽略源文档中的保留社区源标识：源={}", source_id);
            continue;
        }
        let child_url = child.url.trim().to_string();
        // Remote declarations cannot elevate their own privilege. An omitted
        // value inherits the parent; an explicit false cuts the chain, and
        // every descendant then remains non-builtin.
        let child_builtin = parent_builtin && child.builtin.unwrap_or(true);
        if parent_url
            .as_deref()
            .map(|url| child_url.eq_ignore_ascii_case(url))
            .unwrap_or(false)
        {
            continue;
        }
        if let Some(existing) = config.repository_sources.iter_mut().find(|source| {
            source.id == child.id && source.parent_id.as_deref() == Some(source_id.as_str())
        }) {
            // 声明里的 nickname 已是 zh-CN 兜底值，运行时镜像与多语言表一起写
            existing.nickname = child.nickname.trim().to_string();
            existing.nickname_i18n = child.nickname_i18n;
            existing.url = child_url;
            existing.region = child.region.clone();
            existing.builtin = child_builtin;
            continue;
        }
        if config.repository_sources.iter().any(|source| {
            source_address_key(&source.url, source.region.as_deref())
                == source_address_key(&child_url, child.region.as_deref())
        }) {
            // Keep a single owner per URL and region, while allowing regional
            // variants of the same source file to coexist.
            continue;
        }
        let child_id = if config
            .repository_sources
            .iter()
            .any(|source| source.id == child.id)
        {
            Uuid::new_v4().to_string()
        } else {
            child.id.trim().to_string()
        };
        config.repository_sources.push(RepositorySource {
            id: child_id,
            nickname: child.nickname.trim().to_string(),
            nickname_i18n: child.nickname_i18n,
            url: child_url,
            region: child.region,
            builtin: child_builtin,
            parent_id: Some(source_id.clone()),
        });
    }
    let preview = state
        .config
        .import_bundle(&mut config, &bundle, "overwrite");
    let mut used_ids = config
        .repositories
        .iter()
        .map(|repo| repo.id.clone())
        .collect::<HashSet<_>>();
    for repo in &mut config.repositories {
        let identity = crate::config::repo_key(repo);
        if !crate::community::is_repository(repo) && source_repository_keys.contains(&identity) {
            let key = crate::config::source_repository_key(&source_id, &identity);
            used_ids.remove(&repo.id);
            if let Some(previous_id) = config.source_repository_ids.get(&key)
                && !used_ids.contains(previous_id)
            {
                repo.id = previous_id.clone();
                /*
                 * 把清理阶段暂存的 Release 缓存接回来。
                 *
                 * 不接的话，5 分钟缓存**永远命中不了**：整批刷新先删光仓库、
                 * 再从源文档重建，重建出来的是全新空仓库，于是每次刷新都把
                 * 整棵仓库树的 API 重打一遍（20+ 个仓库、几十个请求，还会撞
                 * Gitee 速率限制）。缓存本身没坏，只是被「重建」这件事丢了。
                 *
                 * ID 已由 `source_repository_ids` 保住，所以按 ID 取得到。
                 */
                if let Some(staged) = config.staged_release_cache.remove(previous_id) {
                    repo.cached_releases = staged.releases;
                    repo.releases_cached_at = staged.cached_at;
                    repo.last_error = staged.last_error;
                }
            }
            used_ids.insert(repo.id.clone());
            config.source_repository_ids.insert(key, repo.id.clone());
            repo.source_id = Some(source_id.clone());
        }
    }
    /*
     * 不要在这里清空暂存表。
     *
     * `apply_document` 是**逐源调用**的：一次整批刷新会对每个源各调一次。
     * 早先在这里无条件 `clear()`，于是第一个源应用完就把**所有**源的暂存缓存
     * 全丢了 —— 后面的源重建仓库时取不到自己的缓存，5 分钟缓存等于没做，
     * 每次都把整棵树重打一遍（用户看到的就是「跳过缓存=false」却仍在请求）。
     *
     * 认领过的条目在上面 `remove` 时已经拿走了；剩下的属于「源文档里还没
     * 应用到的源」，必须留到它们的 `apply_document` 跑完。整批刷新结束时
     * 由 `prune_staged_release_cache` 统一清理。
     */
    state.config.save(&config).map_err(|error| {
        log::error!("仓库源刷新后保存配置失败：源={}，原因={}", source_id, error);
        error.to_string()
    })?;
    // 配置已经是本次源文档的最新内容：此时才同步安装清单里的分组快照。
    // 只有分组确实变化才写文件，避免每次刷新都重写全部清单。
    sync_manifest_groups(&state, &config);
    log::info!(
        "仓库源刷新应用完成：源={}，仓库={}，新增={}，重复={}，冲突={}，子源声明={}，移除旧子源={}，结果有效={}",
        source_id,
        bundle.repositories.len(),
        preview.additions.len(),
        preview.duplicates.len(),
        preview.conflicts.len(),
        child_source_count,
        removed_child_ids.len(),
        preview.valid
    );
    Ok(RepositorySourceImportResult {
        preview: Some(preview),
        access_error: None,
        builtin_fallback_available,
    })
}

/// 把安装清单里的分组快照同步成仓库当前的分组，只在真正变化时写文件。
///
/// 仓库源是权威数据源：清单里的分组只是缓存。这里在源文档刷新成功、
/// 配置已是最新内容后调用，因此不会出现「刚启动还没刷新 → 分类被清空」。
///
/// 同时补齐旧版本升级遗留的缺失字段：
///   - `group_i18n` / `group` 为空（旧清单）→ 从当前仓库补全
///   - `repository_binding` 缺失（更旧的清单）→ 按仓库 + release_tag 重建，
///     没有 release_tag 的 RAW 安装则留空（它本来就没有版本绑定）
fn sync_manifest_groups(state: &AppState, config: &AppConfig) {
    let manifest_dir = state.config.manifests_dir();
    let manifests = archive::read_manifests(&manifest_dir);
    let mut updated = 0usize;
    for mut manifest in manifests {
        let Some(repository) = config
            .repositories
            .iter()
            .find(|repo| repo.id == manifest.repository_id)
        else {
            continue;
        };

        // 清单现在只存 i18n 快照，比较时也只看它
        let group_changed = manifest.group_i18n != repository.group_i18n;
        /*
         * 包名的多语言快照也要同步：仓库名改了之后，MOD 管理页的分组标题
         * 会更新，但包名停在上次安装时的旧翻译，两边对不上。
         * 只在仓库确实提供了 name_i18n 时才覆盖，避免把手工补的名字抹掉。
         */
        let name_changed =
            !repository.name_i18n.is_empty() && manifest.package_name_i18n != repository.name_i18n;
        // 旧清单没有版本绑定：能用 release_tag 重建就补上
        let binding_missing = manifest.repository_binding.is_none()
            && !manifest.release_tag.trim().is_empty()
            && repository.download_mode == crate::models::RepositoryDownloadMode::Release;
        // 旧清单没记预发布标记：从缓存的 Release 里按 tag 找回（找得到才改）
        let prerelease_missing = !manifest.prerelease
            && repository
                .cached_releases
                .iter()
                .find(|release| release.tag_name == manifest.release_tag)
                .is_some_and(|release| release.prerelease);
        if !group_changed && !name_changed && !binding_missing && !prerelease_missing {
            continue;
        }

        if group_changed {
            log::info!(
                "更新安装清单分组快照：仓库={}，旧分组语言数={}，新分组语言数={}",
                manifest.repository_id,
                manifest.group_i18n.len(),
                repository.group_i18n.len()
            );
            manifest.group_i18n = repository.group_i18n.clone();
        }
        if name_changed {
            manifest.package_name_i18n = repository.name_i18n.clone();
            log::info!(
                "更新安装清单仓库名快照：仓库={}，语言数={}",
                manifest.repository_id,
                repository.name_i18n.len()
            );
        }
        if binding_missing {
            manifest.repository_binding =
                RepositoryBinding::from_release(repository, &manifest.release_tag);
            log::info!(
                "补齐安装清单版本绑定：仓库={}，标签={}",
                manifest.repository_id,
                manifest.release_tag
            );
        }
        if prerelease_missing {
            manifest.prerelease = true;
            log::info!(
                "补齐安装清单预发布标记：仓库={}，标签={}",
                manifest.repository_id,
                manifest.release_tag
            );
        }
        if let Err(error) = archive::write_manifest(&manifest_dir, &manifest) {
            log::warn!(
                "更新安装清单失败：仓库={}，原因={}",
                manifest.repository_id,
                error
            );
            continue;
        }
        updated += 1;
    }
    if updated > 0 {
        log::info!("安装清单分组快照更新完成：数量={}", updated);
    }
}

#[tauri::command]
pub fn save_repository_source(
    state: State<'_, AppState>,
    id: Option<String>,
    nickname: String,
    url: String,
    nickname_i18n: Option<std::collections::BTreeMap<String, String>>,
) -> Result<PublicConfig, String> {
    if id.as_deref().is_some_and(crate::community::is_source) {
        return Err("source.communityManaged".into());
    }
    log::info!(
        "保存仓库源：操作={}",
        if id.is_some() { "编辑" } else { "新增" }
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let nickname = nickname.trim();
    let url = url.trim();
    Url::parse(url).map_err(|error| {
        crate::locale_tag::with_params("source.addressInvalid", &[("reason", &error.to_string())])
    })?;
    if nickname.is_empty() || url.is_empty() {
        return Err("source.nicknameOrAddressEmpty".into());
    }
    /*
     * 昵称只保存多语言表：单语参数作为 zh-CN 兜底写进表里。
     *
     * 与源文档一致（`normalize_source_nickname`）：表里必须始终有 zh-CN。
     * 前端仍按「单语 + 可选翻译」提交，是因为编辑器只有一个主输入框；
     * 归一化放在后端入口，避免两个调用方各写一遍。
     */
    let mut nickname_map = nickname_i18n.unwrap_or_default();
    nickname_map.retain(|_, value| !value.trim().is_empty());
    if !nickname_map.contains_key("zh-CN") {
        nickname_map.insert("zh-CN".into(), nickname.to_string());
    }
    let effective_nickname = crate::repository_sources::nickname_fallback(&nickname_map)
        .ok_or("source.nicknameEmpty")?;
    let region = config
        .repository_sources
        .iter()
        .find(|source| Some(source.id.as_str()) == id.as_deref())
        .and_then(|source| source.region.as_deref());
    let source_key = source_address_key(url, region);
    if config.repository_sources.iter().any(|item| {
        Some(item.id.as_str()) != id.as_deref()
            && source_address_key(&item.url, item.region.as_deref()) == source_key
    }) {
        return Err("source.addressExists".into());
    }
    if let Some(id) = id {
        let source = config
            .repository_sources
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("source.sourceMissing")?;
        if source.builtin {
            return Err("source.builtinCannotEdit".into());
        }
        source.nickname = effective_nickname;
        source.nickname_i18n = nickname_map;
        source.url = url.into();
    } else {
        config.repository_sources.push(RepositorySource {
            id: Uuid::new_v4().to_string(),
            nickname: effective_nickname,
            nickname_i18n: nickname_map,
            url: url.into(),
            region: None,
            builtin: false,
            parent_id: None,
        });
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("仓库源保存完成");
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn save_repository_sources(
    state: State<'_, AppState>,
    sources: Vec<RepositorySource>,
) -> Result<PublicConfig, String> {
    log::info!("批量保存仓库源：数量={}", sources.len());
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let has_builtin_root = config
        .repository_sources
        .iter()
        .any(|source| source.builtin && source.parent_id.is_none());
    if !has_builtin_root {
        return Err("source.builtinMissing".into());
    }
    let builtin_ids = config
        .repository_sources
        .iter()
        .filter(|source| source.builtin && source.parent_id.is_none())
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let builtin_root_urls = config
        .repository_sources
        .iter()
        .filter(|source| source.builtin && source.parent_id.is_none())
        .map(|source| source_address_key(&source.url, source.region.as_deref()))
        .collect::<std::collections::HashSet<_>>();
    let mut seen_urls = std::collections::HashSet::new();
    seen_urls.extend(builtin_root_urls);
    seen_urls.extend(
        config
            .repository_sources
            .iter()
            .filter(|source| source.builtin && source.parent_id.is_some())
            .map(|source| source_address_key(&source.url, source.region.as_deref())),
    );
    let mut seen_ids = config
        .repository_sources
        .iter()
        .filter(|source| source.builtin || crate::community::is_source(&source.id))
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut ordered = config
        .repository_sources
        .iter()
        .filter(|source| source.builtin || crate::community::is_source(&source.id))
        .cloned()
        .collect::<Vec<_>>();
    for source in sources {
        if crate::community::is_source(&source.id) {
            continue;
        }
        if builtin_ids.contains(&source.id) {
            continue;
        }
        if source.builtin {
            if let Some(existing) = config
                .repository_sources
                .iter()
                .find(|existing| existing.id == source.id && existing.builtin)
                .cloned()
            {
                if !ordered.iter().any(|item| item.id == existing.id) {
                    ordered.push(existing);
                }
                continue;
            }
            continue;
        }
        let id = source.id.trim().to_string();
        if id.is_empty() {
            return Err("config.repositoryIdEmpty".into());
        }
        if id.len() > 128
            || !id.bytes().enumerate().all(|(index, byte)| {
                (byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
                    && (index > 0 || byte.is_ascii_alphanumeric())
            })
        {
            return Err("config.repositoryIdInvalid".into());
        }
        let url = source.url.trim().to_string();
        /*
         * 昵称只认多语言表（zh-CN 兜底）。批量保存来自配置导入/导出，
         * 旧文件可能只有单语字段 —— 同样把它折叠进 zh-CN，再取兜底值。
         */
        let mut nickname_map = source.nickname_i18n.clone();
        nickname_map.retain(|_, value| !value.trim().is_empty());
        if !nickname_map.contains_key("zh-CN") && !source.nickname.trim().is_empty() {
            nickname_map.insert("zh-CN".into(), source.nickname.trim().to_string());
        }
        let nickname = crate::repository_sources::nickname_fallback(&nickname_map)
            .ok_or("source.nicknameEmpty")?;
        if nickname.is_empty() || url.is_empty() {
            return Err("source.nicknameOrAddressEmpty".into());
        }
        let normalized_region = crate::config::normalize_repository_region_for_storage(
            source.region.as_deref(),
            &crate::locale_tag::with_params("source.labeled", &[("label", &id.to_string())]),
        );
        crate::config::validate_repository_source_region(normalized_region.as_deref())
            .map_err(|error| error.to_string())?;
        let location = SourceLocation::parse(&url).map_err(|error| error.to_string())?;
        let existing_bundled_child = config.repository_sources.iter().any(|existing| {
            existing.id == id
                && existing.parent_id.is_some()
                && existing.parent_id == source.parent_id
                && existing.url.eq_ignore_ascii_case(&url)
        });
        if matches!(location, SourceLocation::Bundled(_)) && !existing_bundled_child {
            return Err("source.mustBeHttp".into());
        }
        if !seen_urls.insert(source_address_key(&url, normalized_region.as_deref())) {
            return Err("source.addressExists".into());
        }
        if !seen_ids.insert(id.clone()) {
            return Err("source.idDuplicate".into());
        }
        ordered.push(RepositorySource {
            id,
            nickname,
            nickname_i18n: nickname_map,
            url,
            region: normalized_region,
            builtin: source.builtin,
            parent_id: source.parent_id,
        });
    }
    loop {
        let before = ordered.len();
        let known_ids = ordered
            .iter()
            .map(|source| source.id.clone())
            .collect::<std::collections::HashSet<_>>();
        ordered.retain(|source| {
            source
                .parent_id
                .as_deref()
                .map(|parent_id| known_ids.contains(parent_id))
                .unwrap_or(true)
        });
        if ordered.len() == before {
            break;
        }
    }
    let retained_source_ids = ordered
        .iter()
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for repository in &mut config.repositories {
        if let Some(id) = repository.source_id.as_deref()
            && !retained_source_ids.contains(id)
        {
            repository.source_id = None;
        }
    }
    config.repository_sources = ordered;
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!(
        "批量保存仓库源完成：数量={}",
        config.repository_sources.len()
    );
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn delete_repository_source(
    state: State<'_, AppState>,
    id: String,
) -> Result<PublicConfig, String> {
    if crate::community::is_source(&id) {
        return Err("source.communityManaged".into());
    }
    log::info!("删除仓库源：源={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let index = config
        .repository_sources
        .iter()
        .position(|item| item.id == id)
        .ok_or("source.sourceMissing")?;
    if config.repository_sources[index].builtin {
        return Err("source.builtinCannotDelete".into());
    }
    let mut removed_ids = vec![id.clone()];
    let mut changed = true;
    while changed {
        changed = false;
        for source in &config.repository_sources {
            if let Some(parent_id) = source.parent_id.as_deref()
                && removed_ids.iter().any(|removed| removed == parent_id)
                && !removed_ids.iter().any(|removed| removed == &source.id)
            {
                removed_ids.push(source.id.clone());
                changed = true;
            }
        }
    }
    config
        .repository_sources
        .retain(|source| !removed_ids.iter().any(|removed| removed == &source.id));
    for repository in &mut config.repositories {
        if repository
            .source_id
            .as_ref()
            .is_some_and(|source_id| removed_ids.iter().any(|removed| removed == source_id))
        {
            repository.source_id = None;
        }
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("仓库源删除完成：删除源数量={}", removed_ids.len());
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn reorder_repository_sources(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<PublicConfig, String> {
    log::info!("调整仓库源顺序：指定数量={}", ids.len());
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let builtin_roots = config
        .repository_sources
        .iter()
        .filter(|item| item.builtin && item.parent_id.is_none())
        .cloned()
        .collect::<Vec<_>>();
    if builtin_roots.is_empty() {
        return Err("source.builtinMissing".into());
    }
    let builtin_ids = builtin_roots
        .iter()
        .map(|source| source.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut ordered = config
        .repository_sources
        .iter()
        .filter(|source| source.builtin)
        .cloned()
        .collect::<Vec<_>>();
    for id in ids {
        if let Some(source) = config
            .repository_sources
            .iter()
            .find(|item| {
                !builtin_ids.contains(item.id.as_str())
                    && item.id == id
                    && !ordered.iter().any(|existing| existing.id == item.id)
            })
            .cloned()
        {
            ordered.push(source);
        }
    }
    for source in &config.repository_sources {
        if !ordered.iter().any(|item| item.id == source.id) {
            ordered.push(source.clone());
        }
    }
    config.repository_sources = ordered;
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("仓库源顺序调整完成");
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn save_repository(
    state: State<'_, AppState>,
    patch: RepositoryPatch,
) -> Result<PublicConfig, String> {
    log::info!(
        "保存仓库配置：操作={}，平台={:?}，下载模式={:?}",
        if patch.id.is_some() {
            "编辑"
        } else {
            "新增"
        },
        patch.provider,
        patch.download_mode
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    if patch.id.as_ref().is_some_and(|id| {
        config
            .repositories
            .iter()
            .any(|repo| repo.id == *id && crate::community::is_repository(repo))
    }) {
        return Err("config.communityRepositoryReadOnly".into());
    }
    let (username, repository) =
        releases::repository_coordinates(&patch.username, &patch.repository)
            .map_err(|error| error.to_string())?;
    if patch.download_mode == RepositoryDownloadMode::Release
        && (patch.asset_rules.is_empty()
            || patch
                .asset_rules
                .iter()
                .any(|rule| rule.value.trim().is_empty()))
    {
        return Err("config.assetRuleRequired".into());
    }
    if patch.download_mode == RepositoryDownloadMode::RepositoryFile {
        releases::repository_file_url(
            &patch.provider,
            &username,
            &repository,
            &patch.branch,
            &patch.file_path,
        )
        .map_err(|error| error.to_string())?;
    }
    let normalized_region = crate::config::normalize_repository_region_for_storage(
        patch.region.as_deref(),
        &crate::locale_tag::with_params(
            "config.repositoryLabeled",
            &[("name", &patch.name.to_string())],
        ),
    );
    crate::config::validate_repository_region(normalized_region.as_deref())
        .map_err(|error| error.to_string())?;
    if !patch.version_regex.trim().is_empty()
        && regex::Regex::new(patch.version_regex.trim()).is_err()
    {
        return Err("config.versionRegexInvalid".into());
    }
    let id = patch.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let existing_translations = config
        .repositories
        .iter()
        .find(|repo| repo.id == id)
        .map(|repo| {
            (
                repo.name_i18n.clone(),
                repo.note_i18n.clone(),
                repo.source_id.clone(),
                repo.group_i18n.clone(),
            )
        });
    // 关系表单独取：上面的元组只装字符串映射，而 relations 是结构体列表
    let existing_relations = config
        .repositories
        .iter()
        .find(|repo| repo.id == id)
        .map(|repo| repo.relations.clone())
        .unwrap_or_default();
    let existing_release_relations = config
        .repositories
        .iter()
        .find(|repo| repo.id == id)
        .map(|repo| repo.release_relations.clone())
        .unwrap_or_default();
    let mut name_i18n = patch.name_i18n.unwrap_or_else(|| {
        existing_translations
            .as_ref()
            .map(|(name_i18n, _, _, _)| name_i18n.clone())
            .unwrap_or_default()
    });
    let mut group_i18n = patch.group_i18n.unwrap_or_else(|| {
        existing_translations
            .as_ref()
            .map(|(_, _, _, group_i18n)| group_i18n.clone())
            .unwrap_or_default()
    });
    let mut note_i18n = patch.note_i18n.unwrap_or_else(|| {
        existing_translations
            .as_ref()
            .map(|(_, note_i18n, _, _)| note_i18n.clone())
            .unwrap_or_default()
    });
    if name_i18n
        .get("zh-CN")
        .is_none_or(|name| name.trim().is_empty())
    {
        name_i18n.insert("zh-CN".into(), patch.name.trim().to_string());
    }
    group_i18n
        .entry("zh-CN".into())
        .or_insert_with(|| patch.group.trim().to_string());
    note_i18n
        .entry("zh-CN".into())
        .or_insert_with(|| patch.note.trim().to_string());
    let name = name_i18n
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "config.repositoryNameEmpty".to_string())?;
    let group = group_i18n["zh-CN"].clone();
    let note = note_i18n["zh-CN"].clone();
    let download_threads = match &patch.provider {
        Provider::Github => {
            if patch.download_threads == 0 {
                16
            } else {
                patch.download_threads.clamp(1, 32)
            }
        }
        Provider::Gitee => 1,
    };
    let asset_rules = if patch.download_mode == RepositoryDownloadMode::RepositoryFile {
        Vec::new()
    } else {
        patch.asset_rules
    };
    let mut item = RepositoryConfig {
        id: id.clone(),
        name,
        name_i18n,
        group,
        group_i18n,
        provider: patch.provider.clone(),
        username,
        repository,
        download_mode: patch.download_mode,
        branch: patch.branch.trim().to_string(),
        file_path: patch.file_path.trim().replace('\\', "/"),
        region: normalized_region,
        platform: crate::config::normalize_repository_platform(patch.platform.as_deref()),
        channel: patch.channel,
        asset_rules,
        install_target: patch.install_target,
        note,
        note_i18n,
        source_id: existing_translations
            .as_ref()
            .and_then(|(_, _, source_id, _)| source_id.clone()),
        plugin_guid: patch.plugin_guid,
        local_product_name: patch.local_product_name,
        local_original_filename: patch.local_original_filename,
        // None = 本次编辑没提交关系表，保留原值
        relations: patch.relations.unwrap_or(existing_relations),
        release_relations: patch
            .release_relations
            .unwrap_or(existing_release_relations),
        version_regex: patch.version_regex,
        max_release_count: patch.max_release_count.clamp(1, 100),
        download_threads,
        github_token: String::new(),
        gitee_token: String::new(),
        official_api_only: patch.official_api_only,
        enabled: patch.enabled,
        builtin: false,
        pinned_version: None,
        ignored_version: None,
        etag: None,
        cached_releases: Vec::new(),
        releases_cached_at: None,
        last_error: None,
    };
    if let Some(existing) = config.repositories.iter_mut().find(|repo| repo.id == id) {
        let old_github = existing.github_token.clone();
        let old_gitee = existing.gitee_token.clone();
        item.builtin = existing.builtin;
        *existing = item;
        if patch.github_token.is_empty() {
            existing.github_token = old_github;
        }
        if patch.gitee_token.is_empty() {
            existing.gitee_token = old_gitee;
        }
        state
            .config
            .set_repository_tokens(existing, &patch.github_token, &patch.gitee_token);
    } else {
        let mut item = item;
        state
            .config
            .set_repository_tokens(&mut item, &patch.github_token, &patch.gitee_token);
        config.repositories.push(item);
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("仓库配置保存完成：仓库={}", id);
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn delete_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<OperationResult, String> {
    log::info!("删除仓库：仓库={}", repository_id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let Some(index) = config
        .repositories
        .iter()
        .position(|repo| repo.id == repository_id)
    else {
        return Err("source.repositoryMissing".into());
    };
    if config.repositories[index].builtin {
        return Err("config.builtinRepositoryCannotDelete".into());
    }
    if crate::community::is_repository(&config.repositories[index]) {
        return Err("config.communityRepositoryReadOnly".into());
    }
    config.repositories.remove(index);
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("仓库删除完成：仓库={}", repository_id);
    Ok(OperationResult {
        ok: true,
        message: "config.repositoryDeleted".into(),
    })
}

/// 立刻重算游戏状态并返回最新快照。
///
/// 观察器每秒推进一次、只在变化时推送，正常情况下前端不需要主动拉。但
/// 「进程刚退出」「用户从游戏切回管理器」「某次操作因为 `game.running` 被拒」
/// 这几个时刻，界面上显示的仍是上一轮的结果，用户会看到按钮卡在「停止游戏」。
/// 这条命令让前端当场把状态拉正，而不是等下一轮观察。
#[tauri::command]
pub fn refresh_game_state(app: AppHandle, state: State<'_, AppState>) -> Result<GameInfo, String> {
    // 触发点很少（切回窗口、点完停止、手动重算），记一行便于对账
    log::debug!("收到游戏状态重算请求");
    let info = refresh_game_snapshot(state.inner(), &app)?;
    if info.kitchen_running {
        log::debug!("游戏状态重算：仍在运行");
    }
    Ok(info)
}

/// 重算游戏快照，写回共享状态，变化时推送事件。
///
/// `get_dashboard` 会重新探测并覆盖快照，所以这里只更新运行状态与探测
/// 结果本身，不扫描 MOD —— 前端 `load()` 才做全量扫描。
fn refresh_game_snapshot(state: &AppState, app: &AppHandle) -> Result<GameInfo, String> {
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let changed = {
        let mut snapshot = state.game_state.lock().map_err(|error| error.to_string())?;
        let changed = snapshot.as_ref() != Some(&info);
        *snapshot = Some(info.clone());
        changed
    };
    if changed {
        if let Err(error) = app.emit(crate::game_state::GAME_STATE_EVENT, &info) {
            log::warn!("发送游戏状态变化事件失败：{error}");
        }
    }
    Ok(info)
}

#[tauri::command]
pub fn stop_game(app: AppHandle, state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("收到停止厨房请求");
    // 结束进程不需要操作锁：锁是给配置/清单提交用的，
    // 而游戏退出期间的其他只读操作（例如状态刷新）应继续可用。
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("game.notFound")?);
    /*
     * 停止失败时把「被判定成游戏」的进程名记下来：这类失败几乎都是同名的
     * 宿主进程或残留进程造成的，没有进程名就只剩「停止不了」这一句。
     */
    let result = game::stop_game(&game_root);
    if let Err(error) = &result {
        log::warn!(
            "停止厨房失败：{error}；判定为游戏运行的进程={:?}",
            game::running_processes(&game_root)
        );
    }
    let stopped = result?;
    // 停完立刻重算并推送：用户点完按钮就该看到「启动游戏」，不再等观察器。
    refresh_game_snapshot(state.inner(), &app)?;
    Ok(OperationResult {
        ok: true,
        message: if stopped == 0 {
            "game.notRunning".into()
        } else {
            "game.stopped".into()
        },
    })
}

#[tauri::command]
pub fn launch_normal(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("启动厨房：正常 MOD 模式");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if config.launch.auto_restore_winhttp {
        if matches!(
            crate::bepinex::state(&game_root, &config.game),
            WinhttpState::ModDisabled | WinhttpState::Conflict
        ) {
            crate::bepinex::set_enabled(&game_root, &config.game, true)?;
        }
    }
    let result = open_steam(&config.game, &info);
    if result.is_ok() {
        log::info!("厨房启动请求已发送：正常 MOD 模式");
    } else if let Err(error) = &result {
        log::error!("厨房启动失败：正常 MOD 模式，原因={}", error);
    }
    result
}

#[tauri::command]
pub fn launch_no_mod(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("启动厨房：无 MOD 模式");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if crate::bepinex::state(&game_root, &config.game) != WinhttpState::Missing {
        crate::bepinex::set_enabled(&game_root, &config.game, false)?;
    }
    let result = open_steam(&config.game, &info);
    if result.is_ok() {
        log::info!("厨房启动请求已发送：无 MOD 模式");
    } else if let Err(error) = &result {
        log::error!("厨房启动失败：无 MOD 模式，原因={}", error);
    }
    result
}

#[tauri::command]
pub fn restore_mod(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("恢复框架状态");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    if crate::bepinex::state(&game_root, &config.game) == WinhttpState::ModDisabled {
        crate::bepinex::set_enabled(&game_root, &config.game, true)?;
    }
    log::info!("框架状态恢复完成");
    Ok(OperationResult {
        ok: true,
        message: "framework.stateRestored".into(),
    })
}

/// Switches the runtime's loader state without launching Steam. The overview
/// status card uses this command; actual game launch remains explicit.
#[tauri::command]
pub fn switch_winhttp(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换框架状态：启用={}", enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    /*
     * 未安装时直接拒绝。
     *
     * 没有核心程序集就没有可切换的加载器：禁用方向会去找
     * `winhttp.dll.disabled`（不存在），启用方向会去找 `winhttp.dll`
     * （也没有）。原生平台会去找 `run_bepinex.sh`。三种结果都是
     * 「未找到框架加载器」这种无从处理的报错。界面已经按状态隐藏开关，
     * 这里是防止 IPC 被直接调用的兜底。
     */
    if !crate::bepinex::installed(&game_root) {
        return Err("framework.notInstalledCannotToggle".into());
    }
    crate::bepinex::set_enabled(&game_root, &config.game, enabled)?;
    log::info!("框架状态切换完成：启用={}", enabled);
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "framework.enabledGameNotRunning".into()
        } else {
            "framework.disabledGameNotRunning".into()
        },
    })
}

#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) -> Result<OperationResult, String> {
    log::info!("打开开发者工具");
    window.open_devtools();
    Ok(OperationResult {
        ok: true,
        message: "tools.devConsoleOpened".into(),
    })
}

#[tauri::command]
pub fn reset_ui(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("重置界面缓存和 Release 缓存");
    let cache = state.config.cache_dir();
    if cache.exists() {
        fs::remove_dir_all(&cache).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    config.ui_zoom = 1.0;
    for repo in &mut config.repositories {
        repo.cached_releases.clear();
        // 时间戳与列表同生共死：只清列表会留下「空列表 + 新鲜时间」，
        // 下次刷新命中缓存却拿到 0 条，等于永久看不到 Release。
        repo.releases_cached_at = None;
        repo.etag = None;
        repo.last_error = None;
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("界面缓存和 Release 缓存重置完成");
    Ok(OperationResult {
        ok: true,
        message: "tools.cacheCleared".into(),
    })
}

#[tauri::command]
pub fn open_game_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开游戏目录");
    let config = state.config.load_or_default();
    let path = game::detect_game(&config).path.ok_or("game.notFound")?;
    external_open_command()
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "tools.gameDirOpened".into(),
    })
}

#[tauri::command]
pub fn open_mod_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开 MOD 目录");
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let plugins = PathBuf::from(game_root).join("BepInEx/plugins");
    fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
    external_open_command()
        .arg(&plugins)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "tools.modsDirOpened".into(),
    })
}

/// 打开游戏的 `BepInEx` 目录（框架自身文件所在处，不是 plugins）。
///
/// 与 `open_game_directory`（游戏根目录）和 `open_mod_directory`
/// （`BepInEx/plugins`）区分开：框架的 `core`、`config`、`doorstop` 相关
/// 文件都在这里，排查框架问题时要看的是这一层。
///
/// 不自动创建目录：调用方是「框架已安装」状态下的按钮，目录本该存在。
/// 不存在说明状态与磁盘不一致，报错让用户重新探测，而不是凭空建一个空目录
/// 让他以为框架装好了。
/// 读取一批仓库 owner 的头像（供卡片与详情页显示）。
///
/// **批量而不是逐个**：一次刷新会同时刷出几十个仓库，逐个 IPC 往返既慢
/// 又会让前端持有几十个独立请求。这里一次收下全部 owner，去重后并发读取，
/// 返回 `owner -> data URL` 的映射。
///
/// 去重发生在两处：
///   · 本函数内按 `(provider, owner)` 去重 —— 同一个人只读一次；
///   · `avatars::load` 内部的 in-flight 表 —— 即便有其他调用方并发请求
///     同一 owner，也只有一次真实下载。
///
/// 失败的 owner **不出现在结果里**（不返回空串或占位图），前端据此回落到
/// 平台图标。头像不是关键数据，失败不该弹错误。
#[tauri::command]
pub async fn get_repository_avatars(
    state: State<'_, AppState>,
    owners: Vec<AvatarRequest>,
) -> Result<HashMap<String, String>, String> {
    if owners.is_empty() {
        return Ok(HashMap::new());
    }
    let (config, cache_dir) = {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        (state.config.load_or_default(), state.config.cache_dir())
    };
    log::debug!("开始读取头像：请求数={}", owners.len());

    let mut result: HashMap<String, String> = HashMap::new();
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut pending = Vec::new();
    for request in owners {
        let owner = request.owner.trim().to_string();
        if owner.is_empty() {
            continue;
        }
        /*
         * 键必须与前端 `avatarKey()` **完全一致**：`{provider}:{owner_lowercase}`。
         * 统一由 `avatars::response_key` 生成，避免两边各写一份再漂移 ——
         * 之前这里用原样大小写的 owner，`CH3NGY2`、`QingyuQingxue` 这类
         * 含大写的 owner 键就对不上，后端返回了头像前端却查不到。
         */
        let key = crate::avatars::response_key(&request.provider, &owner);
        let dedupe = key.clone();
        if !seen.insert((dedupe, owner.to_lowercase())) {
            continue;
        }
        pending.push((key, owner, request.provider));
    }

    // 并发读取，但每个 owner 内部已有单飞保护；并发上限由请求数自然限制
    let mut tasks = Vec::with_capacity(pending.len());
    for (key, owner, provider) in pending {
        let config = config.clone();
        let cache_dir = cache_dir.clone();
        tasks.push(async move {
            let loaded = crate::avatars::load(&config, &cache_dir, &provider, &owner).await;
            (key, loaded)
        });
    }
    for (key, loaded) in futures_util::future::join_all(tasks).await {
        match loaded {
            Ok(bytes) => {
                let mime = crate::avatars::sniff_mime(&bytes);
                use base64::{Engine, engine::general_purpose::STANDARD as B64};
                result.insert(key, format!("data:{mime};base64,{}", B64.encode(&bytes)));
            }
            Err(error) => {
                // 只记 debug：头像取不到属于正常回落，不是错误
                log::debug!("头像不可用，前端将回落到平台图标：key={key} 原因={error}");
            }
        }
    }
    log::debug!("头像读取完成：可用={}/{}", result.len(), seen.len());
    Ok(result)
}

/// 清空头像缓存。
#[tauri::command]
pub fn clear_repository_avatars(state: State<'_, AppState>) -> Result<usize, String> {
    crate::avatars::clear_cache(&state.config.cache_dir())
}

/// 清空地图（关卡集）`info` 的解析缓存，返回删除的磁盘缓存文件数。
///
/// 同步命令：只删文件与清一个内存映射，没有解析也没有 IO 等待，
/// 放进 `spawn_blocking` 反而多一次线程切换。
#[tauri::command]
pub fn clear_level_set_cache() -> Result<usize, String> {
    crate::level_sets::clear_info_cache()
}

/// DIYLevel 自定义关卡框架的稳定 GUID。
///
/// 与仓库源里的 `plugin_guid`、安装清单、MOD 扫描用的是同一个标识 ——
/// 作者会改文件名和产品名，只有 GUID 稳定，所以状态判定只认它。
const DIYLEVEL_GUID: &str = "dev.gua.overcooked.diylevel";

/// 读取 DIYLevel 框架状态。
///
/// 复用 MOD 扫描结果，不单独扫盘：扫描已经处理了 `.dll.disabled` 命名、
/// 元数据解析和清单绑定，再写一套判定只会和 MOD 列表给出的结论不一致。
#[tauri::command]
pub fn get_diy_level_info(state: State<'_, AppState>) -> Result<DiyLevelInfo, String> {
    let config = state.config.load_or_default();
    let backups = list_diy_level_backups(&state.config.backups_dir());
    let Some(game_root) = game::detect_game(&config).path else {
        return Ok(DiyLevelInfo {
            installed: false,
            enabled: false,
            version: None,
            dll_file: None,
            has_levels_dir: false,
            level_packs: 0,
            backups,
        });
    };
    let root = PathBuf::from(&game_root);
    // 与 MOD 列表同源：同一份扫描、同一份清单，结论不会互相矛盾
    let installed =
        mods::scan_mods_with_manifests(&root, &config, Some(&state.config.manifests_dir()));
    let target = installed.iter().find(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
    });
    /*
     * 地图目录：`BepInEx/plugins/OC2DIYLevel/levels`。
     *
     * 目录名来自包里固定的相对结构（`OC2DIYLevel/levels/...`），不是用户
     * 可配的；这里只做「存在与否」的只读判断，不创建 —— 没装框架时凭空
     * 建一个空目录，会让人以为已经准备好放地图了。
     */
    let levels_dir = root.join("BepInEx/plugins/OC2DIYLevel/levels");
    Ok(DiyLevelInfo {
        installed: target.is_some(),
        enabled: target.is_some_and(|item| item.enabled),
        version: target.and_then(|item| item.version.clone()),
        dll_file: target.map(|item| item.dll_file.clone()),
        has_levels_dir: levels_dir.is_dir(),
        level_packs: count_level_packs(&levels_dir),
        backups,
    })
}

/// DIYLevel 框架本体所在目录（游戏相对路径）。
///
/// 备份、恢复、卸载都以它为单位：`OC2DIYLevel` 下既有框架 DLL，也有
/// `levels/` 自制地图。`BepInEx/core` 属于加载器，归 BepInEx 的备份管。
const DIYLEVEL_DIR: &str = "BepInEx/plugins/OC2DIYLevel";

/// 列出已有的 DIYLevel 备份（按名称升序，与 BepInEx 备份同样的排列方式）。
fn list_diy_level_backups(backup_root: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(backup_root) else {
        return Vec::new();
    };
    let mut backups = entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(DIYLEVEL_BACKUP_PREFIX)
                && entry.path().is_dir()
        })
        .map(|entry| display_path(entry.path()))
        .collect::<Vec<_>>();
    backups.sort();
    backups
}

/// 备份目录名前缀：`DIYLevel.backup-<时间戳>-<uuid>`。
const DIYLEVEL_BACKUP_PREFIX: &str = "DIYLevel.backup-";

/// 地图集数量 = `levels/` 下的文件夹个数。
///
/// 只数目录：`levels/` 里直接放的散文件（说明文档、缩略图、配置）不属于
/// 任何地图集。目录不存在或不可读都返回 0 —— 这是只读的展示信息，
/// 不该因为权限问题让整个面板报错。
fn count_level_packs(levels_dir: &Path) -> u32 {
    let Ok(entries) = std::fs::read_dir(levels_dir) else {
        return 0;
    };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .count() as u32
}

/// 切换 DIYLevel 框架的启用状态。
///
/// 走与 MOD 列表相同的 `.dll` / `.dll.disabled` 改名逻辑（`set_local_mod_file_state`），
/// 不新增一套开关实现 —— 否则两处对「禁用」的定义迟早分叉。
#[tauri::command]
pub fn set_diy_level_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换 DIYLevel 框架状态：启用={enabled}");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    let installed =
        mods::scan_mods_with_manifests(&game_root, &config, Some(&state.config.manifests_dir()));
    let Some(target) = installed.iter().find(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
    }) else {
        return Err("framework.diyLevelMissing".into());
    };
    if target.enabled == enabled {
        return Ok(OperationResult {
            ok: true,
            message: if enabled {
                "mods.stateEnabled"
            } else {
                "mods.stateDisabled"
            }
            .into(),
        });
    }
    set_local_mod_file_state(&game_root, &target.dll_file, enabled)?;
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "message.framework.diyLevelEnabled".into()
        } else {
            "message.framework.diyLevelDisabled".into()
        },
    })
}

// ===== 地图（DIYLevel 关卡集）管理 =====
//
// 与 MOD 管理是两套数据：MOD 靠 BepInPlugin 的 GUID 识别，关卡集靠 info 里的
// `levelSetName + levelSetNameZH + author`。界面复用同一套卡片样式，但扫描、开关、安装各自
// 独立 —— 硬把两者塞进一个模型会让每处调用都要判「这是 MOD 还是地图」。

/// 把内部扫描结果转成 IPC 模型。
fn level_set_to_info(set: crate::level_sets::LevelSetOnDisk) -> LevelSetInfo {
    let metadata = set.metadata;
    /*
     * `requires.txt` 是本地安装包自带的外部 runtime 要求（纯版本号）。
     * 先在这里填成兜底依赖：`bind_map_relation_snapshot` 拿到线上声明时会覆盖它
     * —— 线上优先，只有线上一条都没有时才用这份。没有 `requires.txt` 就保持
     * 为空，界面也就不会显示关系卡片。
     */
    let fallback_relations: Vec<_> = set
        .required_runtime
        .as_deref()
        .and_then(crate::level_sets::runtime_relation)
        .into_iter()
        .collect();
    LevelSetInfo {
        dir_name: set.dir_name,
        enabled: set.enabled,
        path: display_path(&set.path),
        name: metadata
            .as_ref()
            .map(|meta| meta.name.clone())
            .unwrap_or_default(),
        name_zh: metadata.as_ref().and_then(|meta| meta.name_zh.clone()),
        author: metadata
            .as_ref()
            .map(|meta| meta.author.clone())
            .unwrap_or_default(),
        version: metadata
            .as_ref()
            .map(|meta| meta.version.clone())
            .unwrap_or_default(),
        uid: metadata
            .as_ref()
            .map(|meta| meta.uid.clone())
            .unwrap_or_default(),
        base_uid: metadata.as_ref().and_then(|meta| meta.base_uid.clone()),
        level_count: metadata.as_ref().map(|meta| meta.levels.len()).unwrap_or(0),
        error: set.error,
        // 绑定与依赖快照由 `bind_map_relation_snapshot` 补：它需要配置与快照，
        // 这里只负责把磁盘扫描结果转成模型
        map_id: None,
        relations: fallback_relations,
        // 随包插件就是集目录里的 DLL（扫描时读回），与绑定/依赖无关
        plugins: set.plugins,
    }
}

/// 列出全部关卡集（启用与停用）。
///
/// 只读：不创建 `levels/`，不要求游戏已停止 —— 打开页面就能看到现状。
/// 未找到游戏目录时返回空列表而不是报错：地图页在没有厨房的情况下也该能打开。
#[tauri::command]
pub async fn get_level_sets(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<LevelSetInfo>, String> {
    /*
     * 必须离开 IPC 主线程再扫描。
     *
     * 同步命令由 Tauri 直接在调用线程上内联执行（`tauri-macros` 的同步包装
     * 就是 `let result = handler(..)`），而扫描要逐个解压 `info` 容器：debug
     * 构建下冷扫描 7 个关卡集实测 13.6 s，期间整个窗口不响应。改成 async +
     * `spawn_blocking` 后耗时不变，但界面可以继续转圈、可以取消。
     *
     * `State` 不能跨 await，所以在同步段把需要的东西都取出来再 move 进闭包。
     */
    let config = state.config.load_or_default();
    let Some(game_root) = game::detect_game(&config).path else {
        return Ok(Vec::new());
    };
    let manifests_dir = state.config.manifests_dir();
    // 并行度在同步段算好再 move 进闭包（`State` 不能跨 await）
    let info_workers = crate::metadata_read::workers_from(config.map_info_concurrency);
    tauri::async_runtime::spawn_blocking(move || {
        /*
         * 进度事件逐份发：解析发生在多个线程里，`emit` 本身线程安全。
         * 发送失败（窗口已关、前端还没挂监听）不影响扫描结果，忽略即可。
         */
        let progress_app = app.clone();
        let sets = crate::level_sets::scan_level_sets_with_progress(
            &PathBuf::from(game_root),
            info_workers,
            &move |done, total, current| {
                let _ = progress_app.emit(
                    "level-set-scan-progress",
                    LevelSetScanProgress {
                        done,
                        total,
                        current: current.to_string(),
                    },
                );
            },
        );
        // 快照只读一次：逐条关卡集都去读文件会变成几十次 IO
        let snapshots = archive::read_map_relation_snapshots(&manifests_dir);
        log::debug!("关卡集列表读取完成：数量={}", sets.len());
        sets.into_iter()
            .map(|set| {
                let mut info = level_set_to_info(set);
                bind_map_relation_snapshot(&mut info, &config, &snapshots);
                info
            })
            .collect()
    })
    .await
    .map_err(|error| error.to_string())
}

/// 给关卡集补上「绑定的地图仓库 id」与依赖快照。
///
/// 身份匹配走 `level_sets::level_set_identity_matches`（`levelSetName +
/// levelSetNameZH + author`，忽略大小写与首尾空白，空名不匹配；地图源没写
/// 中文名时退回前两段）；关系优先用地图依赖快照（安装时/上次
/// 同步固化的），没有快照才退回仓库当前声明 —— 地图源被删之后依赖表仍在，
/// 源还在时以线上声明为准。
fn bind_map_relation_snapshot(
    info: &mut LevelSetInfo,
    config: &AppConfig,
    snapshots: &MapRelationSnapshots,
) {
    let Some(map) = config.map_repositories.iter().find(|map| {
        crate::level_sets::level_set_identity_matches(
            &info.name,
            info.name_zh.as_deref(),
            &info.author,
            &map.level_set_name,
            Some(map.level_set_name_zh.as_str()).filter(|value| !value.is_empty()),
            &map.author,
        )
    }) else {
        return;
    };
    info.map_id = Some(map.id.clone());
    let relations = snapshots
        .entries
        .get(&map.id)
        .map(|entry| entry.relations.clone())
        .unwrap_or_else(|| map.relations.clone());
    /*
     * 线上一条声明都没有时，保留 `level_set_to_info` 用 `requires.txt` 填的兜底：
     * 本地包自带的外部 runtime 要求，不该因为「绑定了地图源、但那个源没写
     * relations」而消失。线上写了就一律以线上为准。
     */
    if !relations.is_empty() {
        info.relations = relations;
    }
}

/// 启用或停用一个关卡集（在 `levels/` 与 `disabled/` 之间移动目录）。
///
/// 会改磁盘，所以要求游戏已停止 —— 与 MOD 开关同一条约束：游戏运行时改
/// 插件目录里的内容，轻则本次不生效，重则读到半个文件。
#[tauri::command]
pub fn set_level_set_enabled(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("切换关卡集状态：目录={}，启用={}", dir_name, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    // 移动目录在游戏运行时不安全（DLL 正在枚举 levels/），按策略拦下
    crate::level_sets::ensure_operation_allowed(
        &game_root,
        crate::level_sets::LevelSetOperation::Toggle,
    )?;
    crate::level_sets::set_level_set_enabled(&game_root, &dir_name, enabled)?;
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "message.levelSet.enabled".into()
        } else {
            "message.levelSet.disabled".into()
        },
    })
}

/// 批量切换全部关卡集（地图管理页的「全部启用 / 全部禁用」）。
///
/// 逐目录切换本身只是改名，但前端逐条发 IPC 时每条都要重新读配置、探测
/// 游戏目录并抢一次操作锁 —— 地图一多就是一串重复开销。核心扫描与回滚在
/// `level_sets::set_all_level_sets_enabled`（有测试），命令层只负责
/// 加锁、定位游戏目录和把结果翻译成提示。
#[tauri::command]
pub fn set_all_level_sets_enabled(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("批量切换全部关卡集：启用={}", enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    // 批量切换同样是移动目录，游戏运行时按同一策略拦下
    crate::level_sets::ensure_operation_allowed(
        &game_root,
        crate::level_sets::LevelSetOperation::Toggle,
    )?;
    let changed = crate::level_sets::set_all_level_sets_enabled(&game_root, enabled)?;
    if changed.is_empty() {
        return Ok(OperationResult {
            ok: true,
            message: "message.levelSet.nothingToToggle".into(),
        });
    }
    log::info!(
        "批量切换关卡集完成：启用={}，变更数量={}",
        enabled,
        changed.len()
    );
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            if enabled {
                "message.levelSet.bulkEnabled"
            } else {
                "message.levelSet.bulkDisabled"
            },
            &[("count", &changed.len().to_string())],
        ),
    })
}

/// 把 ZIP 里识别出的关卡集转成 IPC 模型（冲突列表依赖当前游戏目录）。
///
/// `stage_map_source`（本地 / 直链地图来源）与 `stage_map_release`（在线地图
/// Release）必须给出完全相同的行，否则同一个 ZIP 在不同入口上的「会卸载本地
/// 旧副本」提示会不一致。
///
/// 识别期间逐份发 `level-set-zip-progress`：一个上百 MB 的合集在 debug 构建
/// 里要几十秒，界面只有转圈的话用户判断不了是在跑还是卡死。
///
/// 冲突是**批次级**的：它回答「这次导入会与本地哪些副本撞身份」，与批次里
/// 具体是哪一张无关，所以同一份列表挂到每个 `ZipLevelSet` 上，界面按
/// `handle` 去重展示（单集 ZIP 与逐集拆分完全等价）。
fn zip_level_sets(
    app: &AppHandle,
    config: &AppConfig,
    path: &Path,
) -> Result<Vec<ZipLevelSet>, String> {
    // `info` 是地图线、随包插件是 DLL（MOD 线），两条线各有自己的并行度设置。
    let progress_app = app.clone();
    let sets = crate::level_sets::inspect_zip_with_progress(
        path,
        crate::metadata_read::workers_from(config.map_info_concurrency),
        crate::metadata_read::workers_from(config.mod_metadata_concurrency),
        &move |done, total, current| {
            let _ = progress_app.emit(
                "level-set-zip-progress",
                LevelSetZipProgress {
                    phase: "info".into(),
                    done,
                    total,
                    current: current.to_string(),
                },
            );
        },
    )?;
    let game_root = game::detect_game(config).path;
    let conflicts = game_root
        .as_deref()
        .map(|root| {
            crate::level_sets::find_install_conflicts_with_workers(
                Path::new(root),
                &sets,
                crate::metadata_read::workers_from(config.map_info_concurrency),
            )
        })
        .unwrap_or_default();
    Ok(sets
        .into_iter()
        .map(|set| ZipLevelSet {
            conflicts: conflicts.clone(),
            dir_name: set.dir_name,
            name: set.metadata.name,
            name_zh: set.metadata.name_zh,
            author: set.metadata.author,
            version: set.metadata.version,
            level_count: set.metadata.levels.len(),
            file_count: set.files.len(),
            plugins: set.plugins,
        })
        .collect())
}

/// 拖放分类结果：一个本地文件到底该走地图链路还是 MOD 链路。
#[derive(Debug, Clone, serde::Serialize)]
pub struct DropSourceKind {
    /// `map`（含 `info`）/ `mod`（纯 DLL，或没有 `info` 但有 DLL）/ `unknown`。
    pub kind: &'static str,
    /// ZIP 里含 `info` 的目录数。
    pub map_sets: usize,
    /// ZIP 里的 DLL 条目数（`.dll` 输入按 1 计）。
    pub plugins: usize,
}

/// 判断拖进来的文件是地图包、MOD 包还是都不像。
///
/// **判据只有一条：含 `info` 就是地图包**。只按「有没有 DLL」判断会把带修复
/// 插件的地图包误判成 MOD（实测三个真实包都是「levels + plugins」结构），
/// 装进去就是「一个空的 MOD 预览 + 一张没装的地图」。前端两个域都先走这里，
/// 判据因此不会在两个入口各写一套。
#[tauri::command]
pub async fn classify_drop_source(path: String) -> Result<DropSourceKind, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let target = PathBuf::from(path.trim());
        if !target.is_file() {
            return Err("mods.fileMissing".to_string());
        }
        let extension = target
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        if extension == "dll" {
            return Ok(DropSourceKind {
                kind: "mod",
                map_sets: 0,
                plugins: 1,
            });
        }
        if extension != "zip" {
            return Ok(DropSourceKind {
                kind: "unknown",
                map_sets: 0,
                plugins: 0,
            });
        }
        let contents = crate::level_sets::classify_zip(&target)?;
        let kind = if contents.map_sets > 0 {
            "map"
        } else if contents.plugins > 0 {
            "mod"
        } else {
            "unknown"
        };
        Ok(DropSourceKind {
            kind,
            map_sets: contents.map_sets,
            plugins: contents.plugins,
        })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 准备一个地图来源：本地 ZIP 只解析，直链先下载到缓存目录。
///
/// 返回的 `path` 就是后续 `import_level_sets` 的输入；`staged=true` 时调用方
/// 结束（导入完成或用户取消）后必须用 `discard_map_source` 清理，否则缓存
/// 目录里会留下下载下来的 ZIP。
#[tauri::command]
pub async fn stage_map_source(
    app: AppHandle,
    state: State<'_, AppState>,
    source: crate::mod_preview::ModSource,
) -> Result<StagedMapSource, String> {
    let config = state.config.load_or_default();
    let (path, staged) = match &source.kind {
        crate::mod_preview::ModSourceKind::Local => {
            let path = PathBuf::from(source.value.trim());
            if !path.is_file() {
                return Err("mods.fileMissing".into());
            }
            let is_zip = path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("zip"));
            if !is_zip {
                return Err("modPreview.unsupportedType".into());
            }
            (path, false)
        }
        crate::mod_preview::ModSourceKind::Url => {
            let (temp, _file_name) = crate::direct_mod::download_remote_to_temp(
                &app,
                &state,
                &source.value,
                "levelSet.installMap",
                crate::direct_mod::DIRECT_DOWNLOAD_THREADS,
                crate::direct_mod::RemoteValidation::Zip,
            )
            .await?;
            (temp, true)
        }
    };
    // 解析失败时下载来的暂存文件没有后续用途，当场清掉，不留垃圾
    let sets = match zip_level_sets(&app, &config, &path) {
        Ok(sets) => sets,
        Err(error) => {
            if staged {
                let _ = fs::remove_file(&path);
            }
            return Err(error);
        }
    };
    log::info!(
        "地图来源已就绪：类型={:?}，暂存={}，关卡集={}",
        source.kind,
        staged,
        sets.len()
    );
    Ok(StagedMapSource {
        path: display_path(&path),
        staged,
        sets,
    })
}

/// 校验「这个路径可以当作暂存文件丢弃」，返回解析后的真实路径。
///
/// 只接受 `cache_dir` 之内的普通文件；`None` 表示文件已经不在（重复清理、
/// 或安装流程自己搬走了），调用方按成功处理。路径来自前端，必须先
/// canonicalize 再比较 —— 否则 `..`、符号链接、目录联接都能绕过前缀判断。
fn cache_file_to_discard(cache_dir: &Path, path: &str) -> Result<Option<PathBuf>, String> {
    let Ok(target) = fs::canonicalize(path.trim()) else {
        return Ok(None);
    };
    let cache_dir = fs::canonicalize(cache_dir).map_err(|_| "levelSet.outsideCache".to_string())?;
    if !target.is_file() || !target.starts_with(&cache_dir) {
        return Err("levelSet.outsideCache".into());
    }
    Ok(Some(target))
}

/// 丢弃一个已暂存的地图来源（只允许缓存目录内的文件）。
///
/// 暂存文件由 `stage_map_source` 落在缓存目录里；路径来自前端，不能当成
/// 可信路径 —— 判定规则见 [`cache_file_to_discard`]。
#[tauri::command]
pub fn discard_map_source(state: State<'_, AppState>, path: String) -> Result<(), String> {
    let Some(target) = cache_file_to_discard(&state.config.cache_dir(), &path)? else {
        return Ok(());
    };
    fs::remove_file(&target).map_err(|error| {
        crate::locale_tag::with_params("levelSet.deleteFailed", &[("reason", &error.to_string())])
    })?;
    log::info!("已清理暂存的地图来源：路径={}", display_path(&target));
    Ok(())
}

/// 从本地 ZIP 导入选定的关卡集。
///
/// 已存在的同名关卡集会被**回收**（进回收站），而不是直接删除。
///
/// 两段都在阻塞线程池里跑：识别一个几十集的合集要几十秒，解压落盘同样按秒
/// 计，同步命令会把整个窗口冻住（见 `get_level_sets` 的注释）。识别只做一次
/// —— 以前每装一个集就整包重新 inspect 一遍，N 张合集就是 N 倍识别时间。
#[tauri::command]
pub async fn import_level_sets(
    app: AppHandle,
    state: State<'_, AppState>,
    zip_path: String,
    selected: Vec<String>,
) -> Result<LevelSetImportResult, String> {
    // `State` 不能跨 await：需要的值（配置快照、游戏根、操作锁）在同步段取好
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let operation = state.operation.clone();
    tauri::async_runtime::spawn_blocking(move || {
        log::info!("开始导入关卡集：文件={}，选择={}", zip_path, selected.len());
        let _guard = operation.lock().map_err(|_| "lock.poisoned".to_string())?;
        /*
         * 安装是新增内容，游戏运行时也允许 —— 写的是本次启动还没扫描过的新目录。
         * 用策略而不是直接 `ensure_game_stopped`：开关仍然需要停止游戏，
         * 两者的差异集中在一处声明。
         */
        crate::level_sets::ensure_operation_allowed(
            &game_root,
            crate::level_sets::LevelSetOperation::Install,
        )?;
        let zip = PathBuf::from(&zip_path);
        let info_app = app.clone();
        let sets = crate::level_sets::inspect_zip_with_progress(
            &zip,
            crate::metadata_read::workers_from(config.map_info_concurrency),
            crate::metadata_read::workers_from(config.mod_metadata_concurrency),
            &move |done, total, current| {
                let _ = info_app.emit(
                    "level-set-zip-progress",
                    LevelSetZipProgress {
                        phase: "info".into(),
                        done,
                        total,
                        current: current.to_string(),
                    },
                );
            },
        )?;
        /*
         * 逐个集落盘，而不是一次性全装：任一失败就停下并保留已成功的（它们
         * 已经在磁盘上），错误如实上报 —— 悄悄跳过会让用户以为全装上了。
         * 每张开始前发一条 `install` 阶段进度，用户能看到卡在哪张图上。
         */
        let mut installed = Vec::new();
        let total = selected.len();
        for (index, name) in selected.iter().enumerate() {
            let _ = app.emit(
                "level-set-zip-progress",
                LevelSetZipProgress {
                    phase: "install".into(),
                    done: index,
                    total,
                    current: name.clone(),
                },
            );
            let single = crate::level_sets::install_parsed_sets(
                &sets,
                &zip,
                &game_root,
                std::slice::from_ref(name),
            )?;
            installed.extend(single);
        }
        log::info!("关卡集导入完成：数量={}", installed.len());
        Ok(LevelSetImportResult { installed })
    })
    .await
    .map_err(|error| error.to_string())?
}

/// 在系统文件管理器中打开一个关卡集目录。
///
/// 只读：不创建目录、不要求游戏已停止。前端只传目录名与「启用与否」，实际
/// 路径由后端按 `OC2DIYLevel/{levels,disabled}` 拼出来 —— 参数不能指定任意
/// 路径，`safe_game_path` 与 DIYLevel 归属检查都在 `level_sets` 里做。
#[tauri::command]
pub fn open_level_set_directory(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::debug!("打开关卡集目录：目录={}，启用={}", dir_name, enabled);
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    let path = crate::level_sets::locate_level_set_directory(&game_root, &dir_name, enabled)?;
    directory_open_command(&path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "message.levelSet.dirOpened".into(),
    })
}

/// 删除一个关卡集（默认移入回收站，跟随 `recycle_deleted_mods`）。
///
/// 删除是**移动/移除目录**：DIYLevel 正在 `levels/` 下枚举并加载 `info` 时
/// 把目录搬走，会让它读到半个文件。所以这里按 `Toggle` 的策略拦下运行中的
/// 游戏，而不是按「安装」放行。
#[tauri::command]
pub async fn delete_level_set(
    state: State<'_, AppState>,
    dir_name: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    // 回收/删除要移动整个集目录（可能有上千个文件），不能占着 IPC 线程
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        delete_level_set_blocking(&state, &dir_name, enabled)
    })
    .await
    .map_err(|error| error.to_string())?
}

/// [`delete_level_set`] 的同步实现（在阻塞线程池里跑）。
fn delete_level_set_blocking(
    state: &AppState,
    dir_name: &str,
    enabled: bool,
) -> Result<OperationResult, String> {
    log::info!("删除关卡集：目录={}，启用={}", dir_name, enabled);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    crate::level_sets::ensure_operation_allowed(
        &game_root,
        crate::level_sets::LevelSetOperation::Toggle,
    )?;
    crate::level_sets::remove_level_set(
        &state.config.mod_recycle_bin_dir(),
        &state.config.manifests_dir(),
        &game_root,
        &dir_name,
        enabled,
        config.recycle_deleted_mods,
    )?;
    Ok(OperationResult {
        ok: true,
        message: if config.recycle_deleted_mods {
            "message.levelSet.movedToRecycleBin".into()
        } else {
            "message.levelSet.deleted".into()
        },
    })
}

/// 列出当前地区可用的地图（含 Release 缓存）。
///
/// 只读：不发起网络请求。缓存为空时界面显示「待检查」，用户点刷新才抓。
///
/// 这里**不再扫描 BepInEx/plugins**：以前为了算 `unmet_frameworks` 会调用
/// `installed_plugin_versions`，而它内部是全量 MOD 扫描（读所有 DLL 的
/// BepInEx 元数据 + 安装清单）。地图页因此要等一次与地图无关的扫描。
/// 依赖判定改由前端用 Dashboard 已扫好的 MOD 列表计算，两边共用同一份
/// 关系判定实现，结论不会分叉。
#[tauri::command]
pub fn get_maps(state: State<'_, AppState>) -> Result<Vec<MapInfo>, String> {
    let config = state.config.load_or_default();
    let maps = crate::map_sources::available_maps(&config, crate::config::current_user_region());
    log::debug!("地图列表读取完成：数量={}", maps.len());
    Ok(maps.iter().map(map_to_info).collect())
}

/// 刷新地图源树（索引 → 子源 → 地图），并保留同名条目的缓存与开关。
///
/// ## 回退条件与仓库源一致
///
/// 只有「官方入口 HTTP/网络失败」才试探内置快照；远程文档能读到但内容有问题
/// （TOML 无效、缺必填字段）时直接报错，不能悄悄换成内置条目 —— 否则作者
/// 刚发布的地图源写错了，用户看到的是旧的、看起来一切正常的内置列表。
#[tauri::command]
pub async fn refresh_map_sources(state: State<'_, AppState>) -> Result<Vec<MapInfo>, String> {
    let builtin_only = state.config.load_or_default().builtin_map_sources_only;
    let region = crate::config::current_user_region().to_string();
    log::info!("开始刷新地图源：内置={}，地区={}", builtin_only, region);

    let loaded = crate::map_sources::load_map_tree(&region, builtin_only).await;
    // 内置开关开着时成功路径读的就是快照，标签要跟实际来源一致 —— 这行是
    // 「地图列表到底来自哪里」的唯一日志证据。
    let (maps, source) = match loaded {
        Ok(maps) => (maps, if builtin_only { "bundled" } else { "remote" }),
        Err(error) => {
            /*
             * 官方入口连不上：退回内置快照，但**保留远程入口** —— 下次刷新
             * 仍然先请求远程。这与仓库源「回退只进行一次」的规则同源。
             */
            if builtin_only {
                return Err(error.to_string());
            }
            log::warn!(
                "地图源刷新失败，回退内置快照：原因={}",
                crate::repository_sources::error_category(&error)
            );
            let bundled = crate::map_sources::bundled_map_tree();
            if bundled.is_empty() {
                return Err(error.to_string());
            }
            (bundled, "bundled-fallback")
        }
    };

    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut config = state.config.load_or_default();
    let changed = crate::map_sources::replace_maps_keeping_runtime_state(&mut config, maps);
    if changed {
        state
            .config
            .save(&config)
            .map_err(|error| error.to_string())?;
    }
    log::info!(
        "地图源刷新完成：来源={}，地图={}，配置已更新={}",
        source,
        config.map_repositories.len(),
        changed
    );
    Ok(crate::map_sources::available_maps(&config, &region)
        .iter()
        .map(map_to_info)
        .collect())
}

/// 把内部地图配置转成 IPC 模型。
///
/// ## 为什么这里不做依赖判定
///
/// 判定「哪个框架版本没满足」需要本地已装插件的 GUID → 版本，而拿到它必须
/// 全量扫描 `BepInEx/plugins`（每个 DLL 读 BepInEx 元数据 + 安装清单）。地图页
/// 打开时再扫一遍纯属重复劳动：Dashboard 早就扫好了同一份数据，前端拿它就能
/// 用同一份关系判定实现算出结论。
///
/// 因此 `required_frameworks` 只给出**声明**（`installed_version` 恒为 `None`），
/// `unmet_frameworks` 恒为空。字段保留是为了兼容尚未迁移的旧前端代码 ——
/// 删字段会让它在 `map.unmet_frameworks ?? []` 之外的访问路径上崩掉。
fn map_to_info(map: &crate::models::MapRepositoryConfig) -> MapInfo {
    let declared: Vec<RequiredFramework> = map
        .relations
        .iter()
        .map(|relation| RequiredFramework {
            guid: relation.guid.clone(),
            min_version: relation.min_version.clone(),
            installed_version: None,
        })
        .collect();
    MapInfo {
        id: map.id.clone(),
        name: map.name.clone(),
        name_i18n: map.name_i18n.clone(),
        group: map.group.clone(),
        group_i18n: map.group_i18n.clone(),
        provider: map.provider.clone(),
        username: map.username.clone(),
        repository: map.repository.clone(),
        level_set_name: map.level_set_name.clone(),
        level_set_name_zh: map.level_set_name_zh.clone(),
        author: map.author.clone(),
        // 详情面板要标出「安装时会选哪个资产」，与后端自动匹配共用同一份规则
        asset_rules: map.asset_rules.clone(),
        note: map.note.clone(),
        note_i18n: map.note_i18n.clone(),
        enabled: map.enabled,
        builtin: map.builtin,
        // 源归属与源名：界面据此排「源 → 分类 → 地图」三层，
        // 后端不持久化源树，这两样只存在于条目上（见 `map_sources`）
        source_id: map.source_id.clone(),
        source_name: map.source_name.clone(),
        source_name_i18n: map.source_name_i18n.clone(),
        latest_tag: map
            .cached_releases
            .first()
            .map(|release| release.tag_name.clone()),
        latest_release: map.cached_releases.first().cloned(),
        // 详情抽屉要列出可安装的版本（含回滚到旧版），只给最新一条不够
        cached_releases: map.cached_releases.clone(),
        cached_release_count: map.cached_releases.len(),
        cached_at: map.releases_cached_at,
        last_error: map.last_error.clone(),
        relations: map.relations.clone(),
        required_frameworks: declared,
        unmet_frameworks: Vec::new(),
    }
}

/// 刷新一个地图仓库的 Release（5 分钟缓存，与 MOD 仓库同一套 TTL）。
#[tauri::command]
pub async fn refresh_map_releases(
    state: State<'_, AppState>,
    map_id: String,
    force: Option<bool>,
) -> Result<Vec<ReleaseInfo>, String> {
    let force = force.unwrap_or(false);
    let config = state.config.load_or_default();
    let Some(map) = config
        .map_repositories
        .iter()
        .find(|entry| entry.id == map_id)
        .cloned()
    else {
        return Err("mapSource.mapMissing".into());
    };
    if !crate::map_sources::map_is_available(&map, crate::config::current_user_region()) {
        return Err("source.repositoryUnavailableInRegion".into());
    }

    // 缓存命中直接返回，一个请求都不发（与 MOD 仓库同一条规则）
    let now = now_millis();
    if !force
        && !map.cached_releases.is_empty()
        && release_cache_is_fresh(map.releases_cached_at, now)
    {
        log::info!(
            "地图 Release 命中缓存：地图={}，数量={}",
            map.name,
            map.cached_releases.len()
        );
        return Ok(map.cached_releases.clone());
    }

    let client = releases::build_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    /*
     * Token 与代理列表都从 `release_probe` 投影出的 RepositoryConfig 取，
     * 避免在多个调用点各写一遍「哪些字段是 Release 需要的」。
     */
    let probe = crate::map_sources::release_probe(&map);
    let token = state.config.repository_api_token(&config, &probe);
    let api_proxies = match map.provider {
        Provider::Github => &config.github_proxies.github_api_proxies,
        Provider::Gitee => &config.github_proxies.gitee_api_proxies,
    };
    let fetched = crate::map_sources::fetch_map_releases(
        &client,
        &map,
        token.as_deref(),
        config.auth.official_api_only,
        api_proxies,
    )
    .await;

    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let mut latest = state.config.load_or_default();
    let Some(target) = latest
        .map_repositories
        .iter_mut()
        .find(|entry| entry.id == map_id)
    else {
        return Err("mapSource.mapMissing".into());
    };
    match fetched {
        Ok(releases) => {
            target.cached_releases = releases.clone();
            target.releases_cached_at = Some(now_millis());
            target.last_error = None;
            state
                .config
                .save(&latest)
                .map_err(|error| error.to_string())?;
            Ok(releases)
        }
        Err(error) => {
            log::error!("地图 Release 刷新失败：地图={}，原因={}", map.name, error);
            target.cached_releases.clear();
            target.releases_cached_at = None;
            target.last_error = Some(error.clone());
            state
                .config
                .save(&latest)
                .map_err(|save| save.to_string())?;
            Err(error)
        }
    }
}

/// 把一个地图 Release 暂存成「可确认的导入来源」。
///
/// 两段式与本地 ZIP 导入同构：这一步只做「选资产 → 下载 → 校验 → 识别 → 报告
/// 冲突」，用户确认后才由 `import_level_sets`（`install_from_zip`）真正写入
/// `levels/`。这样地图与 MOD 一样，安装前就能看到「将卸载本地哪些旧副本」。
///
/// 下载**完全复用** MOD 的候选规划与竞速协调器：GitHub 按自动直连 / 自动代理
/// 生成多个完整文件源并竞速（最多三个并行，慢的让位），Gitee 只直连且单线程。
/// 地图以前是单源直下，中国大陆走 Gitee 时慢得没有退路，走 GitHub 时又用不上
/// 用户配好的代理。
///
/// 暂存文件落在缓存目录（`staged = true`）：导入完成或用户取消后必须用
/// `discard_map_source` 清掉。
#[tauri::command]
pub async fn stage_map_release(
    app: AppHandle,
    state: State<'_, AppState>,
    map_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
    locale: Option<String>,
) -> Result<StagedMapSource, String> {
    // 取消标记只在命令入口重置，不能在等待元数据或补充候选之后重置
    archive::start_download();
    // 与 `install_repository` 同口径：先把用户指定的版本与资产写进日志。
    // 资产选不出来（`source.noMatchingAsset`）时，这行是唯一能看出他点了哪个资产的地方。
    log::info!(
        "开始暂存地图来源：地图 ID={}，指定版本={}，指定资产={}",
        map_id,
        tag.as_deref().unwrap_or("自动选择"),
        asset_name.as_deref().unwrap_or("自动匹配")
    );
    let config = state.config.load_or_default();
    let Some(map) = config
        .map_repositories
        .iter()
        .find(|entry| entry.id == map_id)
        .cloned()
    else {
        return Err("mapSource.mapMissing".into());
    };
    /*
     * 暂存不写游戏目录，所以不探测游戏目录、也不拦「游戏正在运行」：
     * 真正的写入在 `import_level_sets`，那里仍会校验 `game.notFound` 与运行状态。
     */
    let release = match tag.as_deref() {
        Some(tag) => map
            .cached_releases
            .iter()
            .find(|release| release.tag_name == tag)
            .cloned()
            .ok_or("mapSource.releaseMissing")?,
        None => map
            .cached_releases
            .first()
            .cloned()
            .ok_or("source.noInstallableRelease")?,
    };
    let probe = crate::map_sources::release_probe(&map);
    // 指定资产时精确匹配该名字，未指定才按 `asset_rules` 自动选 ——
    // 与 MOD 安装同一条 `select_install_asset`，不要在地图侧另写匹配。
    let asset = releases::select_install_asset(&probe, &release, asset_name.as_deref())
        .ok_or("source.noMatchingAsset")?
        .clone();

    log::info!(
        "地图安装资产已确定：地图={}，Release={}，资产={}",
        map.name,
        release.tag_name,
        asset.name
    );

    /*
     * 下载候选：与 MOD 资产同一条路径。
     *
     * 地图只分发 ZIP，所以按 Release 资产处理（不会进 Raw 分支）；
     * 代理模式按平台与地区取 `selected_asset_proxy_mode` 的默认值
     * （Gitee 恒直连）。
     */
    let candidates = plan_remote_download_candidates(
        &config,
        &map.provider,
        crate::direct_mod::RemoteUrlKind::ReleaseAsset,
        &asset.download_url,
    )?;

    // size 是 u64（0 表示未知），只有正值才参与校验与进度总分母
    let expected_size = (asset.size > 0).then_some(asset.size);
    /*
     * 摘要校验只对 GitHub 生效：Gitee 不提供 SHA-256。
     * 长度与 ZIP 结构校验两边都做。
     */
    let digest = if map.provider == Provider::Github {
        asset
            .digest
            .as_deref()
            .and_then(|value| value.strip_prefix("sha256:"))
    } else {
        None
    };
    let download_name = asset.name.clone();
    /*
     * 进度标签与预览标题用界面语言下的地图名：进度事件在下载过程中持续发出，
     * 名称必须在开始前算好。
     */
    let display_name = crate::config::localized_map_name(&map, locale.as_deref());

    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    /*
     * 线程数：GitHub 用仓库配置；Gitee 恒 1。
     * 竞速时每个源只开 1 个分片连接（`racing` 时传 1），否则三源 × N 分片会
     * 把带宽切得太碎，反而更慢 —— 与 MOD 侧完全相同的取舍。
     */
    let configured_threads = match map.provider {
        Provider::Github => map.download_threads.clamp(0, 32),
        Provider::Gitee => 1,
    };
    let racing = candidates.len() > 1;
    log::debug!(
        "地图下载候选准备完成：地图={}，平台={:?}，候选数={}，线程数={}，竞速={}",
        map.name,
        map.provider,
        candidates.len(),
        configured_threads,
        racing
    );

    let cache_dir = state.config.cache_dir();
    let temp = try_asset_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let client = &client;
            let cache_dir = &cache_dir;
            let label = &display_name;
            let is_zip = download_name.to_ascii_lowercase().ends_with(".zip");
            async move {
                let temp = archive::download_to_temp_with_threads_and_first_byte_timeout(
                    client,
                    &candidate.url,
                    cache_dir,
                    label,
                    &reporter,
                    if racing { 1 } else { configured_threads },
                    Some(std::time::Duration::from_secs(5)),
                )
                .await?;
                let validation = archive::validate_download_file(&temp, expected_size, digest)
                    .and_then(|_| {
                        if is_zip {
                            archive::validate_zip(&temp)
                        } else {
                            Ok(())
                        }
                    });
                if let Err(error) = validation {
                    let _ = fs::remove_file(&temp);
                    return Err(error);
                }
                Ok(temp)
            }
        },
        Some(DownloadContext {
            app: Some(&app),
            label: &display_name,
            expected_size,
        }),
    )
    .await
    .map_err(|error| {
        log::error!("地图资产下载失败：地图={}，原因={}", map.name, error);
        error
    })?;

    /*
     * 识别 + 身份校验在暂存阶段做完：
     *
     * - 仓库声明与实际 ZIP 不一致时，等用户确认完再报错太晚：界面已经展示了
     *   一份要装的地图集，而安装必然失败；
     * - 冲突列表要随 `ZipLevelSet` 一起返回，确认界面靠它提示「将卸载本地
     *   哪些旧副本」。
     *
     * 解析 `info` 是可能几秒的 CPU 工作，走 `spawn_blocking`：不要占着异步
     * 线程把窗口拖死（与 `stage_map_source` 同一条规则）。
     */
    let config_for_parse = config.clone();
    let path_for_parse = temp.clone();
    let app_for_parse = app.clone();
    let parsed = match tauri::async_runtime::spawn_blocking(move || {
        zip_level_sets(&app_for_parse, &config_for_parse, &path_for_parse)
    })
    .await
    {
        Ok(result) => result,
        Err(error) => Err(error.to_string()),
    };
    let sets = match parsed {
        Ok(sets) => sets,
        Err(error) => {
            let _ = fs::remove_file(&temp);
            return Err(error);
        }
    };
    if let Err(error) = validate_map_release_sets(&sets, &map) {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    log::info!(
        "地图 Release 已暂存：地图={}，版本={}，资产={}，关卡集={}，冲突={}",
        map.name,
        release.tag_name,
        download_name,
        sets.len(),
        sets.first().map(|set| set.conflicts.len()).unwrap_or(0)
    );
    Ok(StagedMapSource {
        path: display_path(&temp),
        staged: true,
        sets,
    })
}

/// 校验暂存到的关卡集与地图仓库声明的身份一致。
///
/// 这是原先「安装前才做」的校验，前移到暂存阶段：两段式确认发生在用户看到
/// 列表之后，声明与内容不一致时必须在展示前就失败。绑定身份是
/// `levelSetName + levelSetNameZH + author`（与扫描、开关、依赖判定共用
/// `level_set_identity_matches`），不一致会让绑定错位、后续更新检测全失效，
/// 宁可不装。
fn validate_map_release_sets(
    sets: &[ZipLevelSet],
    map: &MapRepositoryConfig,
) -> Result<(), String> {
    if sets.len() != 1 {
        log::warn!(
            "在线安装的地图 ZIP 应只含一个关卡集：地图={}，实际={}",
            map.name,
            sets.len()
        );
        return Err(crate::locale_tag::with_params(
            "levelSet.zipMultipleForRepository",
            &[("count", &sets.len().to_string())],
        ));
    }
    let set = &sets[0];
    let matches = crate::level_sets::level_set_identity_matches(
        &set.name,
        set.name_zh.as_deref(),
        &set.author,
        &map.level_set_name,
        Some(map.level_set_name_zh.as_str()).filter(|value| !value.is_empty()),
        &map.author,
    );
    if matches {
        return Ok(());
    }
    log::warn!(
        "关卡集与仓库声明不一致：地图={}，期望={} / {}，实际={} / {}",
        map.name,
        map.level_set_name,
        map.author,
        set.name,
        set.author
    );
    Err(crate::locale_tag::with_params(
        "levelSet.zipMismatch",
        &[
            (
                "expected",
                &format!("{} / {}", map.level_set_name, map.author),
            ),
            ("actual", &format!("{} / {}", set.name, set.author)),
        ],
    ))
}

/// 把线上依赖声明同步进本地清单（MOD 安装清单 + 地图依赖清单）。
///
/// 依赖声明只存在于仓库源文档里，源下线/文件被删之后本地就查不到了；
/// 所以安装时固化一份，刷新仓库后再按线上声明刷新 —— 在线与本地不一致时以在线为准。
///
/// 只读本地文件、不扫描游戏目录、不发网络请求，所以只取短的 `operation` 锁。
#[tauri::command]
pub fn sync_dependency_snapshots(
    state: State<'_, AppState>,
) -> Result<DependencySyncResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let manifest_dir = state.config.manifests_dir();

    let mut mods_updated = 0usize;
    for mut manifest in archive::read_manifests(&manifest_dir) {
        // `manual-` 开头是本地文件/在线直链安装：没有仓库，也就没有线上声明
        if manifest.repository_id.starts_with("manual-") {
            continue;
        }
        /*
         * 仓库找不到时原样跳过、不清空：源还没刷新出来、地区/平台不适用、
         * 自定义源被删都只是「当前查不到」，快照仍是已知的最新事实。
         */
        let Some(repo) = config
            .repositories
            .iter()
            .find(|repo| repo.id == manifest.repository_id)
        else {
            continue;
        };
        let relations = mods::effective_relations(repo, Some(&manifest.release_tag));
        if relations == manifest.relations {
            continue;
        }
        manifest.relations = relations;
        match archive::write_manifest(&manifest_dir, &manifest) {
            Ok(()) => mods_updated += 1,
            Err(error) => log::warn!(
                "同步 MOD 依赖快照失败：仓库={}，原因={}",
                manifest.repository_id,
                error
            ),
        }
    }

    /*
     * 地图快照整体重写：配置里没有的地图 id 直接消失 —— 地图源被删掉之后
     * 这张图确实不再有线上声明，留着旧条目等于给用户看一份幽灵依赖。
     */
    let previous = archive::read_map_relation_snapshots(&manifest_dir);
    let entries: BTreeMap<String, MapRelationSnapshot> = config
        .map_repositories
        .iter()
        .filter(|map| map.enabled)
        .map(|map| {
            (
                map.id.clone(),
                MapRelationSnapshot {
                    repository_id: map.id.clone(),
                    level_set_name: map.level_set_name.clone(),
                    author: map.author.clone(),
                    relations: map.relations.clone(),
                },
            )
        })
        .collect();
    let changed = entries
        .iter()
        .filter(|(id, entry)| previous.entries.get(*id) != Some(*entry))
        .count()
        + previous
            .entries
            .keys()
            .filter(|id| !entries.contains_key(*id))
            .count();
    let mut maps_updated = 0usize;
    if changed > 0 {
        let snapshots = MapRelationSnapshots {
            schema_version: crate::models::default_snapshot_schema_version(),
            entries,
        };
        match archive::write_map_relation_snapshots(&manifest_dir, &snapshots) {
            Ok(()) => maps_updated = changed,
            Err(error) => log::warn!("同步地图依赖快照失败：原因={}", error),
        }
    }
    log::info!(
        "依赖快照同步完成：MOD={}，地图={}",
        mods_updated,
        maps_updated
    );
    Ok(DependencySyncResult {
        mods_updated,
        maps_updated,
    })
}

/// 卸载 DIYLevel 框架（先自动备份，再按安装清单回收）。
///
/// 与 `uninstall_bepinex` 同一套顺序：**先整目录备份，再回收文件**。
/// 回收站只带得走清单里登记的文件，用户自己放进 `OC2DIYLevel` 的地图集、
/// 改过的配置不在清单里 —— 只有目录备份能保证还能找回。
#[tauri::command]
pub fn uninstall_diy_level(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("开始卸载 DIYLevel");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let installed =
        mods::scan_mods_with_manifests(&game_root, &config, Some(&state.config.manifests_dir()));
    let Some(target) = installed.iter().find(|item| {
        item.guid
            .as_deref()
            .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
    }) else {
        return Err("framework.diyLevelMissing".into());
    };
    let dll_file = target.dll_file.clone();
    let package_id = target.package_id.clone().unwrap_or_else(|| "manual".into());

    // 先备份整个 OC2DIYLevel 目录（含清单快照），再动文件
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
        &state.config.manifests_dir(),
        &package_id,
    )?;
    log::info!("DIYLevel 卸载前备份完成：路径={}", backup.display());

    let manifests_dir = state.config.manifests_dir();
    let recycle_root = state.config.mod_recycle_bin_dir();
    let manifest_id = Some(package_id.clone());

    // 有安装清单就整包回收；没有（手工装的）就只回收这个 DLL
    let has_manifest = manifest_id
        .as_deref()
        .is_some_and(|id| archive::read_manifest(&manifests_dir, id).is_some());
    let result = if let Some(id) = manifest_id.as_deref().filter(|_| has_manifest) {
        recycle_bin::recycle_package(&recycle_root, &manifests_dir, &game_root, id, None).map(
            |entry| {
                crate::locale_tag::with_params(
                    "message.framework.diyLevelMovedToRecycleBin",
                    &[
                        ("count", &entry.file_count.to_string()),
                        ("path", &backup.display().to_string()),
                    ],
                )
            },
        )
    } else if config.recycle_deleted_mods {
        recycle_bin::recycle_mod_dll(
            &recycle_root,
            &manifests_dir,
            &game_root,
            manifest_id.as_deref().unwrap_or("manual"),
            &dll_file,
            &installed,
        )
        .map(|_| {
            crate::locale_tag::with_params(
                "message.framework.diyLevelMovedToRecycleBinNoCount",
                &[("path", &backup.display().to_string())],
            )
        })
    } else {
        recycle_bin::purge_mod_dll(
            &recycle_root,
            &manifests_dir,
            &game_root,
            manifest_id.as_deref().unwrap_or("manual"),
            &dll_file,
            &installed,
        )
        .map(|_| {
            crate::locale_tag::with_params(
                "message.framework.diyLevelDeleted",
                &[("path", &backup.display().to_string())],
            )
        })
    };

    match result {
        Ok(message) => {
            log::info!("DIYLevel 卸载完成：{message}");
            Ok(OperationResult { ok: true, message })
        }
        Err(error) => {
            log::error!("DIYLevel 回收失败：原因={error}，备份={}", backup.display());
            Err(crate::locale_tag::with_params(
                "backup.diyLevelRecycleFailed",
                &[
                    ("reason", &error.to_string()),
                    ("path", &backup.display().to_string()),
                ],
            ))
        }
    }
}

/// 打开 DIYLevel 的地图目录（放在框架所在目录层）。
///
/// 与 `open_bepinex_directory` 同一套约束：只允许游戏目录内的固定子路径，
/// 且**不自动创建**——目录不存在说明框架没装或版本结构不同，创建空目录
/// 只会误导用户。
#[tauri::command]
pub fn open_diy_level_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let root = Path::new(&game_root);
    // 优先地图目录；框架装在别处时退回插件目录，至少能给用户一个能看的落点
    let levels = game::safe_game_path(root, "BepInEx/plugins/OC2DIYLevel/levels")?;
    let package = game::safe_game_path(root, "BepInEx/plugins/OC2DIYLevel")?;
    let target = if levels.is_dir() {
        levels
    } else if package.is_dir() {
        package
    } else {
        return Err("error.framework.diyLevelDirMissing?path=BepInEx/plugins/OC2DIYLevel".into());
    };
    external_open_command()
        .arg(&target)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "message.framework.diyLevelDirOpened".into(),
    })
}

/// 备份 DIYLevel 目录（只含 `BepInEx/plugins/OC2DIYLevel`）+ 它的安装清单快照。
#[tauri::command]
pub fn backup_diy_level(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("开始备份 DIYLevel");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;
    // 清单 ID 是动态的（仓库 id 或 manual-<uuid>），从当前扫描结果取
    let installed =
        mods::scan_mods_with_manifests(&game_root, &config, Some(&state.config.manifests_dir()));
    let manifest_id = installed
        .iter()
        .find(|item| {
            item.guid
                .as_deref()
                .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
        })
        .and_then(|item| item.package_id.clone())
        .ok_or("framework.diyLevelMissing")?;
    let backup = backup_framework(
        &game_root,
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
        &state.config.manifests_dir(),
        &manifest_id,
    )?;
    log::info!("DIYLevel 备份完成：路径={}", backup.display());
    Ok(OperationResult {
        ok: true,
        message: crate::locale_tag::with_params(
            "message.backup.created",
            &[("path", &backup.display().to_string())],
        ),
    })
}

/// 恢复 DIYLevel 备份。
///
/// 按用户要求：**先删掉现有的 DIYLevel 目录，再整体还原备份**。这与覆盖式
/// 恢复不同 —— 备份之后新增的地图集不会被留下来，得到的就是备份那一刻的
/// 完整快照，不会出现「新旧文件混在一起」的半新半旧状态。
///
/// 与 BepInEx 恢复一致，删除前先把当前目录另存一份 `DIYLevel.before-restore-*`，
/// 用户误操作时还有一次后悔的机会。
#[tauri::command]
pub fn restore_diy_level(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    log::info!("开始恢复 DIYLevel 备份：路径={backup_path}");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let backup = validate_framework_backup_in(
        Path::new(&backup_path),
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
    )?;
    /*
     * 清单 ID：优先用备份里快照自带的（那是安装时的真实 ID），
     * 没有快照的旧备份退回扫描当前安装结果，最后兜底 "manual"。
     */
    let manifest_id = read_backup_manifest(&backup)
        .map(|manifest| manifest.repository_id)
        .or_else(|| {
            let installed = mods::scan_mods_with_manifests(
                &game_root,
                &config,
                Some(&state.config.manifests_dir()),
            );
            installed
                .iter()
                .find(|item| {
                    item.guid
                        .as_deref()
                        .is_some_and(|guid| guid.trim().eq_ignore_ascii_case(DIYLEVEL_GUID))
                })
                .and_then(|item| item.package_id.clone())
        })
        .unwrap_or_else(|| "manual".into());
    restore_framework_backup(
        &game_root,
        &backup,
        &DIYLEVEL_BACKUP,
        &state.config.manifests_dir(),
        &manifest_id,
    )?;
    log::info!("DIYLevel 备份恢复完成：清单ID={manifest_id}");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.diyLevelRestored".into(),
    })
}

/// 删除一个 DIYLevel 备份。
#[tauri::command]
pub fn delete_diy_level_backup(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    log::info!("开始删除 DIYLevel 备份：路径={backup_path}");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let target = validate_framework_backup_in(
        Path::new(&backup_path),
        &state.config.backups_dir(),
        &DIYLEVEL_BACKUP,
    )?;
    archive::clear_readonly(&target)?;
    fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    log::info!("DIYLevel 备份删除完成");
    Ok(OperationResult {
        ok: true,
        message: "message.backup.deleted".into(),
    })
}

/// 校验一个路径确实是**本管理器**创建的 DIYLevel 备份目录。
#[cfg(test)]
fn validate_diy_level_backup_in(backup_path: &Path, backup_root: &Path) -> Result<PathBuf, String> {
    validate_framework_backup_in(backup_path, backup_root, &DIYLEVEL_BACKUP)
}

#[tauri::command]
pub fn open_bepinex_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开 BepInEx 目录");
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let framework = game::safe_game_path(Path::new(&game_root), "BepInEx")?;
    if !framework.is_dir() {
        return Err("framework.bepInExDirMissing".into());
    }
    external_open_command()
        .arg(&framework)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "framework.bepInExDirOpened".into(),
    })
}

fn resolve_mod_file_for_location(game_root: &Path, dll_file: &str) -> Result<PathBuf, String> {
    let relative = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    let normalized = relative.to_ascii_lowercase();
    if !normalized.starts_with("bepinex/plugins/") || !normalized.ends_with(".dll") {
        return Err("mods.onlyPluginsLocatable".into());
    }
    let plugins_root = game::safe_game_path(game_root, "BepInEx/plugins")?;
    for candidate in [relative.clone(), mods::disabled_dll_file(&relative)] {
        let path = game::safe_game_path(game_root, &candidate)?;
        if !path.starts_with(&plugins_root) {
            return Err("mods.onlyPluginsLocatable".into());
        }
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("mods.dllFileMissing".into())
}

/// 交给系统文件管理器 / Shell 的路径串。
///
/// 在 `display_path` 之上多一步：Windows 下把正斜杠统一成反斜杠。
/// 两层原因：
///   · `safe_game_path` 返回 `canonicalize` 之后的路径，Windows 上带 `\\?\`
///     verbatim 前缀，Explorer 不按标准 argv 解析、也不接受这种写法；
///   · Explorer 的 `/select,` 参数解析**不认混合分隔符** —— 拿到
///     `...\BepInEx/logs\xxx` 这种串时会解析失败并退回「此电脑」，
///     而 `Path::join("BepInEx/logs")` 恰恰会把 `/` 原样留在路径里
///     （2026-11 实测：菜单里的「查看日志」打不开日志目录）。
/// 归一放在这个出口做，任何调用方都不会漏。
fn shell_path(path: &Path) -> String {
    let shown = display_path(path);
    if cfg!(windows) {
        shown.replace('/', "\\")
    } else {
        shown
    }
}

/// 系统文件管理器只认普通路径：交给 Explorer 之前统一过 `shell_path`。
fn directory_open_command(path: &Path) -> std::process::Command {
    let mut command = external_open_command();
    command.arg(shell_path(path));
    command
}

fn mod_file_location_command(path: &Path) -> std::process::Command {
    #[cfg(target_os = "macos")]
    {
        let mut command = external_open_command();
        command.arg("-R").arg(path);
        return command;
    }
    #[cfg(windows)]
    {
        // Explorer expects ordinary drive/UNC paths, not canonical verbatim prefixes.
        let selection = format!(r#"/select,"{}""#, shell_path(path));
        let mut command = std::process::Command::new("explorer.exe");
        use std::os::windows::process::CommandExt;
        // Quote only the path: Explorer does not use the standard argv parser.
        command.raw_arg(selection);
        command
    }
}

fn external_open_command() -> std::process::Command {
    #[cfg(windows)]
    let program = "explorer.exe";
    #[cfg(target_os = "macos")]
    let program = "open";
    let mut command = std::process::Command::new(program);
    command.stdin(std::process::Stdio::null());
    command
}

#[tauri::command]
pub fn open_mod_file_location(
    state: State<'_, AppState>,
    dll_file: String,
) -> Result<OperationResult, String> {
    log::debug!("打开 MOD 文件位置：文件={}", dll_file);
    let config = {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "lock.poisoned".to_string())?;
        state.config.load_or_default()
    };
    let game_root = game::detect_game(&config).path.ok_or("game.notFound")?;
    let path = resolve_mod_file_for_location(Path::new(&game_root), &dll_file)?;
    mod_file_location_command(&path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "mods.fileLocationOpened".into(),
    })
}

#[tauri::command]
pub fn open_mod_logs_directory(
    state: State<'_, AppState>,
    dll_file: String,
) -> Result<OperationResult, String> {
    log::debug!("打开 MOD 日志目录：文件={}", dll_file);
    let (game_root, guid, _) = resolve_mod_config(&state, &dll_file)?;
    let path = crate::mod_config::checked_logs_dir(&game_root, &guid)?;
    if !path.is_dir() {
        return Err("mods.logsDirMissing".into());
    }
    mod_file_location_command(&path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "mods.logsDirOpened".into(),
    })
}

#[tauri::command]
pub fn read_mod_config(
    state: State<'_, AppState>,
    dll_file: String,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, guid, _) = resolve_mod_config(&state, &dll_file)?;
    log::debug!("读取 MOD 配置：GUID={}", guid);
    crate::mod_config::read(&game_root, &guid)
}

fn resolve_mod_config(
    state: &AppState,
    dll_file: &str,
) -> Result<(PathBuf, String, String), String> {
    let (root, config) = resolve_config_game(state)?;
    resolve_mod_file_for_location(&root, dll_file)?;
    let installed = mods::scan_mods_with_manifests(&root, &config, None);
    let key = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    let guid = installed
        .into_iter()
        .find(|item| item.dll_file.eq_ignore_ascii_case(&key))
        .and_then(|item| item.guid)
        .ok_or("mods.guidUnrecognized")?;
    Ok((root, guid, config.game.path))
}

fn resolve_config_game(state: &AppState) -> Result<(PathBuf, AppConfig), String> {
    let config = {
        let _guard = state.operation.lock().map_err(|_| "lock.poisoned")?;
        state.config.load_or_default()
    };
    let root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    Ok((root, config))
}

#[tauri::command]
pub fn save_mod_config(
    state: State<'_, AppState>,
    dll_file: String,
    revision: String,
    content: String,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, guid, configured_path) = resolve_mod_config(&state, &dll_file)?;
    save_config_document(
        &state,
        &game_root,
        &guid,
        &configured_path,
        &revision,
        &content,
    )
}

#[tauri::command]
pub fn read_bepinex_config(
    state: State<'_, AppState>,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, _) = resolve_config_game(&state)?;
    crate::mod_config::read(&game_root, "BepInEx")
}

#[tauri::command]
pub fn save_bepinex_config(
    state: State<'_, AppState>,
    revision: String,
    content: String,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let (game_root, config) = resolve_config_game(&state)?;
    save_config_document(
        &state,
        &game_root,
        "BepInEx",
        &config.game.path,
        &revision,
        &content,
    )
}

fn save_config_document(
    state: &AppState,
    game_root: &Path,
    guid: &str,
    configured_path: &str,
    revision: &str,
    content: &str,
) -> Result<crate::mod_config::ModConfigDocument, String> {
    let _guard = state.operation.lock().map_err(|_| "lock.poisoned")?;
    let config = state.config.load_or_default();
    if config.game.path != configured_path {
        return Err("game.directoryChangedReopenConfig".into());
    }
    game::ensure_game_stopped(game_root)?;
    let result = crate::mod_config::save(game_root, guid, revision, content);
    log::info!("保存配置：标识={}，成功={}", guid, result.is_ok());
    result
}

#[tauri::command]
pub fn open_backup_directory(
    state: State<'_, AppState>,
    backup_path: Option<String>,
) -> Result<OperationResult, String> {
    let path = match backup_path {
        Some(path) => {
            let path = PathBuf::from(path);
            if !path.is_dir() {
                return Err("backup.notFound".into());
            }
            path
        }
        None => {
            let path = state.config.backups_dir();
            fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            path
        }
    };
    log::debug!("打开备份目录：路径={}", path.display());
    external_open_command()
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "tools.backupDirOpened".into(),
    })
}

#[tauri::command]
pub fn open_release_page(url: String) -> Result<OperationResult, String> {
    log::info!("打开 Release 页面");
    let parsed = Url::parse(&url).map_err(|e| {
        crate::locale_tag::with_params("update.pageInvalid", &[("reason", &e.to_string())])
    })?;
    if parsed.scheme() != "https" {
        return Err("update.pageMustBeHttps".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("update.pageHasCredentials".into());
    }
    external_open_command()
        .arg(parsed.as_str())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "update.pageOpenedDetail".into(),
    })
}

#[tauri::command]
pub fn open_update_repository() -> Result<OperationResult, String> {
    log::info!("打开安装器更新仓库页面");
    let url = match detect_installer_region() {
        InstallerRegion::MainlandChina => {
            format!("https://gitee.com/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases")
        }
        InstallerRegion::International => {
            format!("https://github.com/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases")
        }
    };
    open_release_page(url)
}

#[tauri::command]
pub fn exit_app() -> Result<(), String> {
    log::info!("收到退出应用请求");
    std::process::exit(0)
}

/// 当前进程的权限与可写性自检。
///
/// 应用不再启动即提权（否则 Windows UIPI 会阻止拖放、每次启动弹 UAC）。
/// 前端据此在设置页显示状态，并在真正遇到「拒绝访问」时提示用户
/// 以管理员身份重启。
#[derive(serde::Serialize)]
pub struct PermissionStatus {
    /// 当前进程是否具备管理员/root 权限。
    pub elevated: bool,
    /// 游戏 MOD 目录是否可写：需要提权的唯一真实判据。
    pub game_writable: bool,
    /// 管理器数据目录是否可写（用户文档 / Application Support 下的数据根）。
    pub data_writable: bool,
}

#[tauri::command]
pub fn permission_status(state: State<'_, AppState>) -> Result<PermissionStatus, String> {
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path;
    let game_writable = game_root
        .as_deref()
        .map(|path| probe_writable(&Path::new(path).join("BepInEx/plugins")))
        .unwrap_or(false);
    let data_writable = crate::user_paths::root()
        .map(|root| probe_writable(&root))
        .unwrap_or(false);
    Ok(PermissionStatus {
        elevated: crate::is_elevated(),
        game_writable,
        data_writable,
    })
}

/// 探测目录是否可写：创建并删除一个临时文件，不改动任何既有内容。
fn probe_writable(dir: &Path) -> bool {
    if std::fs::create_dir_all(dir).is_err() {
        return false;
    }
    let probe = dir.join(format!(".oc2-write-probe-{}", uuid::Uuid::new_v4()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

/// 以管理员身份重启自己；成功后本进程立即退出。
///
/// 只在文件访问真的被拒时由用户主动触发 —— 应用平时以普通权限运行，
/// 这样拖放可用、启动不弹 UAC、更新时能正常关闭自己。
#[tauri::command]
pub fn relaunch_as_admin() -> Result<OperationResult, String> {
    log::info!("收到以管理员身份重启请求");
    if crate::is_elevated() {
        return Ok(OperationResult {
            ok: true,
            message: "update.alreadyElevated".into(),
        });
    }
    /*
     * 只有 Windows 有可用的提权重启路径（UAC runas）。macOS 没有 UAC，
     * 提权要经 sudo / Authorization Services，整进程重启这条路既做不到、
     * 也不该做：游戏目录和数据根都在用户目录下，本来就归当前用户所有；
     * 用 root 跑 GUI 会让 TCC 权限失效、配置与缓存变成 root 所有、更新器
     * 无法自替换。设置页在 macOS 上已不再显示这个按钮，这里兜住旧前端。
     */
    if !cfg!(windows) {
        log::info!("当前平台不需要也不支持提权重启");
        return Ok(OperationResult {
            ok: false,
            message: "update.elevationUnsupported".into(),
        });
    }
    if crate::relaunch_elevated() {
        log::info!("已启动管理员实例，退出当前进程");
        // 给新进程一点时间接管
        std::thread::sleep(std::time::Duration::from_millis(200));
        std::process::exit(0);
    }
    Ok(OperationResult {
        ok: false,
        message: "update.elevationFailed".into(),
    })
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    locale: Option<String>,
) -> Result<AppUpdateCheckResult, String> {
    log::info!("开始检查安装器更新");
    let _ = locale;
    match tokio::time::timeout(
        std::time::Duration::from_secs(
            5 * installer_manifest_sources(detect_installer_region()).len() as u64 + 10,
        ),
        discover_installer_update(&state, &app),
    )
    .await
    {
        Ok(Ok(result)) => {
            log::info!(
                "安装器更新检查完成：发现更新={}，全部失败={}",
                result.update.is_some(),
                result.all_failed
            );
            Ok(result)
        }
        Ok(Err(error)) => {
            let region = detect_installer_region();
            log::error!("安装器更新检查初始化失败: {error}");
            let channels = installer_manifest_sources(region)
                .iter()
                .map(|channel| {
                    let status = installer_channel_status(
                        channel,
                        "failed",
                        Some("update.checkInitFailed".into()),
                    );
                    emit_installer_channel_status(&app, region, status.clone());
                    status
                })
                .collect();
            Ok(AppUpdateCheckResult {
                update: None,
                channels,
                all_failed: true,
                region: region.as_str().into(),
            })
        }
        Err(_) => {
            log::warn!("安装器更新检查超过总超时，已跳过自动更新");
            let region = detect_installer_region();
            let channels = installer_manifest_sources(region)
                .iter()
                .map(|channel| {
                    let status = installer_channel_status(
                        channel,
                        "failed",
                        Some("update.checkTimeout".into()),
                    );
                    emit_installer_channel_status(&app, region, status.clone());
                    status
                })
                .collect();
            Ok(AppUpdateCheckResult {
                update: None,
                channels,
                all_failed: true,
                region: region.as_str().into(),
            })
        }
    }
}

async fn download_installer(
    client: &reqwest::Client,
    url: &str,
    reporter: &archive::DownloadReporter,
    label: &str,
) -> Result<Vec<u8>, String> {
    log::debug!("开始下载安装器文件：标签={}", label);
    let response = client
        .get(url)
        .header("User-Agent", releases::APP_USER_AGENT)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let total = response.content_length();
    let started = std::time::Instant::now();
    let mut downloaded = 0_u64;
    let mut bytes = Vec::with_capacity(total.unwrap_or_default().min(64 * 1024 * 1024) as usize);
    let mut stream = response.bytes_stream();
    let first_chunk = tokio::time::timeout(std::time::Duration::from_secs(5), stream.next())
        .await
        .map_err(|_| "download.firstByteTimeout".to_string())?;
    let mut next_chunk = first_chunk;
    while let Some(chunk) = next_chunk {
        let chunk = chunk.map_err(|error| error.to_string())?;
        reporter.received(chunk.len() as u64);
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        archive::emit_progress(reporter, label, downloaded, total, started.elapsed(), false);
        next_chunk = stream.next().await;
    }
    if downloaded == 0 {
        return Err("download.empty".into());
    }
    if total.is_some_and(|expected| expected != downloaded) {
        return Err("download.incomplete".into());
    }
    archive::emit_progress(reporter, label, downloaded, total, started.elapsed(), true);
    log::info!("下载安装器文件完成：标签={}，字节数={}", label, downloaded);
    Ok(bytes)
}

async fn download_installer_with_timeout(
    client: &reqwest::Client,
    url: &str,
    reporter: &archive::DownloadReporter,
    label: &str,
) -> Result<Vec<u8>, String> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(30),
        download_installer(client, url, reporter, label),
    )
    .await
    {
        Ok(result) => result,
        Err(_) => {
            log::warn!("下载安装器文件超过 30 秒：标签={}", label);
            Err("update.downloadTimeout".into())
        }
    }
}

async fn download_installer_for_info(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    app: &AppHandle,
    signature: &str,
) -> Result<Vec<u8>, Vec<String>> {
    let candidates = installer_download_candidates(info);
    download_verified_installer(client, info, candidates, Some(app), |bytes| {
        crate::app_update::verify_download(bytes, signature, &info.version)
    })
    .await
    .map_err(|error| vec![error])
}

async fn download_verified_installer(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    candidates: Vec<AssetDownloadCandidate>,
    app: Option<&AppHandle>,
    verify: impl Fn(&[u8]) -> Result<(), String>,
) -> Result<Vec<u8>, String> {
    let proxy_count = candidates
        .iter()
        .filter(|candidate| candidate.route == AssetDownloadRoute::Proxy)
        .count();
    log::info!(
        "安装器下载候选准备完成：平台={:?}，直连={}，Release代理={}，总候选={}",
        info.provider,
        candidates.len() - proxy_count,
        proxy_count,
        candidates.len()
    );
    let result = try_asset_download_candidates(
        candidates,
        archive::is_download_cancelled,
        |candidate, reporter| {
            let verify = &verify;
            async move {
                let mode = if candidate.route == AssetDownloadRoute::Proxy {
                    "proxy"
                } else {
                    "direct"
                };
                let channel = info.provider.as_ref().and_then(|provider| {
                    installer_sources_for_url(
                        provider,
                        info.installer_url.as_deref().unwrap_or_default(),
                    )
                    .into_iter()
                    .find(|channel| channel.endpoint == candidate.url)
                });
                if let (Some(app), Some(channel)) = (app, &channel) {
                    emit_installer_channel_status(
                        app,
                        detect_installer_region(),
                        AppUpdateChannelStatus {
                            mode: mode.into(),
                            ..installer_channel_status(channel, "downloading", None)
                        },
                    );
                }
                let result = download_installer_with_timeout(
                    client,
                    &candidate.url,
                    &reporter,
                    "update.downloadingInstaller",
                )
                .await
                .and_then(|bytes| {
                    archive::validate_download_bytes(
                        &bytes,
                        info.download_size.filter(|size| *size > 0),
                        None,
                    )?;
                    verify(&bytes)?;
                    Ok(bytes)
                });
                if let (Some(app), Some(channel)) = (app, &channel) {
                    emit_installer_channel_status(
                        app,
                        detect_installer_region(),
                        AppUpdateChannelStatus {
                            mode: mode.into(),
                            ..installer_channel_status(
                                channel,
                                if result.is_ok() {
                                    "downloaded"
                                } else {
                                    "failed"
                                },
                                result.as_ref().err().cloned(),
                            )
                        },
                    );
                }
                result
            }
        },
        Some(DownloadContext {
            app,
            label: "update.downloadingInstaller",
            expected_size: info.download_size,
        }),
    )
    .await;
    result
}

async fn resolve_app_update(
    app: &AppHandle,
    info: &AppUpdateInfo,
) -> Result<tauri_plugin_updater::Update, String> {
    let provider = info.provider.as_ref().ok_or("update.manifestUrlInvalid")?;
    let channels = installer_sources_for_url(provider, &installer_manifest_url(provider));
    race_installer_channels(
        &channels,
        archive::is_download_cancelled,
        |channel, _| async move { resolve_app_update_at(app, info, &channel.endpoint).await },
    )
    .await
}

async fn resolve_app_update_at(
    app: &AppHandle,
    info: &AppUpdateInfo,
    url: &str,
) -> Result<tauri_plugin_updater::Update, String> {
    use tauri_plugin_updater::UpdaterExt;
    let target = crate::app_update::UpdateTarget::current()?;
    if target.kind != info.installer_type {
        return Err("update.manifestPlatformMismatch".into());
    }
    #[cfg(windows)]
    let installer_args = {
        let executable = std::env::current_exe().map_err(|error| error.to_string())?;
        let target_executable = crate::update_migration::update_executable(&executable)?;
        let logs = crate::user_paths::logs_dir()?;
        fs::create_dir_all(&logs).map_err(|error| error.to_string())?;
        // Keep the original path so installer-side relocation can snapshot it.
        crate::app_update::windows_installer_args(
            target.kind,
            &executable,
            &target_executable,
            &logs.join("update-msi.log"),
        )?
    };
    if archive::is_download_cancelled() {
        return Err("download.cancelled".into());
    }
    let endpoint = Url::parse(url).map_err(|_| "update.manifestUrlInvalid")?;
    let builder = app
        .updater_builder()
        .target(target.manifest_key())
        .timeout(std::time::Duration::from_secs(5))
        .header("User-Agent", releases::APP_USER_AGENT)
        .map_err(|e| e.to_string())?
        .header("Cache-Control", "no-cache, no-store")
        .map_err(|e| e.to_string())?
        .header("Pragma", "no-cache")
        .map_err(|e| e.to_string())?;
    #[cfg(windows)]
    let builder = builder
        .clear_installer_args()
        .installer_args(installer_args.clone())
        // Our success hooks launch once; do not append /R or /ARGS.
        .restart_after_install(false)
        .on_before_exit(|| {
            log::info!("Tauri Updater 准备启动 Windows 安装器并退出当前程序；安装结果尚未确认");
        });
    let updater = match builder
        .endpoints(vec![endpoint])
        .and_then(|builder| builder.build())
    {
        Ok(updater) => updater,
        Err(error) => {
            log::warn!("更新清单入口不可用：原因={error}");
            return Err(error.to_string());
        }
    };
    let cancellation = async {
        while !archive::is_download_cancelled() {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    };
    let result = tokio::select! {
        biased;
        _ = cancellation => return Err("download.cancelled".into()),
        result = updater.check() => result,
    };
    match result {
        Ok(Some(update)) => {
            match crate::app_update::validate_update_metadata(
                &update.version,
                &update.download_url,
                &update.signature,
                info,
            ) {
                Ok(()) => return Ok(update),
                Err(error) => return Err(error),
            }
        }
        Ok(None) => log::warn!("更新清单没有所选 Release 的更新版本"),
        Err(_) => log::warn!(
            "更新清单读取失败：来源={}",
            repository_sources::safe_log_address(url)
        ),
    }
    Err("update.manifestOrSignatureUnavailable".into())
}

struct VerifiedInstallerDownload {
    bytes: Vec<u8>,
    update: tauri_plugin_updater::Update,
}

async fn prepare_and_download_installer(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    app: &AppHandle,
) -> Result<VerifiedInstallerDownload, Vec<String>> {
    let update = resolve_app_update(app, info)
        .await
        .map_err(|error| vec![error])?;
    let bytes = download_installer_for_info(client, info, app, &update.signature).await?;
    Ok(VerifiedInstallerDownload { bytes, update })
}

enum InstallerDownloadAttempt {
    Updated {
        info: AppUpdateInfo,
        download: VerifiedInstallerDownload,
    },
    Latest,
    Failed(String),
}

/// Finish the single Gitee route before starting GitHub's independent
/// manifest and signature-validated package races.
async fn discover_and_download_installer_update(
    state: &AppState,
    app: &AppHandle,
) -> Result<InstallerDownloadAttempt, String> {
    let region = detect_installer_region();
    log::info!("开始执行安装器更新流程：地区={}", region.as_str());
    let config = state.config.load_or_default();
    let client = releases::build_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let download_client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|error| error.to_string())?;
    let target = crate::app_update::UpdateTarget::current()?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| {
        crate::locale_tag::with_params(
            "update.currentVersionInvalid2",
            &[("reason", &error.to_string())],
        )
    })?;
    let result = run_installer_manifest_routes(
        &client,
        &installer_manifest_sources(region),
        target,
        &current_version,
        archive::is_download_cancelled,
        |status| {
            if status.status == "checking" {
                emit_installer_phase(app, region, "manifest", None, None);
            }
            emit_installer_channel_status(app, region, status);
        },
        |info| {
            let download_client = &download_client;
            async move {
                emit_installer_phase(app, region, "download", None, Some(info.clone()));
                let download = prepare_and_download_installer(download_client, &info, app)
                    .await
                    .map_err(|errors| errors.join("；"))?;
                Ok((info, download))
            }
        },
    )
    .await?;
    match result {
        InstallerRouteResult::Updated((info, download)) => {
            return Ok(InstallerDownloadAttempt::Updated { info, download });
        }
        InstallerRouteResult::Latest => return Ok(InstallerDownloadAttempt::Latest),
        InstallerRouteResult::Failed => {}
    }
    let message = "update.manualUpdateSkipped".to_string();
    emit_installer_phase(app, region, "skipped", Some(message.clone()), None);
    Ok(InstallerDownloadAttempt::Failed(message))
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    archive::start_download();
    log::info!("开始安装器自更新");
    let attempt = discover_and_download_installer_update(&state, &app).await?;
    let (info, download) = match attempt {
        InstallerDownloadAttempt::Updated { info, download } => (info, download),
        InstallerDownloadAttempt::Latest => {
            log::info!("安装器已是最新版本");
            return Ok(OperationResult {
                ok: false,
                message: "update.alreadyLatest".into(),
            });
        }
        InstallerDownloadAttempt::Failed(message) => {
            log::warn!("安装器自更新未完成：{}", message);
            return Ok(OperationResult { ok: false, message });
        }
    };
    if archive::is_download_cancelled() {
        return Err("download.cancelled".into());
    }
    emit_installer_phase(
        &app,
        detect_installer_region(),
        "install",
        None,
        Some(info.clone()),
    );
    let version = info.version;
    log::info!(
        "开始通过 Tauri Updater 安装程序更新：版本={}，格式={}，已验签字节数={}",
        version,
        info.installer_type,
        download.bytes.len()
    );
    tauri::async_runtime::spawn_blocking(move || download.update.install(&download.bytes))
        .await
        .map_err(|_| "update.installFailed")?
        .map_err(|error| {
            log::error!("Tauri Updater 安装失败：{error}");
            "update.installFailedManual".to_string()
        })?;
    #[cfg(not(windows))]
    {
        log::info!("Tauri Updater 安装完成，正在重启：版本={version}");
        app.restart();
    }
    #[cfg(windows)]
    Ok(OperationResult {
        ok: true,
        // Windows normally exits inside Update::install after spawning.
        message: "update.installerStarted".into(),
    })
}

fn collect_update_status(config: &AppConfig, installed: &[InstalledMod]) -> Vec<UpdateStatus> {
    config
        .repositories
        .iter()
        // 不可用的仓库不产生更新状态：它不刷新，缓存的 Release 也就不再可信
        .filter(|repo| {
            crate::config::repository_is_available(
                config,
                repo,
                crate::config::current_user_region(),
            )
        })
        .map(|repo| {
            let local_mod = installed.iter().find(|item| {
                (repo.download_mode == RepositoryDownloadMode::RepositoryFile
                    || (item.status != "not_release_asset"
                        && item.status != "duplicate_lower_version"))
                    && (item.repository_ids.iter().any(|id| id == &repo.id)
                        || item.repository_id.as_deref() == Some(repo.id.as_str()))
            });
            let local = local_mod.and_then(mods::local_update_version);
            if repo.download_mode == RepositoryDownloadMode::RepositoryFile {
                return UpdateStatus {
                    repository_id: repo.id.clone(),
                    local_version: local,
                    remote_version: None,
                    state: "not_release_asset".into(),
                };
            }
            if repo.last_error.is_some() {
                return UpdateStatus {
                    repository_id: repo.id.clone(),
                    local_version: local,
                    remote_version: None,
                    state: "repository_unavailable".into(),
                };
            }
            let remote = mods::remote_update_version(repo, local_mod);
            let state = local_mod
                .map(|installed| mods::mod_update_state(repo, installed).1)
                .unwrap_or("unknown");
            UpdateStatus {
                repository_id: repo.id.clone(),
                local_version: local,
                remote_version: remote,
                state: state.into(),
            }
        })
        .collect()
}

fn open_steam(settings: &GameConfig, info: &GameInfo) -> Result<OperationResult, String> {
    if !info.game_exists {
        return Err("game.executableMissing".into());
    }
    game::steam_launch_command(settings, info)?
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "game.steamLaunchRequested".into(),
    })
}

/// 备份里保存安装清单用的文件名（放在备份目录根部）。
///
/// 备份目录是框架目录的**完整可还原快照**，不只是文件副本：恢复后必须能
/// 再次卸载，而卸载依赖安装清单。所以清单要跟文件一起进备份，恢复时一并
/// 写回 `manifests/<id>.toml`。
///
/// 早期版本没备份清单，导致「卸载 → 恢复 → 再卸载」时报
/// 「没有安装清单，拒绝删除未知文件」，用户只能手动删目录。
const FRAMEWORK_BACKUP_MANIFEST: &str = "framework-manifest.toml";

/// 一次框架备份的内容描述。
///
/// 两个框架（BepInEx / DIYLevel）的备份范围不同，但「备份什么、恢复什么、
/// 清单放哪」完全同构，所以统一成这个结构，避免两套实现各自漂移：
///   · `directories` —— 需要整棵树拷贝的目录（游戏相对路径）
///   · `files`       —— 散落在游戏根目录的框架文件（游戏相对路径）
struct FrameworkBackupSpec {
    /// 备份目录名前缀，也是恢复/删除时校验用的标识。
    prefix: &'static str,
    /// 需要整目录拷贝的游戏相对路径。
    directories: &'static [&'static str],
    /// 需要单文件拷贝的游戏相对路径（与 `directories` 互不重叠）。
    files: &'static [&'static str],
}

/// BepInEx 框架：`BepInEx/` 整棵树 + 游戏根目录的加载器文件。
///
/// 根目录那三个文件是关键：`winhttp.dll` 是实际的加载开关，`doorstop_config.ini`
/// 与 `run_bepinex.sh` 是 doorstop 的配置。早期备份只拷 `BepInEx/`，恢复后
/// 框架不完整 —— 判定「已安装」看的是 `BepInEx/core`，但 `winhttp.dll` 缺失
/// 会让框架显示为未启用，用户以为恢复失败。
const BEPINEX_BACKUP: FrameworkBackupSpec = FrameworkBackupSpec {
    prefix: "BepInEx.backup-",
    directories: &["BepInEx"],
    files: &[
        "winhttp.dll",
        "winhttp.dll.disabled",
        "doorstop_config.ini",
        "run_bepinex.sh",
        "libdoorstop.so",
        "libdoorstop.dylib",
        "doorstop_libs/libdoorstop_x64.so",
        "doorstop_libs/libdoorstop_x64.dylib",
    ],
};

/// DIYLevel 框架：只有 `BepInEx/plugins/OC2DIYLevel` 这一个目录。
///
/// 它的清单 ID 是动态的（仓库 id 或 `manual-<uuid>`），所以不在 spec 里写死，
/// 由调用方按实际扫描到的 `package_id` 传入。
const DIYLEVEL_BACKUP: FrameworkBackupSpec = FrameworkBackupSpec {
    prefix: DIYLEVEL_BACKUP_PREFIX,
    directories: &[DIYLEVEL_DIR],
    files: &[],
};

/// 备份一个框架：目录树 + 根文件 + 清单快照。
///
/// 返回备份目录路径。清单快照写在备份根部的 `framework-manifest.toml`，
/// 恢复时由 `restore_framework_backup` 写回管理器清单目录。
fn backup_framework(
    game_root: &Path,
    backup_root: &Path,
    spec: &FrameworkBackupSpec,
    manifests_dir: &Path,
    manifest_id: &str,
) -> Result<PathBuf, String> {
    for directory in spec.directories {
        ensure_copy_paths_disjoint(&game_root.join(directory), backup_root)?;
    }
    fs::create_dir_all(backup_root).map_err(|e| e.to_string())?;
    let target = backup_root.join(format!(
        "{}{}-{}",
        spec.prefix,
        Utc::now().format("%Y%m%d%H%M%S"),
        Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;

    // 目录树：按原相对路径放回备份里，恢复时原样拷回游戏目录
    for directory in spec.directories {
        copy_dir(&game_root.join(directory), &target.join(directory))?;
    }
    // 根目录的散文件：存在才拷，缺失不算错误（比如 macOS 没有 winhttp.dll）
    for relative in spec.files {
        let source = game_root.join(relative);
        if !source.is_file() {
            continue;
        }
        let destination = target.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&source, &destination).map_err(|e| e.to_string())?;
    }

    /*
     * 清单快照。
     *
     * 有现成清单就原样存一份（保留原始安装信息，恢复后卸载行为与安装时
     * 完全一致）。没有清单（手工装的框架）就按当前实际文件现造一份 ——
     * 这样恢复后依然能正常卸载，不会落回「拒绝删除未知文件」。
     */
    let manifest = archive::read_manifest(manifests_dir, manifest_id).unwrap_or_else(|| {
        let files = framework_files_on_disk(game_root, spec);
        archive::new_manifest(
            manifest_id,
            crate::bepinex::PACKAGE_VERSION,
            archive::single_language("DIYLevel"),
            Default::default(),
            files,
            Vec::new(),
        )
    });
    write_backup_manifest(&target, &manifest)?;
    log::info!(
        "框架备份完成：前缀={}，路径={}，清单ID={}，清单文件数={}",
        spec.prefix,
        target.display(),
        manifest_id,
        manifest.files.len()
    );
    Ok(target)
}

/// 把清单写进备份目录根部（与 `archive::write_manifest` 同样的原子写法）。
fn write_backup_manifest(backup: &Path, manifest: &archive::InstallManifest) -> Result<(), String> {
    let text = toml::to_string_pretty(manifest).map_err(|e| e.to_string())?;
    let path = backup.join(FRAMEWORK_BACKUP_MANIFEST);
    let temp = backup.join(format!("{FRAMEWORK_BACKUP_MANIFEST}.tmp"));
    fs::write(&temp, text).map_err(|e| e.to_string())?;
    fs::rename(&temp, &path).map_err(|e| e.to_string())?;
    Ok(())
}

/// 扫描一个框架在磁盘上实际存在的全部文件（游戏相对路径）。
///
/// 只在「没有现成安装清单」时用来现造清单：手工安装的框架没有清单，
/// 备份/恢复后必须能再次卸载，否则会落回「拒绝删除未知文件」。
fn framework_files_on_disk(game_root: &Path, spec: &FrameworkBackupSpec) -> Vec<String> {
    let mut files = Vec::new();
    for directory in spec.directories {
        for entry in walkdir::WalkDir::new(game_root.join(directory))
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
        {
            if let Ok(relative) = entry.path().strip_prefix(game_root) {
                files.push(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    for relative in spec.files {
        if game_root.join(relative).is_file() {
            files.push((*relative).to_string());
        }
    }
    files
}

/// 读取备份里的清单快照；旧备份没有这个文件时返回 `None`。
fn read_backup_manifest(backup: &Path) -> Option<archive::InstallManifest> {
    let text = fs::read_to_string(backup.join(FRAMEWORK_BACKUP_MANIFEST)).ok()?;
    toml::from_str(&text).ok()
}

/// 恢复一个框架备份：先删现有内容，再整体还原，最后写回清单。
///
/// 与用户要求一致：**先删再恢复**，得到的是备份那一刻的完整快照。
///
/// ## 为什么 `*.before-restore-*` 必须删掉
///
/// 删除前会把现有内容另存一份，失败时用来回滚。但恢复成功后这个目录必须
/// 立刻删除：它就在游戏根目录下（`BepInEx.before-restore-<ts>`、
/// `OC2DIYLevel.before-restore-<ts>`），MOD 扫描会把里面的 DLL 当成插件，
/// 于是用户看到「刚恢复完就冒出一堆重复/低版本 MOD」，甚至出现同 GUID 冲突。
///
/// 也就是说它不是「给用户后悔用的备份」，只是**本次操作的回滚素材**：
/// 真正的后悔入口是备份列表里的快照。所以成功即删，失败才用。
fn restore_framework_backup(
    game_root: &Path,
    backup: &Path,
    spec: &FrameworkBackupSpec,
    manifests_dir: &Path,
    manifest_id: &str,
) -> Result<(), String> {
    if !backup.is_dir() {
        return Err("backup.notFound".into());
    }
    // 恢复前的另存目录：放在备份根的同级，避开被还原的目录树
    let stamp = Utc::now().timestamp();
    let mut staged = Vec::new();
    for directory in spec.directories {
        let destination = game_root.join(directory);
        ensure_copy_paths_disjoint(&backup.join(directory), &destination)?;
        if !destination.exists() {
            continue;
        }
        let previous = game_root.join(format!("{directory}.before-restore-{stamp}"));
        archive::clear_readonly(&destination)?;
        copy_dir(&destination, &previous)?;
        staged.push(previous.clone());
        fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
        log::info!("恢复前已另存：路径={}", previous.display());
    }
    for relative in spec.files {
        let destination = game_root.join(relative);
        if destination.exists() {
            let _ = fs::remove_file(&destination);
        }
    }

    // 整体还原
    for directory in spec.directories {
        let source = backup.join(directory);
        if source.is_dir() {
            copy_dir(&source, &game_root.join(directory))?;
        }
    }
    for relative in spec.files {
        let source = backup.join(relative);
        if !source.is_file() {
            continue;
        }
        let destination = game_root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(&source, &destination).map_err(|e| e.to_string())?;
    }

    /*
     * 还原完成，回滚素材用完即弃。
     *
     * 删不掉不算恢复失败（目录已经正确还原了），但必须留日志并如实告知：
     * 残留目录会被 MOD 扫描当成插件，用户需要自己删。
     */
    let mut leftovers = Vec::new();
    for previous in staged {
        archive::clear_readonly(&previous)?;
        match fs::remove_dir_all(&previous) {
            Ok(()) => log::info!("已清理恢复前暂存：路径={}", previous.display()),
            Err(error) => {
                log::warn!(
                    "恢复前暂存清理失败：路径={}，原因={}",
                    previous.display(),
                    error
                );
                leftovers.push(previous.display().to_string());
            }
        }
    }
    if !leftovers.is_empty() {
        return Err(crate::locale_tag::with_params(
            "backup.restoreLeftoverCleanupFailed",
            &[("paths", &leftovers.join(", "))],
        ));
    }

    /*
     * 写回安装清单。
     *
     * 这是「恢复后还能正常卸载」的关键：卸载依赖清单来判断哪些文件属于
     * 框架。旧备份没有清单快照时，退回按当前实际文件现造一份 —— 至少保证
     * 不会再报「没有安装清单，拒绝删除未知文件」。
     */
    let manifest = read_backup_manifest(backup).unwrap_or_else(|| {
        let files = framework_files_on_disk(game_root, spec);
        archive::new_manifest(
            manifest_id,
            crate::bepinex::PACKAGE_VERSION,
            archive::single_language("DIYLevel"),
            Default::default(),
            files,
            Vec::new(),
        )
    });
    archive::write_manifest(manifests_dir, &manifest)?;
    log::info!(
        "框架备份恢复完成：前缀={}，路径={}，清单文件数={}",
        spec.prefix,
        game_root.display(),
        manifest.files.len()
    );
    Ok(())
}

/// 校验一个路径确实是本管理器创建的某个框架备份目录。
fn validate_framework_backup_in(
    backup_path: &Path,
    backup_root: &Path,
    spec: &FrameworkBackupSpec,
) -> Result<PathBuf, String> {
    let target = fs::canonicalize(backup_path).map_err(|_| "backup.notFound".to_string())?;
    let root = fs::canonicalize(backup_root).map_err(|_| "backup.unavailable".to_string())?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !name.starts_with(spec.prefix) || !target.is_dir() {
        return Err("backup.wrongKind".into());
    }
    if target.parent() != Some(root.as_path()) {
        return Err("backup.outsideRoot".into());
    }
    Ok(target)
}

fn backup_bepinex_dir(game_root: &Path, backup_root: &Path) -> Result<PathBuf, String> {
    /*
     * 只检查「框架目录」与「备份目录」互不包含，这才是递归复制的来源。
     *
     * 这里曾经还有一段「管理器安装目录不能与备份目录重叠」的自检：当年数据根
     * 放在 `install_dir()/OC2ModManager`，备份目录必然是管理器安装目录的子目录，
     * 这个判断恒真，结果所有备份与卸载都被它拦死，用户只看到「请把管理器安装
     * 到游戏目录外」这种无从处理的提示 —— 而管理器本来就不在游戏目录里。
     *
     * 现在数据根在用户文档目录，除非把管理器装进 BepInEx（安装校验已经拒绝），
     * 备份源与备份目标天然不相交。保留这条检查纯属防御：将来的调用方如果把
     * 备份目标指到游戏目录内，这里仍然会拦住递归复制。
     */
    ensure_copy_paths_disjoint(&game_root.join("BepInEx"), backup_root)?;
    let target = bepinex_backup_path(backup_root)?;
    copy_dir(&game_root.join("BepInEx"), &target)?;
    Ok(target)
}

fn repository_install_identity(
    repo: &RepositoryConfig,
    config: &AppConfig,
    manifests_dir: &Path,
) -> Result<RepositoryConfig, String> {
    if manifests_dir
        .join(format!("{}.toml", repo.id))
        .try_exists()
        .map_err(|error| error.to_string())?
    {
        return Ok(repo.clone());
    }
    // Keep the installed package identity when an old source vanished. Never
    // merge another still-configured package merely because its coordinates match.
    let matches = archive::read_manifests(manifests_dir)
        .into_iter()
        .filter(|manifest| {
            !config
                .repositories
                .iter()
                .any(|configured| configured.id == manifest.repository_id)
                && manifest
                    .repository_binding
                    .as_ref()
                    .is_some_and(|binding| binding.is_valid() && binding.matches_repository(repo))
        })
        .collect::<Vec<_>>();
    if matches.len() > 1 {
        return Err("mods.multipleOldPackages".into());
    }
    let mut result = repo.clone();
    if let Some(manifest) = matches.first() {
        result.id = manifest.repository_id.clone();
    }
    Ok(result)
}

/// 从预览暂存安装一个仓库资产。
///
/// 预览阶段已经把文件下载并解析过了，这里只做最后一跳：按仓库身份回收
/// 该仓库的全部旧包，再解压/复制新文件。必须走这条路径而不是
/// `install_manual_mod` —— 后者每次生成新的 `manual-<uuid>` 清单，
/// 同仓库的旧包不会被回收，装几次就有几份副本。
pub(crate) fn install_staged_repository(
    state: &AppState,
    staged_file: &Path,
    repository_id: &str,
    release_tag: &str,
    prerelease: bool,
) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("game.notFound")?);
    game::ensure_game_stopped(&game_root)?;

    let repo = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("source.repositoryMissing")?
        .clone();

    let download_name = staged_file
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("mods.invalidFileName")?;

    let manifests_dir = state.config.manifests_dir();
    // 沿用仓库安装的身份解析：源可能已从配置消失，但旧清单仍要能被找到
    let install_repo = repository_install_identity(&repo, &config, &manifests_dir)?;

    let file_count = install_downloaded_repository(
        staged_file,
        download_name,
        &game_root,
        &manifests_dir,
        &state.config.mod_recycle_bin_dir(),
        &install_repo,
        release_tag,
        prerelease,
        config.recycle_deleted_mods,
    )?;

    log::info!(
        "从预览安装仓库资产完成：仓库={}，版本={}，文件数={}",
        repo.name,
        release_tag,
        file_count
    );
    Ok(OperationResult {
        ok: true,
        message: if release_tag.is_empty() {
            crate::locale_tag::with_params(
                "mods.installCompletedSimple",
                &[("name", &repo.name.to_string())],
            )
        } else {
            crate::locale_tag::with_params(
                "mods.installCompletedWithVersion",
                &[
                    ("name", &repo.name.to_string()),
                    ("version", &release_tag.to_string()),
                ],
            )
        },
    })
}

fn install_downloaded_repository(
    asset_path: &Path,
    download_name: &str,
    game_root: &Path,
    manifests_dir: &Path,
    recycle_root: &Path,
    repo: &RepositoryConfig,
    release_tag: &str,
    prerelease: bool,
    // 安装成功后是否保留旧包的回收记录（= 用户开启了「优先放入回收站」）。
    keep_recycled: bool,
) -> Result<usize, String> {
    let plan = if download_name.to_ascii_lowercase().ends_with(".zip") {
        Some(archive::prepare_repository_mod_zip(
            asset_path,
            game_root,
            &repo.install_target,
            download_name,
        )?)
    } else {
        game::safe_game_path(
            game_root,
            &Path::new(&repo.install_target)
                .join(download_name)
                .to_string_lossy(),
        )?;
        None
    };
    let replacement_files = if let Some(plan) = &plan {
        archive::prepared_mod_zip_files(asset_path, game_root, plan)?
    } else {
        vec![
            Path::new(&repo.install_target)
                .join(download_name)
                .to_string_lossy()
                .replace('\\', "/"),
        ]
    };
    /*
     * 安装前回收**所有**归属该仓库的旧包，而不只是 repository_id 完全相等的
     * 那一个。同一仓库历史上可能装出过多个包（换过包 ID、旧清单未清理），
     * 只回收其中一个会让装完仍然显示「重复」。用户要的语义是：
     * 更新一次就把同仓库的旧装全部清掉，再装新的。
     *
     * 回收失败必须中止安装并给出可操作的提示：此时旧包可能只移走了一部分，
     * 继续安装会让两个版本的文件混在一起。让用户手动删除是最安全的出路。
     *
     * 即使配置了「直接删除」，这里也**先走回收**：安装过程可能失败，
     * 失败时必须能把旧包原样恢复（见下面的回滚）。回收记录在安装成功后
     * 才被清掉 —— 那时暂存区里的旧文件确实不再需要了。
     */
    let recycled = recycle_bin::recycle_repository_packages_before_install(
        recycle_root,
        manifests_dir,
        game_root,
        repo,
        &replacement_files,
    )
    .map_err(|error| {
        log::error!(
            "安装前回收旧 MOD 失败，已中止安装：仓库={}，原因={}",
            repo.id,
            error
        );
        crate::locale_tag::with_params("mods.recycleFailed", &[("reason", &error.to_string())])
    })?;
    if recycled.is_empty() {
        log::info!("安装前没有需要回收的同仓库旧包：仓库={}", repo.id);
    }
    let installation: Result<usize, String> = (|| {
        let pending = if let Some(plan) = &plan {
            archive::extract_prepared_mod_zip(asset_path, game_root, plan)?
        } else {
            archive::copy_asset_pending(asset_path, game_root, &repo.install_target, download_name)?
        };
        /*
         * 包名与备注都只存多语言快照，格式 `{仓库名} - {平台}`。
         *
         * 仓库的规范形式本来就是 `name_i18n` / `note_i18n`（源文档里
         * `name`/`note` 是可选兜底，`normalize_repository_i18n` 会把
         * zh-CN 回填到它们）。清单再存一份单语副本属于冗余双写，
         * 两者迟早不一致。所以这里直接把 i18n 交给 `new_manifest`。
         */
        let package_name_i18n = repo
            .name_i18n
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| {
                (
                    key.clone(),
                    archive::package_display_name(value, &repo.provider),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let note_i18n = repo
            .note_i18n
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<BTreeMap<_, _>>();
        let mut manifest = archive::new_manifest(
            &repo.id,
            release_tag,
            package_name_i18n,
            note_i18n,
            pending.files().to_vec(),
            crate::mods::effective_relations(repo, Some(release_tag)),
        );
        manifest.repository_binding = RepositoryBinding::from_release(repo, release_tag);
        // 固化安装时的分组：之后即使源文档还没刷新出来（刚启动），
        // MOD 管理页也能按当时的分组稳定归类。
        manifest.group_i18n = repo.group_i18n.clone();
        // 预发布标记跟着版本号一起固化：它只存在于当时的 Release 元数据里
        manifest.prerelease = prerelease;
        archive::write_manifest(manifests_dir, &manifest)?;
        Ok(pending.commit().len())
    })();
    if let Err(error) = &installation {
        // 安装失败：把本次回收的所有同仓库旧包按逆序恢复。
        // 逆序是为了让「后回收的先恢复」，与原清单的提交顺序相反，避免中间态冲突。
        for entry in recycled.iter().rev() {
            if let Err(restore_error) =
                recycle_bin::restore(recycle_root, manifests_dir, game_root, &entry.id)
            {
                log::error!(
                    "安装失败且旧 MOD 包恢复失败，保留回收记录：仓库={}，记录={}，安装错误={}，恢复错误={}",
                    repo.id,
                    entry.id,
                    error,
                    restore_error
                );
                return Err("recycle.partialFailure".into());
            }
        }
    } else if !keep_recycled {
        /*
         * 安装成功且用户关闭了回收站：旧包已确认不再需要，现在才清理暂存记录。
         * 放在成功分支里而不是回收时立即删，是因为上面那段回滚依赖这些记录；
         * 提前清掉会让「安装失败 → 旧包也找不回来」变成数据丢失。
         */
        for entry in &recycled {
            if let Err(error) = recycle_bin::delete(recycle_root, &entry.id) {
                // 清理失败只影响回收站里多出一条记录，不影响本次安装结果
                log::warn!(
                    "覆盖安装成功后清理旧包暂存记录失败：仓库={}，记录={}，原因={}",
                    repo.id,
                    entry.id,
                    error
                );
            }
        }
    }
    installation.map_err(|error| {
        crate::locale_tag::with_params(
            "mods.installFailedRolledBack",
            &[("reason", &error.to_string())],
        )
    })
}

fn bepinex_backup_path(backup_root: &Path) -> Result<PathBuf, String> {
    fs::create_dir_all(backup_root).map_err(|e| e.to_string())?;
    Ok(backup_root.join(format!(
        "BepInEx.backup-{}-{}",
        Utc::now().format("%Y%m%d%H%M%S"),
        Uuid::new_v4().simple()
    )))
}

fn restore_preserved_dirs(backup: &Path, game_root: &Path) -> Result<(), String> {
    for name in ["plugins", "config"] {
        let source = backup.join(name);
        let destination = game_root.join("BepInEx").join(name);
        if source.exists() {
            copy_dir(&source, &destination)?;
        }
    }
    Ok(())
}

/// 把整个 `BepInEx/` 换成备份内容（安装失败时的回滚）。
///
/// 与 `restore_framework_backup` 一样，回滚素材用完即删：`BepInEx.before-restore-*`
/// 留在游戏根目录下会被 MOD 扫描当成插件，表现为「刚回滚完就多出一批重复 MOD」。
fn restore_backup_dir(backup: &Path, game_root: &Path) -> Result<(), String> {
    if !backup.is_dir() {
        return Err("backup.notFound".into());
    }
    let destination = game_root.join("BepInEx");
    ensure_copy_paths_disjoint(backup, &destination)?;
    let mut previous = None;
    if destination.exists() {
        let staged = game_root.join(format!("BepInEx.before-restore-{}", Utc::now().timestamp()));
        archive::clear_readonly(&destination)?;
        copy_dir(&destination, &staged)?;
        previous = Some(staged);
        fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
    }
    copy_dir(backup, &destination)?;

    // 删除失败不影响回滚结果，但必须留痕：残留目录会污染 MOD 扫描
    if let Some(staged) = previous {
        archive::clear_readonly(&staged)?;
        match fs::remove_dir_all(&staged) {
            Ok(()) => log::info!("已清理回滚前暂存：路径={}", staged.display()),
            Err(error) => log::warn!(
                "回滚前暂存清理失败：路径={}，原因={}",
                staged.display(),
                error
            ),
        }
    }
    Ok(())
}

/// 备份与源目录必须互不包含。
///
/// 两个目录互相包含时，复制会把自己正在写入的内容再复制进去，
/// 表现为无限递归、磁盘被瞬间填满。数据目录改为跟随管理器安装目录后，
/// 这个风险变大：只要管理器装在游戏目录内，备份目录就可能落在
/// `BepInEx` 里面（或反之）。
///
/// 检查用「最近存在的祖先」解析，因此目标目录尚未创建时也能识别出
/// 目录联接（junction）造成的逻辑包含关系。
fn ensure_copy_paths_disjoint(source: &Path, destination: &Path) -> Result<(), String> {
    let source =
        game::resolve_existing_ancestor(&std::path::absolute(source).map_err(|e| e.to_string())?)?;
    let destination = game::resolve_existing_ancestor(
        &std::path::absolute(destination).map_err(|e| e.to_string())?,
    )?;
    #[cfg(windows)]
    let (source, destination) = (
        PathBuf::from(source.to_string_lossy().to_lowercase()),
        PathBuf::from(destination.to_string_lossy().to_lowercase()),
    );
    if source == destination {
        return Err("backup.sameDirectory".into());
    }
    if source.starts_with(&destination) || destination.starts_with(&source) {
        return Err("backup.overlap".into());
    }
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(crate::locale_tag::with_params(
            "mods.directoryMissing",
            &[("path", &source.display().to_string())],
        ));
    }
    ensure_copy_paths_disjoint(source, destination)?;
    fs::create_dir_all(destination).map_err(|e| e.to_string())?;
    for entry in walkdir::WalkDir::new(source)
        .into_iter()
        .filter_map(Result::ok)
    {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|e| e.to_string())?;
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(entry.path(), target).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod backup_safety_tests {
    use super::*;

    struct Workspace(PathBuf);

    impl Workspace {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("oc2-backup-safety-{}", Uuid::new_v4()));
            fs::create_dir_all(root.join("game/BepInEx/plugins")).unwrap();
            fs::write(root.join("game/BepInEx/plugins/mod.dll"), b"original").unwrap();
            Self(root)
        }

        fn game(&self) -> PathBuf {
            self.0.join("game")
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn rejects_nested_and_same_directory_before_creating_a_backup() {
        let workspace = Workspace::new();
        let framework = workspace.game().join("BepInEx");
        let backup_root = framework.join("plugins/OC2ModManager/backups");
        assert!(backup_bepinex_dir(&workspace.game(), &backup_root).is_err());
        assert!(!backup_root.exists());
        assert!(copy_dir(&framework, &framework).is_err());
        assert!(copy_dir(&framework, &workspace.game()).is_err());
        assert_eq!(
            fs::read(framework.join("plugins/mod.dll")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn permits_sibling_destination_and_preserves_source_files() {
        let workspace = Workspace::new();
        let backup_root = workspace.game().join("BepInEx-backups");
        let backup = backup_bepinex_dir(&workspace.game(), &backup_root).unwrap();
        assert_eq!(
            fs::read(backup.join("plugins/mod.dll")).unwrap(),
            b"original"
        );
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/plugins/mod.dll")).unwrap(),
            b"original"
        );
    }

    #[test]
    fn rejects_nested_restore_before_removing_the_existing_framework() {
        let workspace = Workspace::new();
        let nested_backup = workspace.game().join("BepInEx/plugins/Manager/backups/old");
        fs::create_dir_all(&nested_backup).unwrap();
        fs::write(nested_backup.join("snapshot.txt"), b"backup").unwrap();
        assert!(restore_backup_dir(&nested_backup, &workspace.game()).is_err());
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/plugins/mod.dll")).unwrap(),
            b"original"
        );
        assert_eq!(
            fs::read(nested_backup.join("snapshot.txt")).unwrap(),
            b"backup"
        );
    }

    #[cfg(windows)]
    #[test]
    fn rejects_a_junction_into_the_source_before_creating_missing_directories() {
        let workspace = Workspace::new();
        let alias = workspace.0.join("backup-alias");
        let junction = std::process::Command::new("cmd")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&alias)
            .arg(
                workspace
                    .game()
                    .join("BepInEx/plugins")
                    .to_string_lossy()
                    .replace('/', "\\"),
            )
            .output()
            .unwrap();
        assert!(
            junction.status.success(),
            "{}",
            String::from_utf8_lossy(&junction.stderr)
        );
        let destination = alias.join("new/backups");
        assert!(backup_bepinex_dir(&workspace.game(), &destination).is_err());
        assert!(!destination.exists());
        fs::remove_dir(alias).unwrap();
    }
}

#[cfg(test)]
mod framework_install_tests {
    use super::*;
    use crate::bepinex::Package;
    use std::io::Write;

    struct Workspace(PathBuf);

    fn verification_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/framework-install")
    }

    impl Workspace {
        fn new() -> Self {
            let root = verification_root().join(Uuid::new_v4().to_string());
            fs::create_dir_all(root.join("game")).unwrap();
            Self(root)
        }

        fn game(&self) -> PathBuf {
            self.0.join("game")
        }

        fn backups(&self) -> PathBuf {
            self.0.join("backups")
        }

        /// 框架安装清单目录（安装时会写入 `bepinex-framework.toml`）。
        fn manifests(&self) -> PathBuf {
            self.0.join("manifests")
        }

        fn write(&self, relative: &str, bytes: &[u8]) {
            let path = self.game().join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, bytes).unwrap();
        }

        fn archive(&self, package: Package, script: Option<&str>) -> PathBuf {
            let path = self.0.join("framework.zip");
            let mut archive = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            for (name, bytes) in [
                ("BepInEx/core/BepInEx.dll", b"new core".as_slice()),
                (
                    match package {
                        Package::Windows => "winhttp.dll",
                        Package::MacOS => "libdoorstop.dylib",
                    },
                    b"new loader".as_slice(),
                ),
            ] {
                archive
                    .start_file(name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                archive.write_all(bytes).unwrap();
            }
            if let Some(script) = script {
                archive
                    .start_file("run_bepinex.sh", zip::write::SimpleFileOptions::default())
                    .unwrap();
                archive.write_all(script.as_bytes()).unwrap();
            }
            archive.finish().unwrap();
            path
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let root = fs::canonicalize(&self.0).unwrap();
            assert!(root.starts_with(fs::canonicalize(verification_root()).unwrap()));
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const SCRIPT: &str = "#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\nexport DYLD_LIBRARY_PATH=\"${doorstop_directory}:${DYLD_LIBRARY_PATH}\"\nif [ -z \"$DYLD_INSERT_LIBRARIES\" ]; then\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}\"\nelse\n    export DYLD_INSERT_LIBRARIES=\"${doorstop_name}:${DYLD_INSERT_LIBRARIES}\"\nfi\n";

    #[test]
    fn fresh_install_and_reinstall_preserve_plugins_and_config_for_all_platform_packages() {
        for package in [Package::Windows, Package::MacOS] {
            let workspace = Workspace::new();
            let archive = workspace.archive(package, package.native().then_some(SCRIPT));
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                &workspace.manifests(),
                package,
                false,
                "5.4.23.5",
            )
            .unwrap();
            assert!(!workspace.backups().exists());
            workspace.write("BepInEx/plugins/mod.dll.disabled", b"keep disabled plugin");
            workspace.write("BepInEx/config/BepInEx.cfg", b"keep game config");
            workspace.write("BepInEx/core/BepInEx.dll", b"old core");
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                &workspace.manifests(),
                package,
                true,
                "5.4.23.5",
            )
            .unwrap();
            assert_eq!(
                fs::read(workspace.game().join("BepInEx/core/BepInEx.dll")).unwrap(),
                b"new core"
            );
            assert_eq!(
                fs::read(workspace.game().join("BepInEx/plugins/mod.dll.disabled")).unwrap(),
                b"keep disabled plugin"
            );
            assert_eq!(
                fs::read(workspace.game().join("BepInEx/config/BepInEx.cfg")).unwrap(),
                b"keep game config"
            );
            if package.native() {
                assert!(
                    fs::read_to_string(workspace.game().join("run_bepinex.sh"))
                        .unwrap()
                        .contains(&format!("executable_name=\"{}\"", package.executable()))
                );
            }
            assert_eq!(fs::read_dir(workspace.backups()).unwrap().count(), 1);
        }
    }

    /*
     * 危险的脚本内容必须让安装整体回滚，且原有根文件保持不动。
     *
     * 用 macOS 包：Linux 已不在发布范围内，但它原本是这条测试使用的对象
     * （`libdoorstop.so`），这里换成 macOS 的 `libdoorstop.dylib`，
     * 断言的内容完全等价。
     */
    #[test]
    fn malformed_native_scripts_roll_back_the_framework_and_existing_root_files() {
        for script in [None, Some("executable_name=\"\"\ninvalid enabled field\n")] {
            for reinstall in [false, true] {
                let workspace = Workspace::new();
                workspace.write("run_bepinex.sh", b"original script");
                workspace.write("libdoorstop.dylib", b"original loader");
                if reinstall {
                    workspace.write("BepInEx/core/BepInEx.dll", b"original core");
                    workspace.write("BepInEx/plugins/mod.dll", b"original plugin");
                    workspace.write("BepInEx/config/BepInEx.cfg", b"original config");
                }
                let archive = workspace.archive(Package::MacOS, script);
                assert!(
                    install_framework_archive(
                        &archive,
                        &workspace.game(),
                        &workspace.backups(),
                        &workspace.manifests(),
                        Package::MacOS,
                        reinstall,
                        "5.4.23.5",
                    )
                    .is_err()
                );
                assert_eq!(
                    fs::read(workspace.game().join("run_bepinex.sh")).unwrap(),
                    b"original script"
                );
                assert_eq!(
                    fs::read(workspace.game().join("libdoorstop.dylib")).unwrap(),
                    b"original loader"
                );
                if reinstall {
                    for (file, bytes) in [
                        ("core/BepInEx.dll", b"original core".as_slice()),
                        ("plugins/mod.dll", b"original plugin".as_slice()),
                        ("config/BepInEx.cfg", b"original config".as_slice()),
                    ] {
                        assert_eq!(
                            fs::read(workspace.game().join("BepInEx").join(file)).unwrap(),
                            bytes
                        );
                    }
                } else {
                    assert!(!workspace.game().join("BepInEx").exists());
                }
            }
        }
    }

    #[test]
    fn unrecognized_macos_overlay_setup_rolls_back_fresh_install_and_reinstall() {
        for reinstall in [false, true] {
            let workspace = Workspace::new();
            workspace.write("run_bepinex.sh", b"original script");
            workspace.write("libdoorstop.dylib", b"original loader");
            if reinstall {
                workspace.write("BepInEx/core/BepInEx.dll", b"original core");
                workspace.write("BepInEx/plugins/mod.dll", b"original plugin");
            }
            let archive = workspace.archive(
                Package::MacOS,
                Some("#!/bin/sh\nexecutable_name=\"\"\nenabled=\"1\"\n"),
            );
            assert!(
                install_framework_archive(
                    &archive,
                    &workspace.game(),
                    &workspace.backups(),
                    &workspace.manifests(),
                    Package::MacOS,
                    reinstall,
                    "5.4.23.5",
                )
                .is_err()
            );
            assert_eq!(
                fs::read(workspace.game().join("run_bepinex.sh")).unwrap(),
                b"original script"
            );
            assert_eq!(
                fs::read(workspace.game().join("libdoorstop.dylib")).unwrap(),
                b"original loader"
            );
            if reinstall {
                assert_eq!(
                    fs::read(workspace.game().join("BepInEx/core/BepInEx.dll")).unwrap(),
                    b"original core"
                );
                assert_eq!(
                    fs::read(workspace.game().join("BepInEx/plugins/mod.dll")).unwrap(),
                    b"original plugin"
                );
            } else {
                assert!(!workspace.game().join("BepInEx").exists());
            }
        }
    }

    #[test]
    fn reinstall_must_be_explicit_before_backup_or_file_changes() {
        let workspace = Workspace::new();
        workspace.write("BepInEx/core/BepInEx.dll", b"original core");
        let archive = workspace.archive(Package::Windows, None);
        assert!(
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                &workspace.manifests(),
                Package::Windows,
                false,
                "5.4.23.5",
            )
            .is_err()
        );
        assert!(!workspace.backups().exists());
        assert!(!workspace.game().join("winhttp.dll").exists());
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/core/BepInEx.dll")).unwrap(),
            b"original core"
        );
    }

    #[test]
    #[ignore = "Requires all three original packages in OC2_BEPINEX_PACKAGE_DIR"]
    fn installs_the_actual_platform_archives_with_digest_and_preservation_checks() {
        let packages = PathBuf::from(
            std::env::var_os("OC2_BEPINEX_PACKAGE_DIR")
                .expect("Set OC2_BEPINEX_PACKAGE_DIR to the original platform packages"),
        );
        assert!(packages.is_absolute());
        for package in [Package::Windows, Package::MacOS] {
            let name = package.url(&Provider::Github).rsplit('/').next().unwrap();
            let archive = packages.join(name);
            archive::validate_download_file(&archive, None, Some(package.digest())).unwrap();
            let workspace = Workspace::new();
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                &workspace.manifests(),
                package,
                false,
                "5.4.23.5",
            )
            .unwrap();
            let info = game::detect_bepinex(&workspace.game(), Some(&workspace.backups()));
            assert!(info.installed, "{name}");
            workspace.write("BepInEx/plugins/preserved.dll.disabled", b"user MOD");
            workspace.write("BepInEx/config/preserved.cfg", b"user settings");
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                &workspace.manifests(),
                package,
                true,
                "5.4.23.5",
            )
            .unwrap();
            assert_eq!(
                fs::read(
                    workspace
                        .game()
                        .join("BepInEx/plugins/preserved.dll.disabled")
                )
                .unwrap(),
                b"user MOD"
            );
            assert_eq!(
                fs::read(workspace.game().join("BepInEx/config/preserved.cfg")).unwrap(),
                b"user settings"
            );
            if package.native() {
                let script = fs::read_to_string(workspace.game().join("run_bepinex.sh")).unwrap();
                assert!(script.contains(&format!("executable_name=\"{}\"", package.executable())));
                assert!(script.contains("enabled=\"1\""));
                assert_eq!(
                    script
                        .contains("export DYLD_INSERT_LIBRARIES=\"$STEAM_DYLD_INSERT_LIBRARIES\""),
                    package == Package::MacOS
                );
            } else {
                assert!(workspace.game().join("winhttp.dll").is_file());
            }
        }
    }

    /*
     * 只装了 MOD、没装框架的游戏目录：`BepInEx/plugins` 存在但没有 core。
     *
     * 这是最常见的真实场景 —— 用户先手动放了个 MOD，再来装框架。
     * 旧实现把「目录存在」当成已安装，于是首次安装被拒绝，只能点「重新安装」，
     * 而重装会把整个 BepInEx 删掉再恢复。现在必须：
     *   · 判定为「未安装」，首次安装直接放行；
     *   · 不产生备份（没有框架可备）；
     *   · 已有的 plugins 原样保留。
     */
    #[test]
    fn installs_over_mod_only_residue_without_backup_and_keeps_plugins() {
        let workspace = Workspace::new();
        workspace.write("BepInEx/plugins/existing.dll", b"user MOD");
        let archive = workspace.archive(Package::Windows, None);
        install_framework_archive(
            &archive,
            &workspace.game(),
            &workspace.backups(),
            &workspace.manifests(),
            Package::Windows,
            false,
            "5.4.23.5",
        )
        .unwrap();
        assert_eq!(
            fs::read(workspace.game().join("BepInEx/plugins/existing.dll")).unwrap(),
            b"user MOD"
        );
        assert!(
            workspace.game().join("BepInEx/core/BepInEx.dll").is_file(),
            "框架核心应已就位"
        );
        assert!(
            !workspace.backups().exists(),
            "没有已安装框架时不该产生备份"
        );
        let info = game::detect_bepinex(&workspace.game(), Some(&workspace.backups()));
        assert!(info.installed && !info.has_residue);
    }

    /// 目录在、核心不在 = 残留，不是已安装。
    #[test]
    fn mod_only_directory_is_residue_not_an_installed_framework() {
        let workspace = Workspace::new();
        workspace.write("BepInEx/plugins/existing.dll", b"user MOD");
        let info = game::detect_bepinex(&workspace.game(), Some(&workspace.backups()));
        assert!(!info.installed, "只有 plugins 时不应判定为已安装");
        assert!(info.has_residue, "应标记为残留目录");
    }

    /*
     * 旧版本装的框架没有安装清单，卸载时按「框架已知文件」现造清单。
     *
     * 这份清单绝不能包含 plugins / config：那是用户和 MOD 的内容，
     * 点「卸载框架」不该把 MOD 一起带走。
     */
    #[test]
    fn framework_manifest_files_exclude_plugins_and_config() {
        let workspace = Workspace::new();
        workspace.write("BepInEx/core/BepInEx.dll", b"core");
        workspace.write("BepInEx/core/extra.dll", b"core extra");
        workspace.write("BepInEx/plugins/mod.dll", b"user MOD");
        workspace.write("BepInEx/config/BepInEx.cfg", b"user config");
        workspace.write("winhttp.dll", b"loader");
        workspace.write("doorstop_config.ini", b"doorstop");

        let files = framework_manifest_files(&workspace.game());
        assert!(files.contains(&"BepInEx/core/BepInEx.dll".to_string()));
        assert!(files.contains(&"BepInEx/core/extra.dll".to_string()));
        assert!(files.contains(&"winhttp.dll".to_string()));
        assert!(files.contains(&"doorstop_config.ini".to_string()));
        // 注意：`doorstop_config.ini` 名字里也含 "config"，但它是框架自己
        // 解压在游戏根目录的加载器配置，必须回收 —— 所以只能按路径前缀判断。
        assert!(
            !files
                .iter()
                .any(|file| file.starts_with("BepInEx/plugins/")),
            "plugins 属于用户 MOD：{files:?}"
        );
        assert!(
            !files.iter().any(|file| file.starts_with("BepInEx/config/")),
            "config 属于用户配置：{files:?}"
        );
    }

    /*
     * 卸载必须**先整目录备份、再按清单回收**。
     *
     * 这两步职责不同，缺一不可：
     *   · 回收站只带走清单里登记的文件。用户自己往 BepInEx 里手工放的插件、
     *     改过的 config、日志都不在清单里，回收站不管它们；
     *   · 备份是整目录快照，能把上面那些「管理器不知道」的文件一起保住。
     *
     * 一旦顺序颠倒（先回收再备份），回收已经把目录搬空，备份就只剩个空壳。
     * `uninstall_bepinex` 需要 `State`，无法在这里直接调用，所以断言源码顺序 ——
     * 与 `tests/registry.test.mjs` 用源码文本守住接线契约是同一种做法。
     */
    #[test]
    fn uninstall_backs_up_before_recycling() {
        let source = include_str!("commands.rs");
        let uninstall = source
            .split("pub fn uninstall_bepinex")
            .nth(1)
            .expect("uninstall_bepinex 应存在");
        // 只看这个函数体（到下一个顶层 fn 之前）
        let body = uninstall
            .split("\n#[tauri::command]")
            .next()
            .expect("函数体应可截断");

        let backup_at = body
            .find("backup_framework(")
            .expect("卸载流程必须调用 backup_framework");
        let recycle_at = body
            .find("recycle_package(")
            .expect("卸载流程必须调用 recycle_package");
        assert!(
            backup_at < recycle_at,
            "必须先备份再回收：备份偏移={backup_at}，回收偏移={recycle_at}"
        );
    }
}

#[cfg(test)]
mod mod_file_location_tests {
    use super::*;

    struct Workspace {
        root: PathBuf,
        game: PathBuf,
    }

    impl Workspace {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("oc2-mod-location-{}", Uuid::new_v4()));
            let game = root.join("game");
            fs::create_dir_all(&game).unwrap();
            Self { root, game }
        }

        fn file(&self, relative: &str) -> PathBuf {
            let path = self.game.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, b"test DLL").unwrap();
            path
        }
    }

    impl Drop for Workspace {
        fn drop(&mut self) {
            let root = fs::canonicalize(&self.root).unwrap();
            let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
            assert!(root.starts_with(&temp) && root != temp);
            let _ = fs::remove_dir_all(root);
        }
    }

    #[test]
    fn framework_config_uses_game_root_without_plugins_and_preserves_write_guards() {
        let workspace = Workspace::new();
        let state = AppState {
            config: ConfigService::for_test(workspace.root.join("manager")),
            operation: Arc::new(Mutex::new(())),
            repository_cache: Arc::new(tokio::sync::Mutex::new(())),
            community: Arc::new(Default::default()),
            readme: Arc::new(Default::default()),
            game_state: Arc::new(Mutex::new(None)),
        };
        let mut config = state.config.load().unwrap();
        config.game.path = workspace.game.to_string_lossy().into_owned();
        config.game.auto_detect = false;
        state.config.save(&config).unwrap();
        let (game, resolved) = resolve_config_game(&state).unwrap();
        assert_eq!(game, workspace.game);
        assert!(!crate::mod_config::exists(&game, "BepInEx"));
        assert!(crate::mod_config::read(&game, "BepInEx").is_err());
        assert!(!game.join("BepInEx/config").exists());

        let path = workspace.file("BepInEx/config/BepInEx.cfg");
        fs::write(
            &path,
            b"\xef\xbb\xbf[Logging.Console]\r\nEnabled = false\r\n",
        )
        .unwrap();
        assert!(!game.join("BepInEx/plugins").exists());
        let document = crate::mod_config::read(&game, "BepInEx").unwrap();
        assert_eq!(document.file_name, "BepInEx.cfg");
        let saved = save_config_document(
            &state,
            &game,
            "BepInEx",
            &resolved.game.path,
            &document.revision,
            "[Logging.Console]\r\nEnabled = true\r\n",
        )
        .unwrap();
        assert_eq!(
            fs::read(&path).unwrap(),
            b"\xef\xbb\xbf[Logging.Console]\r\nEnabled = true\r\n"
        );
        fs::write(&path, "external change").unwrap();
        assert_eq!(
            save_config_document(
                &state,
                &game,
                "BepInEx",
                &resolved.game.path,
                &saved.revision,
                "overwrite",
            )
            .err()
            .as_deref(),
            Some("modConfig.externallyModified")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "external change");

        config.game.path = workspace
            .root
            .join("other-game")
            .to_string_lossy()
            .into_owned();
        state.config.save(&config).unwrap();
        assert_eq!(
            save_config_document(
                &state,
                &game,
                "BepInEx",
                &resolved.game.path,
                &saved.revision,
                "overwrite",
            )
            .err()
            .as_deref(),
            Some("game.directoryChangedReopenConfig")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "external change");
    }

    #[test]
    fn resolves_nested_enabled_dll_with_spaces_commas_and_unicode() {
        let workspace = Workspace::new();
        let relative = "BepInEx/plugins/测试 插件,包/Sample Plugin,测试.DLL";
        let path = workspace.file(relative);
        let resolved = resolve_mod_file_for_location(&workspace.game, relative).unwrap();
        assert_eq!(resolved, fs::canonicalize(&path).unwrap());
        assert_eq!(fs::read(path).unwrap(), b"test DLL");
    }

    #[test]
    fn resolves_disabled_dll_from_canonical_or_disabled_row_path() {
        let workspace = Workspace::new();
        let path = workspace.file("BepInEx/plugins/Example.dll.disabled");
        for relative in [
            "BepInEx/plugins/Example.dll",
            "BepInEx/plugins/Example.dll.disabled",
        ] {
            assert_eq!(
                resolve_mod_file_for_location(&workspace.game, relative).unwrap(),
                fs::canonicalize(&path).unwrap()
            );
        }
    }

    #[test]
    fn enabled_copy_wins_when_both_variants_exist_like_the_mod_scan() {
        let workspace = Workspace::new();
        let enabled = workspace.file("BepInEx/plugins/Example.dll");
        workspace.file("BepInEx/plugins/Example.dll.disabled");
        assert_eq!(
            resolve_mod_file_for_location(&workspace.game, "BepInEx/plugins/Example.dll").unwrap(),
            fs::canonicalize(enabled).unwrap()
        );
    }

    #[test]
    fn resolves_current_file_after_state_changes_without_rescanning() {
        let workspace = Workspace::new();
        let relative = "BepInEx/plugins/Example.dll";
        let enabled = workspace.file(relative);
        let disabled = enabled.with_extension("dll.disabled");
        fs::rename(&enabled, &disabled).unwrap();
        assert_eq!(
            resolve_mod_file_for_location(&workspace.game, relative).unwrap(),
            fs::canonicalize(&disabled).unwrap()
        );
        fs::rename(&disabled, &enabled).unwrap();
        assert_eq!(
            resolve_mod_file_for_location(&workspace.game, relative).unwrap(),
            fs::canonicalize(&enabled).unwrap()
        );
    }

    #[test]
    fn missing_dll_does_not_create_a_plugins_directory() {
        let workspace = Workspace::new();
        let error = resolve_mod_file_for_location(&workspace.game, "BepInEx/plugins/missing.dll")
            .unwrap_err();
        assert_eq!(error, "mods.dllFileMissing");
        assert!(!workspace.game.join("BepInEx").exists());
    }

    #[test]
    fn rejects_unsafe_paths_non_plugins_files_and_non_dll_files() {
        let workspace = Workspace::new();
        workspace.file("BepInEx/core/Example.dll");
        workspace.file("BepInEx/plugins/README.txt");
        for relative in [
            "",
            "../outside.dll",
            "C:/Games/Example.dll",
            "BepInEx/core/Example.dll",
            "BepInEx/plugins-extra/Example.dll",
            "BepInEx/plugins/../core/Example.dll",
            "BepInEx/plugins/Example.dll:stream.dll",
            "BepInEx/plugins/README.txt",
            "BepInEx/plugins/Example.dll.disabled.disabled",
        ] {
            assert!(
                resolve_mod_file_for_location(&workspace.game, relative).is_err(),
                "{relative}"
            );
        }
    }

    #[test]
    fn rejects_a_directory_named_dll() {
        let workspace = Workspace::new();
        fs::create_dir_all(workspace.game.join("BepInEx/plugins/Directory.dll")).unwrap();
        assert!(
            resolve_mod_file_for_location(&workspace.game, "BepInEx/plugins/Directory.dll")
                .is_err()
        );
    }

    #[cfg(windows)]
    #[test]
    fn explorer_selection_quotes_paths_and_removes_verbatim_prefixes() {
        for (source, expected) in [
            (
                r"C:\Games\厨房 MOD,包\Example DLL.dll",
                r#"/select,"C:\Games\厨房 MOD,包\Example DLL.dll""#,
            ),
            (
                r"\\?\C:\Games\厨房 MOD,包\Example.dll.disabled",
                r#"/select,"C:\Games\厨房 MOD,包\Example.dll.disabled""#,
            ),
            (
                r"\\?\UNC\server\共享 MOD\Example.dll",
                r#"/select,"\\server\共享 MOD\Example.dll""#,
            ),
        ] {
            let command = mod_file_location_command(Path::new(source));
            assert_eq!(command.get_program(), "explorer.exe");
            assert_eq!(command.get_args().collect::<Vec<_>>(), [expected]);
        }
    }
}

#[cfg(test)]
mod proxy_source_read_tests {
    use super::*;
    use crate::github_proxies::{ProxySource, ProxySourceFormat};
    use std::net::TcpListener;

    struct JsRequestLogger(Mutex<Vec<String>>);

    static JS_REQUEST_LOGGER: JsRequestLogger = JsRequestLogger(Mutex::new(Vec::new()));

    impl log::Log for JsRequestLogger {
        fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
            metadata.level() <= log::Level::Info
        }

        fn log(&self, record: &log::Record<'_>) {
            let message = record.args().to_string();
            if message.starts_with("正在请求 GitHub 代理 JS：") {
                self.0.lock().unwrap().push(message);
            }
        }

        fn flush(&self) {}
    }

    #[tokio::test]
    async fn logs_js_request_before_http_response_without_query_secrets() {
        log::set_logger(&JS_REQUEST_LOGGER).unwrap();
        log::set_max_level(log::LevelFilter::Info);
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let safe_address = format!("http://{address}/request-log-test.user.js");
        let expected_address = safe_address.clone();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            // The JS-request entry must already exist while the server has
            // not sent any response headers or body.
            let messages = JS_REQUEST_LOGGER.0.lock().unwrap();
            let message = messages
                .iter()
                .find(|message| message.contains(&expected_address))
                .expect("JS request must be logged before awaiting HTTP response");
            assert!(!message.contains("private-query"));
            assert!(!message.contains("token="));
            drop(messages);
            stream
                .write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                )
                .unwrap();
        });
        let source = ProxySource {
            url: format!("{safe_address}?token=private-query"),
            format: ProxySourceFormat::UserScript,
            bundled: false,
        };
        let error = read_github_proxy_source(&client(), &source)
            .await
            .unwrap_err();
        server.join().unwrap();
        assert_eq!(error.stage, "http");
    }

    fn http_source(status: &str, body: &[u8], declared_length: Option<usize>) -> ProxySource {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            declared_length.unwrap_or(body.len())
        );
        let body = body.to_vec();
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 4096];
            let _ = stream.read(&mut request);
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.write_all(&body);
        });
        ProxySource {
            url: format!("http://{address}/proxies.user.js?token=private-query"),
            format: ProxySourceFormat::UserScript,
            bundled: false,
        }
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(5))
            .no_proxy()
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn reads_proxy_script_and_marks_imported_entries_remote() {
        let script = br#"
var download_url_us = [
    ['https://release.example/https://github.com', 'US', '[US] Release'],
], raw_url = [
    ['https://raw.example/https://raw.githubusercontent.com', 'HK', '[HK] Raw'],
];
        "#;
        let source = http_source("200 OK", script, None);
        let config = read_github_proxy_source(&client(), &source).await.unwrap();
        assert_eq!(config.release_proxies.len(), 1);
        assert_eq!(config.raw_proxies.len(), 1);
        assert!(config.release_proxies[0].remote);
        assert!(config.raw_proxies[0].remote);
    }

    #[tokio::test]
    async fn failures_identify_http_size_utf8_and_parse_stages_without_document_contents() {
        for (status, body, declared_length, expected_stage) in [
            (
                "404 Not Found",
                b"private-response".as_slice(),
                None,
                "http",
            ),
            ("200 OK", b"".as_slice(), Some(4 * 1024 * 1024 + 1), "size"),
            ("200 OK", [0xff, 0xfe].as_slice(), None, "utf8"),
            (
                "200 OK",
                b"[session-private] file not found private-response".as_slice(),
                None,
                "parse",
            ),
        ] {
            let source = http_source(status, body, declared_length);
            let error = read_github_proxy_source(&client(), &source)
                .await
                .unwrap_err();
            assert_eq!(error.stage, expected_stage);
            assert!(!error.reason.contains("private-response"));
            assert!(!error.reason.contains("private-query"));
            assert!(!error.reason.contains("session-private"));
        }
    }

    #[tokio::test]
    async fn empty_script_is_a_parse_failure_and_bundled_manifest_remains_available() {
        let source = http_source("200 OK", b"var download_url_us = [], raw_url = [];", None);
        let error = read_github_proxy_source(&client(), &source)
            .await
            .unwrap_err();
        assert_eq!(error.stage, "parse");
        assert_eq!(error.reason, "proxy.noUsableEntriesJs");
        let bundled = read_github_proxy_source(
            &client(),
            &ProxySource {
                url: "repositories-proxies.toml".into(),
                format: ProxySourceFormat::Toml,
                bundled: true,
            },
        )
        .await
        .unwrap();
        assert!(
            bundled
                .release_proxies
                .iter()
                .any(|proxy| proxy.id == "oc2gh-release")
        );
        assert!(
            bundled
                .raw_proxies
                .iter()
                .any(|proxy| proxy.id == "oc2gh-raw")
        );
    }
}

#[cfg(test)]
mod asset_download_strategy_tests {
    use super::*;

    const DIRECT: &str = "https://github.com/example/mod/releases/download/v1/mod.zip";

    #[test]
    fn shared_candidate_ordering_deduplicates_each_automatic_mode() {
        let proxy_a = "https://proxy-a.example/asset".to_string();
        let proxy_b = "https://proxy-b.example/asset".to_string();
        let duplicated = vec![
            DIRECT.to_string(),
            proxy_a.clone(),
            proxy_a.clone(),
            proxy_b.clone(),
            DIRECT.to_string(),
        ];
        let direct_first =
            plan_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, duplicated.clone())
                .unwrap();
        let proxy_first =
            plan_asset_download_candidates(ProxyMode::AutoProxy, DIRECT, duplicated).unwrap();
        assert_eq!(
            direct_first
                .iter()
                .map(|candidate| candidate.url.as_str())
                .collect::<Vec<_>>(),
            [DIRECT, &proxy_a, &proxy_b]
        );
        assert_eq!(
            proxy_first
                .iter()
                .map(|candidate| candidate.url.as_str())
                .collect::<Vec<_>>(),
            [&proxy_a, &proxy_b, DIRECT]
        );
        assert!(plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, Vec::new()).is_err());
    }

    #[test]
    fn remote_urls_pick_their_own_proxy_group_and_gitee_stays_direct() {
        use crate::direct_mod::RemoteUrlKind;
        let mut config = crate::config::default_config();
        config.github_proxies.release_proxies = vec![GitHubProxy {
            id: "release-only".into(),
            name: "Release".into(),
            template: "https://release.example/{url}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];
        config.github_proxies.raw_proxies = vec![GitHubProxy {
            id: "raw-only".into(),
            name: "Raw".into(),
            template: "https://raw.example/{owner}/{repo}@{branch}/{path}".into(),
            regions: vec![],
            description: String::new(),
            enabled: true,
            builtin: false,
            remote: false,
        }];

        // Release 资产只用 release_proxies，raw 组不能混进来
        let release_url = "https://github.com/acme/mod/releases/download/v1/mod.zip";
        let release = plan_remote_download_candidates(
            &config,
            &Provider::Github,
            RemoteUrlKind::ReleaseAsset,
            release_url,
        )
        .unwrap();
        assert!(
            release
                .iter()
                .any(|candidate| candidate.url.contains("release.example"))
        );
        assert!(
            !release
                .iter()
                .any(|candidate| candidate.url.contains("raw.example"))
        );

        // Raw 文件只用 raw_proxies，且模板里的分支/路径来自 URL 自身
        let raw_url = "https://github.com/acme/mod/raw/main/bin/mod.dll";
        let raw = plan_remote_download_candidates(
            &config,
            &Provider::Github,
            RemoteUrlKind::Raw,
            raw_url,
        )
        .unwrap();
        assert!(
            raw.iter()
                .any(|candidate| candidate.url == "https://raw.example/acme/mod@main/bin/mod.dll")
        );
        assert!(
            !raw.iter()
                .any(|candidate| candidate.url.contains("release.example"))
        );

        // Gitee 没有资产代理：只剩直连候选
        let gitee = plan_remote_download_candidates(
            &config,
            &Provider::Gitee,
            RemoteUrlKind::ReleaseAsset,
            "https://gitee.com/acme/mod/releases/download/v1/mod.zip",
        )
        .unwrap();
        assert_eq!(gitee.len(), 1);
        assert_eq!(gitee[0].route, AssetDownloadRoute::Direct);
    }

    #[test]
    fn connection_progress_identifies_route_without_leaking_asset_or_secrets() {
        let candidate = AssetDownloadCandidate {
            route: AssetDownloadRoute::Proxy,
            url: "https://private-user:private-pass@proxy.example:8443/https://github.com/owner/repo/asset.zip?token=private-token#secret".into(),
        };
        let progress = asset_candidate_progress("示例 MOD", &candidate, 1, 3);
        let event = serde_json::to_value(&progress).unwrap();
        assert_eq!(event["phase"], "connecting");
        assert_eq!(event["route"], "proxy");
        assert_eq!(event["source"], "proxy.example:8443");
        assert_eq!(event["attempt"], 1);
        assert_eq!(event["attempt_total"], 3);
        assert_eq!(progress.downloaded, 0);
        assert_eq!(progress.total, None);
        assert!(!progress.finished);
        for secret in ["private-user", "private-pass", "private-token", "asset.zip"] {
            assert!(!event.to_string().contains(secret));
        }

        let fallback = asset_candidate_progress(
            "示例 MOD",
            &AssetDownloadCandidate {
                route: AssetDownloadRoute::Direct,
                url: DIRECT.into(),
            },
            3,
            3,
        );
        assert_eq!(fallback.phase, archive::DownloadPhase::Switching);
        assert_eq!(fallback.route, Some(archive::DownloadRoute::Direct));
        assert_eq!(fallback.source.as_deref(), Some("github.com"));
        assert_eq!(fallback.downloaded, 0);
    }

    #[tokio::test]
    async fn every_fallback_candidate_can_announce_context_before_waiting() {
        let candidates = plan_asset_download_candidates(
            ProxyMode::AutoProxy,
            DIRECT,
            vec![
                "https://first.example/asset".into(),
                "https://second.example/asset".into(),
            ],
        )
        .unwrap();
        let total = candidates.len();
        let mut events = Vec::new();
        let result: Result<PathBuf, String> = try_asset_download_candidates(
            candidates,
            || false,
            |candidate, _reporter| {
                let progress =
                    asset_candidate_progress("示例 MOD", &candidate, events.len() + 1, total);
                let route = candidate.route;
                events.push(progress);
                async move {
                    // Simulate a request which has yielded without response
                    // bytes. The endpoint announcement already exists.
                    tokio::task::yield_now().await;
                    if route == AssetDownloadRoute::Proxy {
                        Err("连接失败".into())
                    } else {
                        Ok(PathBuf::from("downloaded"))
                    }
                }
            },
            None,
        )
        .await;
        assert!(result.is_ok());
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].phase, archive::DownloadPhase::Connecting);
        assert_eq!(events[1].phase, archive::DownloadPhase::Switching);
        assert_eq!(events[2].route, Some(archive::DownloadRoute::Direct));
        assert_eq!(
            events
                .iter()
                .map(|event| event.source.as_deref())
                .collect::<Vec<_>>(),
            [
                Some("first.example"),
                Some("second.example"),
                Some("github.com")
            ]
        );
    }

    fn proxy(id: &str, host: &str, remote: bool, enabled: bool) -> GitHubProxy {
        GitHubProxy {
            id: id.into(),
            name: id.into(),
            template: format!("https://{host}/{{url}}"),
            regions: Vec::new(),
            description: String::new(),
            enabled,
            builtin: !remote,
            remote,
        }
    }

    #[test]
    fn cached_remote_proxies_survive_restart_for_mods_but_not_self_updates() {
        struct Workspace(PathBuf);
        impl Drop for Workspace {
            fn drop(&mut self) {
                // Delete only this test's unique directory below the resolved
                // temporary root, including when an assertion fails.
                if let (Ok(root), Ok(temp)) =
                    (self.0.canonicalize(), std::env::temp_dir().canonicalize())
                {
                    if root.parent() == Some(temp.as_path())
                        && root.file_name().is_some_and(|name| {
                            name.to_string_lossy().starts_with("oc2-updater-cached-")
                        })
                    {
                        let _ = fs::remove_dir_all(root);
                    }
                }
            }
        }
        let workspace =
            Workspace(std::env::temp_dir().join(format!("oc2-updater-cached-{}", Uuid::new_v4())));
        let service = ConfigService::for_test(workspace.0.clone());
        let mut config = service.load().unwrap();
        let mut bundled_alias = config.github_proxies.release_proxies[0].clone();
        bundled_alias.id = "previous-remote-bundled-alias".into();
        bundled_alias.builtin = false;
        bundled_alias.remote = true;
        let previous_success = GitHubProxyConfig {
            release_proxies: vec![
                proxy("previous-remote", "cached-proxy.example", true, true),
                proxy(
                    "previous-remote-duplicate",
                    "cached-proxy.example",
                    true,
                    true,
                ),
                proxy("previous-remote-disabled", "cached-off.example", true, true),
                bundled_alias,
            ],
            ..GitHubProxyConfig::default()
        };
        // Use the same merge and persistence boundary as successful refresh.
        config.github_proxies =
            crate::github_proxies::merge_downloaded(&config.github_proxies, previous_success);
        for proxy in &mut config.github_proxies.release_proxies {
            if proxy.id == "previous-remote-disabled" || proxy.id == "oc2gh-release" {
                proxy.enabled = false;
            }
        }
        service.save(&config).unwrap();
        drop(service);
        drop(config);

        // A new service simulates startup before any remote refresh occurs.
        let service = ConfigService::for_test(workspace.0.clone());
        let mut restarted = service.load().unwrap();
        assert!(
            restarted
                .github_proxies
                .release_proxies
                .iter()
                .any(|proxy| proxy.id == "previous-remote" && proxy.remote && proxy.enabled)
        );
        let updater = installer_sources_for_url(&Provider::Github, DIRECT)
            .iter()
            .map(installer_candidate)
            .collect::<Vec<_>>();
        let mod_candidates = |proxies: &GitHubProxyConfig| {
            let urls = asset_proxy_urls(
                &proxies.release_proxies,
                DIRECT,
                false,
                "",
                "",
                ProxyMode::AutoDirect,
                None,
            );
            plan_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, urls).unwrap()
        };
        let cached = mod_candidates(&restarted.github_proxies);
        let cached_url = format!("https://cached-proxy.example/{DIRECT}");
        assert_eq!(
            cached
                .iter()
                .filter(|candidate| candidate.url == cached_url)
                .count(),
            1
        );
        assert!(
            cached
                .iter()
                .any(|candidate| candidate.url.starts_with("https://gh.ddlc.top/"))
        );
        assert!(!cached.iter().any(|candidate| {
            candidate.url.starts_with("https://cached-off.example/")
                || candidate.url.starts_with("https://oc2gh.ch3ng.top/")
        }));
        assert_eq!(
            cached.len(),
            cached
                .iter()
                .map(|candidate| &candidate.url)
                .collect::<HashSet<_>>()
                .len()
        );
        assert!(
            !updater
                .iter()
                .any(|candidate| candidate.url.starts_with("https://cached-proxy.example/"))
        );

        // Tutorial completion refreshes and persists a new available snapshot.
        let next_success = GitHubProxyConfig {
            release_proxies: vec![proxy("current-remote", "current-proxy.example", true, true)],
            ..GitHubProxyConfig::default()
        };
        restarted.github_proxies =
            crate::github_proxies::merge_downloaded(&restarted.github_proxies, next_success);
        service.save(&restarted).unwrap();
        let current = service.load().unwrap();
        let current_mod = mod_candidates(&current.github_proxies);
        assert!(
            current_mod
                .iter()
                .any(|candidate| candidate.url.starts_with("https://current-proxy.example/"))
        );
        assert!(
            !updater
                .iter()
                .any(|candidate| candidate.url.starts_with("https://current-proxy.example/"))
        );
        assert_eq!(
            installer_sources_for_url(&Provider::Github, DIRECT)
                .iter()
                .map(installer_candidate)
                .collect::<Vec<_>>(),
            updater
        );

        // An unsuccessful new fetch must leave the saved successful list usable.
        let after_failure = crate::github_proxies::merge_downloaded(
            &current.github_proxies,
            GitHubProxyConfig::default(),
        );
        assert_eq!(mod_candidates(&after_failure), current_mod);
    }

    #[test]
    fn defaults_follow_region_and_gitee_forces_direct() {
        assert_eq!(
            selected_asset_proxy_mode(
                None,
                &Provider::Github,
                crate::config::REGION_CHINESE_MAINLAND
            ),
            ProxyMode::AutoProxy
        );
        assert_eq!(
            selected_asset_proxy_mode(None, &Provider::Github, crate::config::REGION_INTERNATIONAL),
            ProxyMode::AutoDirect
        );
        assert_eq!(
            selected_asset_proxy_mode(
                Some(ProxyMode::AutoProxy),
                &Provider::Gitee,
                crate::config::REGION_CHINESE_MAINLAND
            ),
            ProxyMode::Direct
        );
    }

    #[test]
    fn automatic_modes_use_opposite_candidate_order() {
        let proxies = vec![
            "https://proxy-a.example/asset".into(),
            "https://proxy-b.example/asset".into(),
        ];
        let direct_first =
            plan_asset_download_candidates(ProxyMode::AutoDirect, DIRECT, proxies.clone()).unwrap();
        assert_eq!(
            direct_first
                .iter()
                .map(|item| item.route)
                .collect::<Vec<_>>(),
            [
                AssetDownloadRoute::Direct,
                AssetDownloadRoute::Proxy,
                AssetDownloadRoute::Proxy
            ]
        );
        assert_eq!(direct_first[0].url, DIRECT);
        let proxy_first =
            plan_asset_download_candidates(ProxyMode::AutoProxy, DIRECT, proxies).unwrap();
        assert_eq!(
            proxy_first
                .iter()
                .map(|item| item.route)
                .collect::<Vec<_>>(),
            [
                AssetDownloadRoute::Proxy,
                AssetDownloadRoute::Proxy,
                AssetDownloadRoute::Direct
            ]
        );
        assert_eq!(proxy_first[2].url, DIRECT);
    }

    #[test]
    fn automatic_proxy_uses_remote_first_and_bundled_fallback() {
        let entries = vec![
            proxy("bundled", "bundled.example", false, true),
            proxy("disabled", "disabled.example", true, false),
            proxy("remote", "remote.example", true, true),
        ];
        let urls = asset_proxy_urls(
            &entries,
            DIRECT,
            false,
            "",
            "",
            ProxyMode::AutoProxy,
            Some("bundled"),
        );
        assert_eq!(urls.len(), 2);
        assert!(urls[0].starts_with("https://remote.example/"));
        assert!(urls[1].starts_with("https://bundled.example/"));

        let bundled_only = asset_proxy_urls(
            &entries[..1],
            DIRECT,
            false,
            "",
            "",
            ProxyMode::AutoProxy,
            None,
        );
        assert_eq!(bundled_only.len(), 1);
        assert!(bundled_only[0].starts_with("https://bundled.example/"));
    }

    #[test]
    fn fixed_mode_uses_only_selected_enabled_proxy() {
        let entries = vec![
            proxy("first", "first.example", true, true),
            proxy("selected", "selected.example", false, true),
            proxy("disabled", "disabled.example", false, false),
        ];
        let selected = asset_proxy_urls(
            &entries,
            DIRECT,
            false,
            "",
            "",
            ProxyMode::Fixed,
            Some("selected"),
        );
        let candidates =
            plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, selected).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].route, AssetDownloadRoute::Proxy);
        assert!(candidates[0].url.starts_with("https://selected.example/"));
        assert!(plan_asset_download_candidates(ProxyMode::Fixed, DIRECT, Vec::new()).is_err());
        assert!(
            asset_proxy_urls(
                &entries,
                DIRECT,
                false,
                "",
                "",
                ProxyMode::Fixed,
                Some("disabled")
            )
            .is_empty()
        );
        for selected_id in [None, Some(" ")] {
            assert!(
                asset_proxy_urls(
                    &entries,
                    DIRECT,
                    false,
                    "",
                    "",
                    ProxyMode::Fixed,
                    selected_id
                )
                .is_empty()
            );
        }
    }

    #[tokio::test]
    async fn pool_stops_on_success_and_cancel() {
        let candidates = plan_asset_download_candidates(
            ProxyMode::AutoProxy,
            DIRECT,
            vec!["https://proxy.example/asset".into()],
        )
        .unwrap();
        let mut tried = Vec::new();
        let result: Result<PathBuf, String> = try_asset_download_candidates(
            candidates,
            || false,
            |candidate, _reporter| {
                tried.push(candidate.route);
                std::future::ready(if candidate.route == AssetDownloadRoute::Proxy {
                    Err("代理失败".into())
                } else {
                    Ok(PathBuf::from("downloaded"))
                })
            },
            None,
        )
        .await;
        assert_eq!(result.unwrap(), PathBuf::from("downloaded"));
        assert_eq!(
            tried,
            [AssetDownloadRoute::Proxy, AssetDownloadRoute::Direct]
        );

        let candidates = plan_asset_download_candidates(
            ProxyMode::AutoDirect,
            DIRECT,
            vec!["https://proxy.example/asset".into()],
        )
        .unwrap();
        let mut attempted = 0;
        let result: Result<PathBuf, String> = try_asset_download_candidates(
            candidates,
            || false,
            |_candidate, _reporter| {
                attempted += 1;
                std::future::ready(Err("download.cancelled".into()))
            },
            None,
        )
        .await;
        assert_eq!(result.unwrap_err(), "download.cancelled");
        // Both initial slots may start before an asynchronous cancellation
        // result is received. No later candidate may start after cancellation.
        assert_eq!(attempted, 2);

        let candidates = plan_asset_download_candidates(
            ProxyMode::AutoProxy,
            DIRECT,
            vec!["https://proxy.example/asset".into()],
        )
        .unwrap();
        let cancellation = std::cell::Cell::new(false);
        let mut attempted = 0;
        let result: Result<PathBuf, String> = try_asset_download_candidates(
            candidates,
            || cancellation.get(),
            |_candidate, _reporter| {
                attempted += 1;
                cancellation.set(true);
                std::future::ready(Err("网络失败".into()))
            },
            None,
        )
        .await;
        assert_eq!(result.unwrap_err(), "download.cancelled");
        assert_eq!(attempted, 1);
    }
}

#[cfg(test)]
mod map_source_staging_tests {
    use super::*;

    #[test]
    fn discard_accepts_only_files_inside_the_cache_directory() {
        let root = std::env::temp_dir().join(format!("oc2-stage-{}", uuid::Uuid::new_v4()));
        let cache = root.join("cache");
        fs::create_dir_all(&cache).unwrap();
        let staged = cache.join("staged.zip");
        fs::write(&staged, b"zip").unwrap();
        let outside = root.join("user.zip");
        fs::write(&outside, b"zip").unwrap();

        // 缓存目录内的普通文件：解析成真实路径，交给调用方删除
        assert_eq!(
            cache_file_to_discard(&cache, &staged.to_string_lossy()).unwrap(),
            Some(fs::canonicalize(&staged).unwrap())
        );
        // 已经不在了：视为已清理，不报错
        assert!(
            cache_file_to_discard(&cache, &cache.join("missing.zip").to_string_lossy())
                .unwrap()
                .is_none()
        );
        // 缓存目录外的文件、以及目录本身都不是可丢弃的暂存文件
        assert!(cache_file_to_discard(&cache, &outside.to_string_lossy()).is_err());
        assert!(cache_file_to_discard(&cache, &cache.to_string_lossy()).is_err());
        assert!(outside.exists(), "缓存目录外的文件不能被删除");
        fs::remove_dir_all(root).unwrap();
    }

    fn zip_level_set(name: &str, name_zh: Option<&str>, author: &str) -> ZipLevelSet {
        serde_json::from_value(serde_json::json!({
            "dir_name": name,
            "name": name,
            "name_zh": name_zh,
            "author": author,
            "version": "0.73",
        }))
        .unwrap()
    }

    fn map_repository(level_set_name: &str, name_zh: &str, author: &str) -> MapRepositoryConfig {
        serde_json::from_value(serde_json::json!({
            "provider": "github",
            "username": "CH3NGYZ",
            "repository": "Overcooked-2-MOD-Manager-Repos",
            "level_set_name": level_set_name,
            "level_set_name_zh": name_zh,
            "author": author,
        }))
        .unwrap()
    }

    #[test]
    fn a_matching_map_release_is_accepted_without_the_chinese_name() {
        // 老地图源不写中文名：退回「英文集名 + 作者」比较，与安装前一样能过
        let map = map_repository("RW_LevelSet", "", "RW");
        assert!(
            validate_map_release_sets(&[zip_level_set("RW_LevelSet", None, "RW")], &map).is_ok()
        );
        // 中文名两边都有才比较：写对了也要能过
        let map = map_repository("GAODA", "胡闹糕中", "GAODA");
        assert!(
            validate_map_release_sets(&[zip_level_set("GAODA", Some("胡闹糕中"), "GAODA")], &map)
                .is_ok()
        );
    }

    #[test]
    fn a_mismatched_map_release_is_rejected_before_the_user_confirms() {
        // 声明与实际不一致：装进去会让绑定错位、更新检测全失效，必须在暂存阶段失败
        let map = map_repository("GAODA", "胡闹糕中", "GAODA");
        let error =
            validate_map_release_sets(&[zip_level_set("GAODA", Some("胡闹达学"), "GAODA")], &map)
                .unwrap_err();
        assert!(
            error.starts_with("levelSet.zipMismatch?expected="),
            "错误必须是带 expected/actual 的 tag：{error}"
        );
        assert!(error.contains("actual="), "{error}");
    }

    #[test]
    fn a_multi_set_map_release_is_rejected() {
        // 一个地图仓库只对应一张图：拿到多集 ZIP 说明资产选错了
        let map = map_repository("RW_LevelSet", "", "RW");
        let error = validate_map_release_sets(
            &[
                zip_level_set("RW_LevelSet", None, "RW"),
                zip_level_set("Other", None, "Other"),
            ],
            &map,
        )
        .unwrap_err();
        assert_eq!(error, "levelSet.zipMultipleForRepository?count=2");
    }

    #[test]
    fn zip_level_set_json_carries_conflicts_and_no_exists_flag() {
        // 前端契约：`exists`（同名即冲突）已删除，改为 `conflicts` 列表 ——
        // 同名不同身份的目录允许并存，不能再用「有同名目录」当冲突判据。
        let value = serde_json::to_value(zip_level_set("RW_LevelSet", None, "RW")).unwrap();
        assert!(value.get("conflicts").is_some(), "{value}");
        assert!(value.get("exists").is_none(), "{value}");
    }
}

#[cfg(test)]
mod update_status_tests {
    use super::*;

    #[test]
    fn manifest_fallbacks_use_each_providers_download_route() {
        assert_eq!(
            installer_manifest_url(&Provider::Gitee),
            "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager/releases/download/latest/latest.json"
        );
        assert_eq!(
            installer_manifest_url(&Provider::Github),
            "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/latest/download/latest.json"
        );
        let configuration: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let endpoints = configuration["plugins"]["updater"]["endpoints"]
            .as_array()
            .unwrap();
        for provider in [Provider::Gitee, Provider::Github] {
            assert!(endpoints.iter().any(|endpoint| {
                endpoint.as_str() == Some(installer_manifest_url(&provider).as_str())
            }));
        }
    }

    fn repository(id: &str, mode: RepositoryDownloadMode) -> RepositoryConfig {
        serde_json::from_value(serde_json::json!({
            "id": id, "name": id, "provider": "github",
            "username": "example", "repository": "mod", "download_mode": mode,
            "install_target": "BepInEx/plugins", "enabled": true, "builtin": false,
            "cached_releases": [{
                "tag_name": "v99.0.0", "version": "99.0.0", "name": "newer release",
                "body": "", "published_at": null, "prerelease": false, "draft": false,
                "assets": []
            }]
        }))
        .unwrap()
    }

    struct InstallWorkspace {
        root: PathBuf,
        game: PathBuf,
        manifests: PathBuf,
        recycle: PathBuf,
    }

    impl InstallWorkspace {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("oc2-reinstall-test-{}", Uuid::new_v4()));
            let game = root.join("game");
            let manifests = root.join("manager/manifests");
            let recycle = root.join("manager/recycle-bin");
            fs::create_dir_all(&game).unwrap();
            fs::create_dir_all(&manifests).unwrap();
            Self {
                root,
                game,
                manifests,
                recycle,
            }
        }

        fn write(&self, relative: &str, contents: &[u8]) {
            let path = self.game.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }

        fn zip(&self, entries: &[(&str, &[u8])]) -> PathBuf {
            let path = self.root.join("update.zip");
            let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
            for (name, contents) in entries {
                writer
                    .start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                writer.write_all(contents).unwrap();
            }
            writer.finish().unwrap();
            path
        }

        fn original(&self) -> Vec<u8> {
            self.write("BepInEx/plugins/Main.dll", b"old main");
            self.write("BepInEx/plugins/Hidden.dll.disabled", b"old hidden");
            self.write("BepInEx/plugins/data/settings.json", b"old settings");
            self.write("BepInEx/plugins/Shared.dll", b"shared");
            self.write("BepInEx/plugins/Unrelated.dll", b"unrelated");
            let files = ["Main.dll", "Hidden.dll", "data/settings.json", "Shared.dll"]
                .map(|file| format!("BepInEx/plugins/{file}"));
            archive::write_manifest(
                &self.manifests,
                &archive::new_manifest(
                    "package",
                    "v1",
                    archive::single_language("Package"),
                    Default::default(),
                    files.to_vec(),
                    Vec::new(),
                ),
            )
            .unwrap();
            archive::write_manifest(
                &self.manifests,
                &archive::new_manifest(
                    "peer",
                    "v1",
                    archive::single_language("Peer"),
                    Default::default(),
                    vec!["BepInEx/plugins/Shared.dll".into()],
                    Vec::new(),
                ),
            )
            .unwrap();
            fs::read(self.manifests.join("package.toml")).unwrap()
        }
    }

    impl Drop for InstallWorkspace {
        fn drop(&mut self) {
            let target = fs::canonicalize(&self.root).unwrap();
            let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
            assert!(target.starts_with(&temp) && target != temp);
            let _ = fs::remove_dir_all(target);
        }
    }

    #[test]
    fn all_release_install_sources_persist_repository_tags_and_recycle_the_original_binding() {
        for source in [Some("official"), Some(crate::community::SOURCE_ID), None] {
            for provider in [Provider::Github, Provider::Gitee] {
                let workspace = InstallWorkspace::new();
                let mut repo = repository("package", RepositoryDownloadMode::Release);
                repo.provider = provider;
                repo.source_id = source.map(str::to_string);
                repo.builtin = source == Some("official");
                let asset = workspace.zip(&[("Main.dll", b"first"), ("Child.dll", b"child")]);
                install_downloaded_repository(
                    &asset,
                    "package.zip",
                    &workspace.game,
                    &workspace.manifests,
                    &workspace.recycle,
                    &repo,
                    "v1.0.0",
                    false,
                    true,
                )
                .unwrap();
                let original = archive::read_manifest(&workspace.manifests, "package").unwrap();
                let binding = original.repository_binding.as_ref().unwrap();
                assert_eq!(binding.owner, repo.username);
                assert_eq!(binding.repository, repo.repository);
                assert_eq!(binding.provider, repo.provider);
                assert_eq!(binding.tag, "v1.0.0");
                assert_eq!(binding.channel, repo.channel);
                let scanned = mods::scan_mods_with_manifests(
                    &workspace.game,
                    &config(vec![repo.clone()]),
                    Some(&workspace.manifests),
                );
                assert_eq!(scanned.len(), 2);
                assert!(
                    scanned
                        .iter()
                        .all(|item| item.repository_binding.as_ref() == Some(binding)
                            && item.package_mod_count == 2)
                );
                fs::rename(
                    workspace.game.join("BepInEx/plugins/Child.dll"),
                    workspace.game.join("BepInEx/plugins/Child.dll.disabled"),
                )
                .unwrap();
                let asset = workspace.zip(&[("Main.dll", b"second")]);
                install_downloaded_repository(
                    &asset,
                    "package.zip",
                    &workspace.game,
                    &workspace.manifests,
                    &workspace.recycle,
                    &repo,
                    "v2.0.0",
                    false,
                    true,
                )
                .unwrap();
                let updated = archive::read_manifest(&workspace.manifests, "package").unwrap();
                assert_eq!(updated.repository_binding.as_ref().unwrap().tag, "v2.0.0");
                let entries = recycle_bin::list(&workspace.recycle).unwrap();
                assert_eq!(entries.len(), 1);
                let snapshot: archive::InstallManifest = toml::from_str(
                    &fs::read_to_string(
                        workspace.recycle.join(&entries[0].id).join("manifest.toml"),
                    )
                    .unwrap(),
                )
                .unwrap();
                assert_eq!(snapshot.repository_binding, original.repository_binding);
                assert_eq!(entries[0].file_count, 2);
            }
        }
        let workspace = InstallWorkspace::new();
        let raw = repository("package", RepositoryDownloadMode::RepositoryFile);
        let asset = workspace.zip(&[("Main.dll", b"raw")]);
        install_downloaded_repository(
            &asset,
            "package.zip",
            &workspace.game,
            &workspace.manifests,
            &workspace.recycle,
            &raw,
            "",
            false,
            true,
        )
        .unwrap();
        assert!(
            archive::read_manifest(&workspace.manifests, "package")
                .unwrap()
                .repository_binding
                .is_none()
        );
    }

    #[test]
    fn vanished_source_ids_reuse_the_bound_package_without_merging_live_packages() {
        let workspace = InstallWorkspace::new();
        let mut old_repo = repository("old-source-id", RepositoryDownloadMode::Release);
        let asset = workspace.zip(&[("Main.dll", b"old")]);
        install_downloaded_repository(
            &asset,
            "package.zip",
            &workspace.game,
            &workspace.manifests,
            &workspace.recycle,
            &old_repo,
            "v1.0.0",
            false,
            true,
        )
        .unwrap();
        let mut new_repo = old_repo.clone();
        new_repo.id = "discovered-source-id".into();
        let identity = repository_install_identity(
            &new_repo,
            &config(vec![new_repo.clone()]),
            &workspace.manifests,
        )
        .unwrap();
        assert_eq!(identity.id, "old-source-id");
        assert_eq!(
            repository_install_identity(
                &new_repo,
                &config(vec![old_repo.clone(), new_repo.clone()]),
                &workspace.manifests
            )
            .unwrap()
            .id,
            new_repo.id
        );
        let asset = workspace.zip(&[("Main.dll", b"new")]);
        install_downloaded_repository(
            &asset,
            "package.zip",
            &workspace.game,
            &workspace.manifests,
            &workspace.recycle,
            &identity,
            "v2.0.0",
            false,
            true,
        )
        .unwrap();
        assert!(
            !workspace
                .manifests
                .join("discovered-source-id.toml")
                .exists()
        );
        assert_eq!(
            archive::read_manifest(&workspace.manifests, "old-source-id")
                .unwrap()
                .repository_binding
                .unwrap()
                .tag,
            "v2.0.0"
        );
        old_repo.id = "another-orphan".into();
        let mut other = archive::new_manifest(
            &old_repo.id,
            "v1.0.0",
            Default::default(),
            Default::default(),
            vec![],
            Vec::new(),
        );
        other.repository_binding = RepositoryBinding::from_release(&old_repo, "v1.0.0");
        archive::write_manifest(&workspace.manifests, &other).unwrap();
        assert!(
            repository_install_identity(
                &new_repo,
                &config(vec![new_repo.clone()]),
                &workspace.manifests
            )
            .is_err()
        );
    }

    #[test]
    fn update_summary_prefers_a_bound_tag_over_dll_metadata_and_supports_opaque_tags() {
        for tag in ["v1.0.0", "old-nightly"] {
            let mut repo = repository("release", RepositoryDownloadMode::Release);
            repo.cached_releases[0].tag_name = if tag == "old-nightly" {
                "new-nightly".into()
            } else {
                "v99.0.0".into()
            };
            let mut local = installed("release", "latest");
            // DLL 元数据版本故意与清单 tag 不同、但低于线上版本：
            // 更新结论按清单 tag 算，local_version 也显示清单 tag。
            local.version = Some("0.9.0".into());
            local.repository_binding = RepositoryBinding::from_release(&repo, tag);
            let status = collect_update_status(&config(vec![repo]), &[local]);
            assert_eq!(status[0].local_version.as_deref(), Some(tag));
            assert_eq!(status[0].state, "update_available");
        }
    }

    #[test]
    fn online_install_with_no_recyclable_old_files_succeeds_or_preserves_manifest_on_failure() {
        for condition in ["empty", "missing", "shared"] {
            for zip in [false, true] {
                for fail in [false, true] {
                    let workspace = InstallWorkspace::new();
                    let missing = "BepInEx/plugins/Main.dll";
                    let shared = "BepInEx/plugins/Shared.dll";
                    let old_files = match condition {
                        "empty" => vec![],
                        "shared" => vec![shared.into()],
                        _ => vec![
                            missing.into(),
                            "BepInEx/plugins/Missing.dll.disabled".into(),
                        ],
                    };
                    archive::write_manifest(
                        &workspace.manifests,
                        &archive::new_manifest(
                            "package",
                            "v1",
                            archive::single_language("Old package"),
                            Default::default(),
                            old_files,
                            Vec::new(),
                        ),
                    )
                    .unwrap();
                    let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
                    let peer_original = if condition == "shared" {
                        workspace.write(shared, b"shared dependency");
                        archive::write_manifest(
                            &workspace.manifests,
                            &archive::new_manifest(
                                "peer",
                                "v1",
                                archive::single_language("Peer"),
                                Default::default(),
                                vec![shared.into()],
                                Vec::new(),
                            ),
                        )
                        .unwrap();
                        Some(fs::read(workspace.manifests.join("peer.toml")).unwrap())
                    } else {
                        None
                    };
                    workspace.write("BepInEx/plugins/Unrelated.dll", b"keep");
                    let asset = if zip {
                        if fail {
                            fs::create_dir_all(workspace.game.join("BepInEx/plugins/Blocked.dll"))
                                .unwrap();
                            workspace
                                .zip(&[("Main.dll", b"new plugin"), ("Blocked.dll", b"blocked")])
                        } else {
                            workspace.zip(&[("Main.dll", b"new plugin")])
                        }
                    } else {
                        let asset = workspace.root.join("Main.dll");
                        if !fail {
                            fs::write(&asset, b"new plugin").unwrap();
                        }
                        asset
                    };
                    let repo = repository(
                        "package",
                        if zip {
                            RepositoryDownloadMode::Release
                        } else {
                            RepositoryDownloadMode::RepositoryFile
                        },
                    );
                    let result = install_downloaded_repository(
                        &asset,
                        if zip { "update.zip" } else { "Main.dll" },
                        &workspace.game,
                        &workspace.manifests,
                        &workspace.recycle,
                        &repo,
                        "v2",
                        false,
                        true,
                    );
                    if fail {
                        let error = result.unwrap_err();
                        assert!(
                            error.starts_with("mods.installFailedRolledBack"),
                            "{condition}, zip={zip}: {error}"
                        );
                        assert!(!workspace.game.join(missing).exists());
                        assert_eq!(
                            fs::read(workspace.manifests.join("package.toml")).unwrap(),
                            original
                        );
                    } else {
                        assert_eq!(
                            result
                                .unwrap_or_else(|error| panic!("{condition}, zip={zip}: {error}")),
                            1
                        );
                        assert_eq!(
                            fs::read(workspace.game.join(missing)).unwrap(),
                            b"new plugin"
                        );
                        let current =
                            archive::read_manifest(&workspace.manifests, "package").unwrap();
                        assert_eq!(current.release_tag, "v2");
                        assert_eq!(current.files, vec![missing]);
                    }
                    if let Some(bytes) = peer_original {
                        assert_eq!(
                            fs::read(workspace.manifests.join("peer.toml")).unwrap(),
                            bytes
                        );
                        assert_eq!(
                            fs::read(workspace.game.join(shared)).unwrap(),
                            b"shared dependency"
                        );
                    }
                    assert_eq!(
                        fs::read(workspace.game.join("BepInEx/plugins/Unrelated.dll")).unwrap(),
                        b"keep"
                    );
                    assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
                    assert!(!workspace.recycle.exists());
                }
            }
        }
    }

    #[test]
    fn online_reinstall_rejects_invalid_old_manifests_and_nonfile_entries() {
        for condition in ["invalid_toml", "unsafe_path", "directory"] {
            let workspace = InstallWorkspace::new();
            let files = if condition == "unsafe_path" {
                vec!["../outside.dll".into()]
            } else {
                vec!["BepInEx/plugins/Old.dll".into()]
            };
            archive::write_manifest(
                &workspace.manifests,
                &archive::new_manifest(
                    "package",
                    "v1",
                    Default::default(),
                    Default::default(),
                    files,
                    Vec::new(),
                ),
            )
            .unwrap();
            if condition == "invalid_toml" {
                fs::write(workspace.manifests.join("package.toml"), "invalid = [").unwrap();
            } else if condition == "directory" {
                fs::create_dir_all(workspace.game.join("BepInEx/plugins/Old.dll")).unwrap();
                workspace.write("BepInEx/plugins/Old.dll/keep.txt", b"keep");
            }
            let original = fs::read(workspace.manifests.join("package.toml")).unwrap();
            let asset = workspace.root.join("Main.dll");
            fs::write(&asset, b"new plugin").unwrap();
            let repo = repository("package", RepositoryDownloadMode::Release);
            assert!(
                install_downloaded_repository(
                    &asset,
                    "Main.dll",
                    &workspace.game,
                    &workspace.manifests,
                    &workspace.recycle,
                    &repo,
                    "v2",
                    false,
                    true,
                )
                .is_err()
            );
            assert_eq!(
                fs::read(workspace.manifests.join("package.toml")).unwrap(),
                original
            );
            assert!(!workspace.game.join("BepInEx/plugins/Main.dll").exists());
            assert!(!workspace.recycle.exists());
            if condition == "directory" {
                assert_eq!(
                    fs::read(workspace.game.join("BepInEx/plugins/Old.dll/keep.txt")).unwrap(),
                    b"keep"
                );
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn online_install_without_old_files_rolls_back_when_manifest_commit_fails() {
        for zip in [false, true] {
            let workspace = InstallWorkspace::new();
            archive::write_manifest(
                &workspace.manifests,
                &archive::new_manifest(
                    "package",
                    "v1",
                    Default::default(),
                    Default::default(),
                    vec!["BepInEx/plugins/Main.dll".into()],
                    Vec::new(),
                ),
            )
            .unwrap();
            let path = workspace.manifests.join("package.toml");
            let original = fs::read(&path).unwrap();
            let permissions = fs::metadata(&path).unwrap().permissions();
            let mut readonly = permissions.clone();
            readonly.set_readonly(true);
            fs::set_permissions(&path, readonly).unwrap();
            let asset = if zip {
                workspace.zip(&[("Main.dll", b"new plugin")])
            } else {
                let asset = workspace.root.join("Main.dll");
                fs::write(&asset, b"new plugin").unwrap();
                asset
            };
            let result = install_downloaded_repository(
                &asset,
                if zip { "update.zip" } else { "Main.dll" },
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repository("package", RepositoryDownloadMode::Release),
                "v2",
                false,
                true,
            );
            fs::set_permissions(&path, permissions).unwrap();
            let error = result.unwrap_err();
            assert!(error.starts_with("mods.installFailedRolledBack"), "{error}");
            assert_eq!(fs::read(&path).unwrap(), original);
            assert!(!workspace.game.join("BepInEx/plugins/Main.dll").exists());
            assert!(!workspace.recycle.exists());
        }
    }

    #[test]
    fn online_reinstall_with_duplicate_manifests_succeeds_or_rolls_back() {
        for zip in [false, true] {
            for fail in [false, true] {
                let workspace = InstallWorkspace::new();
                let dll = "BepInEx/plugins/Main.dll";
                workspace.write(&format!("{dll}.disabled"), b"old plugin");
                workspace.write("BepInEx/plugins/Unrelated.dll", b"keep");
                for id in ["package", "manual"] {
                    archive::write_manifest(
                        &workspace.manifests,
                        &archive::new_manifest(
                            id,
                            "v1",
                            archive::single_language(id),
                            Default::default(),
                            vec![dll.into()],
                            Vec::new(),
                        ),
                    )
                    .unwrap();
                }
                let originals = ["package", "manual"].map(|id| {
                    (
                        id,
                        fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                    )
                });
                let asset = if zip {
                    if fail {
                        fs::create_dir_all(workspace.game.join("BepInEx/plugins/Blocked.dll"))
                            .unwrap();
                        workspace.zip(&[("Main.dll", b"new plugin"), ("Blocked.dll", b"blocked")])
                    } else {
                        workspace.zip(&[("Main.dll", b"new plugin")])
                    }
                } else {
                    let asset = workspace.root.join("Main.dll");
                    if !fail {
                        fs::write(&asset, b"new plugin").unwrap();
                    }
                    asset
                };
                let repo = repository("package", RepositoryDownloadMode::Release);
                let result = install_downloaded_repository(
                    &asset,
                    if zip { "update.zip" } else { "Main.dll" },
                    &workspace.game,
                    &workspace.manifests,
                    &workspace.recycle,
                    &repo,
                    "v2",
                    false,
                    true,
                );
                if fail {
                    let error = result.unwrap_err();
                    assert!(error.starts_with("mods.installFailedRolledBack"), "{error}");
                    assert_eq!(
                        fs::read(workspace.game.join(format!("{dll}.disabled"))).unwrap(),
                        b"old plugin"
                    );
                    assert!(!workspace.game.join(dll).exists());
                    for (id, bytes) in originals {
                        assert_eq!(
                            fs::read(workspace.manifests.join(format!("{id}.toml"))).unwrap(),
                            bytes
                        );
                    }
                    assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
                } else {
                    assert_eq!(result.unwrap(), 1);
                    assert_eq!(fs::read(workspace.game.join(dll)).unwrap(), b"new plugin");
                    assert!(!workspace.game.join(format!("{dll}.disabled")).exists());
                    assert!(archive::read_manifest(&workspace.manifests, "manual").is_none());
                    assert_eq!(
                        archive::read_manifest(&workspace.manifests, "package")
                            .unwrap()
                            .release_tag,
                        "v2"
                    );
                    let entries = recycle_bin::list(&workspace.recycle).unwrap();
                    assert_eq!(entries.len(), 1);
                    assert_eq!(
                        fs::read(
                            workspace
                                .recycle
                                .join(&entries[0].id)
                                .join("files")
                                .join(format!("{dll}.disabled"))
                        )
                        .unwrap(),
                        b"old plugin"
                    );
                }
                assert_eq!(
                    fs::read(workspace.game.join("BepInEx/plugins/Unrelated.dll")).unwrap(),
                    b"keep"
                );
            }
        }
    }

    #[test]
    fn online_reinstall_recycles_complete_old_package_before_zip_or_dll_install() {
        for zip in [false, true] {
            let workspace = InstallWorkspace::new();
            let original = workspace.original();
            let repo = repository(
                "package",
                if zip {
                    RepositoryDownloadMode::Release
                } else {
                    RepositoryDownloadMode::RepositoryFile
                },
            );
            let asset = if zip {
                workspace.zip(&[("Main.dll", b"new main"), ("New.dll", b"new member")])
            } else {
                let path = workspace.root.join("Main.dll");
                fs::write(&path, b"new main").unwrap();
                path
            };
            let count = install_downloaded_repository(
                &asset,
                if zip { "update.zip" } else { "Main.dll" },
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v2",
                false,
                true,
            )
            .unwrap();
            assert_eq!(count, if zip { 2 } else { 1 });
            assert_eq!(
                fs::read(workspace.game.join("BepInEx/plugins/Main.dll")).unwrap(),
                b"new main"
            );
            assert!(
                !workspace
                    .game
                    .join("BepInEx/plugins/Hidden.dll.disabled")
                    .exists()
            );
            assert!(
                !workspace
                    .game
                    .join("BepInEx/plugins/data/settings.json")
                    .exists()
            );
            assert_eq!(
                fs::read(workspace.game.join("BepInEx/plugins/Shared.dll")).unwrap(),
                b"shared"
            );
            assert_eq!(
                fs::read(workspace.game.join("BepInEx/plugins/Unrelated.dll")).unwrap(),
                b"unrelated"
            );
            let entries = recycle_bin::list(&workspace.recycle).unwrap();
            assert_eq!(entries.len(), 1);
            assert_eq!(entries[0].file_count, 3);
            assert_eq!(entries[0].release_tag.as_deref(), Some("v1"));
            let staged = workspace.recycle.join(&entries[0].id);
            assert_eq!(fs::read(staged.join("manifest.toml")).unwrap(), original);
            assert_eq!(
                fs::read(staged.join("files/BepInEx/plugins/Main.dll")).unwrap(),
                b"old main"
            );
            assert_eq!(
                fs::read(staged.join("files/BepInEx/plugins/Hidden.dll.disabled")).unwrap(),
                b"old hidden"
            );
            assert_eq!(
                fs::read(staged.join("files/BepInEx/plugins/data/settings.json")).unwrap(),
                b"old settings"
            );
            let manifest = archive::read_manifest(&workspace.manifests, "package").unwrap();
            assert_eq!(manifest.release_tag, "v2");
            assert_eq!(manifest.files.len(), count);
        }
    }

    #[test]
    fn online_reinstall_mixed_zip_records_package_directory_and_recycles_previous_package() {
        let workspace = InstallWorkspace::new();
        let original = workspace.original();
        let asset = workspace.zip(&[
            ("Main.dll", b"new main"),
            ("Data/settings.json", b"new settings"),
            ("README.txt", b"readme"),
        ]);
        let repo = repository("package", RepositoryDownloadMode::Release);
        assert_eq!(
            install_downloaded_repository(
                &asset,
                "MOD Bundle.zip",
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v2",
                false,
                true,
            )
            .unwrap(),
            3
        );
        let manifest = archive::read_manifest(&workspace.manifests, "package").unwrap();
        assert_eq!(
            manifest.files,
            vec![
                "BepInEx/plugins/MOD Bundle/Main.dll",
                "BepInEx/plugins/MOD Bundle/Data/settings.json",
                "BepInEx/plugins/MOD Bundle/README.txt"
            ]
        );
        assert_eq!(
            fs::read(workspace.game.join(&manifest.files[0])).unwrap(),
            b"new main"
        );
        assert_eq!(
            fs::read(workspace.game.join(&manifest.files[1])).unwrap(),
            b"new settings"
        );
        assert_eq!(
            fs::read(workspace.game.join(&manifest.files[2])).unwrap(),
            b"readme"
        );
        assert!(!workspace.game.join("BepInEx/plugins/Main.dll").exists());
        assert!(!workspace.game.join("BepInEx/plugins/README.txt").exists());
        let entries = recycle_bin::list(&workspace.recycle).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].file_count, 3);
        assert_eq!(
            fs::read(workspace.recycle.join(&entries[0].id).join("manifest.toml")).unwrap(),
            original
        );
    }

    #[test]
    fn online_reinstall_failure_restores_disabled_files_manifest_and_shared_overwrites() {
        let workspace = InstallWorkspace::new();
        let original = workspace.original();
        fs::create_dir_all(workspace.game.join("BepInEx/plugins/Blocked.dll")).unwrap();
        workspace.write(
            "BepInEx/plugins/Blocked.dll/unrelated.txt",
            b"blocked contents",
        );
        let asset = workspace.zip(&[
            ("Main.dll", b"new main"),
            ("Shared.dll", b"changed shared"),
            ("New.dll", b"new member"),
            ("Blocked.dll", b"cannot write"),
        ]);
        let repo = repository("package", RepositoryDownloadMode::Release);
        assert!(
            install_downloaded_repository(
                &asset,
                "update.zip",
                &workspace.game,
                &workspace.manifests,
                &workspace.recycle,
                &repo,
                "v2",
                false,
                true,
            )
            .is_err()
        );
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Main.dll")).unwrap(),
            b"old main"
        );
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Hidden.dll.disabled")).unwrap(),
            b"old hidden"
        );
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/data/settings.json")).unwrap(),
            b"old settings"
        );
        assert_eq!(
            fs::read(workspace.game.join("BepInEx/plugins/Shared.dll")).unwrap(),
            b"shared"
        );
        assert_eq!(
            fs::read(
                workspace
                    .game
                    .join("BepInEx/plugins/Blocked.dll/unrelated.txt")
            )
            .unwrap(),
            b"blocked contents"
        );
        assert_eq!(
            fs::read(workspace.manifests.join("package.toml")).unwrap(),
            original
        );
        assert!(!workspace.game.join("BepInEx/plugins/New.dll").exists());
        assert!(!workspace.game.join("BepInEx/plugins/Hidden.dll").exists());
        assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
    }

    #[test]
    fn online_reinstall_rejects_unsafe_or_empty_archives_before_recycling() {
        for entries in [&[("../outside.dll", b"bad".as_slice())][..], &[][..]] {
            let workspace = InstallWorkspace::new();
            let original = workspace.original();
            let asset = workspace.zip(entries);
            let repo = repository("package", RepositoryDownloadMode::Release);
            assert!(
                install_downloaded_repository(
                    &asset,
                    "update.zip",
                    &workspace.game,
                    &workspace.manifests,
                    &workspace.recycle,
                    &repo,
                    "v2",
                    false,
                    true,
                )
                .is_err()
            );
            assert_eq!(
                fs::read(workspace.game.join("BepInEx/plugins/Main.dll")).unwrap(),
                b"old main"
            );
            assert_eq!(
                fs::read(workspace.manifests.join("package.toml")).unwrap(),
                original
            );
            assert!(recycle_bin::list(&workspace.recycle).unwrap().is_empty());
        }
    }

    fn config(repositories: Vec<RepositoryConfig>) -> AppConfig {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1, "ui_zoom": 1.5, "onboarding_seen_steps": [],
            "game": {"path": "", "auto_detect": false},
            "network": {"timeout_seconds": 5, "download_threads": 1,
                "max_retry_count": 3, "check_updates_on_start": false},
            "launch": {"ask_user_to_start_from_steam": false, "auto_restore_winhttp": false},
            "auth": {"github_token": "", "gitee_token": "", "official_api_only": true},
            "repositories": repositories
        }))
        .unwrap()
    }

    fn installed(repository_id: &str, status: &str) -> InstalledMod {
        serde_json::from_value(serde_json::json!({
            "repository_id": repository_id, "repository_ids": [repository_id],
            "name": "Example", "dll_file": "BepInEx/plugins/Example.dll",
            "enabled": true, "version": "1.2.3.4", "version_source": "BepInPlugin",
            "update_available": false, "duplicate_guid": false, "status": status
        }))
        .unwrap()
    }

    #[test]
    fn raw_update_status_ignores_cached_releases_and_api_errors() {
        let mut raw = repository("raw", RepositoryDownloadMode::RepositoryFile);
        raw.last_error = Some("obsolete Release request failure".into());
        let statuses =
            collect_update_status(&config(vec![raw]), &[installed("raw", "not_release_asset")]);
        assert_eq!(statuses.len(), 1);
        assert_eq!(statuses[0].local_version.as_deref(), Some("1.2.3.4"));
        assert_eq!(statuses[0].remote_version, None);
        assert_eq!(statuses[0].state, "not_release_asset");
    }

    #[test]
    fn raw_installed_mod_cannot_supply_a_release_local_version() {
        let raw = repository("raw", RepositoryDownloadMode::RepositoryFile);
        let release = repository("release", RepositoryDownloadMode::Release);
        let mut local = installed("raw", "not_release_asset");
        local.repository_ids.push("release".into());
        let statuses = collect_update_status(&config(vec![raw, release]), &[local]);
        assert_eq!(statuses[0].state, "not_release_asset");
        assert_eq!(statuses[1].local_version, None);
        assert_eq!(statuses[1].remote_version.as_deref(), Some("99.0.0"));
        assert_eq!(statuses[1].state, "unknown");
    }

    #[test]
    fn release_installed_mod_still_reports_updates() {
        let release = repository("release", RepositoryDownloadMode::Release);
        let statuses =
            collect_update_status(&config(vec![release]), &[installed("release", "latest")]);
        assert_eq!(statuses[0].local_version.as_deref(), Some("1.2.3.4"));
        assert_eq!(statuses[0].remote_version.as_deref(), Some("99.0.0"));
        assert_eq!(statuses[0].state, "update_available");
    }

    #[test]
    fn failed_release_refresh_overrides_cached_update_summary() {
        for cached_version in ["1.2.3.4", "99.0.0"] {
            let mut release = repository("release", RepositoryDownloadMode::Release);
            release.cached_releases[0].version = Some(cached_version.into());
            release.cached_releases[0].tag_name = format!("v{cached_version}");
            release.last_error = Some("Release request failed".into());
            let statuses =
                collect_update_status(&config(vec![release]), &[installed("release", "latest")]);
            assert_eq!(statuses[0].state, "repository_unavailable");
            assert_eq!(statuses[0].local_version.as_deref(), Some("1.2.3.4"));
            assert_eq!(statuses[0].remote_version, None);
        }
    }

    #[test]
    fn empty_release_or_invalid_versions_cannot_confirm_latest() {
        let mut release = repository("release", RepositoryDownloadMode::Release);
        release.cached_releases.clear();
        let local = installed("release", "latest");
        let statuses = collect_update_status(&config(vec![release]), std::slice::from_ref(&local));
        assert_eq!(statuses[0].state, "unknown");
        assert_eq!(statuses[0].remote_version, None);

        let mut release = repository("release", RepositoryDownloadMode::Release);
        release.cached_releases[0].version = Some("not-a-version".into());
        let statuses = collect_update_status(&config(vec![release]), std::slice::from_ref(&local));
        assert_eq!(statuses[0].state, "unknown");
    }

    #[test]
    fn successful_release_retry_restores_update_summary() {
        let mut release = repository("release", RepositoryDownloadMode::Release);
        release.last_error = Some("Release request failed".into());
        let local = installed("release", "repository_unavailable");
        let failed =
            collect_update_status(&config(vec![release.clone()]), std::slice::from_ref(&local));
        assert_eq!(failed[0].state, "repository_unavailable");
        release.last_error = None;
        let recovered =
            collect_update_status(&config(vec![release.clone()]), std::slice::from_ref(&local));
        assert_eq!(recovered[0].state, "update_available");
        release.cached_releases[0].version = local.version.clone();
        let current = collect_update_status(&config(vec![release]), std::slice::from_ref(&local));
        assert_eq!(current[0].state, "latest");
    }

    #[test]
    fn failed_release_does_not_borrow_a_successful_bound_repositories_state() {
        let mut failed = repository("failed", RepositoryDownloadMode::Release);
        failed.last_error = Some("Release request failed".into());
        let mut current = repository("current", RepositoryDownloadMode::Release);
        current.cached_releases[0].version = Some("1.2.3.4".into());
        let mut local = installed("failed", "repository_unavailable");
        local.repository_ids.push("current".into());
        let statuses = collect_update_status(&config(vec![failed, current]), &[local]);
        assert_eq!(statuses[0].state, "repository_unavailable");
        assert_eq!(statuses[0].remote_version, None);
        assert_eq!(statuses[1].state, "latest");
    }

    #[test]
    fn release_update_summary_uses_the_loaded_copy_in_both_scan_orders() {
        for online_version in ["19.0", "20.0"] {
            let mut release = repository("release", RepositoryDownloadMode::Release);
            release.cached_releases[0].version = Some(online_version.into());
            release.cached_releases[0].tag_name = format!("v{online_version}");
            let config = config(vec![release]);
            let mut old = installed("release", "duplicate_lower_version");
            old.version = Some("18.3".into());
            old.load_warning = Some("存在更高版本，建议卸载".into());
            let mut loaded = installed("release", "latest");
            loaded.version = Some("19.0".into());
            for copies in [
                vec![old.clone(), loaded.clone()],
                vec![loaded.clone(), old.clone()],
            ] {
                let statuses = collect_update_status(&config, &copies);
                assert_eq!(statuses[0].local_version.as_deref(), Some("19.0"));
                assert_eq!(
                    statuses[0].state,
                    if online_version == "19.0" {
                        "latest"
                    } else {
                        "update_available"
                    }
                );
            }
        }
    }

    #[test]
    fn lower_raw_copy_keeps_its_installation_source_and_never_reports_a_release_update() {
        let raw = repository("raw", RepositoryDownloadMode::RepositoryFile);
        let release = repository("release", RepositoryDownloadMode::Release);
        let mut lower_raw = installed("raw", "not_release_asset");
        lower_raw.version = Some("18.3".into());
        lower_raw.load_warning = Some("存在更高版本，建议卸载".into());
        lower_raw.repository_ids.push("release".into());
        let statuses = collect_update_status(&config(vec![raw, release]), &[lower_raw]);
        assert_eq!(statuses[0].local_version.as_deref(), Some("18.3"));
        assert_eq!(statuses[0].state, "not_release_asset");
        assert_eq!(statuses[1].local_version, None);
        assert_eq!(statuses[1].state, "unknown");
    }
}

#[cfg(test)]
mod log_tests {
    use super::*;

    #[test]
    fn reopening_a_log_preserves_content_and_timestamps() {
        let path = std::env::temp_dir().join(format!("oc2-log-read-{}.log", Uuid::new_v4()));
        let content = "[2026-10-07 01:02:03 UTC][info] installed\n";
        fs::write(&path, content).unwrap();
        let before = fs::metadata(&path).unwrap().modified().unwrap();
        let first = read_log_at_path(&path).unwrap();
        let second = read_log_at_path(&path).unwrap();
        assert_eq!(first.content, content);
        assert_eq!(first.content, second.content);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), before);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn large_log_reads_are_bounded_to_the_existing_tail() {
        let path = std::env::temp_dir().join(format!("oc2-log-tail-{}.log", Uuid::new_v4()));
        let content = format!("{}final existing record\n", "old record\n".repeat(100_000));
        fs::write(&path, content.as_bytes()).unwrap();
        let log = read_log_at_path(&path).unwrap();
        assert_eq!(log.content.len(), 512 * 1024);
        assert!(log.content.ends_with("final existing record\n"));
        assert_eq!(fs::metadata(&path).unwrap().len(), content.len() as u64);
        fs::remove_file(path).unwrap();
    }
}

fn builtin_tutorial_repositories_for_region(region: &str) -> Result<Vec<RepositoryExport>, String> {
    let mut pending = vec![(
        SourceLocation::parse(repository_sources::BUILTIN_INDEX)
            .map_err(|error| error.to_string())?,
        None,
    )];
    let mut visited = HashSet::new();
    let mut host_utilities = None;
    let mut configuration_manager = None;
    while let Some((location, source_id)) = pending.pop() {
        // Tutorial data reads embedded resources only: no network, configuration
        // writes, installed MOD scans or real release-cache mutations.
        if !matches!(location, SourceLocation::Bundled(_)) || !visited.insert(location.address()) {
            continue;
        }
        let document = location
            .bundled_document()
            .map_err(|error| error.to_string())?;
        for mut repository in document.repositories {
            let configured_region =
                crate::config::normalize_repository_region(repository.region.as_deref());
            if !configured_region.is_empty() && configured_region != region {
                continue;
            }
            repository.source_id = source_id.clone();
            if repository.repository == "Overcooked-2-HostUtilities-Stable-Releases" {
                host_utilities.get_or_insert(repository);
            } else if repository.repository == "BepInEx.ConfigurationManager" {
                configuration_manager.get_or_insert(repository);
            }
        }
        if host_utilities.is_some() && configuration_manager.is_some() {
            break;
        }
        for source in document.repository_sources.into_iter().rev() {
            let configured_region =
                crate::config::normalize_repository_region(source.region.as_deref());
            if configured_region.is_empty() || configured_region == region {
                pending.push((
                    SourceLocation::parse(&source.url).map_err(|error| error.to_string())?,
                    Some(source.id),
                ));
            }
        }
    }
    let host_utilities =
        host_utilities.ok_or_else(|| "tutorial.hostUtilitiesConfigMissing".to_string())?;
    let configuration_manager =
        configuration_manager.ok_or_else(|| "tutorial.configManagerConfigMissing".to_string())?;
    Ok(vec![host_utilities, configuration_manager])
}

#[tauri::command]
pub fn get_builtin_tutorial_repositories() -> Result<Vec<RepositoryExport>, String> {
    let region = crate::config::current_user_region();
    let repositories = builtin_tutorial_repositories_for_region(region)?;
    log::debug!(
        "已读取内置教程 MOD 配置：地区={}，仓库数={}",
        region,
        repositories.len()
    );
    Ok(repositories)
}

#[cfg(test)]
mod repository_source_refresh_tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn test_state() -> AppState {
        AppState {
            config: ConfigService::for_test(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("target/test-source-refresh")
                    .join(Uuid::new_v4().to_string()),
            ),
            operation: Arc::new(Mutex::new(())),
            repository_cache: Arc::new(tokio::sync::Mutex::new(())),
            community: Arc::new(Default::default()),
            readme: Arc::new(Default::default()),
            game_state: Arc::new(Mutex::new(None)),
        }
    }

    async fn refresh(state: &AppState, source: &RepositorySource) -> Result<ImportPreview, String> {
        let result = import_repository_source_into_config(
            state,
            source.id.clone(),
            source.url.clone(),
            false,
        )
        .await?;
        if let Some(error) = result.access_error {
            return Err(error);
        }
        result
            .preview
            .ok_or_else(|| "missing import preview".into())
    }

    #[tokio::test]
    async fn bundled_fallback_keeps_remote_entry_and_rejects_custom_sources() {
        let state = test_state();
        /*
         * 这条用例验证的是「在线官方根失败 → 回退内置」。默认配置现在只读
         * 内置索引（`default_config`），所以先显式切回在线入口：只有远端
         * URL 才能触发 `builtin_fallback_available`。
         *
         * 根地址要一起换掉 —— `save` 会把这份配置写进读取缓存，随后的
         * `load` 不会再跑一遍按开关重建官方根的逻辑。
         */
        let mut online = state.config.load().unwrap();
        online.builtin_sources_only = false;
        online.repository_sources = crate::config::builtin_repository_sources_for_region(
            crate::config::current_user_region(),
            false,
        );
        state.config.save(&online).unwrap();
        let mut config = state.config.load().unwrap();
        let root = config.repository_sources[0].clone();
        let custom = RepositorySource {
            id: "custom-source".into(),
            nickname: "Custom".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/custom.toml".into(),
            region: None,
            builtin: false,
            parent_id: None,
        };
        config.repository_sources.push(custom.clone());
        state.config.save(&config).unwrap();
        assert!(
            import_repository_source_into_config(
                &state,
                custom.id.clone(),
                custom.url.clone(),
                true
            )
            .await
            .is_err()
        );
        assert_eq!(
            state.config.load().unwrap().repository_sources,
            config.repository_sources
        );
        let result =
            import_repository_source_into_config(&state, root.id.clone(), root.url.clone(), true)
                .await
                .unwrap();
        assert!(result.preview.unwrap().valid);
        assert!(result.access_error.is_none());
        assert!(!result.builtin_fallback_available);
        let loaded = state.config.load().unwrap();
        assert_eq!(loaded.repository_sources[0], root);
        assert!(loaded.repository_sources.contains(&custom));
        let children = loaded
            .repository_sources
            .iter()
            .filter(|s| s.parent_id.as_deref() == Some(root.id.as_str()))
            .collect::<Vec<_>>();
        /*
         * 数量从内置索引**实际声明**推导，而不是写死。
         *
         * 之前这里是 `assert_eq!(children.len(), 5)`：作者往索引里加一个子源
         * 就会让这条测试失败，而失败原因与回退逻辑毫无关系 —— 纯粹是常量过期。
         * 真正要断言的是「索引声明了几个，回退后就该有几个」。
         */
        let declared_children = {
            let location = SourceLocation::parse(repository_sources::BUILTIN_INDEX).unwrap();
            let document =
                repository_sources::parse_document(location.bundled_text().unwrap(), &location)
                    .unwrap();
            document.repository_sources.len()
        };
        assert!(declared_children > 0, "内置索引应当至少声明一个子源");
        assert_eq!(children.len(), declared_children);
        for child in &children {
            assert!(matches!(
                SourceLocation::parse(&child.url).unwrap(),
                SourceLocation::Bundled(_)
            ));
            refresh(&state, child).await.unwrap();
        }
        let loaded = state.config.load().unwrap();
        assert_eq!(loaded.repository_sources[0].url, root.url);
        let expected_repository_count = children
            .iter()
            .map(|child| {
                SourceLocation::parse(&child.url)
                    .unwrap()
                    .bundled_document()
                    .unwrap()
                    .repositories
                    .len()
            })
            .sum::<usize>();
        assert_eq!(loaded.repositories.len(), expected_repository_count);
        assert!(loaded.repository_sources.contains(&custom));
        fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    async fn live_index_changes_replace_children_and_failure_keeps_no_old_content() {
        use std::io::{BufRead, BufReader, Write};
        let declaration = |id, nickname: &str| {
            format!(
                "[[repository_sources]]\nid = \"{id}\"\nnickname = \"{nickname}\"\nurl = \"{id}.toml\"\n[repository_sources.nickname_i18n]\nzh-CN = \"{nickname}\"\n"
            )
        };
        let repository = |name| {
            format!(
                "[[repositories]]\nname = \"{name}\"\nname_i18n.zh-CN = \"{name}\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{type = \"glob\", value = \"*.dll\"}}]\n"
            )
        };
        let responses = vec![
            ("/index.toml", "200 OK", declaration("a", "Old source")),
            ("/a.toml", "200 OK", repository("Old MOD")),
            (
                "/index.toml",
                "200 OK",
                format!(
                    "{}{}",
                    declaration("a", "Changed source"),
                    declaration("b", "New source")
                ),
            ),
            ("/a.toml", "200 OK", repository("Changed MOD")),
            ("/index.toml", "200 OK", declaration("b", "New source")),
            ("/b.toml", "200 OK", repository("New MOD")),
            ("/index.toml", "503 Service Unavailable", String::new()),
            ("/index.toml", "200 OK", "invalid = [".into()),
            (
                "/index.toml",
                "200 OK",
                declaration("b", "Recovered source"),
            ),
            ("/b.toml", "200 OK", repository("Recovered MOD")),
        ];
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        listener.set_nonblocking(true).unwrap();
        let server = std::thread::spawn(move || {
            for (path, status, body) in responses {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "request did not arrive");
                            std::thread::sleep(Duration::from_millis(10));
                        }
                        Err(error) => panic!("{error}"),
                    }
                };
                // Accepted sockets inherit nonblocking mode on Windows.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                loop {
                    let mut line = String::new();
                    assert!(reader.read_line(&mut line).unwrap() > 0);
                    if line == "\r\n" {
                        break;
                    }
                    request.push_str(&line);
                }
                assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
                assert!(
                    request
                        .to_ascii_lowercase()
                        .contains("cache-control: no-cache, no-store, max-age=0\r\n")
                );
                assert!(!request.to_ascii_lowercase().contains("if-none-match:"));
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .unwrap();
            }
        });
        let state = test_state();
        let mut config = state.config.load().unwrap();
        let root = RepositorySource {
            id: "live-index-test".into(),
            nickname: "Live index".into(),
            nickname_i18n: Default::default(),
            url: format!("http://{address}/index.toml"),
            region: None,
            builtin: false,
            parent_id: None,
        };
        let mut unrelated = root.clone();
        unrelated.id = "unrelated-custom-source".into();
        unrelated.url = "https://example.com/custom.toml".into();
        config
            .repository_sources
            .extend([root.clone(), unrelated.clone()]);
        state.config.save(&config).unwrap();
        let mut installed_repository_ids = std::collections::HashMap::new();
        for (expected_source_name, expected_mod_name, child_id) in [
            ("Old source", "Old MOD", "a"),
            ("Changed source", "Changed MOD", "a"),
            ("New source", "New MOD", "b"),
        ] {
            refresh(&state, &root).await.unwrap();
            let index_config = state.config.load().unwrap();
            assert!(index_config.repositories.is_empty());
            let child = index_config
                .repository_sources
                .iter()
                .find(|s| s.id == child_id)
                .unwrap();
            assert_eq!(child.nickname, expected_source_name);
            assert_eq!(child.nickname_i18n["zh-CN"], expected_source_name);
            assert_eq!(child.url, format!("http://{address}/{child_id}.toml"));
            assert_eq!(child.parent_id.as_deref(), Some(root.id.as_str()));
            refresh(&state, child).await.unwrap();
            let loaded = state.config.load().unwrap();
            assert_eq!(loaded.repositories.len(), 1);
            assert_eq!(loaded.repositories[0].name, expected_mod_name);
            assert!(loaded.repository_sources.contains(&unrelated));
            if let Some(previous_id) = installed_repository_ids.get(child_id) {
                assert_eq!(
                    &loaded.repositories[0].id, previous_id,
                    "refresh must preserve the ID used by installed MOD manifests"
                );
            } else {
                installed_repository_ids.insert(child_id, loaded.repositories[0].id.clone());
            }
            if child_id == "b" {
                assert!(!loaded.repository_sources.iter().any(|s| s.id == "a"));
            }
        }
        let inaccessible =
            import_repository_source_into_config(&state, root.id.clone(), root.url.clone(), false)
                .await
                .unwrap();
        assert!(inaccessible.access_error.is_some());
        assert!(inaccessible.preview.is_none());
        assert!(
            !inaccessible.builtin_fallback_available,
            "custom sources cannot fallback"
        );
        assert!(
            refresh(&state, &root).await.is_err(),
            "invalid TOML remains a content error"
        );
        let failed = state.config.load().unwrap();
        assert!(failed.repositories.is_empty());
        assert!(
            !failed
                .repository_sources
                .iter()
                .any(|s| s.parent_id.as_deref() == Some(root.id.as_str()))
        );
        assert!(failed.repository_sources.contains(&root));
        assert!(failed.repository_sources.contains(&unrelated));
        // A new state owns no in-memory identity cache. Recovery must use the
        // persisted ID map, while still fetching the changed remote content.
        let restarted = AppState {
            config: ConfigService::for_test(
                state.config.config_path().parent().unwrap().to_path_buf(),
            ),
            operation: Arc::new(Mutex::new(())),
            repository_cache: Arc::new(tokio::sync::Mutex::new(())),
            community: Arc::new(Default::default()),
            readme: Arc::new(Default::default()),
            game_state: Arc::new(Mutex::new(None)),
        };
        refresh(&restarted, &root).await.unwrap();
        let restored = restarted.config.load().unwrap();
        let child = restored
            .repository_sources
            .iter()
            .find(|source| source.id == "b")
            .unwrap();
        refresh(&restarted, child).await.unwrap();
        let restored = restarted.config.load().unwrap();
        assert_eq!(restored.repositories[0].id, installed_repository_ids["b"]);
        assert_eq!(restored.repositories[0].name, "Recovered MOD");
        assert!(restored.repositories[0].cached_releases.is_empty());
        server.join().unwrap();
        fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
    }

    #[tokio::test]
    #[ignore = "requires access to the maintained official remote source"]
    async fn official_remote_index_loads_each_declared_child() {
        let state = test_state();
        // 线上索引用例：默认配置只读内置快照，这里显式切回在线入口。
        // `save` 会把配置放进读取缓存，所以根地址一并换掉。
        let mut online = state.config.load().unwrap();
        online.builtin_sources_only = false;
        online.repository_sources = crate::config::builtin_repository_sources_for_region(
            crate::config::current_user_region(),
            false,
        );
        state.config.save(&online).unwrap();
        let config = state.config.load().unwrap();
        let root = config.repository_sources[0].clone();
        assert_eq!(
            root.url,
            repository_sources::default_index_url(crate::config::current_user_region())
        );
        // Simulate a previous offline run, then ensure an ordinary refresh
        // returns to the live URL and replaces the bundled children.
        import_repository_source_into_config(&state, root.id.clone(), root.url.clone(), true)
            .await
            .unwrap();
        refresh(&state, &root).await.unwrap();
        let mut pending = state
            .config
            .load()
            .unwrap()
            .repository_sources
            .into_iter()
            .filter(|s| s.parent_id.as_deref() == Some(root.id.as_str()))
            .collect::<Vec<_>>();
        assert!(!pending.is_empty());
        let mut visited = HashSet::from([root.id]);
        while let Some(source) = pending.pop() {
            assert!(visited.insert(source.id.clone()), "source cycle");
            assert!(matches!(
                SourceLocation::parse(&source.url).unwrap(),
                SourceLocation::Remote(_)
            ));
            refresh(&state, &source).await.unwrap();
            let loaded = state.config.load().unwrap();
            eprintln!(
                "source={} repositories={}",
                source.id,
                loaded
                    .repositories
                    .iter()
                    .filter(|r| r.source_id.as_deref() == Some(source.id.as_str()))
                    .count()
            );
            pending.extend(
                loaded
                    .repository_sources
                    .into_iter()
                    .filter(|s| s.parent_id.as_deref() == Some(source.id.as_str())),
            );
        }
        let loaded = state.config.load().unwrap();
        assert!(!loaded.repositories.is_empty());
        assert!(loaded.repository_sources.iter().all(|s| s.builtin));
        eprintln!(
            "live index: sources={} repositories={}",
            loaded.repository_sources.len(),
            loaded.repositories.len()
        );
        fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
    }
}

#[cfg(test)]
mod builtin_tutorial_repository_tests {
    use super::*;

    #[test]
    fn tutorial_repository_uses_bundled_configuration_for_each_region_without_legacy_urls() {
        let source = SourceLocation::parse("repositories-azhe-hostutilities.toml").unwrap();
        let document = source.bundled_document().unwrap();
        for (region, provider) in [
            (crate::config::REGION_CHINESE_MAINLAND, Provider::Gitee),
            (crate::config::REGION_INTERNATIONAL, Provider::Github),
        ] {
            let repositories = builtin_tutorial_repositories_for_region(region).unwrap();
            assert_eq!(repositories.len(), 2);
            let expected = document
                .repositories
                .iter()
                .find(|repository| {
                    repository.repository == "Overcooked-2-HostUtilities-Stable-Releases"
                        && repository.provider == provider
                        && {
                            let configured_region = crate::config::normalize_repository_region(
                                repository.region.as_deref(),
                            );
                            configured_region.is_empty() || configured_region == region
                        }
                })
                .unwrap();
            let mut actual = repositories[0].clone();
            assert_eq!(actual.provider, provider);
            assert_eq!(
                actual.source_id.as_deref(),
                Some("repositories-azhe-hostutilities")
            );
            actual.source_id = expected.source_id.clone();
            assert_eq!(
                serde_json::to_value(&actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            assert!(serde_json::to_value(&actual).unwrap().get("url").is_none());
            assert_eq!(repositories[1].repository, "BepInEx.ConfigurationManager");
        }
    }
}

/// 多彩名字推送的请求契约。
///
/// MOD 的 `DisplayNameWebService.IsValidToken` 在没有 `X-OC2-Token` 时只认
/// `Origin` 精确等于官方改名网页，缺这个头会稳定回 403「Token 不匹配」。
/// 真实事故：管理器只发了 Content-Type，用户看到的是 403，日志却写成
/// 「端口均无响应」，把鉴权失败误报成「MOD 没运行」。
#[cfg(test)]
mod colorful_name_push_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// 起一个只服务一次请求的本地服务，返回它的地址和收到的原始请求头。
    fn one_shot_server(response: &'static str) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..read]).into_owned();
            let _ = stream.write_all(response.as_bytes());
            request
        });
        (format!("http://{address}/set-name"), handle)
    }

    fn client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(3000))
            .build()
            .unwrap()
    }

    #[tokio::test]
    async fn sends_the_official_origin_header_the_mod_accepts() {
        let (url, server) = one_shot_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
        );
        let response = send_colorful_name(&client(), &url, "test-name", None)
            .await
            .unwrap();
        assert!(response.status().is_success());
        let request = server.join().unwrap();
        let lower = request.to_ascii_lowercase();
        // MOD 逐字节比较 Origin（忽略大小写），值必须与常量完全一致。
        assert!(
            request.contains(&format!("Origin: {COLORFUL_NAME_ORIGIN}"))
                || lower.contains(&format!(
                    "origin: {}",
                    COLORFUL_NAME_ORIGIN.to_ascii_lowercase()
                )),
            "请求必须带官方 Origin 头，实际请求：{request}"
        );
        assert!(
            lower.contains("content-type: application/json"),
            "请求必须声明 JSON，MOD 只有在 JSON 内容类型下才解析 name 字段：{request}"
        );
        assert!(
            request.contains("\"name\""),
            "正文必须含 name 字段：{request}"
        );
    }

    /// 测试用假 token：32 位十六进制，形状与 MOD 的真实 token 一致。
    ///
    /// 运行期拼出来而不是写成字面量：仓库的敏感信息扫描会把长十六进制串
    /// 当成真实凭据打码，字面量会被改坏成非法形状。
    fn fixture_token() -> String {
        format!(
            "{:032x}",
            0x0c02_c010_ffee_1234_u128 ^ 0x1111_1111_1111_1111
        )
    }

    #[tokio::test]
    async fn a_handshake_token_replaces_the_origin_header() {
        let (url, server) = one_shot_server(
            "HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}",
        );
        let token = fixture_token();
        let response = send_colorful_name(&client(), &url, "test-name", Some(&token))
            .await
            .unwrap();
        assert!(response.status().is_success());
        let request = server.join().unwrap();
        let lower = request.to_ascii_lowercase();
        assert!(
            lower.contains(&format!("x-oc2-token: {token}")),
            "有 token 时必须带 X-OC2-Token：{request}"
        );
        // token 是强凭据，不需要再冒充网页来源。
        assert!(
            !lower.contains("origin:"),
            "token 通道不应再带 Origin：{request}"
        );
    }

    #[tokio::test]
    async fn a_rejected_port_is_not_reported_as_unreachable() {
        // MOD 在缺 token / Origin 不匹配时的真实响应。
        let (url, server) = one_shot_server(
            "HTTP/1.1 403 Forbidden\r\nContent-Length: 13\r\nConnection: close\r\n\r\nToken 不匹配",
        );
        let response = send_colorful_name(&client(), &url, "test-name", None).await;
        match classify_colorful_name(28780, response).await {
            ColorfulNameOutcome::Rejected(port, detail) => {
                assert_eq!(port, 28780);
                // 状态码和 MOD 的正文都要留下，才能区分「鉴权失败」和「版本不对」。
                assert!(detail.contains("403"), "拒绝详情应含状态码：{detail}");
                assert!(
                    detail.contains("Token"),
                    "拒绝详情应保留 MOD 正文：{detail}"
                );
            }
            ColorfulNameOutcome::Delivered(_) => panic!("403 不能算投递成功"),
            ColorfulNameOutcome::Unreachable(_) => {
                panic!("连上并被拒绝不能被当成「端口无响应」")
            }
        }
        server.join().unwrap();

        // 未被占用的端口才应该落到 Unreachable。
        let closed = TcpListener::bind("127.0.0.1:0").unwrap();
        let closed_port = closed.local_addr().unwrap().port();
        drop(closed);
        let unreachable = format!("http://127.0.0.1:{closed_port}/set-name");
        let response = send_colorful_name(&client(), &unreachable, "test-name", None).await;
        assert!(matches!(
            classify_colorful_name(closed_port, response).await,
            ColorfulNameOutcome::Unreachable(port) if port == closed_port
        ));
    }

    /// 握手文件的接受条件：版本、端口段、token 形状三项都必须成立。
    ///
    /// 这个文件位于游戏目录，任何能写游戏目录的程序都能伪造；校验的目的
    /// 是「不把畸形或过期内容当凭据」，随即退回官方 Origin，而不是让推送
    /// 直接失败。
    #[test]
    fn handshake_is_only_accepted_with_a_known_version_port_and_token() {
        let workspace =
            std::env::temp_dir().join(format!("oc2-colorful-handshake-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(workspace.join("BepInEx/config/OC2HostUtilities")).unwrap();
        let write = |content: &str| {
            std::fs::write(
                workspace.join("BepInEx/config/OC2HostUtilities/web-bridge.json"),
                content,
            )
            .unwrap()
        };

        let token = fixture_token();
        let upper = token.to_ascii_uppercase();

        // 正常握手
        write(&format!(
            r#"{{"version":1,"port":28783,"token":"{upper}"}}"#
        ));
        match read_colorful_name_handshake(&workspace) {
            Some(read) => {
                assert_eq!(read.port, 28783);
                // 统一小写，MOD 比较时不受大小写影响
                assert_eq!(read.token, token);
            }
            None => panic!("合法握手应被接受"),
        }

        // 版本未知（未来格式）：退回 Origin，而不是猜着用
        write(&format!(
            r#"{{"version":2,"port":28783,"token":"{token}"}}"#
        ));
        assert!(read_colorful_name_handshake(&workspace).is_none());

        // 端口越界
        write(&format!(r#"{{"version":1,"port":9999,"token":"{token}"}}"#));
        assert!(read_colorful_name_handshake(&workspace).is_none());

        // token 形状不对（长度或字符）
        write(r#"{"version":1,"port":28783,"token":"short"}"#);
        assert!(read_colorful_name_handshake(&workspace).is_none());
        write(&format!(
            r#"{{"version":1,"port":28783,"token":"{}"}}"#,
            "z".repeat(32)
        ));
        assert!(read_colorful_name_handshake(&workspace).is_none());

        // 文件不存在 / JSON 损坏
        std::fs::remove_file(workspace.join("BepInEx/config/OC2HostUtilities/web-bridge.json"))
            .unwrap();
        assert!(read_colorful_name_handshake(&workspace).is_none());
        write("not json");
        assert!(read_colorful_name_handshake(&workspace).is_none());

        std::fs::remove_dir_all(workspace).unwrap();
    }

    /// 握手文件必须按 `detect_game` 探测到的目录去找。
    ///
    /// 真实事故：`config.game.path` 为空（目录是运行时探测的，用户没手动填
    /// 过），读握手文件的代码却只看配置字段，于是判定「没配目录」直接走
    /// Origin 兜底；而当前街机MOD 只认 `X-OC2-Token`，用户拿到的是 403。
    /// 这里用源码断言把「用探测结果」这件事钉死，因为这条路径需要真实游戏
    /// 目录才能触发，纯逻辑测试覆盖不到。
    #[test]
    fn handshake_lookup_uses_the_detected_game_path_not_the_config_field() {
        let source = include_str!("commands.rs");
        let body = source
            .split("pub async fn push_colorful_name")
            .nth(1)
            .and_then(|tail| tail.split("fn parse_tutorial_document").next())
            .expect("push_colorful_name 函数体应当可定位");
        // 注释里会引用这个字段名来说明为什么不能用它，先剥掉块注释与行注释再断言。
        let code = strip_comments(body);
        assert!(
            code.contains("game::detect_game(&config)"),
            "读握手文件必须用 detect_game 的探测结果；配置里的 game.path 可能为空"
        );
        assert!(
            !code.contains("config.game.path"),
            "不要再直接读 config.game.path：该字段为空时会让握手文件永远找不到"
        );
    }

    /// 去掉 `/* … */` 与 `// …`，避免断言命中说明性的注释文字。
    fn strip_comments(source: &str) -> String {
        let mut out = String::new();
        let mut rest = source;
        while let Some(start) = rest.find("/*") {
            out.push_str(&rest[..start]);
            match rest[start..].find("*/") {
                Some(end) => rest = &rest[start + end + 2..],
                None => return out,
            }
        }
        out.push_str(rest);
        out.lines()
            .map(|line| match line.find("//") {
                Some(index) => &line[..index],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// 包确认的触发条件。
///
/// 真实 bug：包标题上的「删除」按钮已经按整包语义调用
/// （`allow_unmanaged = true`），后端却仍然返回
/// `requires_package_confirmation`，提示「当前 MOD 与其他 MOD 一并安装」，
/// 看起来像是没删干净。判定必须读 `allow_unmanaged`。
#[cfg(test)]
mod package_confirmation_tests {
    use super::*;

    fn siblings(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_string()).collect()
    }

    #[test]
    fn single_dll_uninstall_still_asks_before_deleting_the_whole_package() {
        // 点单个 DLL、包内还有别人、没有同 GUID 副本 → 需要二次确认
        assert!(needs_package_confirmation(
            &siblings(&["A", "B"]),
            false,
            false
        ));
    }

    #[test]
    fn package_level_uninstall_never_asks_again() {
        // 包标题的「删除」/ 冲突卸载：已经表达过整包语义，不能重复确认
        assert!(!needs_package_confirmation(
            &siblings(&["A", "B"]),
            false,
            true
        ));
    }

    #[test]
    fn other_cases_skip_the_confirmation() {
        // 包内没有其他 MOD：删的就是单个文件
        assert!(!needs_package_confirmation(&siblings(&[]), false, false));
        // 存在同 GUID 的启用副本：走单 DLL 回收分支，不是整包删除
        assert!(!needs_package_confirmation(
            &siblings(&["A", "B"]),
            true,
            false
        ));
    }
}

/// DIYLevel 地图集计数。
///
/// 约定：`levels/` 下**每个文件夹算一个地图集**，直接放在 `levels/` 里的
/// 散文件（说明、缩略图）不属于任何地图集。这条口径要和总览、设置抽屉
/// 显示的「地图集 N」完全一致，所以钉在这里。
#[cfg(test)]
mod diy_level_packs_tests {
    use super::*;

    #[test]
    fn counts_only_directories_under_levels() {
        let root = std::env::temp_dir().join(format!("oc2-diy-levels-{}", uuid::Uuid::new_v4()));
        let levels = root.join("levels");
        std::fs::create_dir_all(&levels).unwrap();

        // 空目录：0 个地图集
        assert_eq!(count_level_packs(&levels), 0);

        // 两个文件夹 + 三个散文件 → 只数出来 2
        std::fs::create_dir_all(levels.join("Pack A")).unwrap();
        std::fs::create_dir_all(levels.join("Pack B")).unwrap();
        std::fs::write(levels.join("readme.txt"), b"notes").unwrap();
        std::fs::write(levels.join("preview.png"), b"png").unwrap();
        std::fs::write(levels.join("config.json"), b"{}").unwrap();
        assert_eq!(count_level_packs(&levels), 2);

        // 嵌套目录只按顶层算：Pack A 里再建一个不算新地图集
        std::fs::create_dir_all(levels.join("Pack A/inner")).unwrap();
        assert_eq!(count_level_packs(&levels), 2);

        // 目录不存在：0，不报错
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(count_level_packs(&levels), 0);
    }
}

/// DIYLevel 备份的目录结构与校验规则。
///
/// 用户要求：备份只包含 `BepInEx/plugins/OC2DIYLevel` 这一个文件夹；
/// 恢复时先删掉现有目录再整体还原。这里钉住备份产物的形状与校验边界，
/// 避免以后有人把备份范围扩大到整个 BepInEx（那会和 BepInEx 备份重复）。
#[cfg(test)]
mod diy_level_backup_tests {
    use super::*;

    #[test]
    fn backup_copies_only_the_diy_level_folder() {
        let game =
            std::env::temp_dir().join(format!("oc2-diy-backup-game-{}", uuid::Uuid::new_v4()));
        let backups =
            std::env::temp_dir().join(format!("oc2-diy-backup-root-{}", uuid::Uuid::new_v4()));
        // 框架目录：DLL + 一个地图集
        std::fs::create_dir_all(game.join("BepInEx/plugins/OC2DIYLevel/levels/PackA")).unwrap();
        std::fs::write(
            game.join("BepInEx/plugins/OC2DIYLevel/DIYLevel.dll"),
            b"dll",
        )
        .unwrap();
        std::fs::write(
            game.join("BepInEx/plugins/OC2DIYLevel/levels/PackA/map.txt"),
            b"map",
        )
        .unwrap();
        // 同级还有别的东西，绝不能被备份进来
        std::fs::create_dir_all(game.join("BepInEx/plugins/OtherMod")).unwrap();
        std::fs::write(game.join("BepInEx/plugins/OtherMod/other.dll"), b"other").unwrap();
        std::fs::create_dir_all(game.join("BepInEx/core")).unwrap();
        std::fs::write(game.join("BepInEx/core/BepInEx.dll"), b"core").unwrap();

        let manifests =
            std::env::temp_dir().join(format!("oc2-diy-manifests-{}", uuid::Uuid::new_v4()));
        let backup =
            backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "diy-repo").unwrap();
        assert!(backup.starts_with(&backups), "备份必须落在备份根下");
        assert!(
            backup
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(DIYLEVEL_BACKUP_PREFIX),
            "备份目录名必须带前缀，恢复/删除才认得出"
        );
        /*
         * 备份内保留**游戏相对路径**：`BepInEx/plugins/OC2DIYLevel/...`。
         * 恢复时按同一相对路径拷回，不需要为每个框架单独记映射。
         */
        const IN_BACKUP: &str = "BepInEx/plugins/OC2DIYLevel";
        assert_eq!(
            std::fs::read(backup.join(IN_BACKUP).join("DIYLevel.dll")).unwrap(),
            b"dll"
        );
        assert_eq!(
            std::fs::read(backup.join(IN_BACKUP).join("levels/PackA/map.txt")).unwrap(),
            b"map"
        );
        // 兄弟插件与加载器核心都不在备份里
        assert!(!backup.join("BepInEx/plugins/OtherMod").exists());
        assert!(!backup.join("BepInEx/core").exists());

        /*
         * 清单快照必须一起进备份：这是「恢复后还能正常卸载」的前提。
         * 没有现成清单时按磁盘实际文件现造一份。
         */
        let manifest = read_backup_manifest(&backup).expect("备份必须带清单快照");
        assert_eq!(manifest.repository_id, "diy-repo");
        assert!(
            manifest
                .files
                .iter()
                .any(|file| file.ends_with("OC2DIYLevel/DIYLevel.dll")),
            "清单应包含框架主文件：{:?}",
            manifest.files
        );

        // 列表能列出它
        let listed = list_diy_level_backups(&backups);
        assert_eq!(listed.len(), 1);
        assert!(listed[0].contains("DIYLevel.backup-"));

        std::fs::remove_dir_all(&game).unwrap();
        std::fs::remove_dir_all(&backups).unwrap();
        // manifests 目录可能从未被创建（没有现成清单时不需要它）
        let _ = std::fs::remove_dir_all(&manifests);
    }

    #[test]
    fn backup_requires_the_diy_level_folder_to_exist() {
        let game = std::env::temp_dir().join(format!("oc2-diy-missing-{}", uuid::Uuid::new_v4()));
        let backups = std::env::temp_dir().join(format!("oc2-diy-none-{}", uuid::Uuid::new_v4()));
        let manifests = std::env::temp_dir().join(format!("oc2-diy-m-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
        assert!(
            backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "diy").is_err(),
            "没有 OC2DIYLevel 目录时必须报错，而不是生成空备份"
        );
        std::fs::remove_dir_all(&game).unwrap();
    }

    #[test]
    fn only_timestamped_backups_inside_the_backup_root_are_accepted() {
        let root = std::env::temp_dir().join(format!("oc2-diy-valid-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("DIYLevel.backup-20261029120000-abc")).unwrap();
        std::fs::create_dir_all(root.join("SomethingElse")).unwrap();
        // 备份根之外的目录，名字对也不能用
        let outside =
            std::env::temp_dir().join(format!("oc2-diy-outside-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(outside.join("DIYLevel.backup-20261029120000-abc")).unwrap();

        let valid = root.join("DIYLevel.backup-20261029120000-abc");
        assert!(validate_diy_level_backup_in(&valid, &root).is_ok());
        // 名字不带前缀
        assert!(validate_diy_level_backup_in(&root.join("SomethingElse"), &root).is_err());
        // 父目录不是备份根
        assert!(
            validate_diy_level_backup_in(
                &outside.join("DIYLevel.backup-20261029120000-abc"),
                &root
            )
            .is_err()
        );

        std::fs::remove_dir_all(&root).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }
}

/// 「备份 → 卸载 → 恢复 → 再卸载」的完整往返。
///
/// 真实事故：旧备份只拷目录、不带安装清单，用户恢复后再卸载会报
/// 「没有安装清单，拒绝删除未知文件」，只能手动删目录。这里把整条链路
/// 钉住：恢复后清单必须回到管理器清单目录，且内容覆盖框架主文件。
#[cfg(test)]
mod framework_backup_roundtrip_tests {
    use super::*;

    /// 造一个最小 BepInEx 框架目录（含核心程序集与根目录加载器文件）。
    fn write_bepinex(game: &Path) {
        std::fs::create_dir_all(game.join("BepInEx/core")).unwrap();
        std::fs::write(game.join("BepInEx/core/BepInEx.dll"), b"core").unwrap();
        std::fs::write(game.join("winhttp.dll"), b"loader").unwrap();
        std::fs::write(game.join("doorstop_config.ini"), b"cfg").unwrap();
        // MOD 目录：属于用户数据，必须一起备份但不能被当成框架文件
        std::fs::create_dir_all(game.join("BepInEx/plugins/SomeMod")).unwrap();
        std::fs::write(game.join("BepInEx/plugins/SomeMod/mod.dll"), b"mod").unwrap();
    }

    #[test]
    fn bepinex_backup_restores_root_loader_files_and_manifest() {
        let root = std::env::temp_dir().join(format!("oc2-fw-rt-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let backups = root.join("backups");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();
        write_bepinex(&game);

        let backup = backup_framework(
            &game,
            &backups,
            &BEPINEX_BACKUP,
            &manifests,
            BEPINEX_MANIFEST_ID,
        )
        .unwrap();

        /*
         * 备份必须包含游戏根目录的加载器文件。
         * 早期只拷 `BepInEx/`，恢复后 winhttp.dll 缺失 —— 用户在界面看到的
         * 就是「恢复完了但框架没装好」。
         */
        assert!(backup.join("winhttp.dll").is_file(), "备份缺 winhttp.dll");
        assert!(
            backup.join("doorstop_config.ini").is_file(),
            "备份缺 doorstop_config.ini"
        );
        assert!(
            backup.join("BepInEx/core/BepInEx.dll").is_file(),
            "备份缺核心程序集"
        );

        // 模拟卸载：清掉游戏目录里的框架（含根文件）
        std::fs::remove_dir_all(game.join("BepInEx")).unwrap();
        std::fs::remove_file(game.join("winhttp.dll")).unwrap();
        std::fs::remove_file(game.join("doorstop_config.ini")).unwrap();
        assert!(!crate::bepinex::installed(&game));

        // 恢复
        restore_framework_backup(
            &game,
            &backup,
            &BEPINEX_BACKUP,
            &manifests,
            BEPINEX_MANIFEST_ID,
        )
        .unwrap();

        // 核心程序集与根文件都必须回来
        assert!(
            crate::bepinex::installed(&game),
            "恢复后必须重新判定为已安装"
        );
        assert!(game.join("winhttp.dll").is_file(), "恢复后缺 winhttp.dll");
        assert!(
            game.join("doorstop_config.ini").is_file(),
            "恢复后缺 doorstop_config.ini"
        );
        assert!(
            game.join("BepInEx/plugins/SomeMod/mod.dll").is_file(),
            "恢复必须带回 plugins 里的 MOD"
        );

        // 清单必须写回：这是「恢复后还能再卸载」的关键
        let manifest = archive::read_manifest(&manifests, BEPINEX_MANIFEST_ID)
            .expect("恢复后必须存在安装清单");
        assert!(
            manifest.files.iter().any(|file| file == "winhttp.dll"),
            "清单应包含根目录加载器文件：{:?}",
            manifest.files
        );
        assert!(
            manifest
                .files
                .iter()
                .any(|file| file.ends_with("BepInEx/core/BepInEx.dll")),
            "清单应包含核心程序集：{:?}",
            manifest.files
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn restoring_without_a_backup_manifest_still_writes_one() {
        /*
         * 旧版本产生的备份没有清单快照。恢复时必须按磁盘实际文件现造一份，
         * 否则用户升级管理器后恢复旧备份，再卸载仍然会报
         * 「没有安装清单，拒绝删除未知文件」。
         */
        let root = std::env::temp_dir().join(format!("oc2-fw-legacy-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let backups = root.join("backups");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();
        write_bepinex(&game);

        let backup = backup_framework(
            &game,
            &backups,
            &BEPINEX_BACKUP,
            &manifests,
            BEPINEX_MANIFEST_ID,
        )
        .unwrap();
        // 模拟旧备份：删掉清单快照
        std::fs::remove_file(backup.join(FRAMEWORK_BACKUP_MANIFEST)).unwrap();
        assert!(read_backup_manifest(&backup).is_none());

        // 清空管理器清单目录，模拟「从未装过、也没有清单」
        std::fs::remove_dir_all(&manifests).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();

        restore_framework_backup(
            &game,
            &backup,
            &BEPINEX_BACKUP,
            &manifests,
            BEPINEX_MANIFEST_ID,
        )
        .unwrap();
        let manifest = archive::read_manifest(&manifests, BEPINEX_MANIFEST_ID)
            .expect("旧备份恢复后也必须现造出清单");
        assert!(!manifest.files.is_empty());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn diy_level_backup_never_touches_the_bepinex_loader() {
        /*
         * DIYLevel 的备份范围必须严格是 `BepInEx/plugins/OC2DIYLevel`：
         * `BepInEx/core` 归 BepInEx 自己的备份管，两边都备份会互相覆盖。
         */
        let root = std::env::temp_dir().join(format!("oc2-fw-diy-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let backups = root.join("backups");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();
        write_bepinex(&game);
        std::fs::create_dir_all(game.join("BepInEx/plugins/OC2DIYLevel/levels/PackA")).unwrap();
        std::fs::write(
            game.join("BepInEx/plugins/OC2DIYLevel/DIYLevel.dll"),
            b"diy",
        )
        .unwrap();

        let backup =
            backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "diy-id").unwrap();
        // 只有 DIYLevel 目录，没有 core、没有 winhttp.dll
        assert!(
            backup
                .join("BepInEx/plugins/OC2DIYLevel/DIYLevel.dll")
                .is_file()
        );
        assert!(!backup.join("BepInEx/core").exists());
        assert!(!backup.join("winhttp.dll").exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 恢复 BepInEx 后，`.before-restore-*` 暂存目录必须被删掉。
    ///
    /// 它落在游戏根目录（`BepInEx.before-restore-<ts>`）。留着不只是占空间：
    /// MOD 扫描会把里面的 DLL 当成插件，用户看到「刚恢复完就多出一批重复 MOD」。
    #[test]
    fn bepinex_restore_removes_the_staging_directory() {
        let root = std::env::temp_dir().join(format!("oc2-fw-stage-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let backups = root.join("backups");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();
        write_bepinex(&game);

        let backup = backup_framework(
            &game,
            &backups,
            &BEPINEX_BACKUP,
            &manifests,
            BEPINEX_MANIFEST_ID,
        )
        .unwrap();
        // 备份之后改动目录，让「恢复前内容」与备份内容不同
        std::fs::write(game.join("BepInEx/core/Extra.dll"), b"extra").unwrap();

        restore_framework_backup(
            &game,
            &backup,
            &BEPINEX_BACKUP,
            &manifests,
            BEPINEX_MANIFEST_ID,
        )
        .unwrap();

        let leftovers: Vec<_> = std::fs::read_dir(&game)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".before-restore-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "恢复后不应残留暂存目录：{leftovers:?}"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    /// 恢复 DIYLevel 后同样不能残留 —— 它的暂存目录在 `plugins/` 里面，
    /// 不删就必然被 MOD 扫描当成插件（比 BepInEx 的情况更严重）。
    #[test]
    fn diy_level_restore_removes_the_staging_directory_inside_plugins() {
        let root = std::env::temp_dir().join(format!("oc2-diy-stage-{}", uuid::Uuid::new_v4()));
        let game = root.join("game");
        let backups = root.join("backups");
        let manifests = root.join("manifests");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::create_dir_all(&manifests).unwrap();

        let diy = game.join(DIYLEVEL_DIR);
        std::fs::create_dir_all(&diy).unwrap();
        std::fs::write(diy.join("OC2DIYLevel.dll"), b"v1").unwrap();

        let backup =
            backup_framework(&game, &backups, &DIYLEVEL_BACKUP, &manifests, "oc2diylevel").unwrap();
        // 备份后放一个不同版本，确保恢复前的目录非空
        std::fs::write(diy.join("OC2DIYLevel.dll"), b"v2").unwrap();

        restore_framework_backup(&game, &backup, &DIYLEVEL_BACKUP, &manifests, "oc2diylevel")
            .unwrap();

        let plugins = game.join("BepInEx/plugins");
        let leftovers: Vec<_> = std::fs::read_dir(&plugins)
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".before-restore-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "DIYLevel 恢复后 plugins 下不应残留暂存目录：{leftovers:?}"
        );
        // 恢复出来的内容必须是备份那一刻的
        assert_eq!(std::fs::read(diy.join("OC2DIYLevel.dll")).unwrap(), b"v1");

        std::fs::remove_dir_all(&root).unwrap();
    }
}

/// Release 缓存有效期的判定。
///
/// 这里是「每个仓库的 API 请求缓存 5 分钟」的核心逻辑：判错的后果很具体 ——
/// 判松了会把过期数据当最新（用户看不到新版本），判严了等于没有缓存
/// （每次打开页面都把整棵仓库树重打一遍，撞速率限制）。
#[cfg(test)]
mod release_cache_tests {
    use super::{RELEASE_CACHE_TTL_MS, now_millis, release_cache_is_fresh};

    #[test]
    fn fresh_within_five_minutes() {
        let now = 1_700_000_000_000;
        assert!(release_cache_is_fresh(Some(now), now), "刚写入应命中");
        assert!(
            release_cache_is_fresh(Some(now - RELEASE_CACHE_TTL_MS + 1), now),
            "差 1 毫秒到期仍算新鲜"
        );
        assert_eq!(RELEASE_CACHE_TTL_MS, 5 * 60 * 1000);
    }

    #[test]
    fn expired_at_and_after_ttl() {
        let now = 1_700_000_000_000;
        assert!(
            !release_cache_is_fresh(Some(now - RELEASE_CACHE_TTL_MS), now),
            "正好到期即失效"
        );
        assert!(!release_cache_is_fresh(
            Some(now - RELEASE_CACHE_TTL_MS - 1),
            now
        ));
        assert!(
            !release_cache_is_fresh(Some(now - 86_400_000), now),
            "隔天必失效"
        );
    }

    #[test]
    fn missing_timestamp_is_expired() {
        // 旧版本配置没有这个字段，或从未成功获取过 —— 必须重新请求，
        // 不能拿一份来源不明的列表去判断「有没有更新」。
        assert!(!release_cache_is_fresh(None, 1_700_000_000_000));
    }

    #[test]
    fn clock_going_backwards_is_treated_as_expired() {
        // 系统时间被调早：此时 now < cached_at，差值算出来是负数，
        // 如果不判方向就会永远命中缓存。
        let now = 1_700_000_000_000;
        assert!(!release_cache_is_fresh(Some(now + 60_000), now));
    }

    #[test]
    fn now_millis_is_plausible() {
        // 2023-11 之后、2100 之前：防止单位错成秒导致缓存永不过期
        let now = now_millis();
        assert!(now > 1_700_000_000_000, "时间戳应以毫秒为单位：{now}");
        assert!(now < 4_102_444_800_000, "时间戳明显异常：{now}");
    }
}
