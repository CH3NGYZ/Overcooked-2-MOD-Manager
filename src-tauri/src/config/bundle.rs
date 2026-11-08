//! 仓库定义导入、预览、导出与稳定身份/缓存合并。

use super::ConfigError;
use super::ConfigService;
use super::availability::normalize_repository_region;
use super::identity::export_key;
use super::identity::mod_repository_from_export;
use super::identity::repo_key;
use super::identity::repository_address_key;
use super::metadata::migrate_repository_export_i18n;
use super::metadata::normalize_repository_i18n;
use super::metadata::repository_name_from_i18n;
use super::validation::validate_mod_repository;
use crate::models::*;
use chrono::Utc;
use uuid::Uuid;

pub(super) const BUNDLE_FORMAT: &str = "oc2-mod-repository-bundle";

fn reorder_imported_repositories(config: &mut AppConfig, imported_ids: &[String]) {
    if imported_ids.len() < 2 {
        return;
    }
    let imported_set = imported_ids
        .iter()
        .collect::<std::collections::HashSet<_>>();
    let Some(first_index) = config
        .repositories
        .iter()
        .position(|repository| imported_set.contains(&repository.id))
    else {
        return;
    };
    let insertion_index = config.repositories[..first_index]
        .iter()
        .filter(|repository| !imported_set.contains(&repository.id))
        .count();
    let mut imported = Vec::with_capacity(imported_ids.len());
    let mut remaining = Vec::with_capacity(config.repositories.len());
    let mut by_id = std::collections::HashMap::new();
    for repository in config.repositories.drain(..) {
        if imported_set.contains(&repository.id) {
            by_id.insert(repository.id.clone(), repository);
        } else {
            remaining.push(repository);
        }
    }
    for id in imported_ids {
        if let Some(repository) = by_id.remove(id) {
            imported.push(repository);
        }
    }
    let insertion_index = insertion_index.min(remaining.len());
    remaining.splice(insertion_index..insertion_index, imported);
    config.repositories = remaining;
}

fn append_toml_field<T: serde::Serialize + ?Sized>(
    text: &mut String,
    key: &str,
    value: &T,
) -> Result<(), ConfigError> {
    let mut encoded = String::new();
    value
        .serialize(toml::ser::ValueSerializer::new(&mut encoded))
        .map_err(|error| ConfigError::Write(error.to_string()))?;
    text.push_str(&format!("{key} = {encoded}\n"));
    Ok(())
}

fn append_toml_translations(
    text: &mut String,
    key: &str,
    translations: &std::collections::BTreeMap<String, String>,
) -> Result<(), ConfigError> {
    for (locale, value) in translations {
        let quoted_locale = toml::Value::String(locale.clone()).to_string();
        append_toml_field(text, &format!("{key}.{quoted_locale}"), value)?;
    }
    Ok(())
}

