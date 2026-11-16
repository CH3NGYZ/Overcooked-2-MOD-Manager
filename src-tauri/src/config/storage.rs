//! 共享配置缓存、文件读写和恢复；代理快照仍由同一实例提供。

use super::CachedConfig;
use super::ConfigError;
use super::ConfigService;
use super::defaults::default_config;
use super::metadata::migrate_repository_i18n;
use super::migrations::contains_legacy_repository_proxy_fields;
use super::migrations::migrate_schema;
use super::sources::synchronize_builtin_repository_sources;
use crate::models::*;
use chrono::Utc;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::sync::MutexGuard;

const CONFIG_FILE: &str = "config.toml";

#[cfg(test)]
mod network_tests;
#[cfg(test)]
mod region_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
mod transaction;

impl ConfigService {
    pub fn new() -> Result<Self, ConfigError> {
        let root = crate::user_paths::root().map_err(ConfigError::Directory)?;
        fs::create_dir_all(&root).map_err(|e| {
            log::error!("创建配置目录失败：目录={}，原因={}", root.display(), e);
            // 带上完整路径：数据根已经不在安装目录里，用户需要知道到底是
            // 哪个位置不可写（OneDrive 占位、只读的文档目录等）。
            ConfigError::Directory(format!("{}: {e}", root.display()))
        })?;
        Self::open(root)
    }

    /// Initialize from disk before the service is exposed to startup network
    /// consumers. Ordinary loads must never reset a manual session selection.
    fn open(root: PathBuf) -> Result<Self, ConfigError> {
        let service = Self {
            root,
            storage_lock: Default::default(),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
            proxies: Default::default(),
        };
        let guard = service
            .storage_lock
            .lock()
            .map_err(|_| ConfigError::Read("lock.poisoned".into()))?;
        match service.load_from_disk(&guard, true) {
            Ok(config) => service.store_cache(&service.config_path(), &config),
            // Recovery already wrote a valid default. Preserve the existing
            // recoverable-startup behavior without hiding read/write failures.
            Err(ConfigError::Parse(_)) => (),
            Err(error) => return Err(error),
        }
        drop(guard);
        log::debug!("配置服务初始化完成：目录={}", service.root.display());
        Ok(service)
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
        // Keep the fingerprint/read/cache update in the same transaction as save:
        // otherwise readers can observe the gap between target and previous.
        let guard = self
            .storage_lock
            .lock()
            .map_err(|_| ConfigError::Read("lock.poisoned".into()))?;
        let path = self.config_path();
        // 文件指纹没变 → 直接返回缓存副本，避免重复读盘与 TOML 解析
        if let Some(cached) = self.cached_if_fresh(&path) {
            return Ok(self.with_runtime(cached));
        }
        let config = self.load_from_disk(&guard, false)?;
        self.store_cache(&path, &config);
        Ok(self.with_runtime(config))
    }

    pub(super) fn with_runtime(&self, mut config: AppConfig) -> AppConfig {
        config.network.normalize();
        match self.proxies.configuration_snapshot() {
            Ok((provider, proxies, policy, region)) => {
                config.region = region;
                if config.data_provider != provider {
                    config.data_provider = provider;
                    synchronize_builtin_repository_sources(&mut config);
                }
                config.github_proxies = proxies;
                config.network_policy = Some(policy);
            }
            Err(error) => log::error!("选择网络快照失败：{error}"),
        }
        config
    }

    #[cfg(test)]
    pub(crate) fn select_data_provider(&self, provider: DataProvider) {
        self.proxies.select_provider(provider, None);
    }

    pub(crate) fn set_source_browser(&self, browser: tauri::AppHandle) {
        self.proxies.set_source_browser(browser);
    }

    pub(crate) fn data_selection(&self) -> (DataProvider, u64) {
        self.proxies.selection()
    }

