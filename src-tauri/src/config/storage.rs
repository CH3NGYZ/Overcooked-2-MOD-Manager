//! 共享配置缓存、文件读写和恢复；代理快照仍由同一实例提供。

use super::CachedConfig;
use super::ConfigError;
use super::ConfigService;
use super::availability::current_user_region;
use super::defaults::default_config;
use super::metadata::migrate_repository_i18n;
use super::migrations::contains_legacy_repository_proxy_fields;
use super::migrations::migrate_bundled_source_switches;
use super::sources::synchronize_builtin_repository_sources;
use crate::models::*;
use chrono::Utc;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use uuid::Uuid;

const CONFIG_FILE: &str = "config.toml";

#[cfg(test)]
mod test_support;

impl ConfigService {
    pub fn new() -> Result<Self, ConfigError> {
        let root = crate::user_paths::root().map_err(ConfigError::Directory)?;
        fs::create_dir_all(&root).map_err(|e| {
            log::error!("创建配置目录失败：目录={}，原因={}", root.display(), e);
            // 带上完整路径：数据根已经不在安装目录里，用户需要知道到底是
            // 哪个位置不可写（OneDrive 占位、只读的文档目录等）。
            ConfigError::Directory(format!("{}: {e}", root.display()))
        })?;
        log::debug!("配置服务初始化完成：目录={}", root.display());
        Ok(Self {
            root,
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
            proxies: Default::default(),
        })
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
        // 文件指纹没变 → 直接返回缓存副本，避免重复读盘与 TOML 解析
        if let Some(cached) = self.cached_if_fresh(&path) {
            return Ok(self.with_proxies(cached));
        }
        let config = self.load_from_disk()?;
        self.store_cache(&path, &config);
        Ok(self.with_proxies(config))
    }

    fn with_proxies(&self, mut config: AppConfig) -> AppConfig {
        config.github_proxies = self.proxy_snapshot(config.builtin_proxies_only);
        config
    }

    pub(crate) fn proxy_snapshot(&self, builtin_only: bool) -> GitHubProxyConfig {
        if builtin_only {
            crate::github_proxies::bundled_defaults()
        } else {
            self.proxies.snapshot()
        }
    }

    pub(crate) async fn refresh_proxies(&self) -> Result<(), String> {
        // Select before creating a client or resolving any remote manifest. Readers
        // also select by this flag, so an older in-flight refresh cannot override it.
        if self.load_or_default().builtin_proxies_only {
            return Ok(());
        }
        self.proxies.refresh(current_user_region()).await
    }

    fn cached_if_fresh(&self, path: &Path) -> Option<AppConfig> {
        let metadata = fs::metadata(path).ok()?;
        let cache = self.cache.lock().ok()?;
        let cached = cache.as_ref()?;
        let same_time = cached.modified == metadata.modified().ok();
        let same_size = cached.size == metadata.len();
        (same_time && same_size).then(|| cached.config.clone())
    }

    fn store_cache(&self, path: &Path, config: &AppConfig) {
        let Ok(metadata) = fs::metadata(path) else {
            return;
        };
        if let Ok(mut cache) = self.cache.lock() {
            *cache = Some(CachedConfig {
                modified: metadata.modified().ok(),
                size: metadata.len(),
                config: config.clone(),
            });
        }
    }

    fn load_from_disk(&self) -> Result<AppConfig, ConfigError> {
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
                // 先迁移开关，再同步根源地址：同步要按迁移后的开关选内置/远端。
                changed |= migrate_bundled_source_switches(&mut config);
                changed |= synchronize_builtin_repository_sources(&mut config);
                changed |= Self::normalize_configured_regions(&mut config);
                for repository in &mut config.repositories {
                    changed |= migrate_repository_i18n(repository);
                    let normalized = repository.max_release_count.clamp(1, 100);
                    if repository.max_release_count != normalized {
                        repository.max_release_count = normalized;
                        changed = true;
                    }
                }
                // 地图条目的源昵称是后加的字段：老配置里只有源 ID，
                // 不补的话地图页的三层标题会显示成 `maps-rw` 这类 ID
                changed |=
                    crate::map_sources::fill_missing_source_names(&mut config.map_repositories);
                if changed {
                    log::info!("配置需要迁移或同步，保存更新后的配置");
                    self.save(&config)?;
                }
                // 走到这里说明真的读了盘：用 info 记录一次，便于排查迁移/同步问题。
                // 命中缓存的高频读取不再重复打印（见 load()）。
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

    pub fn load_or_default(&self) -> AppConfig {
        self.load().unwrap_or_else(|error| {
            log::error!("配置加载失败，使用默认配置：原因={}", error);
            self.with_proxies(default_config())
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
        // 自己刚写入的内容就是最新值：同步缓存，避免下一次 load 再次读盘
        self.store_cache(&target, config);
        Ok(())
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
}