fn serialize_repository_bundle(bundle: &ModRepositoryBundle) -> Result<String, ConfigError> {
    let mut text = String::new();
    append_toml_field(&mut text, "format", &bundle.format)?;
    append_toml_field(&mut text, "schema_version", &bundle.schema_version)?;
    append_toml_field(&mut text, "exported_at", &bundle.exported_at)?;
    if bundle.repositories.is_empty() {
        text.push_str("repositories = []\n");
    }
    for repo in &bundle.repositories {
        text.push_str("\n[[repositories]]\n# 仓库基本信息\n");
        append_toml_field(&mut text, "name", &repo.name)?;
        append_toml_translations(&mut text, "name_i18n", &repo.name_i18n)?;
        append_toml_field(&mut text, "provider", &repo.provider)?;
        append_toml_field(&mut text, "type", &repo.kind)?;
        append_toml_field(&mut text, "network_plan", &repo.network_plan)?;
        append_toml_field(&mut text, "group", &repo.group)?;
        append_toml_translations(&mut text, "group_i18n", &repo.group_i18n)?;
        if let Some(region) = &repo.region {
            append_toml_field(&mut text, "region", region)?;
        }
        append_toml_field(&mut text, "username", &repo.username)?;
        append_toml_field(&mut text, "repository", &repo.repository)?;
        append_toml_field(&mut text, "enabled", &repo.enabled)?;
        if let Some(source_id) = &repo.source_id {
            append_toml_field(&mut text, "source_id", source_id)?;
        }

        text.push_str("\n# 下载方式\n");
        append_toml_field(&mut text, "download_mode", &repo.download_mode)?;
        match repo.download_mode {
            ModDownloadMode::Release => {
                text.push_str("\n# Release 参数\n");
                append_toml_field(&mut text, "channel", &repo.channel)?;
                append_toml_field(&mut text, "max_release_count", &repo.max_release_count)?;
                text.push_str("\n# 版本资产规则\n");
                append_toml_field(&mut text, "asset_rules", &repo.asset_rules)?;
            }
            ModDownloadMode::RepositoryFile => {
                text.push_str("\n# 仓库文件参数\n");
                append_toml_field(&mut text, "branch", &repo.branch)?;
                text.push_str("\n# 仓库资产规则（仅支持具体文件）\n");
                append_toml_field(&mut text, "file_path", &repo.file_path)?;
            }
        }
        text.push_str("\n# 本地 MOD 识别绑定\n");
        append_toml_field(&mut text, "plugin_guid", &repo.plugin_guid)?;
        append_toml_field(&mut text, "local_product_name", &repo.local_product_name)?;
        append_toml_field(
            &mut text,
            "local_original_filename",
            &repo.local_original_filename,
        )?;
        append_toml_field(&mut text, "version_regex", &repo.version_regex)?;
        text.push_str("\n# 安装目录\n");
        append_toml_field(&mut text, "install_target", &repo.install_target)?;
        text.push_str("\n# 仓库备注\n");
        append_toml_field(&mut text, "note", &repo.note)?;
        append_toml_translations(&mut text, "note_i18n", &repo.note_i18n)?;
    }
    if !bundle.repository_sources.is_empty() {
        #[derive(serde::Serialize)]
        struct Sources<'a> {
            repository_sources: &'a [RepositorySource],
        }
        text.push('\n');
        text.push_str(
            &toml::to_string_pretty(&Sources {
                repository_sources: &bundle.repository_sources,
            })
            .map_err(|error| ConfigError::Write(error.to_string()))?,
        );
    }
    Ok(text)
}

