//! 仓库/地图显示名回退与配置元数据迁移。

use super::ConfigError;
use crate::models::*;

/// 按界面语言从 `name_i18n` 取仓库名，缺失时回退到中文原名。
///
/// 语言标记来自前端（`zh-CN` / `zh-TW` / `en` / `ko` / `ja`），因为界面
/// 语言由前端 localStorage 管理、后端配置里不存这一项。
///
/// 查找顺序：完整标记 → 主语言前缀 → `name`。
/// 加前缀回退是因为源文档里可能只写了 `zh` 或 `en-US` 这类宽标记，
/// 而界面传来的可能是 `zh-CN`，两者应当能对上。
pub(crate) fn localized_mod_repository_name(
    repo: &ModRepositoryConfig,
    locale: Option<&str>,
) -> String {
    localized_name(&repo.name_i18n, &repo.name, locale)
}

/// 地图仓库的本地化名称（`MapRepositoryConfig` 的对应入口）。
pub(crate) fn localized_map_name(
    map: &crate::models::MapRepositoryConfig,
    locale: Option<&str>,
) -> String {
    localized_name(&map.name_i18n, &map.name, locale)
}

/// 按界面语言从多语言表取名字，回退到单语言镜像。
///
/// 仓库与地图共用同一套规则，差别只在字段来源，所以只保留这一份实现 ——
/// 两份拷贝迟早在前缀回退或空值处理上分叉。
fn localized_name(
    table: &std::collections::BTreeMap<String, String>,
    mirror: &str,
    locale: Option<&str>,
) -> String {
    let Some(locale) = locale.map(str::trim).filter(|value| !value.is_empty()) else {
        return mirror.to_string();
    };
    if let Some(value) = table.get(locale).filter(|value| !value.trim().is_empty()) {
        return value.clone();
    }
    // 主语言前缀：zh-CN → zh
    let primary = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();
    table
        .iter()
        .find(|(key, value)| {
            !value.trim().is_empty()
                && key.to_ascii_lowercase().split(['-', '_']).next() == Some(primary.as_str())
        })
        .map(|(_, value)| value.clone())
        .unwrap_or_else(|| mirror.to_string())
}

pub(super) fn migrate_repository_i18n(repository: &mut ModRepositoryConfig) -> bool {
    let mut changed = false;
    if repository
        .name_i18n
        .get("zh-CN")
        .is_none_or(|name| name.trim().is_empty())
    {
        let legacy_name = repository.name.trim();
        if !legacy_name.is_empty() {
            repository
                .name_i18n
                .insert("zh-CN".into(), legacy_name.into());
            changed = true;
        }
    }
    if !repository.group_i18n.contains_key("zh-CN") {
        repository
            .group_i18n
            .insert("zh-CN".into(), repository.group.clone());
        changed = true;
    }
    if !repository.note_i18n.contains_key("zh-CN") {
        repository
            .note_i18n
            .insert("zh-CN".into(), repository.note.clone());
        changed = true;
    }
    if let Some(name) = repository.name_i18n.get("zh-CN") {
        let name = name.trim().to_string();
        if repository.name != name {
            repository.name = name;
            changed = true;
        }
    }
    if let Some(group) = repository.group_i18n.get("zh-CN") {
        if repository.group != *group {
            repository.group = group.clone();
            changed = true;
        }
    }
    if let Some(note) = repository.note_i18n.get("zh-CN") {
        if repository.note != *note {
            repository.note = note.clone();
            changed = true;
        }
    }
    changed
}

/// Static source and bundle entries must declare Chinese values explicitly.
/// Raw fields remain compatibility mirrors and never determine display values.
pub(crate) fn normalize_repository_i18n(repo: &mut ModRepositoryExport) -> Result<(), ConfigError> {
    let name = repo
        .name_i18n
        .get("zh-CN")
        .ok_or_else(|| ConfigError::Invalid("config.missingNameI18n".into()))?;
    if name.trim().is_empty() {
        return Err(ConfigError::Invalid("config.emptyNameI18n".into()));
    }
    if !repo.group_i18n.contains_key("zh-CN") {
        return Err(ConfigError::Invalid("config.missingGroupI18n".into()));
    }
    if !repo.note_i18n.contains_key("zh-CN") {
        return Err(ConfigError::Invalid("config.missingNoteI18n".into()));
    }
    repo.name = name.trim().to_string();
    repo.group = repo.group_i18n["zh-CN"].clone();
    repo.note = repo.note_i18n["zh-CN"].clone();
    Ok(())
}

pub(super) fn repository_name_from_i18n(
    translations: &std::collections::BTreeMap<String, String>,
) -> &str {
    translations
        .get("zh-CN")
        .map(|name| name.trim())
        .unwrap_or_default()
}

pub(super) fn migrate_repository_export_i18n(repo: &mut ModRepositoryExport) {
    repo.name_i18n
        .entry("zh-CN".into())
        .or_insert_with(|| repo.name.trim().to_string());
    repo.group_i18n
        .entry("zh-CN".into())
        .or_insert_with(|| repo.group.clone());
    repo.note_i18n
        .entry("zh-CN".into())
        .or_insert_with(|| repo.note.clone());
    if let Some(name) = repo.name_i18n.get("zh-CN") {
        repo.name = name.trim().to_string();
    }
    if let Some(group) = repo.group_i18n.get("zh-CN") {
        repo.group = group.clone();
    }
    if let Some(note) = repo.note_i18n.get("zh-CN") {
        repo.note = note.clone();
    }
}
