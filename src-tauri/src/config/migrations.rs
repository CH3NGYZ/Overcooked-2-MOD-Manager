//! 配置兼容字段检测、schema迁移与源昵称归一化。

use super::ConfigService;
use super::availability::normalize_repository_region_for_storage;
use super::defaults::CONFIG_SCHEMA_VERSION;
use crate::models::*;

pub(super) fn contains_legacy_repository_proxy_fields(text: &str) -> bool {
    const LEGACY_FIELDS: &[&str] = &[
        "api_proxy_mode",
        "api_proxy_url",
        "release_proxy_mode",
        "release_proxy_url",
        "file_proxy_mode",
        "file_proxy_url",
    ];
    let Ok(value) = toml::from_str::<toml::Value>(text) else {
        return false;
    };
    value.get("github_proxies").is_some()
        || value
            .get("repositories")
            .and_then(toml::Value::as_array)
            .is_some_and(|repositories| {
                repositories.iter().any(|repository| {
                    repository.as_table().is_some_and(|table| {
                        LEGACY_FIELDS.iter().any(|field| table.contains_key(*field))
                    })
                })
            })
}

/// Schema 3 removes the four independent source switches. Unknown legacy fields
/// are ignored on read and dropped when the migrated configuration is saved.
pub(super) fn migrate_schema(config: &mut AppConfig) -> bool {
    if config.schema_version >= CONFIG_SCHEMA_VERSION {
        return false;
    }
    config.schema_version = CONFIG_SCHEMA_VERSION;
    true
}

impl ConfigService {
    pub(super) fn normalize_configured_regions(config: &mut AppConfig) -> bool {
        let mut changed = false;
        for repository in &mut config.repositories {
            let normalized = normalize_repository_region_for_storage(
                repository.region.as_deref(),
                &crate::locale_tag::with_params(
                    "config.repositoryLabeled",
                    &[("name", &repository.id.to_string())],
                ),
            );
            if repository.region != normalized {
                repository.region = normalized;
                changed = true;
            }
        }
        for source in &mut config.repository_sources {
            let normalized = normalize_repository_region_for_storage(
                source.region.as_deref(),
                &crate::locale_tag::with_params(
                    "source.labeled",
                    &[("label", &source.id.to_string())],
                ),
            );
            if source.region != normalized {
                source.region = normalized;
                changed = true;
            }
            // 昵称以多语言表为准：镜像字段跟着 zh-CN 走，缺表项时用镜像回填。
            // 这样旧配置文件（只有单语 nickname）在第一次读取后自动补齐。
            let previous_nickname = source.nickname.clone();
            let previous_map = source.nickname_i18n.clone();
            if !source.nickname_i18n.contains_key("zh-CN") && !source.nickname.trim().is_empty() {
                source
                    .nickname_i18n
                    .insert("zh-CN".into(), source.nickname.trim().to_string());
            }
            if let Some(nickname) =
                crate::repository_sources::nickname_fallback(&source.nickname_i18n)
            {
                source.nickname = nickname;
            }
            if source.nickname != previous_nickname || source.nickname_i18n != previous_map {
                changed = true;
            }
        }
        changed
    }
}
