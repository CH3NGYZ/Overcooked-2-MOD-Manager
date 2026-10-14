use crate::models::*;
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chrono::Utc;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const CONFIG_FILE: &str = "config.toml";
const BUNDLE_FORMAT: &str = "oc2-mod-repository-bundle";
pub const REGION_CHINESE_MAINLAND: &str = "chinese_mainland";
pub const REGION_INTERNATIONAL: &str = "international";

pub(crate) fn normalize_repository_region(region: Option<&str>) -> String {
    match region
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "chinese_mainland" | "mainland-china" => REGION_CHINESE_MAINLAND.into(),
        "international" | "other" => REGION_INTERNATIONAL.into(),
        _ => String::new(),
    }
}

/// Normalize a configured region for persistence and report values that do
/// not identify a supported channel. Empty or unknown values intentionally
/// mean that the repository applies to every region.
pub(crate) fn normalize_repository_region_for_storage(
    region: Option<&str>,
    context: &str,
) -> Option<String> {
    let Some(value) = region.map(str::trim) else {
        return None;
    };
    if value.is_empty() {
        log::debug!("仓库地区为空，按全部地区处理：对象={}", context.trim());
        return None;
    }
    match value.to_ascii_lowercase().as_str() {
        "chinese_mainland" | "mainland-china" => Some(REGION_CHINESE_MAINLAND.into()),
        "international" | "other" => Some(REGION_INTERNATIONAL.into()),
        unknown => {
            log::warn!(
                "仓库地区无法识别，按全部地区处理：对象={}，值={}",
                context.trim(),
                unknown
            );
            None
        }
    }
}

pub fn current_user_region() -> &'static str {
    #[cfg(windows)]
    {
        use winapi::um::winnls::GetUserGeoID;
        // GEOCLASS_NATION is 16 in the Windows SDK and GEOID_CHINA is 45.
        if unsafe { GetUserGeoID(16) } == 45 {
            REGION_CHINESE_MAINLAND
        } else {
            REGION_INTERNATIONAL
        }
    }
    #[cfg(not(windows))]
    {
        REGION_INTERNATIONAL
    }
}

pub fn validate_repository_source_region(region: Option<&str>) -> Result<(), ConfigError> {
    let _ = normalize_repository_region_for_storage(region, "仓库源");
    Ok(())
}

pub fn validate_repository_region(region: Option<&str>) -> Result<(), ConfigError> {
    validate_repository_source_region(region)
        .map_err(|_| ConfigError::Invalid("适用地区无效，请选择中国大陆或非中国大陆".into()))
}

fn region_matches(configured_region: Option<&str>, region: &str) -> bool {
    match normalize_repository_region(configured_region).as_str() {
        "" => true,
        REGION_CHINESE_MAINLAND => region == REGION_CHINESE_MAINLAND,
        REGION_INTERNATIONAL => region != REGION_CHINESE_MAINLAND,
        _ => true,
    }
}

pub(crate) fn repository_address_key(
    provider: &Provider,
    username: &str,
    repository: &str,
    region: Option<&str>,
) -> (&'static str, String, String, String) {
    (
        match provider {
            Provider::Github => "github",
            Provider::Gitee => "gitee",
        },
        username.trim().to_ascii_lowercase(),
        repository.trim().to_ascii_lowercase(),
        normalize_repository_region(region),
    )
}

fn repository_source_matches_region(source: &RepositorySource, region: &str) -> bool {
    (source.id != crate::community::GITEE_SOURCE_ID || region == REGION_CHINESE_MAINLAND)
        && region_matches(source.region.as_deref(), region)
}

