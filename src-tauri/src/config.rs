//! 配置共享状态与领域入口；读写、源同步、仓库定义和密钥各归其模块。

use crate::models::AppConfig;
use std::path::PathBuf;

mod availability;
mod bundle;
mod defaults;
mod identity;
mod metadata;
mod migrations;
mod public;
mod secrets;
mod sources;
mod storage;
mod validation;

pub use availability::REGION_CHINESE_MAINLAND;
pub use availability::REGION_INTERNATIONAL;
pub use availability::current_user_region;
pub(crate) use availability::mod_repository_is_available;
pub(crate) use availability::normalize_repository_platform;
pub(crate) use availability::normalize_repository_region;
pub(crate) use availability::normalize_repository_region_for_storage;
#[cfg(test)]
pub(crate) use availability::repository_matches_channel;
pub(crate) use availability::validate_repository_platform;
pub use availability::validate_repository_region;
pub use availability::validate_repository_source_region;
#[cfg(test)]
pub(crate) use defaults::default_config;
pub(crate) use defaults::default_ui_zoom;
pub(crate) use identity::export_key;
pub(crate) use identity::repo_key;
pub(crate) use identity::source_repository_key;
pub(crate) use metadata::localized_map_name;
pub(crate) use metadata::localized_mod_repository_name;
pub(crate) use metadata::normalize_repository_i18n;
pub(crate) use sources::builtin_repository_sources_for_region;
pub(crate) use validation::check_version_spec;
pub(crate) use validation::validate_mod_repository;

/// 配置读写失败。
///
/// `#[error]` 的值是**给 thiserror 的模板**，不是普通字符串：末尾的 `: {0}`
/// 必须保留，否则会把具体原因（哪个文件、哪一行解析失败）整个丢掉 ——
/// 用户只看到「配置格式无效」却不知道哪里错了，日志里也查不到。
///
/// 这里保留历史 thiserror 的 `tag: 细节` 输出；前端查询串解析器不拆冒号。
/// 若统一错误协议，应单独迁移所有调用方，不能在结构拆分中删掉错误细节。
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
    proxies: std::sync::Arc<crate::github_proxy_runtime::ProxyRuntime>,
}

struct CachedConfig {
    modified: Option<std::time::SystemTime>,
    size: u64,
    config: AppConfig,
}

impl std::fmt::Debug for ConfigService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigService")
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod dependency_validation_tests;
#[cfg(test)]
mod localized_name_tests;
#[cfg(test)]
mod proxy_tests;
#[cfg(test)]
mod relations_syntax;
#[cfg(test)]
mod tests;
