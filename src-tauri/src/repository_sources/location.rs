use super::diagnostics::safe_log_address;
use crate::config::ConfigError;
use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLocation {
    Remote(Url),
    Bundled(String),
}

impl SourceLocation {
    pub fn parse(reference: &str) -> Result<Self, ConfigError> {
        let reference_kind = match Url::parse(reference.trim()) {
            Ok(_) => "absolute",
            Err(url::ParseError::RelativeUrlWithoutBase) => "relative",
            Err(_) => "invalid",
        };
        log::trace!("解析仓库源位置：引用类型={reference_kind}");
        Self::resolve(reference, None)
    }

    /// 解析相对当前文档的地址（子源、同仓库文件都走这里）。
    ///
    /// `pub(crate)`：地图源树与仓库源树共用同一套地址解析，包括重定向后的
    /// 相对基准、raw 仓库根拼接与内置资源相对路径。复制一份只会得到两套
    /// 会各自漂移的规则。
    pub(crate) fn resolve(reference: &str, parent: Option<&Self>) -> Result<Self, ConfigError> {
        let reference = reference.trim();
        if reference.is_empty() {
            return Err(ConfigError::Invalid("source.addressEmpty".into()));
        }
        match Url::parse(reference) {
            Ok(url) => Self::remote(url),
            Err(url::ParseError::RelativeUrlWithoutBase) => match parent {
                Some(Self::Remote(base)) => Self::resolve_remote_path(reference, base),
                _ => {
                    let base = match parent {
                        Some(Self::Bundled(path)) if is_file_relative_path(reference) => {
                            path.rsplit_once('/').map(|(dir, _)| dir)
                        }
                        _ => None,
                    };
                    Self::bundled_path(reference, base)
                }
            },
            Err(error) => Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "source.addressInvalid",
                &[("reason", &error.to_string())],
            ))),
        }
    }

    fn resolve_remote_path(reference: &str, base: &Url) -> Result<Self, ConfigError> {
        /*
         * Release 资产里的文档不许声明相对子源。
         *
         * 资产地址没有分支结构，`raw_repository_root` 认不出来，走 `base.join`
         * 只会拼出一个不存在的路径；而且下载后真正的地址是 CDN 签名 URL，
         * 连作者也无法预测。能表达「我要哪个文档」的只有绝对地址。
         */
        if crate::releases::release_asset_reference(base).is_some() {
            return Err(ConfigError::Invalid(crate::locale_tag::with_params(
                "source.relativeChildInReleaseAsset",
                &[("source", &safe_log_address(base.as_str()))],
            )));
        }
        let Some(root) = raw_repository_root(base) else {
            return Self::remote(base.join(reference).map_err(|error| {
                ConfigError::Invalid(crate::locale_tag::with_params(
                    "source.addressInvalid",
                    &[("reason", &error.to_string())],
                ))
            })?);
        };
        if reference.starts_with("//") {
            return Self::remote(base.join(reference).map_err(|error| {
                ConfigError::Invalid(crate::locale_tag::with_params(
                    "source.addressInvalid",
                    &[("reason", &error.to_string())],
                ))
            })?);
        }
        let resolved = if is_file_relative_path(reference) || reference.starts_with(['?', '#']) {
            base.join(reference)
        } else {
            root.join(reference.trim_start_matches('/'))
        }
        .map_err(|error| {
            ConfigError::Invalid(crate::locale_tag::with_params(
                "source.addressInvalid",
                &[("reason", &error.to_string())],
            ))
        })?;
        if resolved.origin() != root.origin() || !resolved.path().starts_with(root.path()) {
            return Err(ConfigError::Invalid("source.pathEscapesBranch".into()));
        }
        Self::remote(resolved)
    }

    fn remote(url: Url) -> Result<Self, ConfigError> {
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return Err(ConfigError::Invalid("source.mustBeHttp".into()));
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(ConfigError::Invalid("source.addressHasCredentials".into()));
        }
        Ok(Self::Remote(url))
    }

    fn bundled_path(reference: &str, base: Option<&str>) -> Result<Self, ConfigError> {
        if reference.starts_with(['/', '\\']) || reference.contains(['\\', ':', '?', '#']) {
            return Err(ConfigError::Invalid("source.builtinPathUnsafe".into()));
        }
        let mut components = base
            .map(|base| base.split('/').collect::<Vec<_>>())
            .unwrap_or_default();
        for component in reference.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    if components.pop().is_none() {
                        return Err(ConfigError::Invalid("source.builtinPathUnsafe".into()));
                    }
                }
                value => components.push(value),
            }
        }
        if components.is_empty()
            || !components
                .last()
                .is_some_and(|name| name.ends_with(".toml"))
        {
            return Err(ConfigError::Invalid("source.builtinNotToml".into()));
        }
        Ok(Self::Bundled(components.join("/")))
    }

    pub fn address(&self) -> String {
        match self {
            Self::Remote(url) => url.to_string(),
            Self::Bundled(path) => path.clone(),
        }
    }
}

fn is_file_relative_path(reference: &str) -> bool {
    reference.starts_with("./") || reference.starts_with("../")
}

fn raw_repository_root(url: &Url) -> Option<Url> {
    let segments = url.path_segments()?.collect::<Vec<_>>();
    let ref_start = match url.host_str()? {
        "github.com"
        | "www.github.com"
        | "gitee.com"
        | "www.gitee.com"
        | "raw.giteeusercontent.com"
            if segments.get(2) == Some(&"raw") =>
        {
            3
        }
        "raw.githubusercontent.com" | "raw.github.com" => 2,
        _ => return None,
    };
    if segments.first()?.is_empty() || segments.get(1)?.is_empty() {
        return None;
    }
    let ref_length = if segments.get(ref_start) == Some(&"refs")
        && matches!(segments.get(ref_start + 1), Some(&"heads" | &"tags"))
    {
        3
    } else {
        1
    };
    let root_length = ref_start + ref_length;
    if segments
        .get(ref_start..root_length)?
        .iter()
        .any(|segment| segment.is_empty())
    {
        return None;
    }
    let mut root = url.clone();
    {
        let mut path = root.path_segments_mut().ok()?;
        for _ in root_length..segments.len() {
            path.pop();
        }
        path.push("");
    }
    root.set_query(None);
    root.set_fragment(None);
    Some(root)
}

pub(super) fn remote_document_location(
    requested: &Url,
    redirected: Url,
) -> Result<SourceLocation, ConfigError> {
    let redirected = SourceLocation::remote(redirected)?;
    // Raw endpoints can redirect to a CDN with a different path layout. Keep
    // the original repository ref as the base for its relative child sources.
    if raw_repository_root(requested).is_some() {
        Ok(SourceLocation::Remote(requested.clone()))
    } else {
        Ok(redirected)
    }
}

#[cfg(test)]
mod tests;
