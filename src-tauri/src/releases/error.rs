use reqwest::StatusCode;

#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error("Release operation failed: {0}")]
    InvalidUrl(String),
    #[error("Release operation failed: {0}")]
    Http(String),
    #[error("Release operation failed: {0}")]
    Status(StatusCode),
    /// 403/429 的响应及候选限流信号；是否封锁由闸门独立判断。
    #[error("Release operation failed: rate limited ({status})")]
    RateLimited {
        status: StatusCode,
        signal: Box<crate::api_rate_gate::RateLimitSignal>,
    },
    #[error("Release operation failed: {0}")]
    Json(String),
}

impl ReleaseError {
    pub fn is_forbidden(&self) -> bool {
        matches!(
            self,
            Self::Status(status) if *status == StatusCode::FORBIDDEN
        ) || matches!(self, Self::RateLimited { status, .. } if *status == StatusCode::FORBIDDEN)
    }

    /// 提供 403/429 的信号，不代表其中必有足够的限流证据。
    pub fn rate_limit_signal(&self) -> Option<&crate::api_rate_gate::RateLimitSignal> {
        match self {
            Self::RateLimited { signal, .. } => Some(signal),
            _ => None,
        }
    }
}

pub(super) fn release_error_kind(error: &ReleaseError) -> &'static str {
    match error {
        ReleaseError::InvalidUrl(_) => "invalid_url",
        ReleaseError::Http(_) => "http",
        ReleaseError::Status(status) if *status == StatusCode::FORBIDDEN => "http_403",
        ReleaseError::Status(status) if *status == StatusCode::TOO_MANY_REQUESTS => "http_429",
        ReleaseError::Status(_) => "http_status",
        ReleaseError::RateLimited { status, .. } if *status == StatusCode::TOO_MANY_REQUESTS => {
            "rate_limited_429"
        }
        ReleaseError::RateLimited { .. } => "rate_limited_403",
        ReleaseError::Json(_) => "json",
    }
}
