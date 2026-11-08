use super::diagnostics::safe_log_address;
use super::location::SourceLocation;
use super::transport::MAX_SOURCE_BYTES;
use crate::config::ConfigError;
use crate::config::validate_mod_repository;
use crate::config::validate_repository_source_region;
use crate::models::ModRepositoryExport;
use serde::Deserialize;
use serde::Deserializer;

/// 源文档里的 `[[repository_sources]]` 声明。
///
/// 解析后统一补齐 `nickname_i18n` 的 zh-CN 与单语镜像，调用方不必再
/// 分别处理旧单语声明、只有其他语言的声明和完整多语言声明。
#[derive(Debug, Clone, Deserialize)]
pub struct RepositorySourceDeclaration {
    #[serde(default)]
    pub network_plan: crate::models::NetworkPlanSelection,
    pub id: String,
    #[serde(default)]
    pub nickname_i18n: std::collections::BTreeMap<String, String>,
    /// 兼容字段：旧文档的单语昵称。
    ///
    /// 反序列化后由 `normalize_source_nickname` 折叠进 `nickname_i18n`，
    /// 之后不再参与任何展示决策。
    #[serde(default, skip_serializing)]
    pub nickname: String,
    pub url: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default, deserialize_with = "deserialize_builtin")]
    pub builtin: Option<bool>,
}

/// 多语言昵称表里选展示值：`zh-CN` 优先，这是全项目的兜底语言。
///
/// 后端不接当前语言：先取 zh-CN，再取 BTreeMap 中首个非空值。
/// 运行时镜像用此值；前端展示仍按自己的语言回退顺序选取。
pub(crate) fn nickname_fallback(
    translations: &std::collections::BTreeMap<String, String>,
) -> Option<String> {
    translations
        .get("zh-CN")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .or_else(|| {
            translations
                .values()
                .map(|value| value.trim())
                .find(|value| !value.is_empty())
                .map(str::to_string)
        })
}

/// 把单语 `nickname` 折叠进 `nickname_i18n`，并校验 zh-CN 兜底存在。
///
/// 多语言表的 zh-CN、首个非空翻译都优先于旧单语值；取不到才报错。
/// 清除空翻译并补齐 zh-CN 与单语镜像，避免后续分别推导兼容回退值。
pub(crate) fn normalize_source_nickname(
    source: &mut RepositorySourceDeclaration,
) -> Result<String, ConfigError> {
    let legacy = source.nickname.trim().to_string();
    let resolved = nickname_fallback(&source.nickname_i18n)
        .or_else(|| (!legacy.is_empty()).then(|| legacy.clone()))
        .ok_or_else(|| ConfigError::Invalid("source.idOrNicknameEmpty".into()))?;
    // 空白翻译项一律清掉：`pickI18n` 会跳过空值，留着只会让 TOML 变长
    source
        .nickname_i18n
        .retain(|_, value| !value.trim().is_empty());
    source
        .nickname_i18n
        .insert("zh-CN".into(), resolved.clone());
    source.nickname = resolved.clone();
    Ok(resolved)
}

