use crate::app_state::AppState;
use crate::models::{PublicConfig, RepositorySource, RepositorySourceImportResult};
use crate::repository_sources;
use crate::repository_sources::SourceLocation;
use crate::repository_sources::apply::import_repository_source_into_config;
use crate::repository_sources::source_address_key;
use tauri::State;
use url::Url;
use uuid::Uuid;

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
            network_plan: Default::default(),
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
            network_plan: Default::default(),
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
