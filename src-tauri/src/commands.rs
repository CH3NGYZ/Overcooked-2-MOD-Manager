use crate::archive::{self, InstallManifest};
use crate::config::ConfigService;
use crate::game;
use crate::models::*;
use crate::mods;
use crate::releases;
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, State};
use url::Url;
use uuid::Uuid;

pub const BEPINEX_URL: &str = "https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/bepinex.zip";
const INSTALLER_GITEE_OWNER: &str = "ch3ngyz";
const INSTALLER_GITEE_REPO: &str = "Overcooked-2-MOD-Manager-Releases";
const INSTALLER_GITHUB_OWNER: &str = "CH3NGYZ";
const INSTALLER_GITHUB_REPO: &str = "Overcooked-2-MOD-Manager-Releases";
const GITEE_API_PROXY: &str = "https://oc2gtapi.ch3ng.top";
const GITHUB_API_PROXY: &str = "https://oc2ghapi.ch3ng.top";

fn installer_api_sources() -> Vec<(Provider, String, bool)> {
    let gitee = format!(
        "https://gitee.com/api/v5/repos/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases/latest"
    );
    let github = format!(
        "https://api.github.com/repos/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases/latest"
    );
    vec![
        (Provider::Gitee, gitee.clone(), false),
        (
            Provider::Gitee,
            releases::rewrite_proxy(GITEE_API_PROXY, &gitee),
            true,
        ),
        (Provider::Github, github.clone(), false),
        (
            Provider::Github,
            releases::rewrite_proxy(GITHUB_API_PROXY, &github),
            true,
        ),
    ]
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

async fn discover_installer_update(state: &AppState) -> Result<Option<AppUpdateInfo>, String> {
    let config = state.config.load_or_default();
    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let current_version = semver::Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|e| format!("当前程序版本无效: {e}"))?;
    for (provider, endpoint, is_proxy) in installer_api_sources() {
        let token = if is_proxy {
            None
        } else {
            match provider {
                Provider::Github => state.config.github_token(&config),
                Provider::Gitee => state.config.gitee_token(&config),
            }
        };
        match releases::fetch_release_url_with_token(
            &client,
            &endpoint,
            &provider,
            token.as_deref(),
        )
        .await
        {
            Ok(releases) => {
                let filtered = releases::filter_channel(releases, &Channel::Stable);
                let Some((release, version)) = filtered.into_iter().find_map(|release| {
                    let version = release.version.as_deref().and_then(releases::parse_version)?;
                    (version > current_version).then_some((release, version))
                }) else {
                    return Ok(None);
                };
                let Some((installer, installer_type)) = installer_asset_for_release(&release, &version) else {
                    return Err("检查更新失败，请重新启动MOD管理器。".into());
                };
                let installer_url = installer.download_url.clone();
                let installer_size = installer.size;
                let installer_type = installer_type.to_string();
                let mut manifest_endpoints = release
                    .assets
                    .iter()
                    .filter(|asset| {
                        let name = asset.name.to_ascii_lowercase();
                        name == "latest.json"
                            || (name.starts_with("latest-") && name.ends_with(".json"))
                    })
                    .map(|asset| asset.download_url.clone())
                    .collect::<Vec<_>>();
                if manifest_endpoints.is_empty() {
                    manifest_endpoints.push(installer_manifest_fallback(&provider));
                }
                return Ok(Some(AppUpdateInfo {
                    version: version.to_string(),
                    current_version: current_version.to_string(),
                    body: (!release.body.is_empty()).then_some(release.body),
                    date: release.published_at,
                    download_size: Some(installer_size),
                    manifest_endpoints,
                    installer_url: Some(installer_url),
                    installer_type,
                }));
            }
            Err(_) => {}
        }
    }
    Err("检查更新失败，请重新启动MOD管理器。".into())
}

