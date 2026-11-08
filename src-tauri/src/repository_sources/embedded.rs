use super::diagnostics::error_category;
use super::diagnostics::safe_log_address;
use super::document::RepositorySourceDocument;
use super::document::parse_document;
use super::location::SourceLocation;
use crate::config::ConfigError;
use crate::models::RepositoryExport;
use std::collections::HashSet;
use std::sync::OnceLock;

pub const BUILTIN_INDEX: &str = "mods.toml";

pub const DEFAULT_INDEX_ID: &str = "official-repository-index";

pub const DEFAULT_INDEX_GITEE_URL: &str =
    "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Repos/raw/main/mods.toml";

pub const DEFAULT_INDEX_GITHUB_URL: &str =
    "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager-Repos/raw/refs/heads/main/mods.toml";

pub fn default_index_url(region: &str) -> &'static str {
    if region == crate::config::REGION_CHINESE_MAINLAND {
        DEFAULT_INDEX_GITEE_URL
    } else {
        DEFAULT_INDEX_GITHUB_URL
    }
}

include!(concat!(env!("OUT_DIR"), "/repository_sources.rs"));

/// 遍历内置仓库快照，返回其中所有的仓库配置（**不做地区过滤**）。
///
/// 用途：社区搜索结果必须回避全部内置/官方仓库，包括因当前地区
/// 被过滤掉、因而不在 `config.repositories` 里的那些。
/// 只读嵌入资源，不发起网络请求，也不修改任何配置。
pub(crate) fn bundled_repository_configs() -> Vec<RepositoryExport> {
    let mut repositories = Vec::new();
    let mut pending = match SourceLocation::parse(BUILTIN_INDEX) {
        Ok(location) => vec![location],
        Err(_) => return repositories,
    };
    let mut visited = HashSet::new();
    while let Some(location) = pending.pop() {
        // 只遍历内置资源：远程地址在这里没有网络上下文，直接跳过。
        if !matches!(location, SourceLocation::Bundled(_)) || !visited.insert(location.address()) {
            continue;
        }
        let Ok(document) = location.bundled_document() else {
            continue;
        };
        repositories.extend(document.repositories);
        for source in document.repository_sources {
            if let Ok(next) = SourceLocation::parse(&source.url) {
                pending.push(next);
            }
        }
    }
    repositories
}

impl SourceLocation {
    pub fn bundled_document(&self) -> Result<RepositorySourceDocument, ConfigError> {
        // Cache only the immutable embedded index. Real source refreshes use
        // load_document and always request the remote document again.
        static INDEX: OnceLock<Result<RepositorySourceDocument, ConfigError>> = OnceLock::new();
        if matches!(self, Self::Bundled(path) if path.eq_ignore_ascii_case(BUILTIN_INDEX)) {
            return INDEX.get_or_init(|| self.parse_bundled_document()).clone();
        }
        self.parse_bundled_document()
    }

    fn parse_bundled_document(&self) -> Result<RepositorySourceDocument, ConfigError> {
        log::debug!("读取内置仓库源文件：{}", safe_log_address(&self.address()));
        let text = self.bundled_text()?;
        let document = parse_document(text, self);
        match &document {
            Ok(document) => log::debug!(
                "内置仓库源解析完成：地址={}，仓库={}，子源={}",
                safe_log_address(&self.address()),
                document.repositories.len(),
                document.repository_sources.len()
            ),
            Err(error) => log::error!(
                "内置仓库源解析失败：地址={}，错误类别={}",
                safe_log_address(&self.address()),
                error_category(error)
            ),
        }
        document
    }

    pub fn bundled_text(&self) -> Result<&'static str, ConfigError> {
        let Self::Bundled(path) = self else {
            return Err(ConfigError::Invalid("source.notBuiltin".into()));
        };
        BUNDLED_SOURCES
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(path))
            .map(|(_, text)| *text)
            .ok_or_else(|| {
                ConfigError::Read(crate::locale_tag::with_params(
                    "source.builtinFileMissing",
                    &[("path", &path.to_string())],
                ))
            })
    }
}

#[cfg(test)]
mod tests;