impl ConfigService {
    pub fn export_bundle(
        &self,
        config: &AppConfig,
        ids: Option<Vec<String>>,
    ) -> Result<String, ConfigError> {
        log::info!(
            "开始导出仓库配置：指定数量={}",
            ids.as_ref().map_or(0, Vec::len)
        );
        let exported_source_ids = config
            .repository_sources
            .iter()
            .map(|source| source.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        let mut selected = config
            .repositories
            .iter()
            .filter(|repo| {
                ids.as_ref()
                    .map(|list| list.contains(&repo.id))
                    .unwrap_or(true)
            })
            .map(|repo| ModRepositoryExport {
                network_plan: repo.network_plan.clone(),
                kind: repo.kind,
                name: repo.name.clone(),
                name_i18n: repo.name_i18n.clone(),
                group: repo.group.clone(),
                group_i18n: repo.group_i18n.clone(),
                provider: repo.provider.clone(),
                username: repo.username.clone(),
                repository: repo.repository.clone(),
                download_mode: repo.download_mode.clone(),
                branch: repo.branch.clone(),
                file_path: repo.file_path.clone(),
                region: repo.region.clone(),
                platform: repo.platform.clone(),
                channel: repo.channel.clone(),
                asset_rules: repo.asset_rules.clone(),
                install_target: repo.install_target.clone(),
                note: repo.note.clone(),
                note_i18n: repo.note_i18n.clone(),
                plugin_guid: repo.plugin_guid.clone(),
                local_product_name: repo.local_product_name.clone(),
                local_original_filename: repo.local_original_filename.clone(),
                relations: repo.relations.clone(),
                release_relations: repo.release_relations.clone(),
                version_regex: repo.version_regex.clone(),
                max_release_count: repo.max_release_count,
                download_threads: repo.download_threads,
                enabled: repo.enabled,
                source_id: repo.source_id.clone(),
            })
            .collect::<Vec<_>>();
        for repo in &mut selected {
            migrate_repository_export_i18n(repo);
        }
        let mut repository_sources = config.repository_sources.clone();
        for source in &mut repository_sources {
            if source
                .parent_id
                .as_deref()
                .is_some_and(|parent_id| !exported_source_ids.contains(parent_id))
            {
                source.parent_id = None;
            }
        }
        let bundle = ModRepositoryBundle {
            format: BUNDLE_FORMAT.to_string(),
            schema_version: 1,
            exported_at: Utc::now().to_rfc3339(),
            repositories: selected,
            repository_sources,
        };
        let result = serialize_repository_bundle(&bundle);
        match &result {
            Ok(text) => log::info!(
                "仓库配置导出完成：仓库={}，仓库源={}，字节数={}",
                bundle.repositories.len(),
                bundle.repository_sources.len(),
                text.len()
            ),
            Err(error) => log::error!("仓库配置导出失败：原因={}", error),
        }
        result
    }

    pub fn validate_bundle(&self, text: &str) -> Result<ModRepositoryBundle, ConfigError> {
        log::debug!("开始校验仓库配置包：字节数={}", text.len());
        if text.len() > 1024 * 1024 {
            return Err(ConfigError::Invalid("source.tooLarge".into()));
        }
        let mut bundle: ModRepositoryBundle = toml::from_str(text).map_err(|e| {
            ConfigError::Invalid(crate::locale_tag::with_params(
                "config.tomlParseFailed",
                &[("reason", &e.to_string())],
            ))
        })?;
        if bundle.format != BUNDLE_FORMAT {
            return Err(ConfigError::Invalid("config.bundleFormatInvalid".into()));
        }
        if bundle.schema_version != 1 {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "config.unsupportedSchemaVersion",
                &[("version", &bundle.schema_version.to_string())],
            )));
        }
        for repo in &mut bundle.repositories {
            normalize_repository_i18n(repo)?;
            validate_mod_repository(repo)?;
        }
        log::info!(
            "仓库配置包校验完成：仓库={}，仓库源={}",
            bundle.repositories.len(),
            bundle.repository_sources.len()
        );
        Ok(bundle)
    }

    pub fn preview_bundle(
        &self,
        config: &AppConfig,
        bundle: &ModRepositoryBundle,
    ) -> ImportPreview {
        let mut preview = ImportPreview {
            valid: true,
            errors: Vec::new(),
            additions: Vec::new(),
            duplicates: Vec::new(),
            conflicts: Vec::new(),
        };
        for item in &bundle.repositories {
            let item_name = repository_name_from_i18n(&item.name_i18n);
            if config.repositories.iter().any(|repo| {
                !crate::community::is_repository(repo)
                    // The channel separates Stable and Beta entries of the same
                    // repository, so both must be imported side by side.
                    && repo.channel == item.channel
                    && repository_address_key(
                        &repo.provider,
                        &repo.username,
                        &repo.repository,
                        repo.region.as_deref(),
                    ) == repository_address_key(
                        &item.provider,
                        &item.username,
                        &item.repository,
                        item.region.as_deref(),
                    )
            }) {
                preview.duplicates.push(item_name.to_string());
                continue;
            }
            let key = export_key(item);
            let matching = config
                .repositories
                .iter()
                .find(|repo| !crate::community::is_repository(repo) && repo_key(repo) == key);
            if matching.is_some_and(|repo| repo.builtin) {
                preview.duplicates.push(item_name.to_string());
                continue;
            }
            let same_name = config.repositories.iter().any(|repo| {
                !crate::community::is_repository(repo)
                    && repo.channel == item.channel
                    && repo
                        .name_i18n
                        .get("zh-CN")
                        .is_some_and(|name| name.trim() == item_name)
                    && normalize_repository_region(repo.region.as_deref())
                        == normalize_repository_region(item.region.as_deref())
            });
            if matching.is_some() {
                preview.duplicates.push(item_name.to_string());
            } else if same_name {
                preview.conflicts.push(item_name.to_string());
            } else {
                preview.additions.push(item_name.to_string());
            }
        }
        preview
    }

    pub fn import_bundle(
        &self,
        config: &mut AppConfig,
        bundle: &ModRepositoryBundle,
        conflict: &str,
    ) -> ImportPreview {
        log::info!(
            "开始导入仓库配置：仓库={}，仓库源={}，冲突策略={}",
            bundle.repositories.len(),
            bundle.repository_sources.len(),
            conflict
        );
        for source in &bundle.repository_sources {
            if crate::community::is_source(&source.id)
                || source.builtin
                || config.repository_sources.iter().any(|existing| {
                    crate::repository_sources::source_address_key(
                        &existing.url,
                        existing.region.as_deref(),
                    ) == crate::repository_sources::source_address_key(
                        &source.url,
                        source.region.as_deref(),
                    )
                })
            {
                continue;
            }
            let imported = source.clone();
            config.repository_sources.push(imported);
        }
        let mut preview = self.preview_bundle(config, bundle);
        // Keep the order from the imported TOML. A matching repository used
        // to be updated in place while new repositories were appended, so a
        // remote source could be displayed in a different order after a
        // refresh. Track every repository accepted from this bundle and put
        // them back as one ordered block after the merge.
        let mut imported_ids = Vec::with_capacity(bundle.repositories.len());
        for item in &bundle.repositories {
            let mut item = item.clone();
            migrate_repository_export_i18n(&mut item);
            let item_name = repository_name_from_i18n(&item.name_i18n).to_string();
            let key = export_key(&item);
            if let Some(existing) = config
                .repositories
                .iter_mut()
                .find(|repo| !crate::community::is_repository(repo) && repo_key(repo) == key)
            {
                if existing.builtin && conflict != "overwrite" {
                    continue;
                }
                if conflict == "overwrite" {
                    let id = existing.id.clone();
                    let builtin = existing.builtin;
                    /*
                     * 覆盖的是**仓库定义**（名字、坐标、资产规则、安装目录），
                     * 不是 Release 缓存。
                     *
                     * `mod_repository_from_export` 造出来的是全新对象，缓存字段
                     * 一律为空。整批刷新时每个源文档都要走这里，如果连缓存一起
                     * 覆盖，5 分钟缓存就永远命中不了 —— 每次刷新（含每次启动）
                     * 都得把整棵仓库树的 API 重打一遍，实测 20+ 个仓库、几十个
                     * 请求，还会固定撞上 Gitee/GitHub 的速率限制。
                     *
                     * 缓存是否过期由各仓库自己的 `releases_cached_at` 决定，
                     * 与「源文档重新声明了这个仓库」无关。
                     */
                    let cached_releases = std::mem::take(&mut existing.cached_releases);
                    let releases_cached_at = existing.releases_cached_at;
                    let last_error = existing.last_error.clone();
                    *existing = mod_repository_from_export(&item, id, builtin);
                    existing.cached_releases = cached_releases;
                    existing.releases_cached_at = releases_cached_at;
                    existing.last_error = last_error;
                    imported_ids.push(existing.id.clone());
                }
                continue;
            }
            let same_region = |repo: &ModRepositoryConfig| {
                normalize_repository_region(repo.region.as_deref())
                    == normalize_repository_region(item.region.as_deref())
            };
            let same_name = |repo: &ModRepositoryConfig, name: &str| {
                repo.channel == item.channel
                    && repo
                        .name_i18n
                        .get("zh-CN")
                        .is_some_and(|existing_name| existing_name.trim() == name)
            };
            if config.repositories.iter().any(|repo| {
                !crate::community::is_repository(repo)
                    && same_name(repo, &item_name)
                    && same_region(repo)
            }) {
                match conflict {
                    "skip" => continue,
                    "keep_both" => {
                        let imported_name = crate::locale_tag::with_params(
                            "config.importedNameSuffix",
                            &[("name", &item_name.to_string())],
                        );
                        item.name_i18n.insert("zh-CN".into(), imported_name.clone());
                        item.name = imported_name;
                    }
                    "overwrite" => {
                        config.repositories.retain(|repo| {
                            crate::community::is_repository(repo)
                                || !same_name(repo, &item_name)
                                || !same_region(repo)
                        });
                    }
                    _ => continue,
                }
            }
            let export = item;
            let id = Uuid::new_v4().to_string();
            config
                .repositories
                .push(mod_repository_from_export(&export, id.clone(), false));
            imported_ids.push(id);
        }
        reorder_imported_repositories(config, &imported_ids);
        preview.valid = true;
        log::info!(
            "仓库配置导入完成：新增={}，重复={}，冲突={}",
            preview.additions.len(),
            preview.duplicates.len(),
            preview.conflicts.len()
        );
        preview
    }
}
