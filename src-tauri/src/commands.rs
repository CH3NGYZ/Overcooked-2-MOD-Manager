use crate::archive::{self, InstallManifest};
use crate::config::ConfigService;
use crate::game;
use crate::models::*;
use crate::mods;
use crate::releases;
use chrono::Utc;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use futures_util::StreamExt;
use tauri::{AppHandle, Emitter, State};
use url::Url;
use uuid::Uuid;

pub const BEPINEX_GITEE_URL: &str = "https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/bepinex.zip";
pub const BEPINEX_GITHUB_URL: &str =
    "https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/BepInEx_win_x86_5.4.23.5.zip";
const INSTALLER_GITEE_OWNER: &str = "ch3ngyz";
const INSTALLER_GITEE_REPO: &str = "Overcooked-2-MOD-Manager-Releases";
const INSTALLER_GITHUB_OWNER: &str = "CH3NGYZ";
const INSTALLER_GITHUB_REPO: &str = "Overcooked-2-MOD-Manager-Releases";
const GITEE_API_PROXY: &str = "https://oc2gtapi.ch3ng.top";
const GITHUB_API_PROXY: &str = "https://oc2ghapi.ch3ng.top";
const GITHUB_RELEASE_PROXY: &str = "https://oc2gh.ch3ng.top";
// These credentials are embedded at compile time for installer update checks only.
const INSTALLER_GITHUB_TOKEN: Option<&str> = option_env!("OC2_INSTALLER_GITHUB_TOKEN");
const INSTALLER_GITEE_TOKEN: Option<&str> = option_env!("OC2_INSTALLER_GITEE_TOKEN");

#[derive(Debug, Clone)]
struct InstallerChannel {
    id: &'static str,
    provider: Provider,
    endpoint: String,
    is_proxy: bool,
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

fn installer_api_sources(region: InstallerRegion) -> Vec<InstallerChannel> {
    let gitee = format!(
        "https://gitee.com/api/v5/repos/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases/latest"
    );
    let github = format!(
        "https://api.github.com/repos/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases/latest"
    );
    let mut sources = Vec::new();
    if region == InstallerRegion::MainlandChina {
        sources.push(InstallerChannel { id: "gitee-direct", provider: Provider::Gitee, endpoint: gitee.clone(), is_proxy: false });
        sources.push(InstallerChannel { id: "gitee-proxy", provider: Provider::Gitee, endpoint: releases::rewrite_proxy(GITEE_API_PROXY, &gitee), is_proxy: true });
    }
    sources.push(InstallerChannel { id: "github-direct", provider: Provider::Github, endpoint: github.clone(), is_proxy: false });
    sources.push(InstallerChannel { id: "github-proxy", provider: Provider::Github, endpoint: releases::rewrite_proxy(GITHUB_API_PROXY, &github), is_proxy: true });
    sources
}

fn installer_download_urls(
    region: InstallerRegion,
    provider: &Provider,
    direct_url: &str,
) -> Vec<String> {
    match (region, provider) {
        (InstallerRegion::MainlandChina, Provider::Gitee)
        | (InstallerRegion::International, Provider::Gitee) => vec![direct_url.to_string()],
        (InstallerRegion::MainlandChina, Provider::Github) => {
            vec![releases::rewrite_proxy(GITHUB_RELEASE_PROXY, direct_url)]
        }
        (InstallerRegion::International, Provider::Github) => vec![
            direct_url.to_string(),
            releases::rewrite_proxy(GITHUB_RELEASE_PROXY, direct_url),
        ],
    }
}

fn installer_api_token(provider: &Provider) -> Option<&'static str> {
    let token = match provider {
        Provider::Github => INSTALLER_GITHUB_TOKEN,
        Provider::Gitee => INSTALLER_GITEE_TOKEN,
    }?;
    (!token.trim().is_empty()).then_some(token)
}

fn installer_manifest_fallback(provider: &Provider) -> String {
    match provider {
        Provider::Gitee => format!(
            "https://gitee.com/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases/latest/download/latest.json"
        ),
        Provider::Github => format!(
            "https://github.com/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases/latest/download/latest.json"
        ),
    }
}

fn installer_asset_for_release<'a>(
    release: &'a ReleaseInfo,
    version: &semver::Version,
) -> Option<(&'a ReleaseAsset, &'static str)> {
    let setup_name = format!("OC2ModManager_{}_x64-setup.exe", version);
    let msi_name = format!("OC2ModManager_{}_x64_zh-CN.msi", version);
    release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(&setup_name))
        .map(|asset| (asset, "exe"))
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|asset| asset.name.eq_ignore_ascii_case(&msi_name))
                .map(|asset| (asset, "msi"))
        })
}

