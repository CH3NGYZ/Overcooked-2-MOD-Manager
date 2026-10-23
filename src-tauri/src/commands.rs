use crate::archive;
use crate::archive::DownloadRoute as AssetDownloadRoute;
use crate::config::ConfigService;
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
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
    let manifest: tauri_plugin_updater::RemoteRelease =
        serde_json::from_value(document.clone()).map_err(|_| "更新清单格式无效".to_string())?;
    if !manifest.version.pre.is_empty() {
        return Err("更新清单不是稳定版本".into());
    }
    let key = target.manifest_key();
    let url = manifest
        .download_url(&key)
        .map_err(|_| "更新中没有匹配的安装包")?;
    let signature = manifest
        .signature(&key)
        .map_err(|_| "更新清单或签名不可用，已停止安装")?
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
        return Err("更新清单与所选平台安装包不一致，已停止安装".into());
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
            .map_err(|_| "更新清单请求失败")?;
        if !response.status().is_success() {
            return Err(format!("更新清单 HTTP {}", response.status().as_u16()));
        }
        let total = response.content_length();
        if total.is_some_and(|size| size > MAX_MANIFEST_BYTES as u64) {
            return Err("更新清单过大".into());
        }
        let mut bytes = Vec::new();
        let mut stream = response.bytes_stream();
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| "更新清单读取失败")?;
            if bytes.len().saturating_add(chunk.len()) > MAX_MANIFEST_BYTES {
                return Err("更新清单过大".into());
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
        let document = serde_json::from_slice(&bytes).map_err(|_| "更新清单格式无效")?;
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
        _ = cancellation => Err("下载已取消".into()),
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
        return Err("下载已取消".into());
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
            return Err("下载已取消".into());
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
            return Err("下载已取消".into());
        }
        let selected = match selected {
            Ok(selected) => selected,
            Err(error) if error == "下载已取消" => return Err(error),
            Err(_) => continue,
        };
        let result = match selected.info {
            Some(info) => on_update(info).await.map(InstallerRouteResult::Updated),
            None => Ok(InstallerRouteResult::Latest),
        };
        if is_cancelled() {
            return Err("下载已取消".into());
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
            Err(error) if error == "下载已取消" => return Err(error),
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
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| format!("当前程序版本无效: {e}"))?;
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

pub struct AppState {
    pub config: ConfigService,
    pub operation: Mutex<()>,
    pub repository_cache: tokio::sync::Mutex<()>,
    pub community: crate::community::CommunitySearch,
    pub readme: crate::repository_readme::ReadmeDownloads,
    // Display-only snapshot; file operations always inspect the live process.
    pub game_state: Mutex<Option<GameInfo>>,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        log::debug!("创建应用状态");
        Ok(Self {
            config: ConfigService::new().map_err(|e| e.to_string())?,
            operation: Mutex::new(()),
            repository_cache: tokio::sync::Mutex::new(()),
            community: crate::community::CommunitySearch::default(),
            readme: crate::repository_readme::ReadmeDownloads::default(),
            game_state: Mutex::new(None),
        })
    }
}

#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardData, String> {
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
        .map_err(|_| "操作锁异常".to_string())?;
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
        path: path.to_string_lossy().to_string(),
        content,
    })
}

#[tauri::command]
pub fn write_frontend_log(level: String, message: String) -> Result<(), String> {
    log::trace!("写入前端日志：等级={}，长度={}", level, message.len());
    const MAX_FRONTEND_LOG_BYTES: usize = 64 * 1024;
    let message = if message.len() > MAX_FRONTEND_LOG_BYTES {
        format!(
            "{}...[前端日志已截断]",
            &message[..message.floor_char_boundary(MAX_FRONTEND_LOG_BYTES)]
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
    let game_root = game::detect_game(&config).path.ok_or("尚未找到厨房目录")?;
    let installed = mods::scan_mods_with_manifests(
        Path::new(&game_root),
        &config,
        Some(&state.config.manifests_dir()),
    );
    let selected = installed
        .iter()
        .find(|item| item.dll_file == dll_file)
        .ok_or("找不到对应的已安装 MOD")?;
    let note_key = mods::note_key(selected);
    // Scan outside the configuration transaction, then modify the latest
    // snapshot so a concurrent source/proxy refresh is preserved.
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
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
            "MOD 备注已清除".into()
        } else {
            "MOD 备注已保存".into()
        },
    })
}

#[tauri::command]
pub fn install_local_mod(
    state: State<'_, AppState>,
    source_path: String,
) -> Result<OperationResult, String> {
    install_manual_mod(&state, &PathBuf::from(source_path), None)
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    if expected_game_root.is_some_and(|expected| expected != game_root) {
        return Err("厨房目录已变化，请重新添加模组".into());
    }
    if !source.is_file() {
        return Err("所选文件不存在".into());
    }
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or("文件名无效")?;
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
        let pending =
            archive::copy_asset_pending(&source, &game_root, "BepInEx/plugins", file_name)?;
        (pending, file_name.to_string())
    } else if extension.eq_ignore_ascii_case("zip") {
        log::debug!("按本地 ZIP 布局规则解压：名称={}", file_name);
        let folder = source
            .file_stem()
            .and_then(|value| value.to_str())
            .ok_or("ZIP 文件名无效")?;
        if folder.is_empty()
            || folder.contains('/')
            || folder.contains('\\')
            || folder.contains(':')
        {
            return Err("ZIP 文件名无效".into());
        }
        (
            archive::extract_local_mod_zip_pending(&source, &game_root, file_name)?,
            folder.to_string(),
        )
    } else {
        return Err("只支持 DLL 或 ZIP 文件".into());
    };
    if pending.files().is_empty() {
        log::warn!("本地 MOD 没有可安装内容：名称={}", file_name);
        return Err("所选文件没有可安装内容".into());
    }
    let manifest = archive::new_manifest(
        &repository_id,
        &display_name,
        Some(&display_name),
        Some(file_name),
        pending.files().to_vec(),
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
        message: format!("{display_name} 安装完成"),
    })
}

