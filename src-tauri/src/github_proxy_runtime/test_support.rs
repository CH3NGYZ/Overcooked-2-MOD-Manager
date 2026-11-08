use super::ProxyRuntime;
use crate::models::GitHubProxyConfig;

impl ProxyRuntime {
    pub(crate) fn replace_for_test(&self, config: GitHubProxyConfig) {
        *self.current.write().unwrap() = config;
    }
}