fn installer_channel_status(
    channel: &InstallerChannel,
    status: &str,
    error: Option<String>,
) -> AppUpdateChannelStatus {
    AppUpdateChannelStatus {
        id: channel.id.into(),
        provider: channel.provider.clone(),
        mode: if channel.is_proxy { "proxy" } else { "direct" }.into(),
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

fn inspect_installer_releases(
    channel: &InstallerChannel,
    releases: Vec<ReleaseInfo>,
    current_version: &semver::Version,
    region: InstallerRegion,
    update: &mut Option<AppUpdateInfo>,
) -> AppUpdateChannelStatus {
    let filtered = releases::filter_channel(releases, &Channel::Stable);
    let candidate = filtered.into_iter().find_map(|release| {
        let version = release
            .version
            .as_deref()
            .and_then(releases::parse_version)?;
        (version > *current_version).then_some((release, version))
    });
    let Some((release, version)) = candidate else {
        return installer_channel_status(channel, "latest", None);
    };
    let Some((installer, installer_type)) = installer_asset_for_release(&release, &version) else {
        return installer_channel_status(
            channel,
            "failed",
            Some("更新中没有匹配的安装包".into()),
        );
    };
    if update.is_none() {
        let installer_url = installer.download_url.clone();
        let installer_proxy_url = (channel.provider == Provider::Github)
            .then(|| releases::rewrite_proxy(GITHUB_RELEASE_PROXY, &installer_url));
        let installer_download_urls = installer_download_urls(
            region,
            &channel.provider,
            &installer_url,
        );
        let installer_size = installer.size;
        let installer_type = installer_type.to_string();
        let mut manifest_endpoints = release
            .assets
            .iter()
            .filter(|asset| {
                let name = asset.name.to_ascii_lowercase();
                name == "latest.json" || (name.starts_with("latest-") && name.ends_with(".json"))
            })
            .map(|asset| asset.download_url.clone())
            .collect::<Vec<_>>();
        if manifest_endpoints.is_empty() {
            manifest_endpoints.push(installer_manifest_fallback(&channel.provider));
        }
        *update = Some(AppUpdateInfo {
            version: version.to_string(),
            current_version: current_version.to_string(),
            body: (!release.body.is_empty()).then_some(release.body),
            date: release.published_at,
            download_size: Some(installer_size),
            manifest_endpoints,
            installer_url: Some(installer_url),
            installer_proxy_url,
            installer_download_urls,
            installer_type,
            provider: Some(channel.provider.clone()),
            source_channel: Some(channel.id.into()),
        });
    }
    installer_channel_status(channel, "update_available", None)
}

async fn discover_installer_update_for_channels(
    state: &AppState,
    app: &AppHandle,
    region: InstallerRegion,
    installer_channels: Vec<InstallerChannel>,
) -> Result<AppUpdateCheckResult, String> {
    let config = state.config.load_or_default();
    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| format!("当前程序版本无效: {e}"))?;
    let mut channels = Vec::new();
    let mut update = None;

    // API requests are deliberately sequential. A successful "latest"
    // response is authoritative and ends the API phase immediately; update
    // candidates remain available while later channels are queried so the
    // release phase has a platform fallback.
    for (index, channel) in installer_channels.iter().enumerate() {
        emit_installer_channel_status(
            app,
            region,
            installer_channel_status(channel, "checking", None),
        );
        let result = releases::fetch_release_url_with_token(
            &client,
            &channel.endpoint,
            &channel.provider,
            if channel.is_proxy {
                None
            } else {
                installer_api_token(&channel.provider)
            },
        )
        .await;
        match result {
            Ok(releases) => {
                let status = inspect_installer_releases(
                    channel,
                    releases,
                    &current_version,
                    region,
                    &mut update,
                );
                let is_latest = status.status == "latest";
                emit_installer_channel_status(app, region, status.clone());
                channels.push(status);
                if is_latest {
                    for skipped in installer_channels.iter().skip(index + 1) {
                        let skipped_status = installer_channel_status(
                            skipped,
                            "skipped",
                            Some("已确认当前为最新版本，未请求".into()),
                        );
                        emit_installer_channel_status(app, region, skipped_status.clone());
                        channels.push(skipped_status);
                    }
                    return Ok(AppUpdateCheckResult {
                        update: None,
                        channels,
                        all_failed: false,
                        region: region.as_str().into(),
                    });
                }
            }
            Err(error) => {
                let status = installer_channel_status(channel, "failed", Some(error.to_string()));
                emit_installer_channel_status(app, region, status.clone());
                channels.push(status);
            }
        }
    }

    let attempted = channels
        .iter()
        .filter(|channel| channel.status != "skipped")
        .collect::<Vec<_>>();
    let all_failed = !attempted.is_empty()
        && attempted.iter().all(|channel| channel.status == "failed");
    Ok(AppUpdateCheckResult {
        update,
        channels,
        all_failed,
        region: region.as_str().into(),
    })
}

async fn discover_installer_update_for_provider(
    state: &AppState,
    app: &AppHandle,
    provider: Provider,
) -> Result<AppUpdateCheckResult, String> {
    let region = detect_installer_region();
    let channels = installer_api_sources(region)
        .into_iter()
        .filter(|channel| channel.provider == provider)
        .collect();
    discover_installer_update_for_channels(state, app, region, channels).await
}

async fn discover_installer_update(
    state: &AppState,
    _locale: &str,
    app: &AppHandle,
) -> Result<AppUpdateCheckResult, String> {
    let region = detect_installer_region();
    discover_installer_update_for_channels(state, app, region, installer_api_sources(region)).await
}

pub struct AppState {
    pub config: ConfigService,
    pub operation: Mutex<()>,
    pub repository_cache: tokio::sync::Mutex<()>,
    pub release_size_cancellations: tokio::sync::Mutex<HashMap<String, Arc<AtomicBool>>>,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            config: ConfigService::new().map_err(|e| e.to_string())?,
            operation: Mutex::new(()),
            repository_cache: tokio::sync::Mutex::new(()),
            release_size_cancellations: tokio::sync::Mutex::new(HashMap::new()),
        })
    }
}

