use super::SearchRateLimits;
use crate::models::Provider;
use std::time::Duration;

impl SearchRateLimits {
    pub(crate) fn for_gitee_test() -> Self {
        Self {
            provider: Provider::Gitee,
            interval: Duration::ZERO,
            ..Default::default()
        }
    }
}