pub struct AppState {
    pub config: ConfigService,
    pub operation: Mutex<()>,
    pub repository_cache: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            config: ConfigService::new().map_err(|e| e.to_string())?,
            operation: Mutex::new(()),
            repository_cache: tokio::sync::Mutex::new(()),
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
pub fn read_rust_log() -> Result<RustLog, String> {
    let path = std::env::current_exe()
        .map_err(|error| error.to_string())?
        .parent()
        .ok_or("无法确定程序目录")?
        .join("logs")
        .join("rust.log");
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
    let _guard = state.operation.lock().map_err(|_| "操作锁异常".to_string())?;
    let config = state.config.load_or_default();
    let game_root = PathBuf::from(game::detect_game(&config).path.ok_or("尚未找到厨房目录")?);
    game::ensure_game_stopped(&game_root)?;
    let source = PathBuf::from(source_path);
    if !source.is_file() {
        return Err("所选文件不存在".into());
    }
    let file_name = source.file_name().and_then(|value| value.to_str()).ok_or("文件名无效")?;
    let extension = source.extension().and_then(|value| value.to_str()).unwrap_or_default();
    let repository_id = format!("manual-{}", Uuid::new_v4());
    let (files, display_name) = if extension.eq_ignore_ascii_case("dll") {
        let files = archive::copy_asset(&source, &game_root, "BepInEx/plugins", file_name)?;
        (files, file_name.to_string())
    } else if extension.eq_ignore_ascii_case("zip") {
        let folder = source.file_stem().and_then(|value| value.to_str()).ok_or("ZIP 文件名无效")?;
        if folder.is_empty() || folder.contains('/') || folder.contains('\\') || folder.contains(':') {
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
    if let Err(error) = archive::write_manifest(
        &state.config.manifests_dir(),
        &manifest,
    ) {
        for relative in files {
            if let Ok(path) = game::safe_game_path(&game_root, &relative) {
                let _ = archive::clear_readonly(&path);
                let _ = fs::remove_file(path);
            }
        }
        return Err(error);
    }
    Ok(OperationResult { ok: true, message: format!("{display_name} 安装完成") })
}

#[tauri::command]
pub fn uninstall_local_mod(
    state: State<'_, AppState>,
    dll_file: String,
) -> Result<OperationResult, String> {
    let _guard = state.operation.lock().map_err(|_| "操作锁异常".to_string())?;
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
    let safe_relative = relative.to_string_lossy().replace('\\', "/");
    let manifest = manifests.into_iter().find(|manifest| {
        manifest.files.iter().any(|file| file.eq_ignore_ascii_case(&safe_relative))
    }).ok_or("该 MOD 没有安装清单，拒绝删除未知文件")?;
    archive::remove_manifest_files(&game_root, &manifest)?;
    fs::remove_file(manifest_dir.join(format!("{}.toml", manifest.repository_id)))
        .map_err(|error| error.to_string())?;
    Ok(OperationResult { ok: true, message: "MOD 已删除".into() })
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<PublicConfig, String> {
    let mut config = state.config.load_or_default();
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
    let info = game::detect_game(&config);
    if let Some(path) = &info.path {
        if config.game.path != *path {
            let mut next = config;
            next.game.path = path.clone();
            let _ = state.config.save(&next);
        }
    }
    Ok(info)
}

#[tauri::command]
pub fn set_game_path(state: State<'_, AppState>, path: String) -> Result<GameInfo, String> {
    let root = PathBuf::from(&path);
    if !root.join("Overcooked2.exe").is_file() {
        return Err("所选目录中没有 Overcooked2.exe".into());
    }
    let mut config = state.config.load_or_default();
    config.game.path = root.to_string_lossy().to_string();
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

#[tauri::command]
pub async fn install_bepinex(
    app: AppHandle,
    state: State<'_, AppState>,
    reinstall: bool,
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
    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let temp = archive::download_to_temp(
        &client,
        BEPINEX_URL,
        &state.config.cache_dir(),
        "BepInEx",
        Some(&app),
    )
    .await?;
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
    let client =
        releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let proxy_url = if repo.release_proxy_url.trim().is_empty() {
        asset.download_url.clone()
    } else {
        releases::rewrite_proxy(&repo.release_proxy_url, &asset.download_url)
    };
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
        ProxyMode::Direct => archive::download_to_temp_with_threads(&client, &asset.download_url, &state.config.cache_dir(), &repo.name, Some(&app), download_threads).await?,
        ProxyMode::Fixed => archive::download_to_temp_with_threads(&client, &proxy_url, &state.config.cache_dir(), &repo.name, Some(&app), download_threads).await?,
        ProxyMode::Auto => match archive::download_to_temp_with_threads(&client, &asset.download_url, &state.config.cache_dir(), &repo.name, Some(&app), download_threads).await {
            Ok(path) => path,
            Err(_) if proxy_url != asset.download_url => archive::download_to_temp_with_threads(&client, &proxy_url, &state.config.cache_dir(), &repo.name, Some(&app), download_threads).await?,
            Err(error) => return Err(error),
        },
    };
    if game::is_game_running(&game_root) {
        let _ = fs::remove_file(&temp);
        return Err("下载期间厨房重新启动，安装已取消，旧版本未删除".into());
    }
    archive::verify_digest(&temp, asset.digest.as_deref())?;
    let old_manifest = archive::read_manifest(&state.config.manifests_dir(), &repo.id);
    let old_backup = old_manifest
        .as_ref()
        .map(|manifest| backup_manifest_files(&game_root, manifest, &state.config.backups_dir()))
        .transpose()?;
    if let Some(manifest) = &old_manifest {
        archive::remove_manifest_files(&game_root, manifest)?;
    }
    let files_result = if temp.extension().and_then(|value| value.to_str()) == Some("zip")
        || asset.name.to_ascii_lowercase().ends_with(".zip")
    {
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
    let download_threads = match &patch.provider {
        Provider::Github => if patch.download_threads == 0 { 16 } else { patch.download_threads.clamp(1, 32) },
        Provider::Gitee => 1,
    };
    let api_proxy_url = patch.api_proxy_url.clone();
    let release_proxy_url = patch.release_proxy_url.clone();
    let mut item = RepositoryConfig {
        id: id.clone(),
        name: patch.name,
        provider: patch.provider.clone(),
        url: patch.url,
        channel: patch.channel,
        asset_rules: patch.asset_rules,
        install_target: patch.install_target,
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
        if patch.github_token.is_empty() { existing.github_token = old_github; }
        if patch.gitee_token.is_empty() { existing.gitee_token = old_gitee; }
        state.config.set_repository_tokens(existing, &patch.github_token, &patch.gitee_token);
    } else {
        let mut item = item;
        state.config.set_repository_tokens(&mut item, &patch.github_token, &patch.gitee_token);
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
                return Err("winhttp.dll 与禁用文件同时存在，请先切换为有 MOD 状态".into())
            }
            WinhttpState::Missing => return Err("未找到 winhttp.dll，无法切换 MOD 状态".into()),
        }
    }
    Ok(OperationResult {
        ok: true,
        message: if enabled {
            "已切换为有 MOD 状态，尚未启动厨房".into()
        } else {
            "已切换为无 MOD 状态，尚未启动厨房".into()
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
    for repo in &mut config.repositories {
        repo.cached_releases.clear();
        repo.etag = None;
        repo.last_error = None;
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    Ok(OperationResult { ok: true, message: "已清除缓存并重置界面".into() })
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
pub async fn check_app_update(state: State<'_, AppState>) -> Result<Option<AppUpdateInfo>, String> {
    discover_installer_update(&state).await
}

#[tauri::command]
pub async fn install_app_update(
    _app: AppHandle,
    state: State<'_, AppState>,
) -> Result<OperationResult, String> {
    let info = discover_installer_update(&state)
        .await?
        .ok_or("当前已经是最新版本")?;
    let url = info.installer_url.ok_or("更新中没有匹配的安装包")?;
    let config = state.config.load_or_default();
    let client = releases::build_client(config.network.timeout_seconds).map_err(|e| e.to_string())?;
    let bytes = client.get(&url).header("User-Agent", releases::APP_USER_AGENT).send().await
        .map_err(|e| e.to_string())?.error_for_status().map_err(|e| e.to_string())?.bytes().await.map_err(|e| e.to_string())?;
    let installer_type = if info.installer_type.eq_ignore_ascii_case("exe") { "exe" } else { "msi" };
    let path = std::env::temp_dir().join(format!("OC2-Mod-Manager-{}.{}", info.version, installer_type));
    fs::write(&path, &bytes).map_err(|e| e.to_string())?;
    if installer_type == "exe" {
        std::process::Command::new(&path)
            .arg("/S")
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
                .find(|item| item.repository_id.as_deref() == Some(repo.id.as_str()))
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