    pub(crate) fn data_selection_changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.proxies.selection_changes()
    }

    pub(crate) async fn official_root(
        &self,
        force: bool,
    ) -> Result<crate::github_proxy_runtime::OfficialRootSnapshot, String> {
        let network = self.network_snapshot();
        self.proxies.official_root(force, &network).await
    }

    pub(crate) fn in_data_selection<T>(
        &self,
        selection: (DataProvider, u64),
        work: impl std::future::Future<Output = Result<T, String>>,
    ) -> impl std::future::Future<Output = Result<T, String>> {
        self.proxies.in_selection(selection, work)
    }

    pub(crate) async fn refresh_proxies(&self) -> Result<(), String> {
        let network = self.network_snapshot();
        self.proxies.refresh_policy(&network).await
    }

    // Startup diagnostics must not create or migrate configuration. Production
    // open/load/save already refresh this cache before exposing configuration.
    fn network_snapshot(&self) -> NetworkConfig {
        self.cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .as_ref()
            .map(|cached| cached.config.network.clone())
            .unwrap_or_default()
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

    fn initialize_region(&self, config: &mut AppConfig, initialize_session: bool) -> bool {
        let mut changed = false;
        let normalized = super::availability::normalize_repository_region(Some(&config.region));
        if normalized != config.region || normalized.is_empty() {
            config.region = if normalized.is_empty() {
                if initialize_session {
                    super::availability::detect_system_region().into()
                } else {
                    self.proxies.region()
                }
            } else {
                normalized
            };
            changed = true;
        }
        if initialize_session {
            self.proxies.select_provider(
                DataProvider::for_region(&config.region),
                Some(&config.region),
            );
        }
        changed
    }

    fn initialized_default(&self, initialize_session: bool) -> AppConfig {
        let mut config = default_config();
        if initialize_session {
            // The template is deliberately deterministic; only a first disk
            // initialization uses the system, not every default construction.
            config.region.clear();
            self.initialize_region(&mut config, true);
        }
        self.with_runtime(config)
    }

    fn load_from_disk(
        &self,
        guard: &MutexGuard<'_, ()>,
        initialize_session: bool,
    ) -> Result<AppConfig, ConfigError> {
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
            let config = self.initialized_default(initialize_session);
            self.save_locked(&config, guard)?;
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
                changed |= config.network.normalize();
                changed |= self.initialize_region(&mut config, initialize_session);
                config.data_provider = self.proxies.provider();
                changed |= migrate_schema(&mut config);
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
                // 仅内置来源从随包树补历史昵称；远端等待自身源码包刷新，
                // 避免启动阶段把两个来源的业务元数据混在一起。
                if config.data_provider == DataProvider::Builtin {
                    changed |=
                        crate::map_sources::fill_missing_source_names(&mut config.map_repositories);
                }
                if changed {
                    log::info!("配置需要迁移或同步，保存更新后的配置");
                    self.save_locked(&config, guard)?;
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
                let config = self.initialized_default(initialize_session);
                self.save_locked(&config, guard)?;
                Err(ConfigError::Parse(error.to_string()))
            }
        }
    }

    pub fn load_or_default(&self) -> AppConfig {
        self.load().unwrap_or_else(|error| {
            log::error!("配置加载失败，使用默认配置：原因={}", error);
            self.with_runtime(default_config())
        })
    }

    pub fn save(&self, config: &AppConfig) -> Result<(), ConfigError> {
        let guard = self
            .storage_lock
            .lock()
            .map_err(|_| ConfigError::Write("lock.poisoned".into()))?;
        // Other settings and stale snapshots cannot write an old user region
        // back over a completed selection. Region changes use save_with_selection.
        self.save_locked(&self.with_runtime(config.clone()), &guard)
    }

    // load owns the same transaction while it migrates or creates defaults;
    // accepting its guard prevents accidentally trying to acquire it twice.
    pub(super) fn save_locked(
        &self,
        config: &AppConfig,
        _guard: &MutexGuard<'_, ()>,
    ) -> Result<(), ConfigError> {
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
        let target = self.config_path();
        transaction::write_config(&self.root, &target, text.as_bytes())
            .map_err(|error| ConfigError::Write(error.to_string()))?;
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