#[tauri::command]
pub fn uninstall_local_mod(
    state: State<'_, AppState>,
    dll_file: String,
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
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
        if !package_mod_names.is_empty() && !has_other_enabled_same_guid {
            log::warn!(
                "卸载本地 MOD 需要确认整个 MOD 包：文件={}，包={}，同包项目数={}",
                safe_relative,
                manifest.repository_id,
                package_mod_names.len()
            );
            return Ok(UninstallLocalModResult {
                ok: false,
                message: "当前 MOD 与其他 MOD 一并安装".into(),
                requires_confirmation: false,
                requires_package_confirmation: true,
                package_id: Some(manifest.repository_id),
                package_mod_names,
            });
        }
        if has_other_enabled_same_guid {
            recycle_bin::recycle_mod_dll(
                &state.config.mod_recycle_bin_dir(),
                &manifest_dir,
                &game_root,
                &manifest.repository_id,
                &safe_relative,
                &installed,
            )?;
            log::info!(
                "已回收 MOD 包中的单个 DLL：文件={}，包={}",
                safe_relative,
                manifest.repository_id
            );
            return Ok(UninstallLocalModResult {
                ok: true,
                message: "MOD 已移入回收站".into(),
                requires_confirmation: false,
                requires_package_confirmation: false,
                package_id: Some(manifest.repository_id),
                package_mod_names,
            });
        }
        recycle_bin::recycle_selected_mod(
            &state.config.mod_recycle_bin_dir(),
            &manifest_dir,
            &game_root,
            &manifest.repository_id,
            &safe_relative,
            true,
        )?;
        log::info!(
            "已卸载 MOD 包：包={}，文件={}",
            manifest.repository_id,
            safe_relative
        );
        return Ok(UninstallLocalModResult {
            ok: true,
            message: "MOD 包已移入回收站".into(),
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
            message: "该 MOD 不是通过本管理器安装的，仅将 DLL 移入回收站；其他相关文件需手动处理。是否继续？".into(),
            requires_confirmation: true,
            requires_package_confirmation: false,
            package_id: None,
            package_mod_names: Vec::new(),
        });
    }
    recycle_bin::recycle_unmanaged(
        &state.config.mod_recycle_bin_dir(),
        &game_root,
        &safe_relative,
    )?;
    log::info!("已回收未托管 MOD DLL：文件={}", safe_relative);
    Ok(UninstallLocalModResult {
        ok: true,
        message: "MOD 已移入回收站".into(),
        requires_confirmation: false,
        requires_package_confirmation: false,
        package_id: None,
        package_mod_names: Vec::new(),
    })
}

