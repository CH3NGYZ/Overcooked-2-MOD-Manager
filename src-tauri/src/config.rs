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
    #[cfg(target_os = "macos")]
    {
        crate::user_region::current()
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        REGION_INTERNATIONAL
    }
}

pub fn validate_repository_source_region(region: Option<&str>) -> Result<(), ConfigError> {
    let _ = normalize_repository_region_for_storage(region, "config.labelSource");
    Ok(())
}

pub fn validate_repository_region(region: Option<&str>) -> Result<(), ConfigError> {
    validate_repository_source_region(region)
        .map_err(|_| ConfigError::Invalid("config.invalidRegion".into()))
}

fn region_matches(configured_region: Option<&str>, region: &str) -> bool {
    match normalize_repository_region(configured_region).as_str() {
        "" => true,
        REGION_CHINESE_MAINLAND => region == REGION_CHINESE_MAINLAND,
        REGION_INTERNATIONAL => region != REGION_CHINESE_MAINLAND,
        _ => true,
    }
}

/// 支持的桌面平台名：只有两个取值，其余一律当作「全平台」。
pub const PLATFORM_WINDOWS: &str = "windows";
pub const PLATFORM_MACOS: &str = "macos";

/// 归一化仓库的 `platform` 字段。
///
/// 空值、缺失、无法识别的值都归为 `None`（全平台可用）——与 `region`
/// 的处理完全一致：配置写错不该让整个仓库源解析失败，只记一条日志。
pub(crate) fn normalize_repository_platform(platform: Option<&str>) -> Option<String> {
    let Some(value) = platform.map(str::trim).filter(|value| !value.is_empty()) else {
        return None;
    };
    match value.to_ascii_lowercase().as_str() {
        PLATFORM_WINDOWS | "win" => Some(PLATFORM_WINDOWS.into()),
        PLATFORM_MACOS | "mac" | "osx" | "darwin" => Some(PLATFORM_MACOS.into()),
        unknown => {
            log::warn!("仓库平台无法识别，按全平台处理：值={}", unknown);
            None
        }
    }
}

/// 当前运行平台（取值与 `normalize_repository_platform` 一致）。
pub fn current_platform() -> &'static str {
    if cfg!(target_os = "macos") {
        PLATFORM_MACOS
    } else {
        PLATFORM_WINDOWS
    }
}

/// 仓库是否适用于当前平台。`None` 表示全平台可用。
pub(crate) fn platform_matches(configured: Option<&str>, platform: &str) -> bool {
    match normalize_repository_platform(configured) {
        None => true,
        Some(value) => value == normalize_repository_platform(Some(platform)).unwrap_or_default(),
    }
}

/// 校验 `platform` 原文：允许空/缺失（全平台），但写了就必须是已知平台。
///
/// 与 region 的宽容策略不同：region 的未知值只 warn 并按全部地区处理
/// （社区源里可能写各种别名），而 platform 只有两个合法值，写错几乎一定
/// 是笔误 —— 静默当成「全平台」会让本该只给 macOS 的 MOD 出现在 Windows
/// 列表里，所以在导入阶段直接拒绝。
///
/// 注意：**必须在归一化之前调用**。先归一化会把非法值变成 None，校验就
/// 永远看不到它了。
pub(crate) fn validate_repository_platform(
    platform: Option<&str>,
    repository_name: &str,
) -> Result<(), ConfigError> {
    if let Some(value) = platform.map(str::trim).filter(|value| !value.is_empty())
        && normalize_repository_platform(Some(value)).is_none()
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.invalidPlatform",
            &[("repo", repository_name), ("value", value)],
        )));
    }
    Ok(())
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
    /*
     * 来源自身的地区是唯一条件。社区源（GitHub / Gitee）默认不限地区，
     * 所以这里不再有「Gitee 社区仅中国大陆」的特例。
     */
    region_matches(source.region.as_deref(), region)
}

pub(crate) fn repository_matches_channel(
    config: &AppConfig,
    repository: &RepositoryConfig,
    region: &str,
) -> bool {
    region_matches(repository.region.as_deref(), region)
        // 平台不适用与地区不适用同义：仓库在当前环境「不存在」
        && platform_matches(repository.platform.as_deref(), current_platform())
        && repository.source_id.as_deref().is_none_or(|source_id| {
            config.repository_sources.iter().any(|source| {
                source.id == source_id && repository_source_matches_region(source, region)
            })
        })
}

/// 仓库在当前地区「是否存在」：存在才展示、刷新、安装和参与更新检测。
///
/// 这是唯一的判定入口，前端 `repositoryIsAvailable` 与它逐条对应：
///   - `enabled=false`：用户关闭，或被官方仓库源覆盖的社区副本；
///   - 地区不适用：仓库自身地区、或所属源地区与当前地区不符。
///
/// 地区不适用与 `enabled=false` 取同一语义（整个不存在），而不是
/// 「存在但不显示」——否则前端会替一个用户看不见的仓库发请求，
/// 失败时还会弹出一条无从处理的错误。
pub(crate) fn repository_is_available(
    config: &AppConfig,
    repository: &RepositoryConfig,
    region: &str,
) -> bool {
    repository.enabled && repository_matches_channel(config, repository, region)
}

