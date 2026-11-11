use crate::app_state::AppState;
use crate::game;
use crate::models::{GameRuntime, OperationResult, PublicConfig, SettingsPatch, TokenPatch};
use std::fs;
use std::path::Path;
use tauri::State;

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
    if patch
        .game_runtime
        .is_some_and(|runtime| runtime != config.game.runtime)
        || patch
            .game_path
            .as_ref()
            .is_some_and(|path| *path != config.game.path)
    {
        state.invalidate_local_mod_snapshot()?;
    }
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
    if let Some(provider) = patch.data_provider {
        config.data_provider = provider;
        crate::config::synchronize_builtin_repository_sources(&mut config);
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    if let Some(provider) = patch.data_provider {
        // The selection belongs to this session, so stale saves or external edits
        // cannot undo it. Commit it only after the settings write succeeds.
        state.config.select_data_provider(provider);
    }
    crate::diychef::preview::configure_concurrency(config.diychef_preview_concurrency);
    log::info!("应用设置保存完成：界面缩放={}", config.ui_zoom);
    Ok(state.config.public_config(&config))
}

#[tauri::command]
pub async fn refresh_github_proxies(state: State<'_, AppState>) -> Result<PublicConfig, String> {
    let config = state.config.clone();
    config.refresh_proxies().await?;
    Ok(config.public_config(&config.load_or_default()))
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
        config.auth.gitcode_token.clear();
        log::info!("已清除全部 API Token");
    } else {
        state
            .config
            .set_tokens(
                &mut config,
                &patch.github_token,
                &patch.gitee_token,
                &patch.gitcode_token,
            )
            .map_err(|_| "config.tokenProtectionFailed")?;
    }
    state.config.save(&config).map_err(|e| e.to_string())?;
    log::info!("API Token 配置保存完成");
    Ok(state.config.public_config(&config))
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
    config.ui_zoom = crate::config::default_ui_zoom();
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