#[tauri::command]
pub fn get_dashboard(state: State<'_, AppState>) -> Result<DashboardData, String> {
    let config = state.config.load_or_default();
    let game_info = game::detect_game(&config);
    let mods = game_info
        .path
        .as_ref()
        .map(|path| mods::scan_mods(Path::new(path), &config))
        .unwrap_or_default();
    let updates = collect_update_status(&config, &mods);
    Ok(DashboardData {
        config: state.config.public_config(&config),
        game: game_info,
        mods,
        updates,
    })
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<PublicConfig, String> {
    let config = state.config.load_or_default();
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn get_install_language() -> Option<String> {
    let path = std::env::current_exe()
        .ok()?
        .parent()?
        .join("install-language.txt");
    let value = fs::read_to_string(path).ok()?.trim().to_string();
    match value.as_str() {
        "zh-CN" | "zh-TW" | "en" | "ko" => Some(value),
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
    let path = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .ok_or("无法确定程序目录")?
        .join("logs")
        .join(file_name);
    let content = match fs::read(&path) {
        Ok(bytes) => {
            const MAX_LOG_BYTES: usize = 512 * 1024;
            let start = bytes.len().saturating_sub(MAX_LOG_BYTES);
            String::from_utf8_lossy(&bytes[start..]).into_owned()
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
    let path = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .ok_or("无法确定程序目录")?
        .join("logs");
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
    let mut config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("尚未找到厨房目录")?;
    let installed = mods::scan_mods(Path::new(&game_root), &config);
    let selected = installed
        .iter()
        .find(|item| item.dll_file == dll_file)
        .ok_or("找不到对应的已安装 MOD")?;
    state
        .config
        .set_mod_note(&mut config, &mods::note_key(selected), &note);
    state.config.save(&config).map_err(|e| e.to_string())?;
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
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let source = PathBuf::from(source_path);
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
    let repository_id = format!("manual-{}", Uuid::new_v4());
    let (files, display_name) = if extension.eq_ignore_ascii_case("dll") {
        let files = archive::copy_asset(&source, &game_root, "BepInEx/plugins", file_name)?;
        (files, file_name.to_string())
    } else if extension.eq_ignore_ascii_case("zip") {
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
            archive::extract_mod_zip(&source, &game_root, "BepInEx/plugins", file_name)?,
            folder.to_string(),
        )
    } else {
        return Err("只支持 DLL 或 ZIP 文件".into());
    };
    if files.is_empty() {
        return Err("所选文件没有可安装内容".into());
    }
    let manifest = archive::new_manifest(&repository_id, &display_name, files.clone());
    if let Err(error) = archive::write_manifest(&state.config.manifests_dir(), &manifest) {
        for relative in files {
            if let Ok(path) = game::safe_game_path(&game_root, &relative) {
                let _ = archive::clear_readonly(&path);
                let _ = fs::remove_file(path);
            }
        }
        return Err(error);
    }
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
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest_dir = state.config.manifests_dir();
    let manifests = fs::read_dir(&manifest_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|text| toml::from_str::<archive::InstallManifest>(&text).ok())
        .collect::<Vec<_>>();
    let relative = Path::new(&dll_file);
    let safe_relative = mods::canonical_dll_name(&relative.to_string_lossy().replace('\\', "/"));
    let manifest = manifests
        .into_iter()
        .find(|manifest| {
            manifest
                .files
                .iter()
                .any(|file| file.eq_ignore_ascii_case(&safe_relative))
        });
    if let Some(manifest) = manifest {
        archive::remove_manifest_files(&game_root, &manifest)?;
        fs::remove_file(manifest_dir.join(format!("{}.toml", manifest.repository_id)))
            .map_err(|error| error.to_string())?;
        return Ok(UninstallLocalModResult {
            ok: true,
            message: "MOD 已删除".into(),
            requires_confirmation: false,
        });
    }
    if !allow_unmanaged {
        return Ok(UninstallLocalModResult {
            ok: false,
            message: "该 MOD 不是通过本管理器安装的，因此仅删除 DLL 文件，无法删除其他的残留文件，需自己手动处理删除。是否继续？".into(),
            requires_confirmation: true,
        });
    }
    if !Path::new(&safe_relative)
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("dll"))
    {
        return Err("只能删除 BepInEx/plugins 下的 DLL 文件".into());
    }
    let plugins_root = game::safe_game_path(&game_root, "BepInEx/plugins")?;
    let enabled_path = game::safe_game_path(&game_root, &safe_relative)?;
    let disabled_path = game::safe_game_path(&game_root, &mods::disabled_dll_file(&safe_relative))?;
    let dll_path = if enabled_path.is_file() {
        enabled_path
    } else {
        disabled_path
    };
    if !dll_path.starts_with(&plugins_root) {
        return Err("只能删除 BepInEx/plugins 下的 DLL 文件".into());
    }
    if !dll_path.is_file() {
        return Err("找不到对应的 MOD DLL 文件".into());
    }
    archive::clear_readonly(&dll_path)?;
    fs::remove_file(&dll_path).map_err(|error| error.to_string())?;
    Ok(UninstallLocalModResult {
        ok: true,
        message: "MOD DLL 已删除".into(),
        requires_confirmation: false,
    })
}

#[tauri::command]
pub fn toggle_local_mod(
    state: State<'_, AppState>,
    dll_file: String,
    enabled: bool,
) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let relative = mods::canonical_dll_name(
        &Path::new(&dll_file)
            .to_string_lossy()
            .replace('\\', "/"),
    );
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
    if !source.is_file() {
        return Err(if enabled {
            "找不到已禁用的 MOD DLL 文件"
        } else {
            "找不到已启用的 MOD DLL 文件"
        }
        .into());
    }
    if target.exists() {
        return Err("目标 MOD 文件已存在，无法切换".into());
    }
    archive::clear_readonly(source)?;
    fs::rename(source, target).map_err(|error| error.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "MOD 已启用".into()
        } else {
            "MOD 已禁用".into()
        },
    })
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<PublicConfig, String> {
    let mut config = state.config.load_or_default();
    if let Some(value) = patch.ui_zoom {
        config.ui_zoom = value.clamp(0.5, 2.0);
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
    if let Some(value) = patch.max_release_count {
        config.network.max_release_count = value.clamp(1, 100);
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn save_tokens(state: State<'_, AppState>, patch: TokenPatch) -> Result<PublicConfig, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    state
        .config
        .set_tokens(&mut config, &patch.github_token, &patch.gitee_token);
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn detect_game(state: State<'_, AppState>) -> Result<GameInfo, String> {
    let config = state.config.load_or_default();
    Ok(game::detect_game(&config))
}

#[tauri::command]
pub fn set_game_path(state: State<'_, AppState>, path: String) -> Result<GameInfo, String> {
    let root = PathBuf::from(&path);
    if !root.join("Overcooked2.exe").is_file() {
        return Err("所选目录中没有 Overcooked2.exe".into());
    }
    let mut config = state.config.load_or_default();
    config.game.path = root.to_string_lossy().to_string();
    config.game.auto_detect = false;
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(game::detect_game(&config))
}

#[tauri::command]
pub fn reset_game_path(state: State<'_, AppState>) -> Result<GameInfo, String> {
    let mut config = state.config.load_or_default();
    config.game.path.clear();
    config.game.auto_detect = true;
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(game::detect_game(&config))
}

#[tauri::command]
pub async fn refresh_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<Vec<ReleaseInfo>, String> {
    let config = state.config.load_or_default();
    let index = config
        .repositories
        .iter()
        .position(|repo| repo.id == repository_id)
        .ok_or("仓库不存在")?;
    if !config.repositories[index].enabled {
        return Err("仓库已禁用".into());
    }
    let repo = config.repositories[index].clone();
    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let token = state.config.repository_token(&repo);
    let fetched = releases::fetch_releases(
        &client,
        &repo,
        token.as_deref(),
        config.auth.official_api_only,
        config.network.max_release_count,
    )
    .await
    .map(|releases| releases::filter_channel(releases, &repo.channel))
    .map_err(|error| error.to_string());

    let _cache_guard = state.repository_cache.lock().await;
    let mut latest_config = state.config.load_or_default();
    let latest_index = latest_config
        .repositories
        .iter()
        .position(|item| item.id == repository_id)
        .ok_or("仓库已被删除")?;
    let latest_repo = &mut latest_config.repositories[latest_index];
    if latest_repo.url != repo.url
        || latest_repo.provider != repo.provider
        || latest_repo.channel != repo.channel
    {
        return Err("仓库配置已更改，请重新刷新".into());
    }

    match fetched {
        Ok(filtered) => {
            latest_repo.cached_releases = filtered.clone();
            latest_repo.last_error = None;
            state
                .config
                .save(&latest_config)
                .map_err(|error| error.to_string())?;
            Ok(filtered)
        }
        Err(error) => {
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

fn release_size_request_key(repository_id: &str, release_tag: &str) -> String {
    format!("{repository_id}\n{release_tag}")
}

async fn wait_for_release_size_cancellation(cancelled: Arc<AtomicBool>) {
    while !cancelled.load(Ordering::Acquire) {
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
}

#[tauri::command]
pub async fn refresh_release_asset_sizes(
    state: State<'_, AppState>,
    repository_id: String,
    release_tag: String,
) -> Result<ReleaseInfo, String> {
    let config = state.config.load_or_default();
    let repo = config
        .repositories
        .iter()
        .find(|repo| repo.id == repository_id)
        .ok_or("仓库不存在")?
        .clone();
    if repo.provider != Provider::Gitee {
        return Err("GitHub Release 已直接提供附件大小".into());
    }
    let release = repo
        .cached_releases
        .iter()
        .find(|release| release.tag_name == release_tag)
        .cloned()
        .ok_or("Release 不存在")?;
    let release_id = release.release_id.ok_or("Gitee Release 缺少 Release ID")?;
    let key = release_size_request_key(&repository_id, &release_tag);
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut requests = state.release_size_cancellations.lock().await;
        if let Some(previous) = requests.insert(key.clone(), cancelled.clone()) {
            previous.store(true, Ordering::Release);
        }
    }
    let client = match releases::build_client(config.network.timeout_seconds) {
        Ok(client) => client,
        Err(error) => {
            let mut requests = state.release_size_cancellations.lock().await;
            if requests
                .get(&key)
                .is_some_and(|current| Arc::ptr_eq(current, &cancelled))
            {
                requests.remove(&key);
            }
            return Err(error.to_string());
        }
    };
    let token = state.config.repository_token(&repo);
    let result = tokio::select! {
        assets = releases::fetch_gitee_asset_sizes(
            &client,
            &repo,
            release_id,
            token.as_deref(),
            config.auth.official_api_only,
        ) => assets.map_err(|error| error.to_string()),
        _ = wait_for_release_size_cancellation(cancelled.clone()) => Err("附件大小请求已取消".into()),
    };
    let mut requests = state.release_size_cancellations.lock().await;
    if requests
        .get(&key)
        .is_some_and(|current| Arc::ptr_eq(current, &cancelled))
    {
        requests.remove(&key);
    }
    drop(requests);
    let assets = result?;

    let _cache_guard = state.repository_cache.lock().await;
    let mut latest_config = state.config.load_or_default();
    let latest_repo = latest_config
        .repositories
        .iter_mut()
        .find(|repo| repo.id == repository_id)
        .ok_or("仓库已被删除")?;
    let latest_release = latest_repo
        .cached_releases
        .iter_mut()
        .find(|item| item.tag_name == release_tag)
        .ok_or("Release 已被刷新，请重新选择")?;
    for asset in &mut latest_release.assets {
        if let Some(updated) = assets.iter().find(|item| item.name == asset.name) {
            asset.size = updated.size;
            if asset.download_url.trim().is_empty() {
                asset.download_url = updated.download_url.clone();
            }
        }
    }
    let updated = latest_release.clone();
    state
        .config
        .save(&latest_config)
        .map_err(|e| e.to_string())?;
    Ok(updated)
}

#[tauri::command]
pub async fn cancel_release_asset_sizes(
    state: State<'_, AppState>,
    repository_id: String,
    release_tag: String,
) -> Result<OperationResult, String> {
    let key = release_size_request_key(&repository_id, &release_tag);
    if let Some(cancelled) = state.release_size_cancellations.lock().await.get(&key) {
        cancelled.store(true, Ordering::Release);
    }
    Ok(OperationResult {
        ok: true,
        message: "附件大小请求已取消".into(),
    })
}

#[tauri::command]
pub async fn install_bepinex(
    app: AppHandle,
    state: State<'_, AppState>,
    reinstall: bool,
    provider: Provider,
) -> Result<OperationResult, String> {
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = info.path.ok_or("尚未找到厨房目录")?;
    let game_root = PathBuf::from(game_root);
    game::ensure_game_stopped(&game_root)?;
    if info.bepinex.installed && !reinstall {
        return Err("BepInEx 已安装，如需覆盖请使用重新安装".into());
    }
    let backup = if reinstall && game_root.join("BepInEx").exists() {
        let backup = backup_bepinex_dir(&game_root, &state.config.backups_dir())?;
        Some(backup)
    } else {
        None
    };
    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|e| e.to_string())?;
    let download_url = match provider {
        Provider::Gitee => BEPINEX_GITEE_URL,
        Provider::Github => BEPINEX_GITHUB_URL,
    };
    let temp = archive::download_to_temp(
        &client,
        download_url,
        &state.config.cache_dir(),
        "BepInEx",
        Some(&app),
    )
    .await?;
    if let Err(error) = archive::validate_zip(&temp) {
        let _ = fs::remove_file(&temp);
        return Err(format!("BepInEx 下载文件校验失败: {error}"));
    }
    game::ensure_game_stopped(&game_root)?;
    if reinstall && game_root.join("BepInEx").exists() {
        archive::clear_readonly(&game_root.join("BepInEx"))?;
        fs::remove_dir_all(game_root.join("BepInEx"))
            .map_err(|e| format!("清理旧 BepInEx 失败: {e}"))?;
    }
    let backup = backup.or_else(|| {
        game_root
            .join("BepInEx")
            .exists()
            .then(|| backup_bepinex_dir(&game_root, &state.config.backups_dir()).ok())
            .flatten()
    });
    let extracted = archive::extract_zip_safe(&temp, &game_root);
    let _ = fs::remove_file(&temp);
    match extracted {
        Ok(_) => {
            if let Some(backup) = backup {
                restore_preserved_dirs(&backup, &game_root)?;
            }
            Ok(OperationResult {
                ok: true,
                message: "BepInEx 安装完成".into(),
            })
        }
        Err(error) => {
            if let Some(backup) = backup {
                let _ = restore_backup_dir(&backup, &game_root);
            }
            Err(format!("BepInEx 安装失败: {error}"))
        }
    }
}

#[tauri::command]
pub fn uninstall_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let config = state.config.load_or_default();
    let info = game::detect_game(&config);
    let game_root = PathBuf::from(info.path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let source = game_root.join("BepInEx");
    if !source.exists() {
        return Err("BepInEx 未安装".into());
    }
    archive::clear_readonly(&source)?;
    let backup = bepinex_backup_path(&state.config.backups_dir())?;
    // The game and portable manager may be on different drives. `rename`
    // fails with ERROR_NOT_SAME_DEVICE there, so copy and remove explicitly.
    copy_dir(&source, &backup)?;
    archive::clear_readonly(&source)?;
    fs::remove_dir_all(&source).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: format!("BepInEx 已移除，备份保存在 {}", backup.display()),
    })
}

#[tauri::command]
pub fn backup_bepinex(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let backup = backup_bepinex_dir(&game_root, &state.config.backups_dir())?;
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
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    restore_backup_dir(Path::new(&backup_path), &game_root)?;
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
    Ok(OperationResult {
        ok: true,
        message: "BepInEx 备份已删除".into(),
    })
}

#[tauri::command]
pub async fn install_repository(
    app: AppHandle,
    state: State<'_, AppState>,
    repository_id: String,
    tag: Option<String>,
    asset_name: Option<String>,
) -> Result<OperationResult, String> {
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
    let releases = if repo.cached_releases.is_empty() {
        let client =
            releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
        let token = state.config.repository_token(&repo);
        releases::filter_channel(
            releases::fetch_releases(
                &client,
                &repo,
                token.as_deref(),
                config.auth.official_api_only,
                config.network.max_release_count,
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
        .and_then(|value| releases.iter().find(|release| release.tag_name == value))
        .or_else(|| {
            repo.pinned_version.as_deref().and_then(|value| {
                releases.iter().find(|release| {
                    release.tag_name == value || release.version.as_deref() == Some(value)
                })
            })
        })
        .or_else(|| releases.first())
        .ok_or("没有可安装的 Release")?;
    let asset = release
        .assets
        .iter()
        .find(|asset| {
            asset_name
                .as_deref()
                .map(|name| asset.name == name)
                .unwrap_or_else(|| releases::asset_matches(asset, &repo.asset_rules))
        })
        .ok_or("没有匹配的 Release 资产")?;
    let proxy_url = if repo.release_proxy_url.trim().is_empty() {
        asset.download_url.clone()
    } else {
        releases::rewrite_proxy(&repo.release_proxy_url, &asset.download_url)
    };
    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|e| e.to_string())?;
    let download_threads = match repo.provider {
        Provider::Github => repo.download_threads.clamp(1, 32),
        Provider::Gitee => 1,
    };
    log::info!(
        "开始下载 Release：仓库={}，平台={:?}，线程数={}，Release 代理策略={:?}",
        repo.name,
        repo.provider,
        download_threads,
        repo.release_proxy_mode
    );
    archive::start_download();
    let temp = match repo.release_proxy_mode {
        ProxyMode::Direct => {
            archive::download_to_temp_with_threads(
                &client,
                &asset.download_url,
                &state.config.cache_dir(),
                &repo.name,
                Some(&app),
                download_threads,
            )
            .await?
        }
        ProxyMode::Fixed => {
            archive::download_to_temp_with_threads(
                &client,
                &proxy_url,
                &state.config.cache_dir(),
                &repo.name,
                Some(&app),
                download_threads,
            )
            .await?
        }
        ProxyMode::Auto => match archive::download_to_temp_with_threads(
            &client,
            &asset.download_url,
            &state.config.cache_dir(),
            &repo.name,
            Some(&app),
            download_threads,
        )
        .await
        {
            Ok(path) => path,
            Err(_) if proxy_url != asset.download_url => {
                archive::download_to_temp_with_threads(
                    &client,
                    &proxy_url,
                    &state.config.cache_dir(),
                    &repo.name,
                    Some(&app),
                    download_threads,
                )
                .await?
            }
            Err(error) => return Err(error),
        },
    };
    if game::is_game_running(&game_root) {
        let _ = fs::remove_file(&temp);
        return Err("下载期间厨房重新启动，安装已取消，旧版本未删除".into());
    }
    archive::verify_digest(&temp, asset.digest.as_deref())?;
    let is_zip = temp.extension().and_then(|value| value.to_str()) == Some("zip")
        || asset.name.to_ascii_lowercase().ends_with(".zip");
    if is_zip {
        if let Err(error) = archive::validate_zip(&temp) {
            let _ = fs::remove_file(&temp);
            return Err(format!("下载文件校验失败: {error}"));
        }
    }
    let old_manifest = archive::read_manifest(&state.config.manifests_dir(), &repo.id);
    let old_backup = old_manifest
        .as_ref()
        .map(|manifest| backup_manifest_files(&game_root, manifest, &state.config.backups_dir()))
        .transpose()?;
    if let Some(manifest) = &old_manifest {
        archive::remove_manifest_files(&game_root, manifest)?;
    }
    let files_result = if is_zip {
        archive::extract_mod_zip(&temp, &game_root, &repo.install_target, &asset.name)
    } else {
        archive::copy_asset(&temp, &game_root, &repo.install_target, &asset.name)
    };
    let _ = fs::remove_file(&temp);
    match files_result {
        Ok(files) => {
            archive::write_manifest(
                &state.config.manifests_dir(),
                &archive::new_manifest(&repo.id, &release.tag_name, files),
            )?;
            Ok(OperationResult {
                ok: true,
                message: format!("{} {} 安装完成", repo.name, release.tag_name),
            })
        }
        Err(error) => {
            if let Some(backup) = old_backup {
                let _ = restore_manifest_backup(&backup, &game_root);
            }
            Err(format!("安装失败，已尝试回滚: {error}"))
        }
    }
}

#[tauri::command]
pub fn cancel_download() -> Result<OperationResult, String> {
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
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let manifest = archive::read_manifest(&state.config.manifests_dir(), &repository_id)
        .ok_or("没有安装清单，拒绝删除未知文件")?;
    let removed = archive::remove_manifest_files(&game_root, &manifest)?;
    let _ = fs::remove_file(
        state
            .config
            .manifests_dir()
            .join(format!("{repository_id}.toml")),
    );
    Ok(OperationResult {
        ok: true,
        message: format!("已卸载 {removed} 个文件"),
    })
}

#[tauri::command]
pub fn export_repositories(
    state: State<'_, AppState>,
    ids: Option<Vec<String>>,
    include_builtin: bool,
) -> Result<String, String> {
    let config = state.config.load_or_default();
    state
        .config
        .export_bundle(&config, ids, include_builtin)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn export_repositories_to_file(
    state: State<'_, AppState>,
    path: String,
    ids: Option<Vec<String>>,
    include_builtin: bool,
) -> Result<OperationResult, String> {
    let text = export_repositories(state, ids, include_builtin)?;
    if !path.to_ascii_lowercase().ends_with(".oc2repo.toml")
        && !path.to_ascii_lowercase().ends_with(".toml")
    {
        return Err("导出文件必须使用 .oc2repo.toml 或 .toml 扩展名".into());
    }
    fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: format!("已导出到 {path}"),
    })
}

#[tauri::command]
pub fn copy_repositories(
    state: State<'_, AppState>,
    ids: Option<Vec<String>>,
    include_builtin: bool,
) -> Result<OperationResult, String> {
    let text = export_repositories(state, ids, include_builtin)?;
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.set_text(text).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "仓库 TOML 已复制到剪贴板".into(),
    })
}

#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    clipboard.get_text().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn read_text_file(path: String) -> Result<String, String> {
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
    Ok(preview)
}

#[tauri::command]
pub async fn import_repository_source(
    state: State<'_, AppState>,
    source_id: String,
    source_url: String,
) -> Result<ImportPreview, String> {
    let source_url = source_url.trim();
    let parsed = Url::parse(source_url).map_err(|error| format!("仓库源地址无效: {error}"))?;
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("仓库源必须使用 HTTP 或 HTTPS 地址".into());
    }
    // Remove only this source's repositories before downloading. The lock is
    // deliberately released during the network request so other sources can
    // download and apply their own files concurrently.
    let (timeout_seconds, previous_repositories) = {
        let _guard = state.repository_cache.lock().await;
        let original_config = state.config.load_or_default();
        let previous_repositories = original_config
            .repositories
            .iter()
            .filter(|repo| repo.source_id.as_deref() == Some(source_id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let mut cleared_config = original_config.clone();
        cleared_config
            .repositories
            .retain(|repo| repo.source_id.as_deref() != Some(source_id.as_str()));
        state
            .config
            .save(&cleared_config)
            .map_err(|e| e.to_string())?;
        (
            original_config.network.timeout_seconds,
            previous_repositories,
        )
    };
    let client = releases::build_client(timeout_seconds).map_err(|e| e.to_string())?;
    let fetched = client
        .get(source_url)
        .header("User-Agent", releases::APP_USER_AGENT)
        .send()
        .await
        .map_err(|error| error.to_string())
        .and_then(|response| {
            response
                .error_for_status()
                .map_err(|error| error.to_string())
        });
    let fetched = match fetched {
        Ok(response) => response.text().await.map_err(|error| error.to_string()),
        Err(error) => Err(error),
    };
    let text = match fetched {
        Ok(text) => text,
        Err(error) => {
            restore_source_repositories(&state, &source_id, &previous_repositories).await;
            return Err(error);
        }
    };
    let document = match state.config.validate_repository_source_document(&text) {
        Ok(document) => document,
        Err(error) => {
            restore_source_repositories(&state, &source_id, &previous_repositories).await;
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
    let source_urls = repositories
        .iter()
        .map(|repository| repository.url.trim().to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let child_sources = document.repository_sources;
    let bundle = crate::models::RepositoryBundle {
        format: "oc2-mod-repository-bundle".into(),
        schema_version: 1,
        exported_at: Utc::now().to_rfc3339(),
        repositories,
        repository_sources: Vec::new(),
    };
    let _guard = state.repository_cache.lock().await;
    let mut config = state.config.load_or_default();
    config
        .repositories
        .retain(|repo| repo.source_id.as_deref() != Some(source_id.as_str()));
    let parent_url = config
        .repository_sources
        .iter()
        .find(|source| source.id == source_id)
        .map(|source| source.url.trim().to_ascii_lowercase());
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
                && !removed_child_ids.iter().any(|removed| removed == &source.id)
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
        let child_url = child.url.trim().to_string();
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
            existing.url = child_url;
            existing.region = child.region.clone();
            existing.builtin = child.builtin;
            continue;
        }
        if config
            .repository_sources
            .iter()
            .any(|source| source.url.trim().eq_ignore_ascii_case(&child_url))
        {
            // A source URL is authoritative for identity. Keeping the
            // existing owner also prevents nested sources from forming a cycle.
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
            url: child_url,
            region: child.region,
            builtin: child.builtin,
            parent_id: Some(source_id.clone()),
        });
    }
    let preview = state
        .config
        .import_bundle(&mut config, &bundle, "overwrite");
    for repo in &mut config.repositories {
        if source_urls.contains(&repo.url.trim().to_ascii_lowercase()) {
            repo.source_id = Some(source_id.clone());
        }
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(preview)
}

async fn restore_source_repositories(
    state: &AppState,
    source_id: &str,
    repositories: &[RepositoryConfig],
) {
    let _guard = state.repository_cache.lock().await;
    let mut config = state.config.load_or_default();
    config
        .repositories
        .retain(|repo| repo.source_id.as_deref() != Some(source_id));
    config.repositories.extend(repositories.iter().cloned());
    let _ = state.config.save(&config);
}

#[tauri::command]
pub fn save_repository_source(
    state: State<'_, AppState>,
    id: Option<String>,
    nickname: String,
    url: String,
) -> Result<PublicConfig, String> {
    let mut config = state.config.load_or_default();
    let nickname = nickname.trim();
    let url = url.trim();
    Url::parse(url).map_err(|error| format!("仓库源地址无效: {error}"))?;
    if nickname.is_empty() || url.is_empty() {
        return Err("仓库源昵称和地址不能为空".into());
    }
    if config
        .repository_sources
        .iter()
        .any(|item| Some(item.id.as_str()) != id.as_deref() && item.url.eq_ignore_ascii_case(url))
    {
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
        source.url = url.into();
    } else {
        config.repository_sources.push(RepositorySource {
            id: Uuid::new_v4().to_string(),
            nickname: nickname.into(),
            url: url.into(),
            region: None,
            builtin: false,
            parent_id: None,
        });
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn save_repository_sources(
    state: State<'_, AppState>,
    sources: Vec<RepositorySource>,
) -> Result<PublicConfig, String> {
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
        .map(|source| source.url.trim().to_ascii_lowercase())
        .collect::<std::collections::HashSet<_>>();
    let mut seen_urls = std::collections::HashSet::new();
    seen_urls.extend(builtin_root_urls);
    seen_urls.extend(
        config
            .repository_sources
            .iter()
            .filter(|source| source.builtin && source.parent_id.is_some())
            .map(|source| source.url.trim().to_ascii_lowercase()),
    );
    let mut seen_ids = config
        .repository_sources
        .iter()
        .filter(|source| source.builtin)
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut ordered = config
        .repository_sources
        .iter()
        .filter(|source| source.builtin)
        .cloned()
        .collect::<Vec<_>>();
    for source in sources {
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
        if id.len() > 128 || !id.bytes().enumerate().all(|(index, byte)| {
            (byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-' || byte == b'.')
                && (index > 0 || byte.is_ascii_alphanumeric())
        }) {
            return Err("仓库唯一 ID 格式无效".into());
        }
        let nickname = source.nickname.trim().to_string();
        let url = source.url.trim().to_string();
        if nickname.is_empty() || url.is_empty() {
            return Err("仓库源昵称和地址不能为空".into());
        }
        crate::config::validate_repository_source_region(source.region.as_deref())
            .map_err(|error| error.to_string())?;
        let parsed = Url::parse(&url).map_err(|error| format!("仓库源地址无效: {error}"))?;
        if parsed.scheme() != "http" && parsed.scheme() != "https" {
            return Err("仓库源必须使用 HTTP 或 HTTPS 地址".into());
        }
        if !seen_urls.insert(url.to_ascii_lowercase()) {
            return Err("仓库源地址已存在".into());
        }
        if !seen_ids.insert(id.clone()) {
            return Err("仓库源标识重复".into());
        }
        ordered.push(RepositorySource {
            id,
            nickname,
            url,
            region: source.region.map(|value| value.trim().to_ascii_lowercase()),
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
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn delete_repository_source(
    state: State<'_, AppState>,
    id: String,
) -> Result<PublicConfig, String> {
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
    config.repository_sources.retain(|source| !removed_ids.iter().any(|removed| removed == &source.id));
    for repository in &mut config.repositories {
        if repository.source_id.as_ref().is_some_and(|source_id| removed_ids.iter().any(|removed| removed == source_id)) {
            repository.source_id = None;
        }
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn reorder_repository_sources(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<PublicConfig, String> {
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
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn save_repository(
    state: State<'_, AppState>,
    patch: RepositoryPatch,
) -> Result<PublicConfig, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let mut config = state.config.load_or_default();
    if patch.asset_rules.is_empty()
        || patch
            .asset_rules
            .iter()
            .any(|rule| rule.value.trim().is_empty())
    {
        return Err("请创建至少一个有效的资产规则".into());
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
                repo.group.clone(),
                repo.group_i18n.clone(),
            )
        });
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
    let api_proxy_url = patch.api_proxy_url.clone();
    let release_proxy_url = patch.release_proxy_url.clone();
    let mut item = RepositoryConfig {
        id: id.clone(),
        name: patch.name,
        name_i18n: existing_translations
            .as_ref()
            .map(|(name_i18n, _, _, _, _)| name_i18n.clone())
            .unwrap_or_default(),
        group: if patch.group.trim().is_empty() {
            existing_translations
                .as_ref()
                .map(|(_, _, _, group, _)| group.clone())
                .unwrap_or_default()
        } else {
            patch.group.trim().to_string()
        },
        group_i18n: existing_translations
            .as_ref()
            .map(|(_, _, _, _, group_i18n)| group_i18n.clone())
            .unwrap_or_default(),
        provider: patch.provider.clone(),
        url: patch.url,
        channel: patch.channel,
        asset_rules: patch.asset_rules,
        install_target: patch.install_target,
        note: patch.note,
        note_i18n: existing_translations
            .as_ref()
            .map(|(_, note_i18n, _, _, _)| note_i18n.clone())
            .unwrap_or_default(),
        source_id: existing_translations
            .as_ref()
            .and_then(|(_, _, source_id, _, _)| source_id.clone()),
        plugin_guid: patch.plugin_guid,
        local_product_name: patch.local_product_name,
        local_original_filename: patch.local_original_filename,
        download_threads,
        api_proxy_mode: patch.api_proxy_mode,
        api_proxy_url,
        release_proxy_mode: patch.release_proxy_mode,
        release_proxy_url,
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
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn delete_repository(
    state: State<'_, AppState>,
    repository_id: String,
) -> Result<OperationResult, String> {
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
    config.repositories.remove(index);
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "仓库已删除".into(),
    })
}

#[tauri::command]
pub fn launch_normal(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    if config.launch.auto_restore_winhttp {
        match game::winhttp_state(&game_root) {
            WinhttpState::ModDisabled => {
                fs::rename(
                    game_root.join("winhttp.dll.oc2mod.disabled"),
                    game_root.join("winhttp.dll"),
                )
                .map_err(|e| e.to_string())?;
            }
            WinhttpState::Conflict => {
                // A conflict means the MOD DLL is already present. Remove
                // only the disabled marker and keep winhttp.dll in place.
                archive::clear_readonly(&game_root.join("winhttp.dll.oc2mod.disabled"))?;
                fs::remove_file(game_root.join("winhttp.dll.oc2mod.disabled"))
                    .map_err(|e| e.to_string())?;
            }
            _ => {}
        }
    }
    open_steam()
}

#[tauri::command]
pub fn launch_no_mod(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    if game::winhttp_state(&game_root) == WinhttpState::Conflict {
        return Err("winhttp.dll 与禁用备份同时存在，请先处理冲突".into());
    }
    if game_root.join("winhttp.dll").is_file() {
        fs::rename(
            game_root.join("winhttp.dll"),
            game_root.join("winhttp.dll.oc2mod.disabled"),
        )
        .map_err(|e| e.to_string())?;
    }
    open_steam()
}

#[tauri::command]
pub fn restore_mod(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    if game::winhttp_state(&game_root) == WinhttpState::ModDisabled {
        fs::rename(
            game_root.join("winhttp.dll.oc2mod.disabled"),
            game_root.join("winhttp.dll"),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(OperationResult {
        ok: true,
        message: "MOD 启动状态已恢复".into(),
    })
}

/// Switches the MOD loader DLL state without launching Steam. The overview
/// status card uses this command; actual game launch remains explicit.
#[tauri::command]
pub fn switch_winhttp(
    state: State<'_, AppState>,
    enabled: bool,
) -> Result<OperationResult, String> {
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let current = game::winhttp_state(&game_root);
    if enabled {
        match current {
            WinhttpState::ModDisabled => {
                archive::clear_readonly(&game_root.join("winhttp.dll.oc2mod.disabled"))?;
                fs::rename(
                    game_root.join("winhttp.dll.oc2mod.disabled"),
                    game_root.join("winhttp.dll"),
                )
                .map_err(|e| e.to_string())?;
            }
            WinhttpState::Conflict => {
                archive::clear_readonly(&game_root.join("winhttp.dll.oc2mod.disabled"))?;
                fs::remove_file(game_root.join("winhttp.dll.oc2mod.disabled"))
                    .map_err(|e| e.to_string())?;
            }
            WinhttpState::ModEnabled => {}
            WinhttpState::Missing => return Err("未找到 winhttp.dll，无法切换 MOD 状态".into()),
        }
    } else {
        match current {
            WinhttpState::ModEnabled => {
                archive::clear_readonly(&game_root.join("winhttp.dll"))?;
                fs::rename(
                    game_root.join("winhttp.dll"),
                    game_root.join("winhttp.dll.oc2mod.disabled"),
                )
                .map_err(|e| e.to_string())?;
            }
            WinhttpState::ModDisabled => {}
            WinhttpState::Conflict => {
                return Err("winhttp.dll 与禁用文件同时存在，请先切换为有 MOD 状态".into());
            }
            WinhttpState::Missing => return Err("未找到 winhttp.dll，无法切换 MOD 状态".into()),
        }
    }
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "MOD 已启用，尚未启动厨房".into()
        } else {
            "MOD 已关闭，尚未启动厨房".into()
        },
    })
}

#[tauri::command]
pub fn open_devtools(window: tauri::WebviewWindow) -> Result<OperationResult, String> {
    window.open_devtools();
    Ok(OperationResult {
        ok: true,
        message: "开发者控制台已打开".into(),
    })
}

#[tauri::command]
pub fn reset_ui(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let cache = state.config.cache_dir();
    if cache.exists() {
        fs::remove_dir_all(&cache).map_err(|e| e.to_string())?;
    }
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut config = state.config.load_or_default();
    config.ui_zoom = 1.0;
    for repo in &mut config.repositories {
        repo.cached_releases.clear();
        repo.etag = None;
        repo.last_error = None;
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已清除缓存并重置界面".into(),
    })
}

#[tauri::command]
pub fn open_game_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let config = state.config.load_or_default();
    let path = game::detect_game(&config).path.ok_or("尚未找到厨房目录")?;
    std::process::Command::new("explorer.exe")
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
    let config = state.config.load_or_default();
    let game_root = game::detect_game(&config).path.ok_or("尚未找到厨房目录")?;
    let plugins = PathBuf::from(game_root).join("BepInEx/plugins");
    fs::create_dir_all(&plugins).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe")
        .arg(&plugins)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已打开模组目录".into(),
    })
}

#[tauri::command]
pub fn open_backup_directory(state: State<'_, AppState>) -> Result<OperationResult, String> {
    let path = state.config.backups_dir();
    fs::create_dir_all(&path).map_err(|e| e.to_string())?;
    std::process::Command::new("explorer.exe")
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
    let parsed = Url::parse(&url).map_err(|e| format!("发布页地址无效: {e}"))?;
    if parsed.scheme() != "https" {
        return Err("发布页必须使用 HTTPS".into());
    }
    std::process::Command::new("explorer.exe")
        .arg(parsed.as_str())
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已打开发布页".into(),
    })
}

#[tauri::command]
pub fn exit_app() -> Result<(), String> {
    std::process::exit(0)
}

#[tauri::command]
pub async fn check_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    locale: Option<String>,
) -> Result<AppUpdateCheckResult, String> {
    let _ = locale;
    match tokio::time::timeout(
        std::time::Duration::from_secs(90),
        discover_installer_update(&state, "zh-CN", &app),
    )
    .await
    {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => {
            let region = detect_installer_region();
            log::error!("安装器更新检查初始化失败: {error}");
            let channels = installer_api_sources(region)
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
            let region = detect_installer_region();
            let channels = installer_api_sources(region)
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
    app: &AppHandle,
    label: &str,
) -> Result<Vec<u8>, String> {
    archive::start_download();
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
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| error.to_string())?;
        bytes.extend_from_slice(&chunk);
        downloaded += chunk.len() as u64;
        archive::emit_progress(Some(app), label, downloaded, total, started.elapsed(), false);
    }
    archive::emit_progress(Some(app), label, downloaded, total, started.elapsed(), true);
    Ok(bytes)
}

async fn download_installer_with_timeout(
    client: &reqwest::Client,
    url: &str,
    app: &AppHandle,
    label: &str,
) -> Result<Vec<u8>, String> {
    match tokio::time::timeout(
        std::time::Duration::from_secs(15),
        download_installer(client, url, app, label),
    )
    .await
    {
        Ok(result) => {
            if result.is_err() {
                archive::emit_progress(
                    Some(app),
                    label,
                    0,
                    None,
                    std::time::Duration::from_secs(0),
                    true,
                );
            }
            result
        }
        Err(_) => {
            archive::cancel_download();
            archive::emit_progress(
                Some(app),
                label,
                0,
                None,
                std::time::Duration::from_secs(15),
                true,
            );
            Err("单个安装包下载超过 15 秒，已跳过当前渠道".into())
        }
    }
}

async fn download_installer_for_info(
    client: &reqwest::Client,
    info: &AppUpdateInfo,
    app: &AppHandle,
) -> Result<Vec<u8>, Vec<String>> {
    let urls = if info.installer_download_urls.is_empty() {
        info.installer_url.clone().into_iter().collect::<Vec<_>>()
    } else {
        info.installer_download_urls.clone()
    };
    let mut errors = Vec::new();
    for url in urls {
        match download_installer_with_timeout(client, &url, app, "正在下载程序更新").await {
            Ok(value) => return Ok(value),
            Err(error) => errors.push(error),
        }
    }
    Err(errors)
}

fn emit_installer_download_failure(
    app: &AppHandle,
    region: InstallerRegion,
    info: &AppUpdateInfo,
    error: &str,
) {
    let Some(channel_id) = info.source_channel.as_deref() else {
        return;
    };
    let Some(channel) = installer_api_sources(region)
        .into_iter()
        .find(|channel| channel.id == channel_id)
    else {
        return;
    };
    emit_installer_channel_status(
        app,
        region,
        installer_channel_status(&channel, "failed", Some(error.into())),
    );
}

#[tauri::command]
pub async fn install_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    locale: Option<String>,
) -> Result<OperationResult, String> {
    let result = discover_installer_update(&state, locale.as_deref().unwrap_or("zh-CN"), &app).await?;
    let mut info = result.update.ok_or("当前已经是最新版本")?;
    let config = state.config.load_or_default();
    let client = releases::build_download_client(config.network.timeout_seconds)
        .map_err(|e| e.to_string())?;
    let region = detect_installer_region();
    let mut errors = Vec::new();
    let mut bytes = None;
    let mut github_retried = false;
    loop {
        match download_installer_for_info(&client, &info, &app).await {
            Ok(value) => {
                bytes = Some(value);
                break;
            }
            Err(download_errors) => {
                let detail = if download_errors.is_empty() {
                    "更新中没有匹配的安装包".to_string()
                } else {
                    download_errors.join("；")
                };
                errors.push(detail.clone());
                emit_installer_download_failure(&app, region, &info, &detail);

                // Gitee release URLs are direct-only. Once that platform's
                // asset download fails, query GitHub again and hand the new
                // candidate to the same release download layer.
                if info.provider == Some(Provider::Gitee) && !github_retried {
                    github_retried = true;
                    let fallback =
                        discover_installer_update_for_provider(&state, &app, Provider::Github)
                            .await?;
                    if let Some(next_info) = fallback.update {
                        info = next_info;
                        continue;
                    }
                    errors.push(if fallback.all_failed {
                        "GitHub API 请求失败，无法继续下载更新".into()
                    } else {
                        "GitHub 当前没有可用的更新安装包".into()
                    });
                }
                break;
            }
        }
    }
    let bytes = bytes.ok_or_else(|| {
        if errors.is_empty() {
            "更新中没有匹配的安装包".to_string()
        } else {
            format!("更新下载失败：{}", errors.join("；"))
        }
    })?;
    let installer_type = if info.installer_type.eq_ignore_ascii_case("exe") {
        "exe"
    } else {
        "msi"
    };
    let path = std::env::temp_dir().join(format!(
        "OC2-Mod-Manager-{}.{}",
        info.version, installer_type
    ));
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    if installer_type == "exe" {
        std::process::Command::new(&path)
            .arg("/S")
            .arg("/OC2AutoLaunch")
            .spawn()
            .map_err(|e| e.to_string())?;
    } else {
        std::process::Command::new("msiexec.exe")
            .args(["/i", path.to_string_lossy().as_ref(), "/quiet"])
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    std::process::exit(0);
    /*
    let update = app
        .updater_builder()
        .header("User-Agent", releases::APP_USER_AGENT)
        .map_err(|error| error.to_string())?
        .endpoints(endpoints)
        .map_err(|error| error.to_string())?
        .timeout(Duration::from_secs(20))
        .restart_after_install(true)
        .build()
        .map_err(|error| error.to_string())?
        .check()
        .await
        .map_err(|error| error.to_string())?
        .ok_or("更新清单中没有可安装的签名版本")?;
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|error| error.to_string())?;
    */
}

fn collect_update_status(config: &AppConfig, installed: &[InstalledMod]) -> Vec<UpdateStatus> {
    config
        .repositories
        .iter()
        .filter(|repo| repo.enabled)
        .map(|repo| {
            let local = installed
                .iter()
                .find(|item| item.repository_ids.iter().any(|id| id == &repo.id)
                    || item.repository_id.as_deref() == Some(repo.id.as_str()))
                .and_then(|item| item.version.clone());
            let remote = releases::filter_channel(repo.cached_releases.clone(), &repo.channel)
                .first()
                .and_then(|release| release.version.clone());
            let state = match (
                local.as_deref().and_then(releases::parse_version),
                remote.as_deref().and_then(releases::parse_version),
            ) {
                (Some(local), Some(remote)) if remote > local => "update_available",
                (Some(_), Some(_)) => "latest",
                (None, Some(_)) => "unknown",
                (_, None) if repo.last_error.is_some() => "repository_unavailable",
                _ => "unknown",
            };
            UpdateStatus {
                repository_id: repo.id.clone(),
                local_version: local,
                remote_version: remote,
                state: state.into(),
            }
        })
        .collect()
}

fn open_steam() -> Result<OperationResult, String> {
    std::process::Command::new("explorer.exe")
        .arg("steam://rungameid/728880")
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(OperationResult {
        ok: true,
        message: "已请求 Steam 启动厨房，请在 Steam 中确认启动".into(),
    })
}

fn backup_bepinex_dir(game_root: &Path, backup_root: &Path) -> Result<PathBuf, String> {
    let target = bepinex_backup_path(backup_root)?;
    copy_dir(&game_root.join("BepInEx"), &target)?;
    Ok(target)
}

fn backup_manifest_files(
    game_root: &Path,
    manifest: &InstallManifest,
    backup_root: &Path,
) -> Result<PathBuf, String> {
    let target = backup_root.join(format!(
        "manifest-{}-{}",
        manifest.repository_id,
        Uuid::new_v4()
    ));
    fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    for file in &manifest.files {
        let source = game::safe_game_path(game_root, file)?;
        if source.is_file() {
            let destination = target.join(file);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            fs::copy(source, destination).map_err(|e| e.to_string())?;
        }
    }
    Ok(target)
}

fn restore_manifest_backup(backup: &Path, game_root: &Path) -> Result<(), String> {
    if !backup.exists() {
        return Ok(());
    }
    for entry in walkdir::WalkDir::new(backup)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
    {
        let relative = entry
            .path()
            .strip_prefix(backup)
            .map_err(|e| e.to_string())?;
        let destination = game_root.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::copy(entry.path(), destination).map_err(|e| e.to_string())?;
    }
    Ok(())
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
        if source.exists() && !destination.exists() {
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
    if destination.exists() {
        let previous = game_root.join(format!("BepInEx.before-restore-{}", Utc::now().timestamp()));
        archive::clear_readonly(&destination)?;
        copy_dir(&destination, &previous)?;
        fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
    }
    copy_dir(backup, &destination)
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!("目录不存在: {}", source.display()));
    }
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