pub(crate) fn repository_matches_channel(
    config: &AppConfig,
    repository: &RepositoryConfig,
    region: &str,
) -> bool {
    crate::community::ensure_repository_region(repository, region).is_ok()
        && region_matches(repository.region.as_deref(), region)
        && repository.source_id.as_deref().is_none_or(|source_id| {
            config.repository_sources.iter().any(|source| {
                source.id == source_id && repository_source_matches_region(source, region)
            })
        })
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    #[error("配置目录不可用: {0}")]
    Directory(String),
    #[error("配置读取失败: {0}")]
    Read(String),
    #[error("配置解析失败: {0}")]
    Parse(String),
    #[error("配置写入失败: {0}")]
    Write(String),
    #[error("配置格式无效: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug)]
pub struct ConfigService {
    root: PathBuf,
}

impl ConfigService {
    #[cfg(test)]
    pub(crate) fn for_test(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn new() -> Result<Self, ConfigError> {
        let root = crate::user_paths::root().map_err(ConfigError::Directory)?;
        fs::create_dir_all(&root).map_err(|e| {
            log::error!("创建配置目录失败：目录={}，原因={}", root.display(), e);
            ConfigError::Directory(e.to_string())
        })?;
        log::debug!("配置服务初始化完成：目录={}", root.display());
        Ok(Self { root })
    }

    pub fn config_path(&self) -> PathBuf {
        self.root.join(CONFIG_FILE)
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.root.join("cache")
    }

    pub fn manifests_dir(&self) -> PathBuf {
        self.root.join("manifests")
    }

    pub fn mod_recycle_bin_dir(&self) -> PathBuf {
        self.root.join("recycle-bin")
    }

    pub fn backups_dir(&self) -> PathBuf {
        self.root.join("backups")
    }

    pub fn load(&self) -> Result<AppConfig, ConfigError> {
        let path = self.config_path();
        let previous = self.root.join("config.previous.toml");
        log::debug!(
            "开始加载配置：目标={}，备份存在={}",
            path.display(),
            previous.exists()
        );
        let source = if path.exists() {
            log::trace!("使用主配置文件");
            path
        } else if previous.exists() {
            log::warn!("主配置文件不存在，使用上一份配置");
            previous
        } else {
            log::info!("未找到配置文件，创建默认配置");
            let config = default_config();
            self.save(&config)?;
            return Ok(config);
        };
        let text = fs::read_to_string(&source).map_err(|e| {
            log::error!("读取配置文件失败：文件={}，原因={}", source.display(), e);
            ConfigError::Read(e.to_string())
        })?;
        let legacy_repository_proxy_fields = contains_legacy_repository_proxy_fields(&text);
        match toml::from_str::<AppConfig>(&text) {
            Ok(mut config) => {
                let mut changed = legacy_repository_proxy_fields;
                changed |= synchronize_builtin_repository_sources(&mut config);
                changed |=
                    crate::github_proxies::synchronize_bundled_defaults(&mut config.github_proxies);
                changed |= Self::normalize_configured_regions(&mut config);
                for repository in &mut config.repositories {
                    let normalized = repository.max_release_count.clamp(1, 100);
                    if repository.max_release_count != normalized {
                        repository.max_release_count = normalized;
                        changed = true;
                    }
                }
                if changed {
                    log::info!("配置需要迁移或同步，保存更新后的配置");
                    self.save(&config)?;
                }
                log::info!(
                    "配置加载完成：仓库={}，仓库源={}，MOD备注={}，配置已迁移={}",
                    config.repositories.len(),
                    config.repository_sources.len(),
                    config.mod_notes.len(),
                    changed
                );
                Ok(config)
            }
            Err(error) => {
                log::error!("配置解析失败，将备份损坏文件并恢复默认配置：原因={}", error);
                let backup = self.root.join(format!(
                    "config.corrupt-{}.toml",
                    Utc::now().format("%Y%m%d%H%M%S")
                ));
                let _ = fs::rename(&source, backup);
                let config = default_config();
                self.save(&config)?;
                Err(ConfigError::Parse(error.to_string()))
            }
        }
    }

    fn normalize_configured_regions(config: &mut AppConfig) -> bool {
        let mut changed = false;
        for repository in &mut config.repositories {
            let normalized = normalize_repository_region_for_storage(
                repository.region.as_deref(),
                &format!("仓库 {}", repository.id),
            );
            if repository.region != normalized {
                repository.region = normalized;
                changed = true;
            }
        }
        for source in &mut config.repository_sources {
            let normalized = normalize_repository_region_for_storage(
                source.region.as_deref(),
                &format!("仓库源 {}", source.id),
            );
            if source.region != normalized {
                source.region = normalized;
                changed = true;
            }
        }
        changed
    }

    pub fn load_or_default(&self) -> AppConfig {
        self.load().unwrap_or_else(|error| {
            log::error!("配置加载失败，使用默认配置：原因={}", error);
            default_config()
        })
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
        log::debug!(
            "开始保存配置：仓库={}，仓库源={}",
            config.repositories.len(),
            config.repository_sources.len()
        );
        fs::create_dir_all(&self.root).map_err(|e| {
            log::error!("创建配置目录失败：原因={}", e);
            ConfigError::Directory(e.to_string())
        })?;
        let text = toml::to_string_pretty(config).map_err(|e| {
            log::error!("序列化配置失败：原因={}", e);
            ConfigError::Write(e.to_string())
        })?;
        let temp = self.root.join(format!("config.{}.tmp", Uuid::new_v4()));
        fs::write(&temp, text).map_err(|e| {
            log::error!("写入配置临时文件失败：原因={}", e);
            ConfigError::Write(e.to_string())
        })?;
        let target = self.config_path();
        let previous = self.root.join("config.previous.toml");
        if target.exists() {
            if let Err(error) = fs::remove_file(&previous)
                && error.kind() != std::io::ErrorKind::NotFound
            {
                log::warn!("删除旧配置备份失败，将继续覆盖：原因={}", error);
            }
            fs::rename(&target, &previous).map_err(|e| {
                log::error!("备份旧配置失败：原因={}", e);
                ConfigError::Write(e.to_string())
            })?;
        }
        if let Err(error) = fs::rename(&temp, &target) {
            if let Err(cleanup_error) = fs::remove_file(&temp)
                && cleanup_error.kind() != std::io::ErrorKind::NotFound
            {
                log::warn!("替换配置失败后清理临时文件失败：原因={}", cleanup_error);
            }
            if previous.exists() {
                if let Err(rollback_error) = fs::rename(&previous, &target) {
                    log::error!("替换配置失败且恢复旧配置失败：恢复原因={}", rollback_error);
                } else {
                    log::warn!("替换配置失败，已恢复旧配置");
                }
            }
            log::error!("替换配置文件失败：原因={}", error);
            return Err(ConfigError::Write(error.to_string()));
        }
        if let Err(error) = fs::remove_file(previous)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            log::warn!("配置保存成功但旧备份清理失败：原因={}", error);
        }
        log::info!("配置保存完成");
        Ok(())
    }

    pub fn public_config(&self, config: &AppConfig) -> PublicConfig {
        let (community_discovery, gitee_community_discovery) =
            crate::community::visible_discoveries(config);
        let mut repositories = config.repositories.clone();
        for repo in &mut repositories {
            repo.github_token.clear();
            repo.gitee_token.clear();
        }
        PublicConfig {
            schema_version: config.schema_version,
            platform: std::env::consts::OS.into(),
            region: current_user_region().into(),
            ui_zoom: config.ui_zoom,
            onboarding_seen_steps: config.onboarding_seen_steps.clone(),
            hide_unidentified_mods: config.hide_unidentified_mods,
            tutorial_steps: config.tutorial_steps.clone(),
            game: config.game.clone(),
            network: config.network.clone(),
            launch: config.launch.clone(),
            official_api_only: config.auth.official_api_only,
            github_token_set: !config.auth.github_token.is_empty(),
            gitee_token_set: !config.auth.gitee_token.is_empty(),
            repositories,
            repository_sources: config.repository_sources.clone(),
            community_discovery,
            gitee_community_discovery,
            github_proxies: config.github_proxies.clone(),
        }
    }

    pub fn set_tokens(&self, config: &mut AppConfig, github: &str, gitee: &str) {
        if !github.is_empty() {
            config.auth.github_token = protect_secret(github).unwrap_or_default();
        }
        if !gitee.is_empty() {
            config.auth.gitee_token = protect_secret(gitee).unwrap_or_default();
        }
    }

    pub fn set_repository_tokens(&self, repo: &mut RepositoryConfig, github: &str, gitee: &str) {
        if !github.is_empty() {
            repo.github_token = protect_secret(github).unwrap_or_default();
        }
        if !gitee.is_empty() {
            repo.gitee_token = protect_secret(gitee).unwrap_or_default();
        }
    }

    pub fn repository_token(&self, repo: &RepositoryConfig) -> Option<String> {
        match repo.provider {
            Provider::Github => reveal_secret(&repo.github_token),
            Provider::Gitee => reveal_secret(&repo.gitee_token),
        }
    }

    pub(crate) fn repository_api_token(
        &self,
        config: &AppConfig,
        repo: &RepositoryConfig,
    ) -> Option<String> {
        self.repository_token(repo)
            .or_else(|| self.api_token(config, &repo.provider))
    }

    pub(crate) fn api_token(&self, config: &AppConfig, provider: &Provider) -> Option<String> {
        reveal_secret(match provider {
            Provider::Github => &config.auth.github_token,
            Provider::Gitee => &config.auth.gitee_token,
        })
    }

    pub fn set_mod_note(&self, config: &mut AppConfig, key: &str, note: &str) {
        config.mod_notes.retain(|item| item.key != key);
        let note = note.trim();
        if !note.is_empty() {
            config.mod_notes.push(ModNote {
                key: key.to_string(),
                note: note.to_string(),
            });
        }
    }

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
        let selected = config
            .repositories
            .iter()
            .filter(|repo| {
                ids.as_ref()
                    .map(|list| list.contains(&repo.id))
                    .unwrap_or(true)
            })
            .map(|repo| RepositoryExport {
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
                channel: repo.channel.clone(),
                asset_rules: repo.asset_rules.clone(),
                install_target: repo.install_target.clone(),
                note: repo.note.clone(),
                note_i18n: repo.note_i18n.clone(),
                plugin_guid: repo.plugin_guid.clone(),
                local_product_name: repo.local_product_name.clone(),
                local_original_filename: repo.local_original_filename.clone(),
                version_regex: repo.version_regex.clone(),
                max_release_count: repo.max_release_count,
                download_threads: repo.download_threads,
                enabled: repo.enabled,
                source_id: repo.source_id.clone(),
            })
            .collect::<Vec<_>>();
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
        let bundle = RepositoryBundle {
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

    pub fn validate_bundle(&self, text: &str) -> Result<RepositoryBundle, ConfigError> {
        log::debug!("开始校验仓库配置包：字节数={}", text.len());
        if text.len() > 1024 * 1024 {
            return Err(ConfigError::Invalid("导入文件超过 1 MiB".into()));
        }
        let bundle: RepositoryBundle = toml::from_str(text)
            .map_err(|e| ConfigError::Invalid(format!("TOML 解析失败: {e}")))?;
        if bundle.format != BUNDLE_FORMAT {
            return Err(ConfigError::Invalid(
                "format 不是 oc2-mod-repository-bundle".into(),
            ));
        }
        if bundle.schema_version != 1 {
            return Err(ConfigError::Invalid(format!(
                "不支持 schema_version {}",
                bundle.schema_version
            )));
        }
        for repo in &bundle.repositories {
            validate_repository(repo)?;
        }
        log::info!(
            "仓库配置包校验完成：仓库={}，仓库源={}",
            bundle.repositories.len(),
            bundle.repository_sources.len()
        );
        Ok(bundle)
    }

    pub fn preview_bundle(&self, config: &AppConfig, bundle: &RepositoryBundle) -> ImportPreview {
        let mut preview = ImportPreview {
            valid: true,
            errors: Vec::new(),
            additions: Vec::new(),
            duplicates: Vec::new(),
            conflicts: Vec::new(),
        };
        for item in &bundle.repositories {
            if config.repositories.iter().any(|repo| {
                !crate::community::is_repository(repo)
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
                preview.duplicates.push(item.name.clone());
                continue;
            }
            let key = export_key(item);
            let matching = config
                .repositories
                .iter()
                .find(|repo| !crate::community::is_repository(repo) && repo_key(repo) == key);
            if matching.is_some_and(|repo| repo.builtin) {
                preview.duplicates.push(item.name.clone());
                continue;
            }
            let same_name = config.repositories.iter().any(|repo| {
                !crate::community::is_repository(repo)
                    && repo.name == item.name
                    && normalize_repository_region(repo.region.as_deref())
                        == normalize_repository_region(item.region.as_deref())
            });
            if matching.is_some() {
                preview.duplicates.push(item.name.clone());
            } else if same_name {
                preview.conflicts.push(item.name.clone());
            } else {
                preview.additions.push(item.name.clone());
            }
        }
        preview
    }

    pub fn import_bundle(
        &self,
        config: &mut AppConfig,
        bundle: &RepositoryBundle,
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
            let key = export_key(item);
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
                    *existing = repository_from_export(item, id, builtin);
                    imported_ids.push(existing.id.clone());
                }
                continue;
            }
            let mut name = item.name.clone();
            let same_region = |repo: &RepositoryConfig| {
                normalize_repository_region(repo.region.as_deref())
                    == normalize_repository_region(item.region.as_deref())
            };
            if config.repositories.iter().any(|repo| {
                !crate::community::is_repository(repo) && repo.name == name && same_region(repo)
            }) {
                match conflict {
                    "skip" => continue,
                    "keep_both" => {
                        name = format!("{} (导入)", name);
                    }
                    "overwrite" => {
                        config.repositories.retain(|repo| {
                            crate::community::is_repository(repo)
                                || repo.name != name
                                || !same_region(repo)
                        });
                    }
                    _ => continue,
                }
            }
            let mut export = item.clone();
            export.name = name;
            let id = Uuid::new_v4().to_string();
            config
                .repositories
                .push(repository_from_export(&export, id.clone(), false));
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

fn default_config() -> AppConfig {
    AppConfig {
        schema_version: 1,
        ui_zoom: 1.5,
        onboarding_seen_steps: Vec::new(),
        hide_unidentified_mods: true,
        tutorial_steps: Vec::new(),
        game: GameConfig {
            path: String::new(),
            auto_detect: true,
            runtime: if cfg!(windows) {
                GameRuntime::Native
            } else {
                GameRuntime::Wine
            },
        },
        network: NetworkConfig {
            timeout_seconds: 20,
            download_threads: 32,
            max_retry_count: 3,
            check_updates_on_start: true,
        },
        launch: LaunchConfig {
            ask_user_to_start_from_steam: true,
            auto_restore_winhttp: true,
        },
        auth: AuthConfig {
            github_token: String::new(),
            gitee_token: String::new(),
            official_api_only: true,
        },
        repositories: Vec::new(),
        repository_sources: builtin_repository_sources(),
        community_discovery: None,
        gitee_community_discovery: None,
        source_repository_ids: Default::default(),
        github_proxies: crate::github_proxies::bundled_defaults(),
        mod_notes: Vec::new(),
    }
}

/// Older repository entries carried API and asset proxy settings directly on
/// each repository. Those fields are intentionally no longer part of the
/// configuration model: proxy selection is global and is supplied for the
/// current download operation. Detect them while loading so an existing
/// configuration is rewritten immediately using the current schema instead of
/// preserving the obsolete keys on disk until some unrelated save occurs.
fn contains_legacy_repository_proxy_fields(text: &str) -> bool {
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
    value
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

fn builtin_repository_sources() -> Vec<RepositorySource> {
    builtin_repository_sources_for_region(current_user_region())
}

fn builtin_repository_sources_for_region(region: &str) -> Vec<RepositorySource> {
    // Only the entry point is shipped with the application. Its children and
    // repositories belong to the live index, not the embedded tutorial snapshot.
    vec![RepositorySource {
        id: crate::repository_sources::DEFAULT_INDEX_ID.into(),
        nickname: "官方仓库源".into(),
        nickname_i18n: [
            ("zh-CN".into(), "官方仓库源".into()),
            ("zh-TW".into(), "官方倉庫來源".into()),
            ("en".into(), "Official repositories".into()),
            ("ko".into(), "공식 저장소 소스".into()),
        ]
        .into(),
        url: crate::repository_sources::default_index_url(region).into(),
        region: None,
        builtin: true,
        parent_id: None,
    }]
}

fn synchronize_builtin_repository_sources(config: &mut AppConfig) -> bool {
    merge_builtin_repository_sources(config, builtin_repository_sources())
}

fn merge_builtin_repository_sources(
    config: &mut AppConfig,
    builtin_sources: Vec<RepositorySource>,
) -> bool {
    if builtin_sources.is_empty() {
        return false;
    }
    let previous = config.repository_sources.clone();
    let builtin_ids = builtin_sources
        .iter()
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    let mut synchronized = builtin_sources;
    let previous_ids = previous
        .iter()
        .map(|source| source.id.clone())
        .collect::<std::collections::HashSet<_>>();
    for source in previous {
        // Only default root identities are fixed. Descendants are owned by
        // their remote documents and must survive ordinary config reads.
        // This also removes the obsolete embedded roots and their trees.
        if source.builtin && source.parent_id.is_none() {
            continue;
        }
        if builtin_ids.contains(&source.id)
            || synchronized.iter().any(|existing| {
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
        synchronized.push(source);
    }
    loop {
        let before = synchronized.len();
        let known_ids = synchronized
            .iter()
            .map(|source| source.id.clone())
            .collect::<std::collections::HashSet<_>>();
        synchronized.retain(|source| {
            source
                .parent_id
                .as_deref()
                .is_none_or(|parent_id| known_ids.contains(parent_id))
        });
        if synchronized.len() == before {
            break;
        }
    }
    let mut changed = config.repository_sources != synchronized;
    config.repository_sources = synchronized;

    let known_ids = config
        .repository_sources
        .iter()
        .map(|source| &source.id)
        .collect::<std::collections::HashSet<_>>();
    let before = config.repositories.len();
    // Old source-less fallback cards and obsolete regional source trees are
    // replaced by the ordinary source loader after the tutorial completes.
    config.repositories.retain(|repository| {
        !(repository.builtin && repository.source_id.is_none())
            && repository
                .source_id
                .as_ref()
                .is_none_or(|id| !previous_ids.contains(id) || known_ids.contains(id))
    });
    changed |= config.repositories.len() != before;
    changed
}

// Write values rather than nested tables so rules and translations stay beside
// their editor section without changing the TOML table that receives later fields.
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

fn serialize_repository_bundle(bundle: &RepositoryBundle) -> Result<String, ConfigError> {
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
            RepositoryDownloadMode::Release => {
                text.push_str("\n# Release 参数\n");
                append_toml_field(&mut text, "channel", &repo.channel)?;
                append_toml_field(&mut text, "max_release_count", &repo.max_release_count)?;
                append_toml_field(&mut text, "download_threads", &repo.download_threads)?;
                text.push_str("\n# 版本资产规则\n");
                append_toml_field(&mut text, "asset_rules", &repo.asset_rules)?;
            }
            RepositoryDownloadMode::RepositoryFile => {
                text.push_str("\n# 仓库文件参数\n");
                append_toml_field(&mut text, "branch", &repo.branch)?;
                append_toml_field(&mut text, "download_threads", &repo.download_threads)?;
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

pub(crate) fn validate_repository(repo: &RepositoryExport) -> Result<(), ConfigError> {
    validate_repository_region(repo.region.as_deref())?;
    crate::releases::repository_coordinates(&repo.username, &repo.repository)
        .map_err(|error| ConfigError::Invalid(format!("{} 的项目主页无效: {error}", repo.name)))?;
    if Path::new(&repo.install_target).is_absolute()
        || repo
            .install_target
            .split(['/', '\\'])
            .any(|part| part == "..")
    {
        return Err(ConfigError::Invalid(format!(
            "{} 的安装目录不安全",
            repo.name
        )));
    }
    match repo.download_mode {
        RepositoryDownloadMode::Release => {
            if repo.asset_rules.is_empty()
                || repo
                    .asset_rules
                    .iter()
                    .any(|rule| rule.value.trim().is_empty())
            {
                return Err(ConfigError::Invalid("请创建至少一个有效的资产规则".into()));
            }
        }
        RepositoryDownloadMode::RepositoryFile => {
            crate::releases::repository_file_url(
                &repo.provider,
                &repo.username,
                &repo.repository,
                &repo.branch,
                &repo.file_path,
            )
            .map_err(|error| ConfigError::Invalid(error.to_string()))?;
            if repo
                .asset_rules
                .iter()
                .any(|rule| rule.rule_type != AssetRuleType::Exact)
            {
                return Err(ConfigError::Invalid(
                    "仓库文件模式的资产规则只支持具体文件".into(),
                ));
            }
        }
    }
    if !repo.version_regex.trim().is_empty()
        && regex::Regex::new(repo.version_regex.trim()).is_err()
    {
        return Err(ConfigError::Invalid(format!(
            "{} 的版本号正则无效",
            repo.name
        )));
    }
    Ok(())
}

pub(crate) fn repo_key(repo: &RepositoryConfig) -> String {
    let export = RepositoryExport {
        name: String::new(),
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
        channel: repo.channel.clone(),
        asset_rules: repo.asset_rules.clone(),
        install_target: repo.install_target.clone(),
        note: repo.note.clone(),
        note_i18n: repo.note_i18n.clone(),
        plugin_guid: repo.plugin_guid.clone(),
        local_product_name: repo.local_product_name.clone(),
        local_original_filename: repo.local_original_filename.clone(),
        version_regex: repo.version_regex.clone(),
        max_release_count: repo.max_release_count,
        download_threads: repo.download_threads,
        enabled: repo.enabled,
        source_id: repo.source_id.clone(),
    };
    export_key(&export)
}

pub(crate) fn source_repository_key(source_id: &str, identity: &str) -> String {
    format!("{}:{source_id}{identity}", source_id.len())
}

pub(crate) fn export_key(repo: &RepositoryExport) -> String {
    // Fields hidden by the current download mode are not repository identity.
    // Omitting them from an export must not change its ID on re-import.
    let (branch, file_path, asset_rules) = match repo.download_mode {
        RepositoryDownloadMode::Release => ("", "", repo.asset_rules.as_slice()),
        RepositoryDownloadMode::RepositoryFile => {
            (repo.branch.as_str(), repo.file_path.as_str(), &[][..])
        }
    };
    format!(
        "{:?}|{}|{}|{:?}|{}|{}|{:?}|{}|{}|{}",
        repo.provider,
        repo.username.trim().to_ascii_lowercase(),
        repo.repository.trim().to_ascii_lowercase(),
        repo.download_mode,
        branch,
        file_path,
        asset_rules,
        repo.install_target,
        repo.enabled,
        normalize_repository_region(repo.region.as_deref())
    )
}

fn repository_from_export(repo: &RepositoryExport, id: String, builtin: bool) -> RepositoryConfig {
    let download_threads = match &repo.provider {
        Provider::Github => {
            if repo.download_threads == 0 {
                16
            } else {
                repo.download_threads.clamp(1, 32)
            }
        }
        Provider::Gitee => 1,
    };
    RepositoryConfig {
        id,
        name: repo.name.clone(),
        name_i18n: repo.name_i18n.clone(),
        group: repo.group.clone(),
        group_i18n: repo.group_i18n.clone(),
        provider: repo.provider.clone(),
        username: repo.username.trim().to_string(),
        repository: repo.repository.trim().to_string(),
        download_mode: repo.download_mode.clone(),
        branch: repo.branch.clone(),
        file_path: repo.file_path.clone(),
        region: normalize_repository_region_for_storage(
            repo.region.as_deref(),
            &format!("仓库 {}", repo.name),
        ),
        channel: repo.channel.clone(),
        asset_rules: repo.asset_rules.clone(),
        install_target: repo.install_target.clone(),
        note: repo.note.clone(),
        note_i18n: repo.note_i18n.clone(),
        // An exported search result can be imported as a custom repository,
        // but cannot impersonate the generated community catalogue.
        source_id: repo
            .source_id
            .clone()
            .filter(|id| !crate::community::is_source(id)),
        plugin_guid: repo.plugin_guid.clone(),
        local_product_name: repo.local_product_name.clone(),
        local_original_filename: repo.local_original_filename.clone(),
        version_regex: repo.version_regex.clone(),
        max_release_count: repo.max_release_count.clamp(1, 100),
        download_threads,
        github_token: String::new(),
        gitee_token: String::new(),
        official_api_only: true,
        enabled: repo.enabled,
        builtin,
        pinned_version: None,
        ignored_version: None,
        etag: None,
        cached_releases: Vec::new(),
        last_error: None,
    }
}

fn protect_secret(value: &str) -> io::Result<String> {
    if value.is_empty() {
        return Ok(String::new());
    }
    #[cfg(windows)]
    unsafe {
        use std::ptr::null_mut;
        use winapi::um::dpapi::CryptProtectData;
        use winapi::um::winbase::LocalFree;
        use winapi::um::wincrypt::DATA_BLOB;
        let mut input = DATA_BLOB {
            cbData: value.len() as u32,
            pbData: value.as_ptr() as *mut u8,
        };
        let mut output = DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        if CryptProtectData(
            &mut input,
            std::ptr::null(),
            null_mut(),
            null_mut(),
            null_mut(),
            0,
            &mut output,
        ) == 0
        {
            return Err(io::Error::last_os_error());
        }
        let encrypted = std::slice::from_raw_parts(output.pbData, output.cbData as usize);
        let encoded = format!("dpapi:{}", B64.encode(encrypted));
        LocalFree(output.pbData as *mut _);
        return Ok(encoded);
    }
    #[cfg(not(windows))]
    Ok(format!("plain:{}", B64.encode(value.as_bytes())))
}

fn reveal_secret(value: &str) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    let encoded = value.split_once(':').map(|(_, data)| data).unwrap_or(value);
    let encrypted = B64.decode(encoded).ok()?;
    #[cfg(windows)]
    unsafe {
        use std::ptr::null_mut;
        use winapi::um::dpapi::CryptUnprotectData;
        use winapi::um::winbase::LocalFree;
        use winapi::um::wincrypt::DATA_BLOB;
        let mut input = DATA_BLOB {
            cbData: encrypted.len() as u32,
            pbData: encrypted.as_ptr() as *mut u8,
        };
        let mut output = DATA_BLOB {
            cbData: 0,
            pbData: null_mut(),
        };
        if CryptUnprotectData(
            &mut input,
            std::ptr::null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            0,
            &mut output,
        ) == 0
        {
            return None;
        }
        let result = String::from_utf8_lossy(std::slice::from_raw_parts(
            output.pbData,
            output.cbData as usize,
        ))
        .to_string();
        LocalFree(output.pbData as *mut _);
        Some(result)
    }
    #[cfg(not(windows))]
    {
        String::from_utf8(encrypted).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const REGION_REPOSITORY: &str = r#"
[[repositories]]
name = "Regional MOD"
provider = "github"
username = "example"
repository = "mod"
channel = "stable"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.regional-mod"
enabled = true
[[repositories.asset_rules]]
type = "exact"
value = "RegionalMod.dll"
"#;

    #[test]
    fn repository_regions_are_optional_validated_and_preserved() {
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let legacy =
            crate::repository_sources::parse_document(REGION_REPOSITORY, &location).unwrap();
        assert_eq!(legacy.repositories[0].region, None);
        for region in ["chinese_mainland", "international", ""] {
            let text = REGION_REPOSITORY
                .replace("channel =", &format!("region = \"{region}\"\nchannel ="));
            let document = crate::repository_sources::parse_document(&text, &location).unwrap();
            assert_eq!(
                document.repositories[0].region.as_deref(),
                (!region.is_empty()).then_some(region)
            );
            let repository =
                repository_from_export(&document.repositories[0], "regional".into(), false);
            let mut config = default_config();
            config.repositories.push(repository);
            let serialized = toml::to_string(&config).unwrap();
            let restored: AppConfig = toml::from_str(&serialized).unwrap();
            assert_eq!(
                restored.repositories[0].region.as_deref(),
                (!region.is_empty()).then_some(region)
            );
        }
        let unknown = REGION_REPOSITORY.replace("channel =", "region = \"unknown\"\nchannel =");
        let document = crate::repository_sources::parse_document(&unknown, &location).unwrap();
        assert_eq!(document.repositories[0].region, None);
        let invalid_type = REGION_REPOSITORY.replace("channel =", "region = true\nchannel =");
        assert!(crate::repository_sources::parse_document(&invalid_type, &location).is_err());
    }

    #[test]
    fn repository_channel_requires_both_source_and_repository_regions() {
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let document =
            crate::repository_sources::parse_document(REGION_REPOSITORY, &location).unwrap();
        let mut repository =
            repository_from_export(&document.repositories[0], "regional".into(), false);
        let mut config = default_config();
        for (configured, china, international) in [
            (None, true, true),
            (Some(""), true, true),
            (Some("chinese_mainland"), true, false),
            (Some("international"), false, true),
            (Some(" MAINLAND-CHINA "), true, false),
            (Some("Other"), false, true),
        ] {
            repository.region = configured.map(str::to_owned);
            assert_eq!(
                repository_matches_channel(&config, &repository, REGION_CHINESE_MAINLAND),
                china
            );
            assert_eq!(
                repository_matches_channel(&config, &repository, REGION_INTERNATIONAL),
                international
            );
        }
        repository.source_id = Some("restricted-source".into());
        repository.region = None;
        assert!(!repository_matches_channel(
            &config,
            &repository,
            REGION_CHINESE_MAINLAND
        ));
        config.repository_sources.push(RepositorySource {
            id: "restricted-source".into(),
            nickname: "Restricted source".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/source.toml".into(),
            region: Some(REGION_CHINESE_MAINLAND.into()),
            builtin: false,
            parent_id: None,
        });
        assert!(repository_matches_channel(
            &config,
            &repository,
            REGION_CHINESE_MAINLAND
        ));
        assert!(!repository_matches_channel(
            &config,
            &repository,
            REGION_INTERNATIONAL
        ));
        repository.region = Some(REGION_INTERNATIONAL.into());
        assert!(!repository_matches_channel(
            &config,
            &repository,
            REGION_CHINESE_MAINLAND
        ));
        assert!(!repository_matches_channel(
            &config,
            &repository,
            REGION_INTERNATIONAL
        ));
    }

    #[test]
    fn saved_gitee_community_without_region_is_blocked_outside_mainland() {
        let mut config = default_config();
        let mut source = crate::community::source(&Provider::Gitee);
        source.region = None;
        config.repository_sources.push(source);
        let mut repo =
            crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
                .unwrap();
        repo.region = None;
        assert!(repository_matches_channel(
            &config,
            &repo,
            REGION_CHINESE_MAINLAND
        ));
        assert!(!repository_matches_channel(
            &config,
            &repo,
            REGION_INTERNATIONAL
        ));
        assert!(!repository_matches_channel(&config, &repo, ""));
        assert_eq!(
            config.repositories.len(),
            default_config().repositories.len()
        );
    }

    #[test]
    fn regional_repository_variants_coexist_and_export_all_channels() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let document =
            crate::repository_sources::parse_document(REGION_REPOSITORY, &location).unwrap();
        let repositories = [REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL].map(|region| {
            let mut repository = document.repositories[0].clone();
            repository.region = Some(region.into());
            repository.download_threads = 16;
            repository
        });
        let bundle = RepositoryBundle {
            format: BUNDLE_FORMAT.into(),
            schema_version: 1,
            exported_at: String::new(),
            repositories: repositories.to_vec(),
            repository_sources: Vec::new(),
        };
        let mut config = default_config();
        service.import_bundle(&mut config, &bundle, "overwrite");
        assert_eq!(config.repositories.len(), 2);
        assert!(
            config
                .repositories
                .iter()
                .all(|repo| repo.name == "Regional MOD")
        );
        assert_eq!(
            config
                .repositories
                .iter()
                .map(|repo| repo.region.as_deref())
                .collect::<Vec<_>>(),
            vec![Some(REGION_CHINESE_MAINLAND), Some(REGION_INTERNATIONAL)]
        );
        let original_ids = config
            .repositories
            .iter()
            .map(|repo| repo.id.clone())
            .collect::<Vec<_>>();
        service.import_bundle(&mut config, &bundle, "overwrite");
        assert_eq!(
            config
                .repositories
                .iter()
                .map(|repo| repo.id.clone())
                .collect::<Vec<_>>(),
            original_ids
        );
        let exported = service.export_bundle(&config, None).unwrap();
        let restored = service.validate_bundle(&exported).unwrap();
        assert_eq!(
            serde_json::to_value(&restored.repositories).unwrap(),
            serde_json::to_value(&repositories).unwrap()
        );
        let mut imported = default_config();
        service.import_bundle(&mut imported, &restored, "overwrite");
        assert_eq!(
            imported
                .repositories
                .iter()
                .map(|repo| repo.region.as_deref())
                .collect::<Vec<_>>(),
            vec![Some(REGION_CHINESE_MAINLAND), Some(REGION_INTERNATIONAL)]
        );
    }

    #[test]
    fn default_config_uses_the_live_official_index_without_embedded_children() {
        let config = default_config();
        assert!(config.repositories.is_empty());
        assert_eq!(config.repository_sources.len(), 1);
        let root = &config.repository_sources[0];
        assert_eq!(root.id, crate::repository_sources::DEFAULT_INDEX_ID);
        assert_eq!(
            root.url,
            crate::repository_sources::default_index_url(current_user_region())
        );
        assert!(matches!(
            crate::repository_sources::SourceLocation::parse(&root.url).unwrap(),
            crate::repository_sources::SourceLocation::Remote(_)
        ));
        assert!(config.repository_sources.iter().all(|source| source.builtin
            && source.parent_id.is_none()
            && repository_source_matches_region(source, current_user_region())));
        let serialized = toml::to_string(&config).unwrap();
        assert!(!serialized.contains("\n[proxies"));
    }

    #[test]
    fn config_reads_preserve_remote_children_and_load_later_disk_changes() {
        let text = r#"
[[repository_sources]]
id = "china"
nickname = "China"
url = "mods.toml"
region = "chinese_mainland"
[repository_sources.nickname_i18n]
en = "China Mods"
[[repository_sources]]
id = "international"
nickname = "International"
url = "mods.toml"
region = "international"
[repository_sources.nickname_i18n]
en = "International Mods"
[[repository_sources]]
id = "shared"
nickname = "Shared"
url = "shared.toml"
"#;
        let service = ConfigService::for_test(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-config-load")
                .join(Uuid::new_v4().to_string()),
        );
        let mut config = default_config();
        let root = config.repository_sources[0].clone();
        let location = crate::repository_sources::SourceLocation::parse(&root.url).unwrap();
        let document = crate::repository_sources::parse_document(text, &location).unwrap();
        for source in document.repository_sources {
            config.repository_sources.push(RepositorySource {
                id: source.id,
                nickname: source.nickname,
                nickname_i18n: source.nickname_i18n,
                url: source.url,
                region: source.region,
                builtin: true,
                parent_id: Some(root.id.clone()),
            });
        }
        service.save(&config).unwrap();
        let loaded = service.load().unwrap();
        assert_eq!(loaded.repository_sources, config.repository_sources);
        for (region, expected_ids) in [
            (REGION_CHINESE_MAINLAND, vec!["china", "shared"]),
            (REGION_INTERNATIONAL, vec!["international", "shared"]),
        ] {
            let sources = loaded
                .repository_sources
                .iter()
                .filter(|source| {
                    source.parent_id.is_some() && repository_source_matches_region(source, region)
                })
                .collect::<Vec<_>>();
            assert_eq!(
                sources
                    .iter()
                    .map(|source| source.id.as_str())
                    .collect::<Vec<_>>(),
                expected_ids
            );
            assert!(
                sources.iter().all(|source| source.builtin
                    && source.parent_id.as_deref() == Some(root.id.as_str()))
            );
            assert_eq!(
                sources[0].nickname_i18n["en"],
                if region == REGION_CHINESE_MAINLAND {
                    "China Mods"
                } else {
                    "International Mods"
                }
            );
        }
        config.repository_sources[1].nickname = "Remote changed name".into();
        config.ui_zoom = 1.8;
        service.save(&config).unwrap();
        let reloaded = service.load().unwrap();
        assert_eq!(reloaded.repository_sources, config.repository_sources);
        assert_eq!(reloaded.ui_zoom, 1.8);
        fs::remove_dir_all(service.root).unwrap();
    }

    #[test]
    fn official_index_follows_region_and_preserves_custom_sources() {
        let custom = RepositorySource {
            id: "regional-custom".into(),
            nickname: "Custom".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/custom.toml".into(),
            region: None,
            builtin: false,
            parent_id: None,
        };
        let mut config = default_config();
        config.repository_sources.push(custom.clone());
        for (region, expected) in [
            (
                REGION_CHINESE_MAINLAND,
                crate::repository_sources::DEFAULT_INDEX_GITEE_URL,
            ),
            (
                REGION_INTERNATIONAL,
                crate::repository_sources::DEFAULT_INDEX_GITHUB_URL,
            ),
            (
                REGION_CHINESE_MAINLAND,
                crate::repository_sources::DEFAULT_INDEX_GITEE_URL,
            ),
        ] {
            merge_builtin_repository_sources(
                &mut config,
                builtin_repository_sources_for_region(region),
            );
            assert_eq!(config.repository_sources.len(), 2);
            assert_eq!(
                config.repository_sources[0].id,
                crate::repository_sources::DEFAULT_INDEX_ID
            );
            assert_eq!(config.repository_sources[0].url, expected);
            assert!(config.repository_sources[0].builtin);
            assert_eq!(config.repository_sources[1], custom);
            assert!(!merge_builtin_repository_sources(
                &mut config,
                builtin_repository_sources_for_region(region)
            ));
        }
    }

    #[test]
    fn source_nickname_translations_survive_config_and_bundle_round_trips() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let legacy: RepositorySource = toml::from_str(
            r#"
id = "custom-translated-source"
nickname = "Fallback name"
url = "https://example.com/mods.toml"
"#,
        )
        .unwrap();
        assert!(legacy.nickname_i18n.is_empty());
        let mut source = legacy;
        source.nickname_i18n = std::collections::BTreeMap::from([
            ("zh-CN".into(), "中文昵称".into()),
            ("en".into(), "English name".into()),
        ]);
        let mut config = default_config();
        config.repository_sources.push(source.clone());
        let serialized = toml::to_string(&config).unwrap();
        let restored: AppConfig = toml::from_str(&serialized).unwrap();
        let public = service.public_config(&restored);
        assert_eq!(
            public
                .repository_sources
                .iter()
                .find(|item| item.id == source.id),
            Some(&source)
        );
        let exported = service.export_bundle(&restored, None).unwrap();
        let bundle = service.validate_bundle(&exported).unwrap();
        assert_eq!(bundle.repository_sources, restored.repository_sources);
        let mut imported = default_config();
        service.import_bundle(&mut imported, &bundle, "overwrite");
        assert_eq!(
            imported
                .repository_sources
                .iter()
                .find(|item| item.id == source.id),
            Some(&source)
        );
    }

    #[test]
    fn builtin_source_migration_removes_obsolete_trees_and_preserves_custom_sources() {
        let mut config = default_config();
        let old_root = RepositorySource {
            id: "old-builtin-root".into(),
            nickname: "Old built-in source".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/old.toml".into(),
            region: None,
            builtin: true,
            parent_id: None,
        };
        let mut new_root = old_root.clone();
        new_root.id = "new-builtin-root".into();
        new_root.url = "new.toml".into();
        let mut child = old_root.clone();
        child.id = "old-child".into();
        child.parent_id = Some(old_root.id.clone());
        let mut grandchild = child.clone();
        grandchild.id = "old-grandchild".into();
        grandchild.parent_id = Some(child.id.clone());
        grandchild.builtin = false;
        let mut custom = old_root.clone();
        custom.id = "custom-source".into();
        custom.url = "https://example.com/custom.toml".into();
        custom.builtin = false;
        config.repository_sources = vec![old_root, child, grandchild, custom.clone()];
        let export: RepositoryExport = toml::from_str(
            r#"
name = "Custom MOD"
provider = "github"
username = "example"
repository = "mod"
channel = "all"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.mod"
enabled = true
[[asset_rules]]
type = "exact"
value = "MOD.dll"
"#,
        )
        .unwrap();
        let mut old_fallback = repository_from_export(&export, "fallback".into(), true);
        old_fallback.source_id = None;
        let mut old_source_repo = repository_from_export(&export, "old-repo".into(), false);
        old_source_repo.source_id = Some("old-grandchild".into());
        let mut custom_repo = repository_from_export(&export, "custom-repo".into(), false);
        custom_repo.source_id = Some(custom.id.clone());
        config.repositories = vec![old_fallback, old_source_repo, custom_repo];
        assert!(merge_builtin_repository_sources(
            &mut config,
            vec![new_root.clone()]
        ));
        assert_eq!(config.repository_sources.len(), 2);
        assert_eq!(config.repository_sources[1], custom);
        assert_eq!(config.repositories.len(), 1);
        assert_eq!(config.repositories[0].id, "custom-repo");
        assert!(!merge_builtin_repository_sources(
            &mut config,
            vec![new_root]
        ));
    }

    #[test]
    fn first_load_synchronizes_builtin_sources_even_when_removing_legacy_proxy_fields() {
        for has_legacy_proxy_fields in [false, true] {
            let service = ConfigService {
                root: Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("target/test-config-load")
                    .join(Uuid::new_v4().to_string()),
            };
            let mut config = default_config();
            let mut expected_sources = config.repository_sources.clone();
            let old_root = RepositorySource {
                id: "obsolete-builtin".into(),
                nickname: "内置仓库源".into(),
                nickname_i18n: Default::default(),
                url: "https://example.com/obsolete.toml".into(),
                region: None,
                builtin: true,
                parent_id: None,
            };
            let mut old_child = old_root.clone();
            old_child.id = "obsolete-child".into();
            old_child.parent_id = Some(old_root.id.clone());
            let custom = RepositorySource {
                id: "custom-source".into(),
                nickname: "Custom source".into(),
                nickname_i18n: Default::default(),
                url: "https://example.com/custom.toml".into(),
                region: None,
                builtin: false,
                parent_id: None,
            };
            expected_sources.push(custom.clone());
            config.repository_sources = vec![old_root.clone(), old_child.clone(), custom.clone()];
            let location =
                crate::repository_sources::SourceLocation::parse("fixture.toml").unwrap();
            let document =
                crate::repository_sources::parse_document(REGION_REPOSITORY, &location).unwrap();
            config.repositories = [
                ("obsolete-mod", Some(old_root.id), true),
                ("obsolete-child-mod", Some(old_child.id), true),
                ("obsolete-fallback-mod", None, true),
                ("current-mod", Some(expected_sources[0].id.clone()), true),
                ("custom-mod", Some(custom.id), false),
            ]
            .into_iter()
            .map(|(id, source_id, builtin)| {
                let mut repository =
                    repository_from_export(&document.repositories[0], id.into(), builtin);
                repository.source_id = source_id;
                repository
            })
            .collect();
            let mut fixture = toml::Value::try_from(&config).unwrap();
            if has_legacy_proxy_fields {
                fixture["repositories"][0].as_table_mut().unwrap().insert(
                    "api_proxy_mode".into(),
                    toml::Value::String("direct".into()),
                );
            }
            fs::create_dir_all(&service.root).unwrap();
            fs::write(
                service.config_path(),
                toml::to_string_pretty(&fixture).unwrap(),
            )
            .unwrap();

            let loaded = service.load().unwrap();
            assert_eq!(loaded.repository_sources, expected_sources);
            assert_eq!(
                loaded
                    .repositories
                    .iter()
                    .map(|repository| repository.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["current-mod", "custom-mod"]
            );
            let saved = fs::read_to_string(service.config_path()).unwrap();
            assert!(!contains_legacy_repository_proxy_fields(&saved));
            assert!(!saved.contains("obsolete-"));
            let reloaded = service.load().unwrap();
            assert_eq!(reloaded.repository_sources, expected_sources);
            assert_eq!(fs::read_to_string(service.config_path()).unwrap(), saved);
        }
    }

    #[test]
    fn repository_source_does_not_require_bundle_metadata() {
        let text = r#"
[[repositories]]
name = "Example"
provider = "gitee"
username = "example"
repository = "release"
channel = "all"
install_target = "BepInEx/plugins"
note = """
**Example** repository
"""
plugin_guid = "com.example.plugin"
download_threads = 1
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Example.dll"
"#;
        let location = crate::repository_sources::SourceLocation::parse("source.toml").unwrap();
        let document = crate::repository_sources::parse_document(text, &location).unwrap();
        assert_eq!(document.repositories.len(), 1);
        assert_eq!(document.repositories[0].name, "Example");
    }

    #[test]
    fn imported_repositories_follow_bundle_order() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let text = r#"
format = "oc2-mod-repository-bundle"
schema_version = 1
exported_at = ""

[[repositories]]
name = "Order-C"
provider = "github"
username = "example"
repository = "order-c"
channel = "all"
install_target = ""
plugin_guid = ""
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Order-C.zip"

[[repositories]]
name = "Order-A"
provider = "github"
username = "example"
repository = "order-a"
channel = "all"
install_target = ""
plugin_guid = ""
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Order-A.zip"

[[repositories]]
name = "Order-B"
provider = "github"
username = "example"
repository = "order-b"
channel = "all"
install_target = ""
plugin_guid = ""
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Order-B.zip"
"#;
        let bundle = service.validate_bundle(text).unwrap();
        let mut config = default_config();
        service.import_bundle(&mut config, &bundle, "overwrite");
        let names = config
            .repositories
            .iter()
            .filter(|repository| repository.name.starts_with("Order-"))
            .map(|repository| repository.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["Order-C", "Order-A", "Order-B"]);
    }

    #[test]
    fn repository_source_can_declare_child_sources() {
        let text = r#"
[[repository_sources]]
id = "azhe-Others"
nickname = "Azhe的其他MOD仓库源"
url = "https://example.com/others.oc2repo.toml"
builtin = true
"#;
        let location = crate::repository_sources::SourceLocation::parse("source.toml").unwrap();
        let document = crate::repository_sources::parse_document(text, &location).unwrap();
        assert!(document.repositories.is_empty());
        assert_eq!(document.repository_sources.len(), 1);
        assert_eq!(document.repository_sources[0].id, "azhe-Others");
        assert_eq!(document.repository_sources[0].builtin, Some(true));
    }

    #[test]
    fn bundle_round_trip_is_toml() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let config = default_config();
        let text = service.export_bundle(&config, None).unwrap();
        assert!(text.contains("format = \"oc2-mod-repository-bundle\""));
        let bundle = service.validate_bundle(&text).unwrap();
        assert_eq!(bundle.repositories.len(), 0);
        assert_eq!(bundle.repository_sources, config.repository_sources);
    }

    #[test]
    fn community_catalogue_survives_config_reads_and_cannot_be_overwritten_by_import() {
        let verification =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/community-20261013");
        let service = ConfigService::for_test(
            verification.join(format!("catalogue-config-{}", Uuid::new_v4())),
        );
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let template = crate::repository_sources::parse_document(REGION_REPOSITORY, &location)
            .unwrap()
            .repositories
            .remove(0);
        let mut repository = repository_from_export(&template, "community-fixture".into(), false);
        repository.source_id = Some(crate::community::SOURCE_ID.into());
        let mut config = default_config();
        config.repositories.push(repository.clone());
        config
            .repository_sources
            .push(crate::community::source(&Provider::Github));
        config.community_discovery = Some(crate::models::CommunityDiscovery {
            repository_ids: vec![repository.id.clone()],
            total_count: 1,
            ..Default::default()
        });
        service.save(&config).unwrap();

        let mut loaded = service.load().unwrap();
        assert_eq!(loaded.repository_sources, config.repository_sources);
        assert_eq!(loaded.repositories[0].id, repository.id);
        assert_eq!(
            loaded.community_discovery.as_ref().unwrap().repository_ids,
            [repository.id.clone()]
        );
        let text = service.export_bundle(&loaded, None).unwrap();
        let mut bundle = service.validate_bundle(&text).unwrap();
        bundle.repositories[0].note = "Custom imported note".into();
        let preview = service.import_bundle(&mut loaded, &bundle, "overwrite");
        assert!(preview.valid);
        assert_eq!(loaded.repositories.len(), 2);
        let retained = loaded
            .repositories
            .iter()
            .find(|repo| crate::community::is_repository(repo))
            .unwrap();
        assert_eq!(retained.id, repository.id);
        assert_eq!(retained.note, repository.note);
        let imported = loaded
            .repositories
            .iter()
            .find(|repo| !crate::community::is_repository(repo))
            .unwrap();
        assert_ne!(imported.id, repository.id);
        assert!(imported.source_id.is_none());
        assert_eq!(imported.note, "Custom imported note");
        service.save(&loaded).unwrap();
        let reloaded = service.load().unwrap();
        assert_eq!(reloaded.repositories.len(), 2);
        assert_eq!(
            reloaded
                .repository_sources
                .iter()
                .filter(|source| source.id == crate::community::SOURCE_ID)
                .count(),
            1
        );
        let target = fs::canonicalize(&service.root).unwrap();
        assert!(target.starts_with(fs::canonicalize(&verification).unwrap()));
        fs::remove_dir_all(target).unwrap();
    }

    #[test]
    fn public_discovery_hides_late_official_descendants_without_erasing_saved_bindings() {
        let service = ConfigService::for_test(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/verification/community-official-dedup-20261014/config-fixture"),
        );
        for provider in [Provider::Github, Provider::Gitee] {
            let community = crate::community::repository_from_coordinates(
                &provider,
                "Owner",
                "Listed",
                "main",
                "community",
            )
            .unwrap();
            let binding =
                crate::models::RepositoryBinding::from_release(&community, "v1.0.0").unwrap();
            let mut config = default_config();
            let mut child = config.repository_sources[0].clone();
            child.id = "official-child".into();
            child.parent_id = Some(config.repository_sources[0].id.clone());
            child.builtin = false;
            let mut leaf = child.clone();
            leaf.id = "official-leaf".into();
            leaf.parent_id = Some(child.id.clone());
            config.repository_sources.extend([leaf, child]);
            config.repositories.push(community.clone());
            let discovery = crate::models::CommunityDiscovery {
                repository_ids: vec![community.id.clone()],
                total_count: 1,
                ..Default::default()
            };
            match provider {
                Provider::Github => config.community_discovery = Some(discovery),
                Provider::Gitee => config.gitee_community_discovery = Some(discovery),
            }
            let ids = |public: &PublicConfig| {
                match provider {
                    Provider::Github => public.community_discovery.as_ref(),
                    Provider::Gitee => public.gitee_community_discovery.as_ref(),
                }
                .unwrap()
                .repository_ids
                .clone()
            };
            assert_eq!(ids(&service.public_config(&config)), [community.id.clone()]);
            let mut official = community.clone();
            official.id = "official-mod".into();
            official.source_id = Some("official-leaf".into());
            official.username = "OWNER".into();
            official.repository = "LISTED".into();
            official.builtin = false;
            official.enabled = false;
            official.channel = Channel::Stable;
            config.repositories.push(official);
            let before = serde_json::to_value(&config).unwrap();
            let public = service.public_config(&config);
            assert!(ids(&public).is_empty(), "{provider:?}");
            assert_eq!(serde_json::to_value(&config).unwrap(), before);
            assert!(
                public
                    .repositories
                    .iter()
                    .any(|repo| repo.id == community.id
                        && repo.channel == Channel::All
                        && binding.matches_repository(repo))
            );
            config.repositories.retain(|repo| repo.id != "official-mod");
            assert_eq!(ids(&service.public_config(&config)), [community.id]);
        }
    }

    #[test]
    fn export_includes_builtin_disabled_and_other_region_repositories_with_their_sources() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let template = crate::repository_sources::parse_document(REGION_REPOSITORY, &location)
            .unwrap()
            .repositories
            .remove(0);
        let mut config = default_config();
        let builtin_source = config.repository_sources[0].id.clone();
        let other_region = if current_user_region() == REGION_CHINESE_MAINLAND {
            REGION_INTERNATIONAL
        } else {
            REGION_CHINESE_MAINLAND
        };
        config.repository_sources.push(RepositorySource {
            id: "other-region-child".into(),
            nickname: "Child source".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/child.toml".into(),
            region: Some(other_region.into()),
            builtin: true,
            parent_id: Some(builtin_source.clone()),
        });
        let mut builtin = repository_from_export(&template, "builtin".into(), true);
        builtin.name = "Built-in MOD".into();
        builtin.source_id = Some(builtin_source);
        let mut disabled = repository_from_export(&template, "disabled".into(), true);
        disabled.name = "Disabled other-region MOD".into();
        disabled.region = Some(other_region.into());
        disabled.source_id = Some("other-region-child".into());
        disabled.enabled = false;
        let mut detached = repository_from_export(&template, "detached".into(), false);
        detached.name = "Detached MOD".into();
        detached.repository = "detached".into();
        detached.source_id = Some("unavailable-source".into());
        config.repositories = vec![builtin, disabled, detached];

        let text = service.export_bundle(&config, None).unwrap();
        let bundle = service.validate_bundle(&text).unwrap();
        assert_eq!(
            bundle
                .repositories
                .iter()
                .map(|repo| repo.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Built-in MOD", "Disabled other-region MOD", "Detached MOD"]
        );
        assert_eq!(bundle.repository_sources, config.repository_sources);
        assert!(!bundle.repositories[1].enabled);
        assert_eq!(bundle.repositories[1].region.as_deref(), Some(other_region));
        assert_eq!(
            bundle.repositories[1].source_id.as_deref(),
            Some("other-region-child")
        );
        let mut imported = default_config();
        service.import_bundle(&mut imported, &bundle, "overwrite");
        assert_eq!(imported.repositories.len(), 3);

        let text = service
            .export_bundle(&config, Some(vec!["builtin".into()]))
            .unwrap();
        let selected = service.validate_bundle(&text).unwrap();
        assert_eq!(selected.repositories.len(), 1);
        assert_eq!(selected.repositories[0].name, "Built-in MOD");
    }

    #[test]
    fn unsafe_install_target_is_rejected() {
        let result = validate_repository(&RepositoryExport {
            name: "bad".into(),
            name_i18n: std::collections::BTreeMap::new(),
            group: String::new(),
            group_i18n: std::collections::BTreeMap::new(),
            provider: Provider::Github,
            username: "a".into(),
            repository: "b".into(),
            download_mode: RepositoryDownloadMode::Release,
            branch: String::new(),
            file_path: String::new(),
            region: None,
            channel: Channel::Stable,
            asset_rules: vec![AssetRule {
                rule_type: AssetRuleType::Exact,
                value: "a.zip".into(),
            }],
            install_target: "../plugins".into(),
            note: String::new(),
            note_i18n: std::collections::BTreeMap::new(),
            plugin_guid: String::new(),
            local_product_name: String::new(),
            local_original_filename: String::new(),
            version_regex: String::new(),
            max_release_count: 10,
            download_threads: 1,
            enabled: true,
            source_id: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn repository_bundle_export_follows_editor_order_and_roundtrips() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let mut release = crate::repository_sources::parse_document(REGION_REPOSITORY, &location)
            .unwrap()
            .repositories
            .remove(0);
        release.name_i18n.insert("en".into(), "Release MOD".into());
        release.group_i18n.insert("zh-CN".into(), "测试分组".into());
        release.note = "多行备注\n[[repositories]]\n\"quoted\" \\ path".into();
        release.note_i18n.insert("en".into(), release.note.clone());
        release.version_regex = r"^1\.\d+\.\d+$".into();
        release.download_threads = 16;
        let mut raw = release.clone();
        raw.name = "Raw MOD".into();
        raw.download_mode = RepositoryDownloadMode::RepositoryFile;
        raw.branch = "main".into();
        raw.file_path = "bin/Raw MOD.dll".into();
        raw.channel = Channel::All;
        raw.version_regex = r"^1\.2\.\d+(\.\d+)?$".into();
        raw.asset_rules.clear();
        let source = RepositorySource {
            id: "export-source".into(),
            nickname: "自定义源".into(),
            nickname_i18n: std::collections::BTreeMap::from([("en".into(), "Source".into())]),
            url: "https://example.com/source.toml".into(),
            region: None,
            builtin: false,
            parent_id: None,
        };
        release.source_id = Some(source.id.clone());
        raw.source_id = Some(source.id.clone());
        let mut config = default_config();
        config.repository_sources.push(source);
        config.repositories = vec![
            repository_from_export(&release, "release".into(), false),
            repository_from_export(&raw, "raw".into(), false),
        ];
        // The editor keeps drafts for both modes; Release starts with main.
        config.repositories[0].branch = "main".into();
        config.repositories[0].file_path = "bin/UnusedDraft.dll".into();
        let text = service.export_bundle(&config, None).unwrap();
        let restored = service.validate_bundle(&text).unwrap();
        assert_eq!(
            serde_json::to_value(&restored.repositories).unwrap(),
            serde_json::to_value([&release, &raw]).unwrap()
        );
        assert_eq!(restored.repository_sources, config.repository_sources);
        assert_eq!(
            restored.repository_sources.last().unwrap().id,
            "export-source"
        );
        let entries = text
            .split("\n[[repositories]]\n# 仓库基本信息\n")
            .skip(1)
            .collect::<Vec<_>>();
        for (entry, keys) in [
            (
                entries[0],
                vec![
                    "name =",
                    "name_i18n.",
                    "provider =",
                    "group =",
                    "group_i18n.",
                    "username =",
                    "repository =",
                    "enabled =",
                    "download_mode =",
                    "channel =",
                    "max_release_count =",
                    "download_threads =",
                    "asset_rules =",
                    "plugin_guid =",
                    "version_regex =",
                    "install_target =",
                    "note =",
                    "note_i18n.",
                ],
            ),
            (
                entries[1],
                vec![
                    "download_mode =",
                    "branch =",
                    "download_threads =",
                    "file_path =",
                    "plugin_guid =",
                    "local_product_name =",
                    "local_original_filename =",
                    "version_regex =",
                    "install_target =",
                    "note =",
                    "note_i18n.",
                ],
            ),
        ] {
            let positions = keys
                .iter()
                .map(|key| entry.find(key).unwrap_or_else(|| panic!("missing {key}")))
                .collect::<Vec<_>>();
            assert!(positions.windows(2).all(|pair| pair[0] < pair[1]));
        }
        assert!(!entries[1].contains("\nchannel ="));
        assert!(!entries[1].contains("\nasset_rules ="));
        let exported: toml::Value = toml::from_str(&text).unwrap();
        for repository in exported["repositories"].as_array().unwrap() {
            assert!(repository.get("url").is_none());
            assert_eq!(repository["username"].as_str(), Some("example"));
            assert_eq!(repository["repository"].as_str(), Some("mod"));
        }
        assert_eq!(
            exported["repository_sources"]
                .as_array()
                .unwrap()
                .last()
                .unwrap()["url"]
                .as_str(),
            Some("https://example.com/source.toml")
        );
        let parsed = crate::repository_sources::parse_document(&text, &location).unwrap();
        assert_eq!(parsed.repositories.len(), 2);
        assert_eq!(parsed.repositories[0].note, release.note);
        service.import_bundle(&mut config, &restored, "overwrite");
        assert_eq!(
            config
                .repositories
                .iter()
                .map(|repo| repo.id.as_str())
                .collect::<Vec<_>>(),
            ["release", "raw"]
        );
    }

    #[test]
    fn repository_file_mode_does_not_require_release_asset_rules() {
        let valid = r#"
name = "Many Recipes"
provider = "github"
username = "gua248"
repository = "Overcooked2-ManyRecipes"
download_mode = "repository_file"
branch = "master"
file_path = "bin/Release/OC2ManyRecipes.dll"
install_target = "BepInEx/plugins"
enabled = true
"#;
        let repository: RepositoryExport = toml::from_str(valid).unwrap();
        assert!(validate_repository(&repository).is_ok());
        assert_eq!(repository.channel, Channel::All);

        let missing_branch = valid.replace("branch = \"master\"\n", "");
        let repository: RepositoryExport = toml::from_str(&missing_branch).unwrap();
        assert!(validate_repository(&repository).is_err());

        let repository: RepositoryExport = toml::from_str(valid).unwrap();
        for path in ["bin/*.dll", "bin/MOD?.dll", "../MOD.dll", "bin/./MOD.dll"] {
            let mut invalid = repository.clone();
            invalid.file_path = path.into();
            assert!(validate_repository(&invalid).is_err(), "accepted {path}");
        }
        let mut invalid = repository;
        invalid.asset_rules.push(AssetRule {
            rule_type: AssetRuleType::Regex,
            value: ".*\\.dll".into(),
        });
        assert!(validate_repository(&invalid).is_err());
    }

    #[test]
    fn structured_repository_fields_survive_storage_and_public_config_without_urls() {
        let service = ConfigService {
            root: std::env::temp_dir(),
        };
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let export = crate::repository_sources::parse_document(REGION_REPOSITORY, &location)
            .unwrap()
            .repositories
            .remove(0);
        let mut config = default_config();
        config.repositories = vec![repository_from_export(&export, "structured".into(), false)];
        let stored = toml::to_string_pretty(&config).unwrap();
        let restored: AppConfig = toml::from_str(&stored).unwrap();
        let repo = &restored.repositories[0];
        assert_eq!(repo.username, "example");
        assert_eq!(repo.repository, "mod");
        assert_eq!(repo.channel, Channel::Stable);
        let public = serde_json::to_value(service.public_config(&restored)).unwrap();
        let public_repo = &public["repositories"][0];
        assert_eq!(public_repo["username"], "example");
        assert_eq!(public_repo["repository"], "mod");
        assert!(public_repo.get("url").is_none());
        assert!(public_repo.get("owner").is_none());
        let serialized_repo = serde_json::to_value(repo).unwrap();
        assert!(serialized_repo.get("url").is_none());
        let old_format = REGION_REPOSITORY.replace(
            "username = \"example\"\nrepository = \"mod\"",
            "url = \"https://github.com/example/mod\"",
        );
        assert!(crate::repository_sources::parse_document(&old_format, &location).is_err());
    }

    #[test]
    fn repository_identity_uses_provider_username_repository_and_region() {
        let github =
            repository_address_key(&Provider::Github, " Example ", "MOD", Some("international"));
        assert_eq!(
            github,
            repository_address_key(&Provider::Github, "example", "mod", Some("international"))
        );
        assert_ne!(
            github,
            repository_address_key(&Provider::Gitee, "example", "mod", Some("international"))
        );
        assert_ne!(
            github,
            repository_address_key(
                &Provider::Github,
                "example",
                "mod",
                Some("chinese_mainland")
            )
        );
        assert_ne!(
            github,
            repository_address_key(&Provider::Github, "other", "mod", Some("international"))
        );
        assert_ne!(
            github,
            repository_address_key(
                &Provider::Github,
                "example",
                "another",
                Some("international")
            )
        );
    }
}
