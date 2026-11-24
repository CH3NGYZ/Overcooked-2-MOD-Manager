use crate::app_state::AppState;
use crate::game;
use crate::models::{GameRuntime, OperationResult, PublicConfig, SettingsPatch, TokenPatch};
use std::fs;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> Result<PublicConfig, String> {
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
    crate::manager_data::install_language()
}

#[tauri::command]
pub fn save_settings(
    state: State<'_, AppState>,
    patch: SettingsPatch,
) -> Result<PublicConfig, String> {
    let previous_game = if let Some(runtime) = patch.game_runtime {
        if cfg!(windows) && runtime != GameRuntime::Native {
            log::warn!("[config] 拒绝保存游戏运行方式：请求={runtime:?}，原因=game.windowsOnly");
            return Err("game.windowsOnly".into());
        }
        let previous = state.config.load_or_default();
        if previous.game.runtime != runtime {
            if let Some(path) = game::detect_game(&previous).path {
                game::ensure_game_stopped(Path::new(&path)).inspect_err(|_| {
                    log::warn!("[config] 切换游戏运行方式前未通过游戏退出检查：请求={runtime:?}，游戏路径={}", crate::config::diagnostic_game_path(&path));
                })?;
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
    crate::mod_changes::guard::ensure_recovered(&state)?;
    let mut config = state.config.load_or_default();
    if previous_game
        .as_ref()
        .is_some_and(|previous| *previous != config.game)
    {
        log::warn!("[config] 拒绝保存已过期的游戏设置：原因=game.settingsChanged");
        return Err("game.settingsChanged".into());
    }
    let game_changed = patch
        .game_runtime
        .is_some_and(|runtime| runtime != config.game.runtime)
        || patch
            .game_path
            .as_ref()
            .is_some_and(|path| *path != config.game.path);
    if game_changed {
        state.invalidate_local_mod_snapshot()?;
    }
    let previous_game_path =
        game_changed.then(|| crate::config::diagnostic_game_path(&config.game.path));
    let previous_game_runtime = config.game.runtime;
    let previous_official_api_only = config.auth.official_api_only;
    let previous_network = config.network.clone();
    if let Some(runtime) = patch.game_runtime {
        config.game.switch_runtime(runtime);
    }
    if let Some(value) = patch.ui_zoom {
        config.ui_zoom = value.clamp(1.0, 2.0);
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
    if let Some(path) = patch.game_path {
        config.game.path = path;
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
    if let Some(value) = patch.api_timeout_seconds {
        config.network.api_timeout_seconds = value;
    }
    if let Some(value) = patch.first_byte_timeout_seconds {
        config.network.first_byte_timeout_seconds = value;
    }
    config.network.normalize();
    /*
     * 元数据并行度：`0` 原样保存（= 全部核心，换机器后按新机器核心数跑），
     * 其余夹到本机核心数以内 —— 存一个超过本机的值只会让下次扫描白等。
     */
    if let Some(value) = patch.mod_metadata_concurrency {
        config.mod_metadata_concurrency = crate::concurrency::clamp_stored_concurrency(value);
    }
    if let Some(value) = patch.diychef_preview_concurrency {
        config.diychef_preview_concurrency = crate::concurrency::clamp_stored_concurrency(value);
    }
    if let Some(value) = patch.map_compression_concurrency {
        config.map_compression_concurrency = crate::concurrency::clamp_stored_concurrency(value);
    }
    if let Some(value) = patch.map_info_concurrency {
        config.map_info_concurrency = crate::concurrency::clamp_stored_concurrency(value);
    }
    /*
     * 教程进度：前端每走一步就把「已看过」的 id 整批发来，这里做并集。
     * 不校验 id 是否还存在 —— 步骤表在前端，旧配置里可能留着已经删掉的
     * 步骤 id；校验只会让升级后的第一次保存把历史悄悄抹掉。
     */
    if let Some(steps) = &patch.tutorial_watched_steps {
        config.record_tutorial_steps(steps);
    }
    state
        .config
        .save_with_selection(&mut config, patch.region.as_deref(), patch.data_provider)
        .map_err(|e| e.to_string())?;
    crate::diychef::preview::configure_concurrency(config.diychef_preview_concurrency);
    if let Some(previous_path) = previous_game_path {
        log::info!(
            "[config] 游戏设置已保存：运行方式={previous_game_runtime:?}→{:?}，游戏路径={previous_path}→{}",
            config.game.runtime,
            crate::config::diagnostic_game_path(&config.game.path)
        );
    }
    if previous_official_api_only != config.auth.official_api_only
        || previous_network != config.network
    {
        log::info!(
            "[config] 网络设置已保存：仅官方API={}，下载并行={}，API超时秒={}，首字节超时秒={}",
            config.auth.official_api_only,
            config.network.download_threads,
            config.network.api_timeout_seconds,
            config.network.first_byte_timeout_seconds,
        );
    }
    if patch.ui_zoom.is_some() {
        log::info!("[config] 界面缩放设置已保存：缩放={}", config.ui_zoom);
    }
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub async fn refresh_github_proxies(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<PublicConfig, String> {
    let config = state.config.clone();
    config.set_source_browser(app);
    config.refresh_proxies().await?;
    Ok(config.public_config(&config.load_or_default()))
}

#[tauri::command]
pub fn save_tokens(state: State<'_, AppState>, patch: TokenPatch) -> Result<PublicConfig, String> {
    log::info!(
        "[config] 保存 API Token 设置：提供 GitHub/Gitee/GitCode={}/{}/{}，请求清除={}",
        !patch.github_token.trim().is_empty(),
        !patch.gitee_token.trim().is_empty(),
        !patch.gitcode_token.trim().is_empty(),
        patch.clear
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
        config.auth.gitcode_token.clear();
    } else {
        state
            .config
            .set_tokens(
                &mut config,
                &patch.github_token,
                &patch.gitee_token,
                &patch.gitcode_token,
            )
            .map_err(|_| {
                log::warn!("[config] 未保存 API Token 设置：原因=config.tokenProtectionFailed");
                "config.tokenProtectionFailed"
            })?;
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!(
        "[config] API Token 设置已保存：已配置 GitHub/Gitee/GitCode={}/{}/{}",
        !config.auth.github_token.is_empty(),
        !config.auth.gitee_token.is_empty(),
        !config.auth.gitcode_token.is_empty()
    );
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub fn reset_ui(state: State<'_, AppState>) -> Result<OperationResult, String> {
    reset_ui_in(&state)
}

fn reset_ui_in(state: &AppState) -> Result<OperationResult, String> {
    log::info!("重置界面缓存和 Release 缓存");
    let _guard = state
        .operation
        .lock()
        .map_err(|_| "lock.poisoned".to_string())?;
    crate::mod_changes::guard::ensure_recovered(state)?;
    clear_resettable_cache(&state.config.cache_dir())?;
    let mut config = state.config.load_or_default();
    config.ui_zoom = crate::config::default_ui_zoom();
    for repo in &mut config.repositories {
        repo.cached_releases.clear();
        repo.cached_releases_complete = false;
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

fn clear_resettable_cache(cache: &Path) -> Result<(), String> {
    fs::create_dir_all(cache).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(cache).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        // Converters create their backups before taking operation. Preserve the
        // complete reserved tree, including in-flight work and pending recovery.
        if entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.eq_ignore_ascii_case("level-compression"))
        {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                continue;
            }
        }
        if metadata.is_dir() {
            fs::remove_dir_all(entry.path()).map_err(|error| error.to_string())?;
        } else if metadata.is_file() {
            fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "settings/cache_reset_tests.rs"]
mod cache_reset_tests;