fn deserialize_builtin<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<bool>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag {
        Boolean(bool),
        Text(String),
    }
    match Flag::deserialize(deserializer)? {
        Flag::Boolean(value) => Ok(Some(value)),
        Flag::Text(value) if value.eq_ignore_ascii_case("true") => Ok(Some(true)),
        Flag::Text(value) if value.eq_ignore_ascii_case("false") => Ok(Some(false)),
        Flag::Text(_) => Err(serde::de::Error::custom("source.builtinFlagInvalid")),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RepositorySourceDocument {
    #[serde(rename = "type", default)]
    pub kind: Option<crate::models::RepositoryKind>,
    #[serde(default)]
    pub network_plan: crate::models::NetworkPlanSelection,
    #[serde(default)]
    pub repositories: Vec<ModRepositoryExport>,
    #[serde(default)]
    pub repository_sources: Vec<RepositorySourceDeclaration>,
}

pub(crate) fn source_address_key(url: &str, region: Option<&str>) -> (String, String) {
    (
        url.trim().to_ascii_lowercase(),
        crate::config::normalize_repository_region(region),
    )
}

pub(crate) fn parse_document(
    text: &str,
    location: &SourceLocation,
    inherited: &crate::models::NetworkPlanSelection,
) -> Result<RepositorySourceDocument, ConfigError> {
    log::debug!(
        "开始解析仓库源文档：地址={}，字节数={}",
        safe_log_address(&location.address()),
        text.len()
    );
    if text.len() > MAX_SOURCE_BYTES {
        log::error!(
            "仓库源文档超过大小限制：地址={}，字节数={}",
            safe_log_address(&location.address()),
            text.len()
        );
        return Err(ConfigError::Invalid("source.tooLarge".into()));
    }
    let mut document: RepositorySourceDocument = toml::from_str(text).map_err(|error| {
        ConfigError::Invalid(crate::locale_tag::with_params(
            "source.tomlInvalid",
            &[("reason", &error.to_string())],
        ))
    })?;
    document.network_plan = document.network_plan.inherit(inherited);
    if document
        .kind
        .is_some_and(|kind| kind != crate::models::RepositoryKind::Mod)
    {
        return Err(ConfigError::Invalid("proxy.validationFailed".into()));
    }
    if document.repositories.is_empty() && document.repository_sources.is_empty() {
        return Err(ConfigError::Invalid("source.emptyDocument".into()));
    }
    for repository in &mut document.repositories {
        if repository.kind != crate::models::RepositoryKind::Mod {
            return Err(ConfigError::Invalid("proxy.validationFailed".into()));
        }
        repository.network_plan = repository.network_plan.inherit(&document.network_plan);
        crate::config::normalize_repository_i18n(repository)?;
        repository.region = crate::config::normalize_repository_region_for_storage(
            repository.region.as_deref(),
            &crate::locale_tag::with_params(
                "config.repositoryLabeled",
                &[("name", &repository.name.to_string())],
            ),
        );
        /*
         * 顺序很重要：**先校验原文，再归一化**。
         *
         * 反过来做（先归一化）会把 `platform = "linux"` 悄悄变成 None，
         * 校验阶段再也看不到非法值 —— 一个本该拒绝的配置会静默变成
         * 「全平台可用」。归一化只负责把空串/别名整理成规范值。
         */
        crate::config::validate_repository_platform(
            repository.platform.as_deref(),
            &repository.name,
        )?;
        repository.platform =
            crate::config::normalize_repository_platform(repository.platform.as_deref());
        validate_mod_repository(repository)?;
    }
    let mut ids = std::collections::HashSet::new();
    let mut urls = std::collections::HashSet::new();
    for source in &mut document.repository_sources {
        source.network_plan = source.network_plan.inherit(&document.network_plan);
        source.id = source.id.trim().to_string();
        // 昵称只认多语言表（zh-CN 兜底），旧单语字段在此折叠
        let nickname = normalize_source_nickname(source)?;
        if source.id.is_empty() || nickname.is_empty() {
            return Err(ConfigError::Invalid("source.idOrNicknameEmpty".into()));
        }
        source.region = crate::config::normalize_repository_region_for_storage(
            source.region.as_deref(),
            &crate::locale_tag::with_params("source.labeled", &[("label", &source.id.to_string())]),
        );
        validate_repository_source_region(source.region.as_deref())?;
        source.url = SourceLocation::resolve(&source.url, Some(location))?.address();
        if !ids.insert(source.id.clone()) {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "source.idDuplicateValue",
                &[("id", &source.id.to_string())],
            )));
        }
    }
    document
        .repository_sources
        .retain(|source| urls.insert(source_address_key(&source.url, source.region.as_deref())));
    log::debug!(
        "仓库源文档解析完成：地址={}，仓库={}，子源={}",
        safe_log_address(&location.address()),
        document.repositories.len(),
        document.repository_sources.len()
    );
    Ok(document)
}

#[cfg(test)]
mod tests;
