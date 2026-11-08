use crate::app_state::AppState;
use crate::models::{
    ImportPreview, OperationResult, PublicConfig, RepositoryConfig, RepositoryDownloadMode,
    RepositoryPatch,
};
use crate::releases;
use std::fs;
use tauri::State;
use uuid::Uuid;

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
    let asset_rules = if patch.download_mode == RepositoryDownloadMode::RepositoryFile {
        Vec::new()
    } else {
        patch.asset_rules
    };
    let mut item = RepositoryConfig {
        network_plan: patch.network_plan.clone().unwrap_or_else(|| {
            config
                .repositories
                .iter()
                .find(|repo| repo.id == id)
                .map(|repo| repo.network_plan.clone())
                .unwrap_or_default()
        }),
        kind: crate::models::RepositoryKind::Mod,
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
        download_threads: patch.download_threads,
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
    crate::network_policy::NetworkPolicy::shared()?.validate_selection(
        &item.provider,
        item.kind,
        &item.network_plan,
    )?;
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