#[tauri::command]
pub fn uninstall_mod_dll(
    state: State<'_, AppState>,
    dll_file: String,
    package_id: String,
) -> Result<OperationResult, String> {
    log::info!(
        "开始删除 MOD 包内 DLL：包={}，文件={}",
        package_id,
        dll_file
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest_dir = state.config.manifests_dir();
    let manifest = archive::read_manifest(&manifest_dir, &package_id)
        .ok_or("没有安装清单，拒绝删除未知文件")?;
    let safe_relative =
        mods::canonical_dll_name(&Path::new(&dll_file).to_string_lossy().replace('\\', "/"));
    if !manifest
        .files
        .iter()
        .any(|file| mods::canonical_dll_name(file).eq_ignore_ascii_case(&safe_relative))
    {
        return Err("当前 DLL 不属于这个 MOD 包".into());
    }
    let installed = mods::scan_mods_with_manifests(&game_root, &config, Some(&manifest_dir));
    let package_mod_count = installed
        .iter()
        .filter(|item| item.package_id.as_deref() == Some(package_id.as_str()))
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
        recycle_bin::recycle_selected_mod(
            &state.config.mod_recycle_bin_dir(),
            &manifest_dir,
            &game_root,
            &package_id,
            &safe_relative,
            true,
        )?;
        log::info!("已回收 MOD 包最后一个 DLL：包={}", package_id);
        return Ok(OperationResult {
            ok: true,
            message: "MOD 包已移入回收站".into(),
        });
    }
    recycle_bin::recycle_selected_mod(
        &state.config.mod_recycle_bin_dir(),
        &manifest_dir,
        &game_root,
        &package_id,
        &safe_relative,
        false,
    )?;
    log::info!("已回收 MOD 包内 DLL：包={}", package_id);
    Ok(OperationResult {
        ok: true,
        message: "MOD 已移入回收站".into(),
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let relative = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    toggle_local_mod_file(&game_root, &relative, enabled)?;
    log::info!("本地 MOD 状态切换完成：文件={}，启用={}", relative, enabled);
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "MOD 已启用".into()
        } else {
            "MOD 已禁用".into()
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
        return Err("只能切换 BepInEx/plugins 下的 DLL 文件".into());
    }
    let plugins_root = game::safe_game_path(&game_root, "BepInEx/plugins")?;
    let enabled_path = game::safe_game_path(&game_root, &relative)?;
    let disabled_path = game::safe_game_path(&game_root, &mods::disabled_dll_file(&relative))?;
    if !enabled_path.starts_with(&plugins_root) || !disabled_path.starts_with(&plugins_root) {
        return Err("只能切换 BepInEx/plugins 下的 DLL 文件".into());
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
                "找不到已禁用的 MOD DLL 文件"
            } else {
                "找不到已启用的 MOD DLL 文件"
            }
            .into());
        }
        (true, true) => return Err("目标 MOD 文件已存在，无法切换".into()),
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &package_id)
        .ok_or("没有安装清单，无法切换 MOD 包")?;
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
        return Err("这个 MOD 包没有可切换的插件 DLL".into());
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
            "MOD 包已启用".into()
        } else {
            "MOD 包已禁用".into()
        },
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
            return Err("Windows 仅支持 Windows 版厨房".into());
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    if previous_game
        .as_ref()
        .is_some_and(|previous| *previous != config.game)
    {
        return Err("厨房设置已变化，请重试".into());
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut proxies = GitHubProxyConfig {
        release_proxies: patch.release_proxies,
        raw_proxies: patch.raw_proxies,
        github_api_proxies: patch.github_api_proxies,
        gitee_api_proxies: patch.gitee_api_proxies,
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
                format!(
                    "超时={}，连接错误={}，原因={}",
                    error.is_timeout(),
                    error.is_connect(),
                    error.without_url()
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
                format!("代理来源超过 {} MiB", max_bytes / (1024 * 1024)),
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
                format!(
                    "已读取字节数={}，超时={}，原因={}",
                    bytes.len(),
                    error.is_timeout(),
                    error.without_url()
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
            .map_err(|_| ProxySourceReadError::new("utf8", "文件不是有效 UTF-8 文本"))?
    };
    if text.len() > max_bytes {
        return Err(ProxySourceReadError::new(
            "size",
            format!("代理来源超过 {} MiB", max_bytes / (1024 * 1024)),
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
            "GitHub 代理脚本中没有可识别的 Release 或 Raw 代理" => {
                "JS 文件没有可识别的 Release 或 Raw 代理"
            }
            "代理脚本缺少 download_url_us 列表" | "代理脚本缺少 raw_url 列表" => {
                "JS 文件缺少 download_url_us 或 raw_url 列表"
            }
            _ => "文件格式、字段或代理条目校验失败",
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
        .map_err(|_| "操作锁异常".to_string())?;
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    state
        .config
        .set_tokens(&mut config, &patch.github_token, &patch.gitee_token);
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
            .map_err(|_| "操作锁异常".to_string())?;
        let mut config = state.config.load_or_default();
        if config.game != previous.game {
            return Err("厨房设置已变化，请重试".into());
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
            .map_err(|_| "操作锁异常".to_string())?;
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

#[tauri::command]
pub async fn refresh_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<Vec<ReleaseInfo>, String> {
    log::info!("开始刷新仓库 Release：仓库={}", repository_id);
    let config = state.config.load_or_default();
    let index = config
        .repositories
        .iter()
        .position(|repo| repo.id == repository_id)
        .ok_or("仓库不存在")?;
    if !config.repositories[index].enabled {
        return Err("仓库已禁用".into());
    }
    if config.repositories[index].download_mode == RepositoryDownloadMode::RepositoryFile {
        return Err("仓库文件模式不需要刷新 Release".into());
    }
    if !crate::config::repository_matches_channel(
        &config,
        &config.repositories[index],
        crate::config::current_user_region(),
    ) {
        return Err("仓库不属于当前MOD下载渠道".into());
    }
    let repo = config.repositories[index].clone();
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut latest_config = state.config.load_or_default();
    let latest_index = latest_config
        .repositories
        .iter()
        .position(|item| item.id == repository_id)
        .ok_or("仓库已被删除")?;
    let latest_repo = &mut latest_config.repositories[latest_index];
    if latest_repo.username != repo.username
        || latest_repo.repository != repo.repository
        || latest_repo.provider != repo.provider
        || latest_repo.channel != repo.channel
        || latest_repo.download_mode != repo.download_mode
    {
        return Err("仓库配置已更改，请重新刷新".into());
    }

    latest_repo.etag = None;
    match fetched {
        Ok(filtered) => {
            latest_repo.cached_releases = filtered.clone();
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
        .ok_or("仓库不存在")?
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
        return Err("当前模式下未找到厨房可执行文件".into());
    }
    let game_root = info.path.as_deref().ok_or("尚未找到厨房目录")?;
    let game_root = PathBuf::from(game_root);
    game::ensure_game_stopped(&game_root)?;
    if info.bepinex.installed && !reinstall {
        return Err("BepInEx 已安装，如需覆盖请使用重新安装".into());
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
            .map_err(|_| "操作锁异常".to_string())?;
        if state.config.load_or_default().game != config.game {
            return Err("厨房设置已变化，请重试".into());
        }
        if archive::is_download_cancelled() {
            return Err("下载已取消".into());
        }
        game::ensure_game_stopped(&game_root)?;
        install_framework_archive(
            &temp,
            &game_root,
            &state.config.backups_dir(),
            package,
            reinstall,
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
                        let prefix =
                            prefix.ok_or("无法确定游戏使用的 Wine 容器，请重新选择厨房目录")?;
                        crate::wine::configure_winhttp(&prefix)
                    })
                    .await
                    .unwrap_or_else(|_| {
                        Err("无法自动配置 Wine winhttp，请在目标容器的 winecfg 中手动设置".into())
                    }),
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
                    message: "BepInEx 安装完成".into(),
                },
                wine_override_applied,
                wine_override_error,
                macos_overlay_repaired: package == crate::bepinex::Package::MacOS,
            })
        }
        Err(error) => {
            log::error!("BepInEx 安装失败：原因={}", error);
            Err(format!("BepInEx 安装失败: {error}"))
        }
    }
}

fn install_framework_archive(
    archive_path: &Path,
    game_root: &Path,
    backup_root: &Path,
    package: crate::bepinex::Package,
    reinstall: bool,
) -> Result<(), String> {
    let framework = game::safe_game_path(game_root, "BepInEx")?;
    if framework.exists() && !reinstall {
        return Err("BepInEx 已安装，如需覆盖请使用重新安装".into());
    }
    let backup = if framework.exists() {
        Some(backup_bepinex_dir(game_root, backup_root)?)
    } else {
        None
    };
    let result = (|| {
        if framework.exists() {
            archive::clear_readonly(&framework)?;
            fs::remove_dir_all(&framework).map_err(|e| format!("清理旧 BepInEx 失败: {e}"))?;
        }
        let pending = archive::extract_framework_zip_pending(archive_path, game_root)?;
        if package.native() {
            crate::bepinex::prepare_native(game_root, package)?;
        }
        if let Some(backup) = &backup {
            restore_preserved_dirs(backup, game_root)?;
        }
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
                "BepInEx 安装失败，备份恢复未完成，请从框架备份恢复".to_string()
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    if std::env::consts::OS != "macos" || config.game.runtime != GameRuntime::Native {
        return Err("Steam Overlay 修复仅适用于 macOS 原生运行模式".into());
    }
    let info = game::detect_game(&config);
    if !info.game_exists {
        return Err("当前模式下未找到厨房可执行文件".into());
    }
    let game_root = PathBuf::from(info.path.ok_or("尚未找到游戏目录")?);
    game::ensure_game_stopped(&game_root)?;
    crate::bepinex::repair_macos_overlay(&game_root)?;
    log::info!("macOS Steam Overlay 注入修复完成");
    Ok(OperationResult {
        ok: true,
        message: "Steam Overlay 注入修复完成".into(),
    })
}

#[tauri::command]
pub fn uninstall_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("开始卸载 BepInEx");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let source = game_root.join("BepInEx");
    if !source.exists() {
        return Err("BepInEx 未安装".into());
    }
    ensure_copy_paths_disjoint(&source, &state.config.backups_dir())?;
    archive::clear_readonly(&source)?;
    let backup = bepinex_backup_path(&state.config.backups_dir())?;
    // The game and portable manager may be on different drives. `rename`
    // fails with ERROR_NOT_SAME_DEVICE there, so copy and remove explicitly.
    copy_dir(&source, &backup)?;
    archive::clear_readonly(&source)?;
    fs::remove_dir_all(&source).map_err(|e| e.to_string())?;
    log::info!("BepInEx 卸载完成：备份={}", backup.display());
    Ok(OperationResult {
        ok: true,
        message: format!("BepInEx 已移除，备份保存在 {}", backup.display()),
    })
}

#[tauri::command]
pub fn backup_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("开始备份 BepInEx");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let backup = backup_bepinex_dir(&game_root, &state.config.backups_dir())?;
    log::info!("BepInEx 备份完成：路径={}", backup.display());
    Ok(OperationResult {
        ok: true,
        message: format!("备份完成: {}", backup.display()),
    })
}

#[tauri::command]
pub fn restore_bepinex(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    log::info!("开始恢复 BepInEx 备份");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    restore_backup_dir(Path::new(&backup_path), &game_root)?;
    log::info!("BepInEx 备份恢复完成");
    Ok(OperationResult {
        ok: true,
        message: "BepInEx 备份已恢复".into(),
    })
}

#[tauri::command]
pub fn delete_bepinex_backup(
    state: State<'_, AppState>,
    backup_path: String,
) -> Result<OperationResult, String> {
    log::info!("开始删除 BepInEx 备份");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;

    let target = fs::canonicalize(&backup_path).map_err(|_| "备份目录不存在".to_string())?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    if !name.starts_with("BepInEx.backup-") || !target.is_dir() {
        return Err("只能删除 BepInEx 时间戳备份".into());
    }

    let game_root = fs::canonicalize(&game_root).map_err(|e| e.to_string())?;
    let manager_backup_path = state.config.backups_dir();
    fs::create_dir_all(&manager_backup_path).map_err(|e| e.to_string())?;
    let manager_backup_root = fs::canonicalize(manager_backup_path).map_err(|e| e.to_string())?;
    let allowed = target.parent() == Some(game_root.as_path())
        || target.parent() == Some(manager_backup_root.as_path());
    if !allowed {
        return Err("备份目录不在允许的备份位置".into());
    }

    archive::clear_readonly(&target)?;
    fs::remove_dir_all(&target).map_err(|e| e.to_string())?;
    log::info!("BepInEx 备份删除完成");
    Ok(OperationResult {
        ok: true,
        message: "BepInEx 备份已删除".into(),
    })
}

fn selected_asset_proxy_mode(
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

fn plan_asset_download_candidates(
    mode: ProxyMode,
    direct_url: &str,
    proxy_urls: Vec<String>,
) -> Result<Vec<AssetDownloadCandidate>, String> {
    let candidates = ordered_asset_download_candidates(mode, direct_url, proxy_urls);
    if mode == ProxyMode::Fixed && candidates.is_empty() {
        return Err("全局 GitHub 代理列表中没有可用的代理".into());
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

#[tauri::command]
pub async fn install_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    repository_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
    asset_proxy_mode: Option<ProxyMode>,
    asset_proxy_id: Option<String>,
) -> Result<OperationResult, String> {
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
    let game_root = PathBuf::from(info.path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    if info.bepinex.version.is_none() {
        return Err("请先安装 BepInEx 框架，再安装 MOD".into());
    }
    let repo = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("仓库不存在")?
        .clone();
    if !repo.enabled {
        return Err("仓库已禁用".into());
    }
    crate::community::ensure_repository_region(&repo, crate::config::current_user_region())?;
    let selected_proxy_mode = selected_asset_proxy_mode(
        asset_proxy_mode,
        &repo.provider,
        crate::config::current_user_region(),
    );
    let (download_url, download_name, release_tag, digest, is_raw_file, expected_size) = match repo
        .download_mode
    {
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
            (download_url, download_name, String::new(), None, true, None)
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
                .ok_or("没有可安装的 Release")?;
            let asset = releases::select_install_asset(&repo, release, asset_name.as_deref())
                .ok_or("没有匹配的 Release 资产")?;
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
                asset.digest.clone(),
                false,
                (asset.size > 0).then_some(asset.size),
            )
        }
    };
    let proxy_entries: &[GitHubProxy] = if repo.provider == Provider::Github {
        if is_raw_file {
            &config.github_proxies.raw_proxies
        } else {
            &config.github_proxies.release_proxies
        }
    } else {
        &[]
    };
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
            label: &repo.name,
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
    // No network awaits while the operation lock protects recycling and install.
    let files_result = (|| {
        let _guard = state
            .operation
            .lock()
            .map_err(|_| "操作锁异常".to_string())?;
        if archive::is_download_cancelled() {
            return Err("下载已取消".into());
        }
        if game::is_game_running(&game_root) {
            log::warn!("下载期间检测到厨房重新启动，取消安装：仓库={}", repo.name);
            return Err("下载期间厨房重新启动，安装已取消，旧版本未删除".into());
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
            Ok(OperationResult {
                ok: true,
                message: if release_tag.is_empty() {
                    format!("{} 安装完成", repo.name)
                } else {
                    format!("{} {} 安装完成", repo.name, release_tag)
                },
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
        message: "正在取消下载".into(),
    })
}

#[tauri::command]
pub fn uninstall_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<OperationResult, String> {
    log::info!("开始卸载在线仓库 MOD：仓库={}", repository_id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let package_id = if let Some(repo) = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
    {
        repository_install_identity(repo, &config, &state.config.manifests_dir())?.id
    } else {
        repository_id.clone()
    };
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &package_id)
        .ok_or("没有安装清单，拒绝删除未知文件")?;
    let entry = recycle_bin::recycle_package(
        &state.config.mod_recycle_bin_dir(),
        &state.config.manifests_dir(),
        &game_root,
        &manifest.repository_id,
        None,
    )?;
    log::info!(
        "在线仓库 MOD 卸载完成：仓库={}，文件数={}",
        repository_id,
        entry.file_count
    );
    Ok(OperationResult {
        ok: true,
        message: "MOD 包已移入回收站".into(),
    })
}

#[tauri::command]
pub fn get_mod_recycle_bin(state: State<'_, AppState>) -> Result<Vec<ModRecycleEntry>, String> {
    log::debug!("读取 MOD 回收站列表请求");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    recycle_bin::list(&state.config.mod_recycle_bin_dir()).map_err(|error| {
        log::warn!("读取 MOD 回收站失败：{}", error);
        error
    })
}

#[tauri::command]
pub fn restore_recycled_mod(
    state: State<'_, AppState>,
    id: String,
) -> Result<OperationResult, String> {
    log::info!("请求恢复回收站 MOD：ID={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
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
        message: "MOD 已从回收站恢复".into(),
    })
}

#[tauri::command]
pub fn delete_recycled_mod(
    state: State<'_, AppState>,
    id: String,
) -> Result<OperationResult, String> {
    log::info!("请求彻底删除回收站 MOD：ID={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    recycle_bin::delete(&state.config.mod_recycle_bin_dir(), &id).map_err(|error| {
        log::warn!("彻底删除回收站 MOD 失败：ID={}，原因={}", id, error);
        error
    })?;
    Ok(OperationResult {
        ok: true,
        message: "MOD 已彻底删除".into(),
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
        return Err("导出文件必须使用 .oc2repo.toml 或 .toml 扩展名".into());
    }
    fs::write(&path, text).map_err(|e| e.to_string())?;
    log::info!("仓库配置导出完成");
    Ok(OperationResult {
        ok: true,
        message: format!("已导出到 {path}"),
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
        message: "仓库 TOML 已复制到剪贴板".into(),
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
        message: "教程格式已复制".into(),
    })
}

fn parse_tutorial_document(text: &str) -> Result<TutorialDocument, String> {
    if text.len() > 1024 * 1024 {
        return Err("教程配置超过 1 MiB".into());
    }
    let trimmed = text.trim();
    let content = if trimmed.starts_with("```") {
        let mut lines = trimmed.lines();
        let _ = lines.next();
        let body = lines.collect::<Vec<_>>();
        let end = body
            .iter()
            .rposition(|line| line.trim() == "```")
            .ok_or_else(|| "教程配置代码块未闭合".to_string())?;
        body[..end].join("\n")
    } else {
        trimmed.to_string()
    };
    let document = toml::from_str::<TutorialDocument>(&content)
        .map_err(|error| format!("教程 TOML 格式无效：{error}"))?;
    if document.version != 1 {
        return Err(format!("不支持的教程格式版本：{}", document.version));
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
    toml::to_string_pretty(&TutorialDocument { version: 1, steps })
        .map_err(|error| format!("教程 TOML 导出失败：{error}"))
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
    log::debug!("读取文本文件");
    let metadata = fs::metadata(&path).map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 {
        return Err("导入文件超过 1 MiB".into());
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
        .map_err(|_| "操作锁异常".to_string())?;
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
        return Err("社区仓库源由自动搜索管理".into());
    }
    let _cache_guard = state.repository_cache.lock().await;
    let _operation_guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    if source_ids.iter().any(|id| {
        !config
            .repository_sources
            .iter()
            .any(|source| source.id == *id)
    }) {
        return Err("仓库源不存在".into());
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
        return Err("社区仓库源由自动搜索管理".into());
    }
    log::info!("开始刷新仓库源：源={}", source_id);
    // Clear this source and all descendants before loading. The lock is
    // deliberately released during the network request so other sources can
    // download and apply their own files concurrently. Keep the source itself
    // so the UI can show its error state, but remove stale descendants and
    // repositories instead of restoring them after a failed request.
    let (timeout_seconds, source_url, builtin_fallback_available) = {
        let _cache_guard = state.repository_cache.lock().await;
        let _operation_guard = state
            .operation
            .lock()
            .map_err(|_| "操作锁异常".to_string())?;
        let original_config = state.config.load_or_default();
        let Some(source) = original_config
            .repository_sources
            .iter()
            .find(|source| source.id == source_id)
        else {
            log::warn!("刷新仓库源失败：源不存在，源={}", source_id);
            return Err("仓库源不存在".into());
        };
        if !source.url.trim().eq_ignore_ascii_case(source_url.trim()) {
            log::warn!("刷新仓库源失败：请求地址与当前配置不一致，源={}", source_id);
            return Err("仓库源地址与配置不一致".into());
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
            return Err("此仓库源不支持内置回退".into());
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
        (
            original_config.network.timeout_seconds,
            source_url,
            builtin_fallback_available && !builtin_fallback,
        )
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
    let document = match repository_sources::load_document(&location, timeout_seconds).await {
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
        .map_err(|_| "操作锁异常".to_string())?;
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
            }
            used_ids.insert(repo.id.clone());
            config.source_repository_ids.insert(key, repo.id.clone());
            repo.source_id = Some(source_id.clone());
        }
    }
    state.config.save(&config).map_err(|error| {
        log::error!("仓库源刷新后保存配置失败：源={}，原因={}", source_id, error);
        error.to_string()
    })?;
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

#[tauri::command]
pub fn save_repository_source(
    state: State<'_, AppState>,
    id: Option<String>,
    nickname: String,
    url: String,
    nickname_i18n: Option<std::collections::BTreeMap<String, String>>,
) -> Result<PublicConfig, String> {
    if id.as_deref().is_some_and(crate::community::is_source) {
        return Err("社区仓库源由自动搜索管理".into());
    }
    log::info!(
        "保存仓库源：操作={}",
        if id.is_some() { "编辑" } else { "新增" }
    );
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    let nickname = nickname.trim();
    let url = url.trim();
    Url::parse(url).map_err(|error| format!("仓库源地址无效: {error}"))?;
    if nickname.is_empty() || url.is_empty() {
        return Err("仓库源昵称和地址不能为空".into());
    }
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
        return Err("仓库源地址已存在".into());
    }
    if let Some(id) = id {
        let source = config
            .repository_sources
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or("仓库源不存在")?;
        if source.builtin {
            return Err("内置仓库源不能修改".into());
        }
        source.nickname = nickname.into();
        if let Some(nickname_i18n) = nickname_i18n {
            source.nickname_i18n = nickname_i18n;
        }
        source.url = url.into();
    } else {
        config.repository_sources.push(RepositorySource {
            id: Uuid::new_v4().to_string(),
            nickname: nickname.into(),
            nickname_i18n: nickname_i18n.unwrap_or_default(),
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    let has_builtin_root = config
        .repository_sources
        .iter()
        .any(|source| source.builtin && source.parent_id.is_none());
    if !has_builtin_root {
        return Err("缺少内置仓库源".into());
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
            return Err("仓库唯一 ID 不能为空".into());
        }
        if id.len() > 128
            || !id.bytes().enumerate().all(|(index, byte)| {
                (byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
                    && (index > 0 || byte.is_ascii_alphanumeric())
            })
        {
            return Err("仓库唯一 ID 格式无效".into());
        }
        let nickname = source.nickname.trim().to_string();
        let url = source.url.trim().to_string();
        if nickname.is_empty() || url.is_empty() {
            return Err("仓库源昵称和地址不能为空".into());
        }
        let normalized_region = crate::config::normalize_repository_region_for_storage(
            source.region.as_deref(),
            &format!("仓库源 {}", id),
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
            return Err("仓库源必须使用 HTTP 或 HTTPS 地址".into());
        }
        if !seen_urls.insert(source_address_key(&url, normalized_region.as_deref())) {
            return Err("仓库源地址已存在".into());
        }
        if !seen_ids.insert(id.clone()) {
            return Err("仓库源标识重复".into());
        }
        ordered.push(RepositorySource {
            id,
            nickname,
            nickname_i18n: source.nickname_i18n,
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
        return Err("社区仓库源由自动搜索管理".into());
    }
    log::info!("删除仓库源：源={}", id);
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    let index = config
        .repository_sources
        .iter()
        .position(|item| item.id == id)
        .ok_or("仓库源不存在")?;
    if config.repository_sources[index].builtin {
        return Err("内置仓库源不能删除".into());
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    let builtin_roots = config
        .repository_sources
        .iter()
        .filter(|item| item.builtin && item.parent_id.is_none())
        .cloned()
        .collect::<Vec<_>>();
    if builtin_roots.is_empty() {
        return Err("缺少内置仓库源".into());
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    if patch.id.as_ref().is_some_and(|id| {
        config
            .repositories
            .iter()
            .any(|repo| repo.id == *id && crate::community::is_repository(repo))
    }) {
        return Err("社区仓库由自动搜索管理，请导出后作为自定义仓库编辑".into());
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
        return Err("请创建至少一个有效的资产规则".into());
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
        &format!("仓库 {}", patch.name),
    );
    crate::config::validate_repository_region(normalized_region.as_deref())
        .map_err(|error| error.to_string())?;
    if !patch.version_regex.trim().is_empty()
        && regex::Regex::new(patch.version_regex.trim()).is_err()
    {
        return Err("版本号正则无效".into());
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
        .ok_or_else(|| "仓库名称不能为空".to_string())?;
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    let Some(index) = config
        .repositories
        .iter()
        .position(|repo| repo.id == repository_id)
    else {
        return Err("仓库不存在".into());
    };
    if config.repositories[index].builtin {
        return Err("内置仓库不能删除，可禁用它".into());
    }
    if crate::community::is_repository(&config.repositories[index]) {
        return Err("社区仓库由自动搜索管理，请导出后作为自定义仓库编辑".into());
    }
    config.repositories.remove(index);
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("仓库删除完成：仓库={}", repository_id);
    Ok(OperationResult {
        ok: true,
        message: "仓库已删除".into(),
    })
}

#[tauri::command]
pub fn launch_normal(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::info!("启动厨房：正常 MOD 模式");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("尚未找到厨房目录")?);
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.as_deref().ok_or("尚未找到厨房目录")?);
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    if crate::bepinex::state(&game_root, &config.game) == WinhttpState::ModDisabled {
        crate::bepinex::set_enabled(&game_root, &config.game, true)?;
    }
    log::info!("框架状态恢复完成");
    Ok(OperationResult {
        ok: true,
        message: "框架状态已恢复".into(),
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
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    crate::bepinex::set_enabled(&game_root, &config.game, enabled)?;
    log::info!("框架状态切换完成：启用={}", enabled);
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "框架已启用，尚未启动厨房".into()
        } else {
            "框架已禁用，尚未启动厨房".into()
        },
    })
}

#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) -> Result<OperationResult, String> {
    log::info!("打开开发者工具");
    window.open_devtools();
    Ok(OperationResult {
        ok: true,
        message: "开发者控制台已打开".into(),
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
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    config.ui_zoom = 1.0;
    for repo in &mut config.repositories {
        repo.cached_releases.clear();
        repo.etag = None;
        repo.last_error = None;
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("界面缓存和 Release 缓存重置完成");
    Ok(OperationResult {
        ok: true,
        message: "已清除缓存并重置界面".into(),
    })
}

#[tauri::command]
pub fn open_game_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开游戏目录");
    let config = state.config.load_or_default();
    let path = game::detect_game(&config).path.ok_or("尚未找到厨房目录")?;
    external_open_command()
        .arg(path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已打开游戏目录".into(),
    })
}

#[tauri::command]
pub fn open_mod_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    log::debug!("打开 MOD 目录");
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("尚未找到厨房目录")?;
    let plugins = PathBuf::from(game_root).join("BepInEx/plugins");
    fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
    external_open_command()
        .arg(&plugins)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已打开模组目录".into(),
    })
}

fn resolve_mod_file_for_location(game_root: &Path, dll_file: &str) -> Result<PathBuf, String> {
    let relative = mods::canonical_dll_name(&dll_file.replace('\\', "/"));
    let normalized = relative.to_ascii_lowercase();
    if !normalized.starts_with("bepinex/plugins/") || !normalized.ends_with(".dll") {
        return Err("只能定位 BepInEx/plugins 下的 MOD DLL 文件".into());
    }
    let plugins_root = game::safe_game_path(game_root, "BepInEx/plugins")?;
    for candidate in [relative.clone(), mods::disabled_dll_file(&relative)] {
        let path = game::safe_game_path(game_root, &candidate)?;
        if !path.starts_with(&plugins_root) {
            return Err("只能定位 BepInEx/plugins 下的 MOD DLL 文件".into());
        }
        if path.is_file() {
            return Ok(path);
        }
    }
    Err("MOD DLL 文件不存在，请重新扫描".into())
}

fn mod_file_location_command(path: &Path) -> std::process::Command {
    #[cfg(target_os = "linux")]
    {
        let mut command = external_open_command();
        command.arg(path.parent().unwrap_or(path));
        return command;
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = external_open_command();
        command.arg("-R").arg(path);
        return command;
    }
    #[cfg(windows)]
    {
        // Explorer expects ordinary drive/UNC paths, not canonical verbatim prefixes.
        let path = path.to_string_lossy();
        let shell_path = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
            format!(r"\\{unc}")
        } else {
            path.strip_prefix(r"\\?\").unwrap_or(&path).to_string()
        };
        let selection = format!(r#"/select,"{shell_path}""#);
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
    #[cfg(target_os = "linux")]
    let program = "xdg-open";
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
            .map_err(|_| "操作锁异常".to_string())?;
        state.config.load_or_default()
    };
    let game_root = game::detect_game(&config).path.ok_or("尚未找到厨房目录")?;
    let path = resolve_mod_file_for_location(Path::new(&game_root), &dll_file)?;
    mod_file_location_command(&path)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已打开目录并定位 MOD DLL".into(),
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
        .ok_or("无法识别 MOD 的 GUID，请重新扫描")?;
    Ok((root, guid, config.game.path))
}

fn resolve_config_game(state: &AppState) -> Result<(PathBuf, AppConfig), String> {
    let config = {
        let _guard = state.operation.lock().map_err(|_| "操作锁异常")?;
        state.config.load_or_default()
    };
    let root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
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
    let _guard = state.operation.lock().map_err(|_| "操作锁异常")?;
    let config = state.config.load_or_default();
    if config.game.path != configured_path {
        return Err("厨房目录已变化，请重新打开配置".into());
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
                return Err("备份目录不存在".into());
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
        message: "已打开备份目录".into(),
    })
}

#[tauri::command]
pub fn open_release_page(url: String) -> Result<OperationResult, String> {
    log::info!("打开 Release 页面");
    let parsed = Url::parse(&url).map_err(|e| format!("发布页地址无效: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("发布页必须使用 HTTPS".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("发布页地址不能包含认证信息".into());
    }
    external_open_command()
        .arg(parsed.as_str())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已打开发布页".into(),
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
                        Some("更新检查初始化失败，已跳过自动更新".into()),
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
                        Some("更新检查超时，已跳过自动更新".into()),
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
        .map_err(|_| "下载首字节等待超过 5 秒".to_string())?;
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
        return Err("下载文件为空".into());
    }
    if total.is_some_and(|expected| expected != downloaded) {
        return Err("下载长度不完整".into());
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
            Err("单个安装包下载超过 30 秒，已跳过当前渠道".into())
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
                    "正在下载程序更新",
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
            label: "正在下载程序更新",
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
    let provider = info.provider.as_ref().ok_or("更新清单地址无效")?;
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
        return Err("更新清单与所选平台安装包不一致，已停止安装".into());
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
        return Err("下载已取消".into());
    }
    let endpoint = Url::parse(url).map_err(|_| "更新清单地址无效")?;
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
    #[cfg(target_os = "linux")]
    let builder = if target.kind == "appimage" {
        let path = std::env::var_os("APPIMAGE").ok_or("当前安装格式不支持自动更新，请手动更新")?;
        builder.executable_path(PathBuf::from(path))
    } else {
        builder
    };
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
        _ = cancellation => return Err("下载已取消".into()),
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
    Err("更新清单或签名不可用，已停止安装".into())
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
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("当前程序版本无效: {error}"))?;
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
    let message = "请手动更新管理器, 本次跳过更新.".to_string();
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
                message: "当前已是最新版本".into(),
            });
        }
        InstallerDownloadAttempt::Failed(message) => {
            log::warn!("安装器自更新未完成：{}", message);
            return Ok(OperationResult { ok: false, message });
        }
    };
    if archive::is_download_cancelled() {
        return Err("下载已取消".into());
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
        .map_err(|_| "程序更新安装失败")?
        .map_err(|error| {
            log::error!("Tauri Updater 安装失败：{error}");
            "程序更新安装失败，请手动更新".to_string()
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
        message: "更新安装器已启动，请等待安装完成".into(),
    })
}

fn collect_update_status(config: &AppConfig, installed: &[InstalledMod]) -> Vec<UpdateStatus> {
    config
        .repositories
        .iter()
        .filter(|repo| repo.enabled)
        .filter(|repo| {
            crate::config::repository_matches_channel(
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
        return Err("当前模式下未找到厨房可执行文件".into());
    }
    game::steam_launch_command(settings, info)?
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已请求 Steam 启动厨房，请在 Steam 中确认启动".into(),
    })
}

fn backup_bepinex_dir(game_root: &Path, backup_root: &Path) -> Result<PathBuf, String> {
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
        return Err("同一仓库绑定了多个旧 MOD 包，请先在 MOD 管理中确认要替换的包".into());
    }
    let mut result = repo.clone();
    if let Some(manifest) = matches.first() {
        result.id = manifest.repository_id.clone();
    }
    Ok(result)
}

fn install_downloaded_repository(
    asset_path: &Path,
    download_name: &str,
    game_root: &Path,
    manifests_dir: &Path,
    recycle_root: &Path,
    repo: &RepositoryConfig,
    release_tag: &str,
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
    let recycled = if manifests_dir
        .join(format!("{}.toml", repo.id))
        .try_exists()
        .map_err(|error| error.to_string())?
    {
        let entry = recycle_bin::recycle_before_reinstall(
            recycle_root,
            manifests_dir,
            game_root,
            &repo.id,
            &replacement_files,
        )?;
        if let Some(entry) = &entry {
            log::info!(
                "重装前已回收旧 MOD 包：仓库={}，回收记录={}，文件数={}",
                repo.id,
                entry.id,
                entry.file_count
            );
        } else {
            log::info!(
                "重装前无可回收旧文件，继续安装，成功后更新原清单：仓库={}",
                repo.id
            );
        }
        entry
    } else {
        None
    };
    let installation: Result<usize, String> = (|| {
        let pending = if let Some(plan) = &plan {
            archive::extract_prepared_mod_zip(asset_path, game_root, plan)?
        } else {
            archive::copy_asset_pending(asset_path, game_root, &repo.install_target, download_name)?
        };
        let provider_name = match repo.provider {
            Provider::Github => "GitHub",
            Provider::Gitee => "Gitee",
        };
        let package_name = format!("{} - {}", repo.name, provider_name);
        let mut manifest = archive::new_manifest(
            &repo.id,
            release_tag,
            Some(&package_name),
            Some(&repo.name),
            pending.files().to_vec(),
        );
        manifest.repository_binding = RepositoryBinding::from_release(repo, release_tag);
        archive::write_manifest(manifests_dir, &manifest)?;
        Ok(pending.commit().len())
    })();
    if let Err(error) = &installation {
        if let Some(entry) = recycled {
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
                return Err("操作失败，部分文件保留在回收站，请查看日志".into());
            }
        }
    }
    installation.map_err(|error| format!("安装失败，已尝试回滚: {error}"))
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

fn restore_backup_dir(backup: &Path, game_root: &Path) -> Result<(), String> {
    if !backup.is_dir() {
        return Err("备份目录不存在".into());
    }
    let destination = game_root.join("BepInEx");
    ensure_copy_paths_disjoint(backup, &destination)?;
    if destination.exists() {
        let previous = game_root.join(format!("BepInEx.before-restore-{}", Utc::now().timestamp()));
        archive::clear_readonly(&destination)?;
        copy_dir(&destination, &previous)?;
        fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
    }
    copy_dir(backup, &destination)
}

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
    if source.starts_with(&destination) || destination.starts_with(&source) {
        return Err(
            "备份与框架目录重叠，已停止操作以避免递归复制。请将管理器移到游戏目录外后重试。".into(),
        );
    }
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!("目录不存在: {}", source.display()));
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
                        Package::Linux => "libdoorstop.so",
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
        for package in [Package::Windows, Package::Linux, Package::MacOS] {
            let workspace = Workspace::new();
            let archive = workspace.archive(package, package.native().then_some(SCRIPT));
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                package,
                false,
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
                package,
                true,
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

    #[test]
    fn malformed_native_scripts_roll_back_the_framework_and_existing_root_files() {
        for script in [None, Some("executable_name=\"\"\ninvalid enabled field\n")] {
            for reinstall in [false, true] {
                let workspace = Workspace::new();
                workspace.write("run_bepinex.sh", b"original script");
                workspace.write("libdoorstop.so", b"original loader");
                if reinstall {
                    workspace.write("BepInEx/core/BepInEx.dll", b"original core");
                    workspace.write("BepInEx/plugins/mod.dll", b"original plugin");
                    workspace.write("BepInEx/config/BepInEx.cfg", b"original config");
                }
                let archive = workspace.archive(Package::Linux, script);
                assert!(
                    install_framework_archive(
                        &archive,
                        &workspace.game(),
                        &workspace.backups(),
                        Package::Linux,
                        reinstall,
                    )
                    .is_err()
                );
                assert_eq!(
                    fs::read(workspace.game().join("run_bepinex.sh")).unwrap(),
                    b"original script"
                );
                assert_eq!(
                    fs::read(workspace.game().join("libdoorstop.so")).unwrap(),
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
                    Package::MacOS,
                    reinstall
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
                Package::Windows,
                false,
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
        for package in [Package::Windows, Package::Linux, Package::MacOS] {
            let name = package.url(&Provider::Github).rsplit('/').next().unwrap();
            let archive = packages.join(name);
            archive::validate_download_file(&archive, None, Some(package.digest())).unwrap();
            let workspace = Workspace::new();
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                package,
                false,
            )
            .unwrap();
            let info = game::detect_bepinex(&workspace.game(), Some(&workspace.backups()));
            assert!(info.installed && info.complete, "{name}");
            workspace.write("BepInEx/plugins/preserved.dll.disabled", b"user MOD");
            workspace.write("BepInEx/config/preserved.cfg", b"user settings");
            install_framework_archive(
                &archive,
                &workspace.game(),
                &workspace.backups(),
                package,
                true,
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
            operation: Mutex::new(()),
            repository_cache: tokio::sync::Mutex::new(()),
            community: Default::default(),
            readme: Default::default(),
            game_state: Mutex::new(None),
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
            Some("配置文件已被外部修改，请重新加载后再保存")
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
            Some("厨房目录已变化，请重新打开配置")
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
        assert_eq!(error, "MOD DLL 文件不存在，请重新扫描");
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
        assert_eq!(error.reason, "JS 文件没有可识别的 Release 或 Raw 代理");
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
                std::future::ready(Err("下载已取消".into()))
            },
            None,
        )
        .await;
        assert_eq!(result.unwrap_err(), "下载已取消");
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
        assert_eq!(result.unwrap_err(), "下载已取消");
        assert_eq!(attempted, 1);
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
                &archive::new_manifest("package", "v1", Some("Package"), None, files.to_vec()),
            )
            .unwrap();
            archive::write_manifest(
                &self.manifests,
                &archive::new_manifest(
                    "peer",
                    "v1",
                    Some("Peer"),
                    None,
                    vec!["BepInEx/plugins/Shared.dll".into()],
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
        let mut other = archive::new_manifest(&old_repo.id, "v1.0.0", None, None, vec![]);
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
            local.version = Some("100.0.0".into());
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
                            Some("Old package"),
                            None,
                            old_files,
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
                                Some("Peer"),
                                None,
                                vec![shared.into()],
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
                    );
                    if fail {
                        let error = result.unwrap_err();
                        assert!(
                            error.starts_with("安装失败，已尝试回滚:"),
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
                &archive::new_manifest("package", "v1", None, None, files),
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
                    None,
                    None,
                    vec!["BepInEx/plugins/Main.dll".into()],
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
            );
            fs::set_permissions(&path, permissions).unwrap();
            let error = result.unwrap_err();
            assert!(error.starts_with("安装失败，已尝试回滚:"), "{error}");
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
                        &archive::new_manifest(id, "v1", Some(id), None, vec![dll.into()]),
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
                );
                if fail {
                    let error = result.unwrap_err();
                    assert!(error.starts_with("安装失败，已尝试回滚:"), "{error}");
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
                "v2"
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
                "v2"
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
                    "v2"
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
        host_utilities.ok_or_else(|| "内置教程所需的主机实用 MOD 稳定版配置不存在".to_string())?;
    let configuration_manager =
        configuration_manager.ok_or_else(|| "内置教程所需的配置管理器配置不存在".to_string())?;
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
            operation: Mutex::new(()),
            repository_cache: tokio::sync::Mutex::new(()),
            community: Default::default(),
            readme: Default::default(),
            game_state: Mutex::new(None),
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
        assert_eq!(children.len(), 5);
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
            operation: Mutex::new(()),
            repository_cache: tokio::sync::Mutex::new(()),
            community: Default::default(),
            readme: Default::default(),
            game_state: Mutex::new(None),
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
