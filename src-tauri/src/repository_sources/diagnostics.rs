use crate::config::ConfigError;
use url::Url;

pub(crate) fn safe_log_address(value: &str) -> String {
    let Ok(mut url) = Url::parse(value) else {
        return value.to_string();
    };
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.set_query(None);
    url.set_fragment(None);
    url.to_string()
}

pub(crate) fn error_category(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::Directory(_) => "directory",
        ConfigError::Read(_) => "read",
        ConfigError::Parse(_) => "parse",
        ConfigError::Write(_) => "write",
        ConfigError::Invalid(_) => "invalid",
    }
}

#[cfg(test)]
mod tests;