/// 按界面语言从 `name_i18n` 取仓库名，缺失时回退到中文原名。
///
/// 语言标记来自前端（`zh-CN` / `zh-TW` / `en` / `ko` / `ja`），因为界面
/// 语言由前端 localStorage 管理、后端配置里不存这一项。
///
/// 查找顺序：完整标记 → 主语言前缀 → `name`。
/// 加前缀回退是因为源文档里可能只写了 `zh` 或 `en-US` 这类宽标记，
/// 而界面传来的可能是 `zh-CN`，两者应当能对上。
pub(crate) fn localized_repository_name(repo: &RepositoryConfig, locale: Option<&str>) -> String {
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

/// 配置读写失败。
///
/// `#[error]` 的值是**给 thiserror 的模板**，不是普通字符串：末尾的 `: {0}`
/// 必须保留，否则会把具体原因（哪个文件、哪一行解析失败）整个丢掉 ——
/// 用户只看到「配置格式无效」却不知道哪里错了，日志里也查不到。
///
/// 前端翻译时按 `error.<域>.<原因>` 的约定识别 tag 前缀，冒号后面是
/// 参数（技术细节，原样展示）。
#[derive(Debug, Clone, thiserror::Error)]
pub enum ConfigError {
    #[error("config.directoryUnavailableReason: {0}")]
    Directory(String),
    #[error("config.readFailedReason: {0}")]
    Read(String),
    #[error("config.parseFailedReason: {0}")]
    Parse(String),
    #[error("config.writeFailedReason: {0}")]
    Write(String),
    #[error("config.invalidFormatReason: {0}")]
    Invalid(String),
}

#[derive(Clone)]
pub struct ConfigService {
    root: PathBuf,
    /// 上次读取的配置快照 + 文件指纹（Arc 让 Clone 共享同一份缓存）。
    ///
    /// 一次启动里同一份配置会被读上百次（每个仓库刷新、每次 Dashboard 读取都要读一次），
    /// 每次都重新读盘并解析 TOML 完全是浪费。文件 mtime/大小没变时直接复用。
    /// 保存时同步更新缓存，所以自己的写入不会读到旧值；
    /// 外部改动（用户手改 config.toml）会改变 mtime，缓存自动失效。
    cache: std::sync::Arc<std::sync::Mutex<Option<CachedConfig>>>,
}

impl std::fmt::Debug for ConfigService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigService")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

struct CachedConfig {
    modified: Option<std::time::SystemTime>,
    size: u64,
    config: AppConfig,
}

impl ConfigService {
    #[cfg(test)]
    pub(crate) fn for_test(root: PathBuf) -> Self {
        Self {
            root,
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        }
    }

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
            return Ok(cached);
        }
        let config = self.load_from_disk()?;
        self.store_cache(&path, &config);
        Ok(config)
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
                changed |=
                    crate::github_proxies::synchronize_bundled_defaults(&mut config.github_proxies);
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

    fn normalize_configured_regions(config: &mut AppConfig) -> bool {
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
        // 自己刚写入的内容就是最新值：同步缓存，避免下一次 load 再次读盘
        self.store_cache(&target, config);
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
            hide_unidentified_mods: config.hide_unidentified_mods,
            recycle_deleted_mods: config.recycle_deleted_mods,
            mod_metadata_concurrency: config.mod_metadata_concurrency,
            map_info_concurrency: config.map_info_concurrency,
            tutorial_watched_steps: config.tutorial_watched_steps.clone(),
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
            builtin_sources_only: config.builtin_sources_only,
            builtin_map_sources_only: config.builtin_map_sources_only,
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
        let mut selected = config
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
            return Err(ConfigError::Invalid("source.tooLarge".into()));
        }
        let mut bundle: RepositoryBundle = toml::from_str(text).map_err(|e| {
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
                     * `repository_from_export` 造出来的是全新对象，缓存字段
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
                    *existing = repository_from_export(&item, id, builtin);
                    existing.cached_releases = cached_releases;
                    existing.releases_cached_at = releases_cached_at;
                    existing.last_error = last_error;
                    imported_ids.push(existing.id.clone());
                }
                continue;
            }
            let same_region = |repo: &RepositoryConfig| {
                normalize_repository_region(repo.region.as_deref())
                    == normalize_repository_region(item.region.as_deref())
            };
            let same_name = |repo: &RepositoryConfig, name: &str| {
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

/// 默认配置。
///
/// 测试也需要一份结构完整的 `AppConfig`（字段多且部分无 serde 默认值），
/// 所以对 crate 内可见 —— 复制一份字段列表到测试里，新增字段时就会漏。
/// 配置文件的当前结构版本。
///
/// 只在「需要区分旧写法」时递增：加载时按版本号跑一次性迁移，然后把新版本
/// 写回，迁移不会重复执行。历史：1 = V2 初期两个 `builtin_*_sources_only`
/// 默认开启；2 = 两者改为默认关闭，跟随远端索引。
pub(crate) const CONFIG_SCHEMA_VERSION: u32 = 2;

pub(crate) fn default_config() -> AppConfig {
    AppConfig {
        schema_version: CONFIG_SCHEMA_VERSION,
        ui_zoom: 1.5,
        hide_unidentified_mods: true,
        // 默认开启：删除的 MOD 先进回收站，用户能反悔。
        recycle_deleted_mods: true,
        // MOD 元数据默认跑满全部核心；地图 info 默认 4（常量在 metadata_read）。
        mod_metadata_concurrency: crate::metadata_read::AUTO_CONCURRENCY,
        map_info_concurrency: crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY,
        // 全新配置没看过任何教程步骤，首次启动会完整播一遍。
        tutorial_watched_steps: Vec::new(),
        game: GameConfig {
            path: String::new(),
            // 与 serde 的字段默认值同一处定义：macOS 默认也走原生。
            runtime: GameRuntime::default(),
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
        /*
         * 新配置的官方根源按地区指向远端索引，与下面对应的开关保持同一个状态。
         *
         * 两个「仅使用内置源」开关默认关闭：内置快照退居为远程索引读不到时的
         * 回退数据（见 `refresh_sources` / `refresh_map_sources`），正常情况下
         * 用远端的最新条目。
         */
        repository_sources: builtin_repository_sources_for_region(current_user_region(), false),
        community_discovery: None,
        gitee_community_discovery: None,
        source_repository_ids: Default::default(),
        staged_release_cache: Default::default(),
        /*
         * 新配置直接带上内置地图快照。
         *
         * 放在这里而不是「第一次读地图列表时补」：`get_maps` 是只读命令，
         * 在里面写盘会绕开短操作锁，与「整份配置读取、修改与保存须持锁」
         * 的边界冲突。内置快照是编译期资源，同步构造没有副作用。
         *
         * 快照不再是默认数据来源：`builtin_map_sources_only` 默认关闭，刷新
         * 先读远端索引，失败时才用这份快照兜底。
         */
        map_repositories: crate::map_sources::bundled_map_tree(),
        builtin_map_sources_only: false,
        github_proxies: crate::github_proxies::bundled_defaults(),
        mod_notes: Vec::new(),
        builtin_sources_only: false,
    }
}

fn migrate_repository_i18n(repository: &mut RepositoryConfig) -> bool {
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

/// 官方根源的地址：打开「仅用内置仓库源」时直接指向打包资源。
///
/// `repositories.toml` 是内置索引的路径，`SourceLocation` 会把它解析成
/// 嵌入资源 —— 也就是回退时用的那一份。所以这个开关不需要第二套加载
/// 逻辑，只是把源头换成同一棵树。
pub(crate) fn builtin_repository_sources_for_region(
    region: &str,
    builtin_only: bool,
) -> Vec<RepositorySource> {
    // Only the entry point is shipped with the application. Its children are
    // file-relative names inside that index, so they resolve to bundled
    // resources as well; the live index points them at remote raw URLs.
    vec![RepositorySource {
        id: crate::repository_sources::DEFAULT_INDEX_ID.into(),
        nickname: "官方仓库源".into(),
        nickname_i18n: [
            ("zh-CN".into(), "官方仓库源".into()),
            ("zh-TW".into(), "官方倉庫來源".into()),
            ("en".into(), "Official repositories".into()),
            ("ko".into(), "공식 저장소 소스".into()),
            ("ja".into(), "公式リポジトリソース".into()),
        ]
        .into(),
        url: if builtin_only {
            crate::repository_sources::BUILTIN_INDEX.into()
        } else {
            crate::repository_sources::default_index_url(region).into()
        },
        region: None,
        builtin: true,
        parent_id: None,
    }]
}

/// 把「仅使用内置源」两个开关从旧的默认值 `true` 迁到 `false`。
///
/// V2 早期把仓库与地图条目只写进内置快照，两个开关默认开启、并被原样写进
/// 配置文件。改 serde 默认值与 `default_config()` 只能影响全新配置 —— 老用户
/// 的文件里仍写着 `true`，看起来像「默认没改」。这里按 `schema_version` 做
/// 一次性迁移，之后用户自己再打开不会被覆盖。
fn migrate_bundled_source_switches(config: &mut AppConfig) -> bool {
    if config.schema_version >= CONFIG_SCHEMA_VERSION {
        return false;
    }
    config.schema_version = CONFIG_SCHEMA_VERSION;
    config.builtin_sources_only = false;
    config.builtin_map_sources_only = false;
    true
}

fn synchronize_builtin_repository_sources(config: &mut AppConfig) -> bool {
    merge_builtin_repository_sources(
        config,
        builtin_repository_sources_for_region(current_user_region(), config.builtin_sources_only),
    )
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
    // replaced by the ordinary source loader on the next read.
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

/// Static source and bundle entries must declare Chinese values explicitly.
/// Raw fields remain compatibility mirrors and never determine display values.
pub(crate) fn normalize_repository_i18n(repo: &mut RepositoryExport) -> Result<(), ConfigError> {
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

fn repository_name_from_i18n(translations: &std::collections::BTreeMap<String, String>) -> &str {
    translations
        .get("zh-CN")
        .map(|name| name.trim())
        .unwrap_or_default()
}

fn migrate_repository_export_i18n(repo: &mut RepositoryExport) {
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

pub(crate) fn validate_repository(repo: &RepositoryExport) -> Result<(), ConfigError> {
    validate_repository_region(repo.region.as_deref())?;
    validate_repository_platform(repo.platform.as_deref(), &repo.name)?;
    crate::releases::repository_coordinates(&repo.username, &repo.repository).map_err(|error| {
        ConfigError::Invalid(crate::locale_tag::with_params(
            "config.homepageInvalid",
            &[("repo", &repo.name), ("reason", &error.to_string())],
        ))
    })?;
    if Path::new(&repo.install_target).is_absolute()
        || repo
            .install_target
            .split(['/', '\\'])
            .any(|part| part == "..")
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.installTargetUnsafe",
            &[("repo", &repo.name.to_string())],
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
                return Err(ConfigError::Invalid("config.assetRuleRequired".into()));
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
                    "config.fileModeExactAssetsOnly".into(),
                ));
            }
        }
    }
    if !repo.version_regex.trim().is_empty()
        && regex::Regex::new(repo.version_regex.trim()).is_err()
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.versionRegexInvalid",
            &[("repo", &repo.name.to_string())],
        )));
    }
    /*
     * 关系声明校验。
     *
     * GUID 是关系的唯一标识，空值或纯空白的声明永远匹配不上任何插件，
     * 等于一条静默失效的配置 —— 在导入时就拒绝，比等到界面上不提示更容易发现。
     * `min_version`/`max_version` 若填写，必须是能解析的版本号；
     * 否则「是否满足」永远判不出来。
     */
    for relation in &repo.relations {
        validate_relation(relation, &repo.name, &repo.plugin_guid)?;
    }
    /*
     * 按 Release 覆盖的关系表也要校验，且区间不能两端全空。
     *
     * 两端全空意味着「匹配任何版本」，那和在 `relations` 里直接写没有区别，
     * 但作者会以为自己限定了范围。在导入阶段拒绝，比等它悄悄生效好。
     */
    for override_entry in &repo.release_relations {
        let min = override_entry.min_version.trim();
        let max = override_entry.max_version.trim();
        if min.is_empty() && max.is_empty() {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "config.relationsEmpty",
                &[("repo", &repo.name.to_string())],
            )));
        }
        for (label, value) in [("min_version", min), ("max_version", max)] {
            if !value.is_empty()
                && crate::releases::compare_release_versions(value, value).is_none()
            {
                return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                    "config.relationsValueInvalid",
                    &[("repo", &repo.name), ("label", label), ("value", value)],
                )));
            }
        }
        if !min.is_empty() && !max.is_empty() {
            // 区间反了（min > max）永远匹配不到任何版本，属于配置错误
            if crate::releases::compare_release_versions(min, max)
                == Some(std::cmp::Ordering::Greater)
            {
                return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                    "config.relationsRangeInvalid",
                    &[("repo", &repo.name), ("min", min), ("max", max)],
                )));
            }
        }
        for relation in &override_entry.relations {
            validate_relation(relation, &repo.name, &repo.plugin_guid)?;
        }
    }
    Ok(())
}

