//! 地区、平台归一化及仓库可用性。

use super::ConfigError;
use crate::models::*;

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

pub(super) fn repository_source_matches_region(source: &RepositorySource, region: &str) -> bool {
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
    if repository
        .source_id
        .as_deref()
        .is_some_and(crate::community::is_source)
    {
        return false;
    }

    repository.enabled && repository_matches_channel(config, repository, region)
}