/// 单条关系声明的校验：GUID 必填，版本边界若填写必须可解析。
///
/// `optional` 只对 `requires` 有意义 —— 「可选的冲突」自相矛盾，
/// 写出来只会让前端不知道该怎么提示，所以在导入阶段就拒绝。
/// `prerelease` 反过来只对冲突/兼容有意义：依赖要求「必须是测试版」
/// 不是作者会想表达的事，真要那样直接写 `min_version` 更清楚。
///
/// **指向自己 GUID 的冲突/兼容必须写 `prerelease`**：那是「正式版与测试版
/// 互斥」的表达方式（两条发布线同 GUID）。不写标志时它等价于「我不能和
/// 自己共存」，永远为真 —— 这种配置一定写错了，导入阶段就拒绝。
fn validate_relation(
    relation: &RepositoryRelation,
    repository_name: &str,
    owner_guid: &str,
) -> Result<(), ConfigError> {
    let kind = relation.kind.as_str();
    if relation.guid.trim().is_empty() {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationMissingGuid",
            &[("repo", repository_name), ("kind", kind)],
        )));
    }
    if relation.optional && relation.kind != RelationKind::Requires {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationOptionalUnsupported",
            &[
                ("repo", repository_name),
                ("kind", kind),
                ("guid", relation.guid.trim()),
            ],
        )));
    }
    if relation.prerelease.is_some() && relation.kind == RelationKind::Requires {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationRequiresPrerelease",
            &[("repo", repository_name), ("guid", relation.guid.trim())],
        )));
    }
    if relation.kind != RelationKind::Requires
        && relation.prerelease.is_none()
        && !owner_guid.trim().is_empty()
        && relation.guid.trim().eq_ignore_ascii_case(owner_guid.trim())
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationSelfGuid",
            &[("repo", repository_name), ("kind", kind)],
        )));
    }
    for (label, value) in [
        ("min_version", relation.min_version.trim()),
        ("max_version", relation.max_version.trim()),
    ] {
        if !value.is_empty() && crate::releases::compare_release_versions(value, value).is_none() {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "config.relationValueInvalid",
                &[
                    ("repo", repository_name),
                    ("kind", kind),
                    ("guid", relation.guid.trim()),
                    ("label", label),
                    ("value", value),
                ],
            )));
        }
    }
    let min = relation.min_version.trim();
    let max = relation.max_version.trim();
    if !min.is_empty()
        && !max.is_empty()
        && crate::releases::compare_release_versions(min, max) == Some(std::cmp::Ordering::Greater)
    {
        return Err(ConfigError::Invalid(crate::locale_tag::with_params(
            "config.relationRangeInvalid",
            &[
                ("repo", repository_name),
                ("kind", kind),
                ("guid", relation.guid.trim()),
                ("min", min),
                ("max", max),
            ],
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
        "{:?}|{}|{}|{:?}|{:?}|{}|{}|{:?}|{}|{}|{}",
        repo.provider,
        repo.username.trim().to_ascii_lowercase(),
        repo.repository.trim().to_ascii_lowercase(),
        repo.download_mode,
        // Stable and Beta entries of one repository are separate install
        // targets, so the channel belongs to the repository identity.
        repo.channel,
        branch,
        file_path,
        asset_rules,
        repo.install_target,
        repo.enabled,
        normalize_repository_region(repo.region.as_deref())
    )
}

/// 从导出模型构造仓库配置。
///
/// `pub(crate)` 是因为地图仓库要借它做 Release 抓取投影（见
/// `map_sources::release_probe`）：两份模型都需要「一个能喂给
/// `fetch_releases` 的完整 RepositoryConfig」，而字段默认值只有这里知道。
pub(crate) fn repository_from_export(
    repo: &RepositoryExport,
    id: String,
    builtin: bool,
) -> RepositoryConfig {
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
            &crate::locale_tag::with_params(
                "config.repositoryLabeled",
                &[("name", &repo.name.to_string())],
            ),
        ),
        platform: normalize_repository_platform(repo.platform.as_deref()),
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
        relations: repo.relations.clone(),
        release_relations: repo.release_relations.clone(),
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
        releases_cached_at: None,
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
name_i18n.zh-CN = "Regional MOD"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
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

    /*
     * 平台限定的解析、归一化与可用性。
     *
     * 与 region 同一语义层：不适用 = 仓库在当前环境「不存在」。
     * 区别只有一处：region 的未知值按「全部地区」宽容处理（社区源可能写
     * 各种别名），而 platform 只有两个合法值，写错基本是笔误，所以拒绝。
     */
    #[test]
    fn repository_platform_is_optional_normalized_and_enforced() {
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();

        // 缺省：无平台字段 → 全平台可用
        let document =
            crate::repository_sources::parse_document(REGION_REPOSITORY, &location).unwrap();
        assert_eq!(document.repositories[0].platform, None);
        assert!(platform_matches(None, PLATFORM_WINDOWS));
        assert!(platform_matches(None, PLATFORM_MACOS));

        // 显式空串：同样全平台
        let empty = REGION_REPOSITORY.replace("channel =", "platform = \"\"\nchannel =");
        let document = crate::repository_sources::parse_document(&empty, &location).unwrap();
        assert_eq!(document.repositories[0].platform, None);

        // windows / macos：各自只对对应平台可用
        for (value, ok_on_windows, ok_on_macos) in
            [("windows", true, false), ("macos", false, true)]
        {
            let text = REGION_REPOSITORY
                .replace("channel =", &format!("platform = \"{value}\"\nchannel ="));
            let document = crate::repository_sources::parse_document(&text, &location).unwrap();
            assert_eq!(document.repositories[0].platform.as_deref(), Some(value));
            assert_eq!(
                platform_matches(Some(value), PLATFORM_WINDOWS),
                ok_on_windows,
                "{value} 在 windows 上"
            );
            assert_eq!(
                platform_matches(Some(value), PLATFORM_MACOS),
                ok_on_macos,
                "{value} 在 macos 上"
            );
        }

        // 别名归一化
        for (alias, expected) in [
            ("win", PLATFORM_WINDOWS),
            ("osx", PLATFORM_MACOS),
            ("darwin", PLATFORM_MACOS),
            ("mac", PLATFORM_MACOS),
        ] {
            let text = REGION_REPOSITORY
                .replace("channel =", &format!("platform = \"{alias}\"\nchannel ="));
            let document = crate::repository_sources::parse_document(&text, &location).unwrap();
            assert_eq!(
                document.repositories[0].platform.as_deref(),
                Some(expected),
                "别名 {alias}"
            );
        }

        // 无法识别的平台：验证阶段拒绝整条仓库（不是静默忽略）
        let invalid = REGION_REPOSITORY.replace("channel =", "platform = \"linux\"\nchannel =");
        let error = crate::repository_sources::parse_document(&invalid, &location).unwrap_err();
        assert!(error.to_string().contains("invalidPlatform"), "{error}");
    }

    /// 平台不适用时，repository_is_available 与地区不适用表现完全一致。
    #[test]
    fn platform_mismatch_hides_repository_like_region_mismatch() {
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let text = REGION_REPOSITORY.replace("channel =", "platform = \"macos\"\nchannel =");
        let document = crate::repository_sources::parse_document(&text, &location).unwrap();
        let mut repository =
            repository_from_export(&document.repositories[0], "mac-only".into(), false);
        repository.enabled = true;
        let mut config = default_config();
        config.repositories.push(repository);
        let stored = config.repositories[0].clone();
        // 在 macOS 上可用，在 Windows 上不可用 —— 与 region 的判定方式一致
        assert!(
            repository_is_available(&config, &stored, REGION_CHINESE_MAINLAND)
                == (current_platform() == PLATFORM_MACOS)
        );
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
    fn gitee_community_applies_to_every_region_when_its_region_is_empty() {
        /*
         * 两个社区源默认不限地区：Gitee 社区仓库在地区为空时对所有地区可用，
         * 与 GitHub 社区一致。平台限制已移除，判定只看仓库与来源自身的 region。
         */
        let mut config = default_config();
        let mut source = crate::community::source(&Provider::Gitee);
        source.region = None;
        config.repository_sources.push(source);
        let mut repo =
            crate::community::repository_from_coordinates(&Provider::Gitee, "Owner", "Mod", "", "")
                .unwrap();
        repo.region = None;
        for region in [REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL, "", "unknown"] {
            assert!(
                repository_matches_channel(&config, &repo, region),
                "region={region}"
            );
        }
        // 仓库自己声明地区时仍按该地区过滤
        repo.region = Some(REGION_INTERNATIONAL.into());
        assert!(!repository_matches_channel(
            &config,
            &repo,
            REGION_CHINESE_MAINLAND
        ));
        assert!(repository_matches_channel(
            &config,
            &repo,
            REGION_INTERNATIONAL
        ));
        assert_eq!(
            config.repositories.len(),
            default_config().repositories.len()
        );
    }

    #[test]
    fn availability_requires_enabled_and_matching_region() {
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let document =
            crate::repository_sources::parse_document(REGION_REPOSITORY, &location).unwrap();
        let mut repository =
            repository_from_export(&document.repositories[0], "regional".into(), false);
        let config = default_config();

        /*
         * `repository_is_available` 是「这个仓库此刻是否存在」的唯一判定：
         * enabled 与地区两个条件的交集。任一不满足都等于不存在，
         * 因此既不会显示，也不会被刷新、安装或参与更新统计。
         */
        for (enabled, region) in [
            (true, None),
            (true, Some("international")),
            (true, Some("chinese_mainland")),
            (false, None),
            (false, Some("international")),
        ] {
            repository.enabled = enabled;
            repository.region = region.map(str::to_owned);
            for current in [REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL] {
                assert_eq!(
                    repository_is_available(&config, &repository, current),
                    enabled && repository_matches_channel(&config, &repository, current),
                    "enabled={enabled}, region={region:?}, current={current}"
                );
            }
        }

        // international 仓库在中国大陆彻底不存在（这是本次修复的原始症状）
        repository.enabled = true;
        repository.region = Some(REGION_INTERNATIONAL.into());
        assert!(!repository_is_available(
            &config,
            &repository,
            REGION_CHINESE_MAINLAND
        ));
        assert!(repository_is_available(
            &config,
            &repository,
            REGION_INTERNATIONAL
        ));
    }

    #[test]
    fn regional_repository_variants_coexist_and_export_all_channels() {
        let service = ConfigService {
            root: std::env::temp_dir(),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
    fn channel_variants_of_one_repository_coexist_with_distinct_stable_ids() {
        let service = ConfigService {
            root: std::env::temp_dir(),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        };
        let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
        let document =
            crate::repository_sources::parse_document(REGION_REPOSITORY, &location).unwrap();
        let repositories = [Channel::Stable, Channel::Beta].map(|channel| {
            let mut repository = document.repositories[0].clone();
            repository.channel = channel;
            repository.download_threads = 16;
            repository.name_i18n.insert(
                "zh-CN".into(),
                match channel {
                    Channel::Beta => "Regional MOD - Beta".into(),
                    _ => "Regional MOD - Stable".into(),
                },
            );
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
        let preview = service.import_bundle(&mut config, &bundle, "overwrite");
        assert_eq!(config.repositories.len(), 2);
        assert_eq!(preview.additions.len(), 2);
        assert!(preview.duplicates.is_empty());
        assert_eq!(
            config
                .repositories
                .iter()
                .map(|repo| repo.channel)
                .collect::<Vec<_>>(),
            vec![Channel::Stable, Channel::Beta]
        );
        assert_ne!(
            repo_key(&config.repositories[0]),
            repo_key(&config.repositories[1])
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
        let beta = &config.repositories[1];
        let binding =
            crate::models::RepositoryBinding::from_release(beta, "v1.0.0-beta.1").unwrap();
        assert_eq!(binding.channel, Channel::Beta);
        assert!(binding.matches_repository(beta));
        assert!(!binding.matches_repository(&config.repositories[0]));

        let exported = service.export_bundle(&config, None).unwrap();
        let restored = service.validate_bundle(&exported).unwrap();
        let mut imported = default_config();
        service.import_bundle(&mut imported, &restored, "overwrite");
        assert_eq!(
            imported
                .repositories
                .iter()
                .map(|repo| repo.channel)
                .collect::<Vec<_>>(),
            vec![Channel::Stable, Channel::Beta]
        );
    }

    #[test]
    fn default_config_follows_the_remote_index() {
        let config = default_config();
        assert!(config.repositories.is_empty());
        assert_eq!(config.repository_sources.len(), 1);
        let root = &config.repository_sources[0];
        assert_eq!(root.id, crate::repository_sources::DEFAULT_INDEX_ID);
        assert_eq!(
            root.url,
            crate::repository_sources::default_index_url(current_user_region()),
            "默认读远端索引，内置快照只是读不到时的回退"
        );
        assert!(matches!(
            crate::repository_sources::SourceLocation::parse(&root.url).unwrap(),
            crate::repository_sources::SourceLocation::Remote(_)
        ));
        assert!(!config.builtin_sources_only, "默认不再只看内置仓库快照");
        assert!(!config.builtin_map_sources_only, "默认不再只看内置地图快照");
        // 内置地图索引只有 release-tags 源：条目来自各 Release note，离线解析
        // 不出地图（首启/断网地图页为空，联网刷新后才有）。
        assert!(
            config.map_repositories.is_empty(),
            "内置地图索引没有内联条目，离线不应凭空生成地图"
        );
        assert!(config.repository_sources.iter().all(|source| source.builtin
            && source.parent_id.is_none()
            && repository_source_matches_region(source, current_user_region())));
        let serialized = toml::to_string(&config).unwrap();
        assert!(!serialized.contains("\n[proxies"));
    }

    /// 缺字段的旧配置按「远端索引」补齐，显式打开内置快照的才保留用户选择。
    ///
    /// V2 早期这两个开关默认开启；现在默认关闭，缺字段时不能再补成 `true`。
    #[test]
    fn legacy_configs_without_the_bundled_switches_default_to_the_remote_index() {
        let full = toml::to_string_pretty(&default_config()).unwrap();
        let without_switches = full
            .lines()
            .filter(|line| {
                !line.starts_with("builtin_sources_only")
                    && !line.starts_with("builtin_map_sources_only")
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!without_switches.contains("builtin_sources_only"));
        let migrated: AppConfig = toml::from_str(&without_switches).unwrap();
        assert!(!migrated.builtin_sources_only);
        assert!(!migrated.builtin_map_sources_only);

        let mut value: toml::Value = toml::from_str(&full).unwrap();
        let table = value.as_table_mut().unwrap();
        table.insert("builtin_sources_only".into(), toml::Value::Boolean(true));
        table.insert(
            "builtin_map_sources_only".into(),
            toml::Value::Boolean(true),
        );
        let explicit: AppConfig = toml::from_str(&toml::to_string(&value).unwrap()).unwrap();
        assert!(explicit.builtin_sources_only, "显式打开必须保留用户的选择");
        assert!(explicit.builtin_map_sources_only);
    }

    /// 升级路径：V1 配置里写着 `true` 的两个开关，首次读取要迁成 `false`。
    ///
    /// 只改 serde 默认值救不了已经落盘的老配置 —— 升级必须把文件里的值也改掉，
    /// 并把升级后的配置写回，否则每次启动都会重跑迁移。
    #[test]
    fn upgrading_a_v1_config_switches_the_bundled_source_defaults_off() {
        let service = ConfigService {
            root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-config-load")
                .join(Uuid::new_v4().to_string()),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        };
        // V1：两个开关默认开启，官方根指向编译期快照
        let mut v1 = default_config();
        v1.schema_version = 1;
        v1.builtin_sources_only = true;
        v1.builtin_map_sources_only = true;
        v1.repository_sources = builtin_repository_sources_for_region(current_user_region(), true);
        let text = toml::to_string_pretty(&v1).unwrap();
        assert!(text.contains("schema_version = 1"));
        assert!(text.contains("builtin_sources_only = true"));
        fs::create_dir_all(&service.root).unwrap();
        fs::write(service.config_path(), text).unwrap();

        let loaded = service.load().unwrap();
        assert_eq!(loaded.schema_version, CONFIG_SCHEMA_VERSION);
        assert!(!loaded.builtin_sources_only);
        assert!(!loaded.builtin_map_sources_only);
        assert_eq!(
            loaded.repository_sources[0].url,
            crate::repository_sources::default_index_url(current_user_region()),
            "迁移后官方根必须回到远端索引"
        );
        // 迁移必须落盘：否则每次启动都重跑一遍
        let saved = fs::read_to_string(service.config_path()).unwrap();
        assert!(saved.contains("schema_version = 2"));
        assert!(saved.contains("builtin_sources_only = false"));
        assert!(saved.contains("builtin_map_sources_only = false"));
        fs::remove_dir_all(service.root).unwrap();
    }

    /// 已经是当前版本的配置不再迁移：用户手动打开的开关必须留着。
    #[test]
    fn current_schema_version_keeps_the_bundled_source_switches_as_saved() {
        let mut config = default_config();
        config.builtin_sources_only = true;
        config.builtin_map_sources_only = true;
        assert!(!migrate_bundled_source_switches(&mut config));
        assert!(config.builtin_sources_only);
        assert!(config.builtin_map_sources_only);
        assert_eq!(config.schema_version, CONFIG_SCHEMA_VERSION);
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
                builtin_repository_sources_for_region(region, false),
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
                builtin_repository_sources_for_region(region, false)
            ));
        }
    }

    #[test]
    fn source_nickname_translations_survive_config_and_bundle_round_trips() {
        let service = ConfigService {
            root: std::env::temp_dir(),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
name_i18n.zh-CN = "Custom MOD"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
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

    /*
     * 「仅使用内置仓库源」开关决定官方根源的入口地址。
     *
     * 打开时直接指向打包资源（`repositories.toml`），不再按地区选
     * Gitee / GitHub 的 raw 直链；关闭时恢复地区默认地址。
     * 这是整个功能的唯一切换点 —— 不需要第二套加载逻辑。
     */
    #[test]
    fn builtin_sources_only_switch_selects_the_bundled_entry() {
        for region in [REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL] {
            let remote = builtin_repository_sources_for_region(region, false);
            assert_eq!(remote.len(), 1);
            assert_eq!(remote[0].id, crate::repository_sources::DEFAULT_INDEX_ID);
            assert_eq!(
                remote[0].url,
                crate::repository_sources::default_index_url(region),
                "关闭开关时应使用地区默认远端地址"
            );
            assert_ne!(
                remote[0].url,
                crate::repository_sources::BUILTIN_INDEX,
                "远端地址不能等于内置路径"
            );

            let bundled = builtin_repository_sources_for_region(region, true);
            assert_eq!(bundled.len(), 1);
            assert_eq!(
                bundled[0].url,
                crate::repository_sources::BUILTIN_INDEX,
                "打开开关时官方根应指向内置快照"
            );
            // 两个地区得到同一个内置入口 —— 这正是「不再按地区选择」的含义
            assert_eq!(
                bundled[0].url,
                builtin_repository_sources_for_region(REGION_CHINESE_MAINLAND, true)[0].url
            );
            // 其余身份字段不受开关影响
            assert_eq!(bundled[0].id, remote[0].id);
            assert!(bundled[0].builtin);
            assert!(bundled[0].parent_id.is_none());
        }
    }

    /// 读取配置时按开关重建官方根，且保留自定义源与既有子源。
    #[test]
    fn config_load_applies_builtin_sources_only_to_the_official_root() {
        // 从「开关关着时写下的配置」开始：官方根还指向远端，同步才有事可做。
        let mut config = default_config();
        config.repository_sources =
            builtin_repository_sources_for_region(current_user_region(), false);
        config.builtin_sources_only = true;
        config.repository_sources.push(RepositorySource {
            id: "custom".into(),
            nickname: "自定义".into(),
            nickname_i18n: Default::default(),
            url: "https://example.com/custom.toml".into(),
            region: None,
            builtin: false,
            parent_id: None,
        });

        assert!(synchronize_builtin_repository_sources(&mut config));
        let root = config
            .repository_sources
            .iter()
            .find(|source| source.id == crate::repository_sources::DEFAULT_INDEX_ID)
            .expect("官方根应存在");
        assert_eq!(root.url, crate::repository_sources::BUILTIN_INDEX);
        assert!(
            config.repository_sources.iter().any(|s| s.id == "custom"),
            "自定义源不应被开关影响"
        );
    }

    #[test]
    fn first_load_synchronizes_builtin_sources_even_when_removing_legacy_proxy_fields() {
        for has_legacy_proxy_fields in [false, true] {
            let service = ConfigService {
                root: Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("target/test-config-load")
                    .join(Uuid::new_v4().to_string()),
                cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
            // 旧配置只有单语昵称：加载后应自动回填 zh-CN（见 normalize_configured_regions）
            let custom = RepositorySource {
                id: "custom-source".into(),
                nickname: "Custom source".into(),
                nickname_i18n: Default::default(),
                url: "https://example.com/custom.toml".into(),
                region: None,
                builtin: false,
                parent_id: None,
            };
            let mut expected_custom = custom.clone();
            expected_custom
                .nickname_i18n
                .insert("zh-CN".into(), "Custom source".into());
            expected_sources.push(expected_custom);
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
    fn legacy_repository_metadata_migrates_into_chinese_maps_and_persists_empty_fields() {
        let service = ConfigService {
            root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-config-load")
                .join(Uuid::new_v4().to_string()),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        };
        let location = crate::repository_sources::SourceLocation::parse("fixture.toml").unwrap();
        let mut config = default_config();
        let export = crate::repository_sources::parse_document(REGION_REPOSITORY, &location)
            .unwrap()
            .repositories
            .remove(0);
        config.repositories = vec![repository_from_export(
            &export,
            "legacy-repository".into(),
            false,
        )];
        let mut legacy = toml::Value::try_from(&config).unwrap();
        let repository = legacy["repositories"][0].as_table_mut().unwrap();
        repository.remove("name_i18n");
        repository.remove("group_i18n");
        repository.remove("note_i18n");
        repository.insert("name".into(), toml::Value::String("Legacy name".into()));
        repository.insert("group".into(), toml::Value::String("Legacy group".into()));
        repository.insert("note".into(), toml::Value::String(String::new()));
        fs::create_dir_all(&service.root).unwrap();
        fs::write(
            service.config_path(),
            toml::to_string_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let loaded = service.load().unwrap();
        let repository = &loaded.repositories[0];
        assert_eq!(repository.name_i18n["zh-CN"], "Legacy name");
        assert_eq!(repository.group_i18n["zh-CN"], "Legacy group");
        assert_eq!(repository.note_i18n["zh-CN"], "");
        assert_eq!(repository.name, repository.name_i18n["zh-CN"]);
        assert_eq!(repository.group, repository.group_i18n["zh-CN"]);
        assert_eq!(repository.note, repository.note_i18n["zh-CN"]);

        let saved = fs::read_to_string(service.config_path()).unwrap();
        let saved_value: toml::Value = toml::from_str(&saved).unwrap();
        let saved_repository = &saved_value["repositories"][0];
        assert_eq!(
            saved_repository["name_i18n"]["zh-CN"].as_str(),
            Some("Legacy name")
        );
        assert_eq!(
            saved_repository["group_i18n"]["zh-CN"].as_str(),
            Some("Legacy group")
        );
        assert_eq!(saved_repository["note_i18n"]["zh-CN"].as_str(), Some(""));
        let reloaded = service.load().unwrap();
        assert_eq!(reloaded.repositories[0].note_i18n["zh-CN"], "");
        assert_eq!(fs::read_to_string(service.config_path()).unwrap(), saved);
    }

    #[test]
    fn japanese_repository_metadata_survives_import_export_and_config_reload() {
        let service = ConfigService {
            root: Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("target/test-config-load")
                .join(Uuid::new_v4().to_string()),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        };
        let location = crate::repository_sources::SourceLocation::parse("fixture.toml").unwrap();
        let text = r#"
[[repositories]]
name = "日本語名"
name_i18n.zh-CN = "中文名称"
name_i18n.ja = "日本語名"
group_i18n.zh-CN = "工具"
group_i18n.ja = "ツール"
note_i18n.zh-CN = ""
note_i18n.ja = "日本語のメモ"
provider = "github"
username = "example"
repository = "mod"
channel = "stable"
install_target = "BepInEx/plugins"
plugin_guid = "com.example.ja"
enabled = true
[[repositories.asset_rules]]
type = "exact"
value = "Japan.dll"
"#;
        let document = crate::repository_sources::parse_document(text, &location).unwrap();
        let export = document.repositories[0].clone();
        assert_eq!(export.name_i18n["ja"], "日本語名");
        assert_eq!(export.group_i18n["ja"], "ツール");
        assert_eq!(export.note_i18n["ja"], "日本語のメモ");
        let bundle = RepositoryBundle {
            format: BUNDLE_FORMAT.into(),
            schema_version: 1,
            exported_at: String::new(),
            repositories: vec![export],
            repository_sources: Vec::new(),
        };
        let mut config = default_config();
        service.import_bundle(&mut config, &bundle, "overwrite");
        let export_bundle = service.export_bundle(&config, None).unwrap();
        let restored = service.validate_bundle(&export_bundle).unwrap();
        assert_eq!(restored.repositories[0].name_i18n["ja"], "日本語名");
        assert_eq!(restored.repositories[0].group_i18n["ja"], "ツール");
        assert_eq!(restored.repositories[0].note_i18n["ja"], "日本語のメモ");
        fs::create_dir_all(&service.root).unwrap();
        service.save(&config).unwrap();
        let reloaded = service.load().unwrap();
        assert_eq!(reloaded.repositories[0].name_i18n["ja"], "日本語名");
        assert_eq!(reloaded.repositories[0].group_i18n["ja"], "ツール");
        assert_eq!(reloaded.repositories[0].note_i18n["ja"], "日本語のメモ");
        assert_eq!(reloaded.repositories[0].name, "中文名称");
        fs::remove_dir_all(&service.root).unwrap();
    }

    #[test]
    fn repository_source_does_not_require_bundle_metadata() {
        let text = r#"
[[repositories]]
name = "Example"
name_i18n.zh-CN = "Example"
group_i18n.zh-CN = ""
note_i18n.zh-CN = "**Example** repository"
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
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        };
        let text = r#"
format = "oc2-mod-repository-bundle"
schema_version = 1
exported_at = ""

[[repositories]]
name = "Order-C"
name_i18n.zh-CN = "Order-C"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
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
name_i18n.zh-CN = "Order-A"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
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
name_i18n.zh-CN = "Order-B"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
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
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
        bundle.repositories[0]
            .note_i18n
            .insert("zh-CN".into(), "Custom imported note".into());
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
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
        builtin
            .name_i18n
            .insert("zh-CN".into(), builtin.name.clone());
        builtin.source_id = Some(builtin_source);
        let mut disabled = repository_from_export(&template, "disabled".into(), true);
        disabled.name = "Disabled other-region MOD".into();
        disabled
            .name_i18n
            .insert("zh-CN".into(), disabled.name.clone());
        disabled.region = Some(other_region.into());
        disabled.source_id = Some("other-region-child".into());
        disabled.enabled = false;
        let mut detached = repository_from_export(&template, "detached".into(), false);
        detached.name = "Detached MOD".into();
        detached
            .name_i18n
            .insert("zh-CN".into(), detached.name.clone());
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
            platform: None,
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
            relations: Vec::new(),
            release_relations: Vec::new(),
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
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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
        release
            .note_i18n
            .insert("zh-CN".into(), release.note.clone());
        release.group = "测试分组".into();
        release.version_regex = r"^1\.\d+\.\d+$".into();
        release.download_threads = 16;
        let mut raw = release.clone();
        raw.name = "Raw MOD".into();
        raw.name_i18n.insert("zh-CN".into(), "Raw MOD".into());
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
    fn repository_bundle_requires_chinese_metadata_and_ignores_raw_fields() {
        let service = ConfigService {
            root: std::env::temp_dir(),
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        };
        let bundle_for = |metadata: &str| {
            format!(
                r#"format = "{BUNDLE_FORMAT}"
schema_version = 1
exported_at = "2026-01-01T00:00:00Z"

[[repositories]]
{metadata}
provider = "github"
username = "example"
repository = "mod"
install_target = "BepInEx/plugins"
enabled = true
[[repositories.asset_rules]]
type = "exact"
value = "mod.dll"
"#
            )
        };

        let valid = service
            .validate_bundle(&bundle_for(
                r#"name = "Raw name"
name_i18n.zh-CN = " 中文名称 "
group = "Raw group"
group_i18n.zh-CN = "中文分组"
note = "Raw note"
note_i18n.zh-CN = "中文备注""#,
            ))
            .unwrap();
        let repository = &valid.repositories[0];
        assert_eq!(repository.name, "中文名称");
        assert_eq!(repository.group, "中文分组");
        assert_eq!(repository.note, "中文备注");

        for (metadata, expected_error) in [
            (
                r#"name_i18n.zh-CN = "名称"
note_i18n.zh-CN = """#,
                "config.missingGroupI18n",
            ),
            (
                r#"name_i18n.zh-CN = "名称"
group_i18n.zh-CN = """#,
                "config.missingNoteI18n",
            ),
            (
                r#"name_i18n.zh-CN = ""
group_i18n.zh-CN = ""
note_i18n.zh-CN = """#,
                "config.emptyNameI18n",
            ),
            (
                r#"name_i18n.en = "English name"
group_i18n.zh-CN = ""
note_i18n.zh-CN = """#,
                "config.missingNameI18n",
            ),
        ] {
            let error = service.validate_bundle(&bundle_for(metadata)).unwrap_err();
            assert!(error.to_string().contains(expected_error), "{error}");
        }
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
            cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
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

#[cfg(test)]
mod localized_name_tests {
    use super::*;
    use std::collections::BTreeMap;

    fn repo_with(names: &[(&str, &str)]) -> RepositoryConfig {
        let mut name_i18n = BTreeMap::new();
        for (key, value) in names {
            name_i18n.insert((*key).to_string(), (*value).to_string());
        }
        let mut repo: RepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "r1", "name": "主机实用 MOD（街机 MOD）稳定版",
            "group": "", "provider": "github", "username": "u", "repository": "r",
            "download_mode": "release", "branch": "main", "file_path": "",
            "region": "", "channel": "all", "install_target": "",
            "enabled": true, "builtin": false
        }))
        .unwrap();
        repo.name_i18n = name_i18n;
        repo
    }

    /*
     * 下载进度标签与安装预览标题都要跟随界面语言。
     *
     * 真实问题：这两处原来固定用 `repo.name`（中文原名），切到英文后
     * 卡片列表是英文、下载弹窗和安装预览却还是中文，同一件事两个语言。
     */
    #[test]
    fn picks_the_name_matching_the_interface_locale() {
        let repo = repo_with(&[
            ("zh-CN", "主机实用 MOD"),
            ("en", "HostUtilities"),
            ("ja", "ホストツール"),
        ]);
        assert_eq!(
            localized_repository_name(&repo, Some("en")),
            "HostUtilities"
        );
        assert_eq!(localized_repository_name(&repo, Some("ja")), "ホストツール");
        assert_eq!(
            localized_repository_name(&repo, Some("zh-CN")),
            "主机实用 MOD"
        );
    }

    /// 语言标记缺失时退回中文原名，不能返回空串。
    #[test]
    fn falls_back_to_the_base_name_when_locale_is_missing_or_unknown() {
        let repo = repo_with(&[("en", "HostUtilities")]);
        assert_eq!(localized_repository_name(&repo, Some("ko")), repo.name);
        assert_eq!(localized_repository_name(&repo, None), repo.name);
        assert_eq!(localized_repository_name(&repo, Some("")), repo.name);
    }

    /// 源文档可能只写主语言标记（`zh`），界面传来的是 `zh-TW`：应当能对上。
    #[test]
    fn matches_on_the_primary_language_when_the_exact_tag_is_absent() {
        let repo = repo_with(&[("zh", "主机实用 MOD"), ("en-US", "HostUtilities")]);
        assert_eq!(
            localized_repository_name(&repo, Some("zh-TW")),
            "主机实用 MOD"
        );
        assert_eq!(
            localized_repository_name(&repo, Some("en-GB")),
            "HostUtilities"
        );
    }

    /// 空翻译值等同缺失，不要显示成空白。
    #[test]
    fn ignores_blank_translations() {
        let repo = repo_with(&[("en", "   ")]);
        assert_eq!(localized_repository_name(&repo, Some("en")), repo.name);
    }
}

#[cfg(test)]
mod dependency_validation_tests {
    use super::*;

    fn base_export(relations: Vec<RepositoryRelation>) -> RepositoryExport {
        RepositoryExport {
            name: "dep-owner".into(),
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
            platform: None,
            channel: Channel::Stable,
            asset_rules: vec![AssetRule {
                rule_type: AssetRuleType::Exact,
                value: "a.zip".into(),
            }],
            install_target: "BepInEx/plugins".into(),
            note: String::new(),
            note_i18n: std::collections::BTreeMap::new(),
            plugin_guid: String::new(),
            local_product_name: String::new(),
            local_original_filename: String::new(),
            relations,
            release_relations: Vec::new(),
            version_regex: String::new(),
            max_release_count: 10,
            download_threads: 1,
            enabled: true,
            source_id: None,
        }
    }

    fn relation(kind: RelationKind, min: &str, max: &str, optional: bool) -> RepositoryRelation {
        RepositoryRelation {
            kind,
            guid: "com.example.dep".into(),
            min_version: min.into(),
            max_version: max.into(),
            prerelease: None,
            optional,
        }
    }

    /*
     * 关系声明的校验。
     *
     * 空 GUID 的关系永远匹配不上任何插件，等于一条静默失效的声明；
     * `min_version` 无法解析时「是否满足」永远判不出来。
     * 两者都在导入阶段拒绝，比等到界面上不提示更容易发现。
     */
    #[test]
    fn relation_without_guid_is_rejected() {
        let mut entry = relation(RelationKind::Requires, "", "", false);
        entry.guid = "   ".into();
        assert!(validate_repository(&base_export(vec![entry])).is_err());
    }

    #[test]
    fn relation_with_unparsable_min_version_is_rejected() {
        let entry = relation(RelationKind::Requires, "not-a-version", "", false);
        assert!(validate_repository(&base_export(vec![entry])).is_err());
    }

    #[test]
    fn relation_with_unparsable_max_version_is_rejected() {
        let entry = relation(RelationKind::Compatible, "", "not-a-version", false);
        assert!(validate_repository(&base_export(vec![entry])).is_err());
    }

    /// 区间反了（min > max）永远匹配不到任何插件，属于配置错误。
    #[test]
    fn inverted_relation_range_is_rejected() {
        let entry = relation(RelationKind::Conflicts, "2.0", "1.0", false);
        assert!(validate_repository(&base_export(vec![entry])).is_err());
    }

    /// 合法关系（含四段 beta 序号写法）应当通过。
    #[test]
    fn valid_relations_pass() {
        for min_version in ["", "1.8.7", "1.8.7.2", "1.8.7-beta.2"] {
            let entry = relation(RelationKind::Requires, min_version, "", false);
            assert!(
                validate_repository(&base_export(vec![entry])).is_ok(),
                "min_version={min_version}"
            );
        }
        // conflicts / compatible 同样接受双端区间
        for kind in [RelationKind::Conflicts, RelationKind::Compatible] {
            let entry = relation(kind, "1.0", "2.0", false);
            assert!(
                validate_repository(&base_export(vec![entry])).is_ok(),
                "kind={}",
                kind.as_str()
            );
        }
    }

    /// `optional` 只属于依赖：冲突与兼容关系标记可选是自相矛盾的。
    #[test]
    fn optional_is_rejected_for_non_requirement_relations() {
        for kind in [RelationKind::Conflicts, RelationKind::Compatible] {
            let entry = relation(kind, "", "", true);
            let error = validate_repository(&base_export(vec![entry])).unwrap_err();
            assert!(
                error.to_string().contains("relationOptionalUnsupported"),
                "{error}"
            );
        }
    }

    /// Release 覆盖表的区间两端全空等同于不限版本，应当拒绝。
    #[test]
    fn release_relations_without_any_bound_is_rejected() {
        let mut export = base_export(Vec::new());
        export.release_relations.push(ReleaseRelationOverride {
            min_version: String::new(),
            max_version: String::new(),
            relations: Vec::new(),
        });
        let error = validate_repository(&export).unwrap_err();
        assert!(error.to_string().contains("relationsEmpty"), "{error}");
    }

    /*
     * 指向自己的冲突必须带 prerelease。
     *
     * 「正式版与测试版互斥」是两条同 GUID 的发布线各自声明互相冲突，
     * 靠 `prerelease = true/false` 区分。不写标志时它等价于
     * 「我不能和自己共存」—— 永远为真，一定是配置写错了。
     */
    #[test]
    fn self_conflict_without_prerelease_flag_is_rejected() {
        let mut export = base_export(Vec::new());
        export.plugin_guid = "com.example.owner".into();
        let mut entry = relation(RelationKind::Conflicts, "", "", false);
        entry.guid = "com.example.owner".into();
        export.relations = vec![entry];
        let error = validate_repository(&export).unwrap_err();
        assert!(error.to_string().contains("relationSelfGuid"), "{error}");
    }

    /// 带了 prerelease 的自冲突是合法写法（正式版排斥测试版）。
    #[test]
    fn self_conflict_with_prerelease_flag_passes() {
        for flag in [true, false] {
            let mut export = base_export(Vec::new());
            export.plugin_guid = "com.example.owner".into();
            let mut entry = relation(RelationKind::Conflicts, "", "", false);
            entry.guid = "com.example.owner".into();
            entry.prerelease = Some(flag);
            export.relations = vec![entry];
            assert!(validate_repository(&export).is_ok(), "prerelease={flag}");
        }
    }

    /// `prerelease` 不能用在依赖上：那是「必须是测试版依赖」，语义应写进版本区间。
    #[test]
    fn prerelease_is_rejected_for_requires() {
        let mut entry = relation(RelationKind::Requires, "", "", false);
        entry.prerelease = Some(true);
        let error = validate_repository(&base_export(vec![entry])).unwrap_err();
        assert!(
            error.to_string().contains("relationRequiresPrerelease"),
            "{error}"
        );
    }
}

#[cfg(test)]
mod relations_syntax {
    use super::*;

    /*
     * `relations` 是**表数组**（`Vec<RepositoryRelation>`），必须用
     * `[[relations]]` 逐个声明。点号赋值写法（`relations.guid = "..."`）
     * 在 TOML 里创建的是**单个表**，反序列化到 Vec 会失败：
     *
     *     invalid type: map, expected a sequence
     *
     * 这条测试把两种写法的差异固定下来 —— 源文档作者很容易按「配置项」
     * 的直觉写成点号形式，那样整个仓库源文档会解析失败。
     */
    #[test]
    fn dotted_assignment_is_rejected_because_relations_is_a_table_array() {
        let dotted = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
relations.guid = "com.bepis.bepinex.configurationmanager"
relations.min_version = ""
relations.optional = false
[[asset_rules]]
type = "exact"
value = "a.zip"
"#;
        let error = toml::from_str::<RepositoryExport>(dotted).unwrap_err();
        assert!(error.to_string().contains("expected a sequence"), "{error}");
    }

    /// 正确写法：`[[relations]]` 表数组，可重复声明多条关系。
    #[test]
    fn array_of_tables_accepts_multiple_relations() {
        let explicit = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
guid = "com.bepis.bepinex.configurationmanager"

[[relations]]
kind = "conflicts"
guid = "com.example.other"
min_version = "1.8.7"
max_version = "2.0"
"#;
        let v = toml::from_str::<RepositoryExport>(explicit).expect("表数组写法应当成功");
        assert_eq!(v.relations.len(), 2);
        // 省略 kind 时按依赖处理：历史写法（[[requires]] 只写 guid）继续可读
        assert_eq!(v.relations[0].kind, RelationKind::Requires);
        assert_eq!(
            v.relations[0].guid,
            "com.bepis.bepinex.configurationmanager"
        );
        assert_eq!(v.relations[1].kind, RelationKind::Conflicts);
        assert_eq!(v.relations[1].min_version, "1.8.7");
        assert_eq!(v.relations[1].max_version, "2.0");
        assert!(!v.relations[1].optional);
    }

    /// `[[requires]]` 作为历史字段名继续可读（alias，不需要作者改文档）。
    #[test]
    fn legacy_requires_field_still_parses() {
        let legacy = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[requires]]
guid = "com.example.dep"
min_version = "1.8.7"
optional = true
"#;
        let v = toml::from_str::<RepositoryExport>(legacy).expect("旧字段名应当继续可读");
        assert_eq!(v.relations.len(), 1);
        assert_eq!(v.relations[0].kind, RelationKind::Requires);
        assert_eq!(v.relations[0].min_version, "1.8.7");
        assert!(v.relations[0].optional);
    }

    /*
     * 未知的 `kind` 跳过而不是让整份文档解析失败。
     *
     * 前向兼容边界：新版本客户端写入的关系种类（例如以后的 `loads_after`）
     * 不能被旧客户端变成「整个仓库源读取失败」。
     */
    #[test]
    fn unknown_relation_kind_is_skipped_instead_of_failing_the_document() {
        let document = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
kind = "loads_after"
guid = "com.example.future"

[[relations]]
kind = "requires"
guid = "com.example.dep"
"#;
        let v = toml::from_str::<RepositoryExport>(document).expect("未知 kind 不应让文档失败");
        assert_eq!(v.relations.len(), 1);
        assert_eq!(v.relations[0].guid, "com.example.dep");
    }

    /// 未知的**顶层**字段也不该让源文档失败（旧客户端读新文档）。
    #[test]
    fn unknown_top_level_field_is_ignored() {
        let document = r#"
name = "x"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
some_future_field = "whatever"
[[asset_rules]]
type = "exact"
value = "a.zip"
"#;
        let v = toml::from_str::<RepositoryExport>(document).expect("未知字段不应让文档失败");
        assert_eq!(v.name, "x");
    }

    /*
     * 仓库字段写在 `[[repositories.relations]]` **之后**会变成关系的字段。
     *
     * TOML 里表数组切换后，后续键属于新表 —— 作者按「先写基本信息、最后写
     * 备注」的顺序写，很容易把 `install_target` / `note_i18n` 放到关系块后面。
     * serde 默认忽略未知字段，于是那些字段凭空消失，最终报的是
     * 「missing field `install_target`」，作者会以为自己忘了写。
     *
     * 现在解析阶段就点名，能直接对得上症状。
     */
    #[test]
    fn repository_field_after_relations_is_reported_as_misplaced() {
        let misplaced = r#"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
kind = "requires"
guid = "com.example.dep"

# 写在了关系块之后 —— 实际属于关系条目
install_target = "BepInEx/plugins"
"#;
        let error = toml::from_str::<RepositoryExport>(misplaced)
            .expect_err("仓库字段出现在关系块之后应当报错");
        let text = error.to_string();
        assert!(text.contains("install_target"), "{text}");
        assert!(text.contains("写在"), "{text}");
    }

    /// 正确顺序（仓库字段都在关系块之前）照常解析。
    #[test]
    fn repository_fields_before_relations_still_parse() {
        let correct = r#"
name_i18n.zh-CN = "x"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "a"
repository = "b"
install_target = "BepInEx/plugins"
enabled = true
[[asset_rules]]
type = "exact"
value = "a.zip"

[[relations]]
kind = "requires"
guid = "com.example.dep"
min_version = "1.0"
"#;
        let v = toml::from_str::<RepositoryExport>(correct).expect("正确顺序应当成功");
        assert_eq!(v.relations.len(), 1);
        assert_eq!(v.install_target, "BepInEx/plugins");
    }
}
