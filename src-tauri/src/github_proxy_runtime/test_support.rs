use super::ProxyRuntime;
use crate::models::GitHubProxyConfig;

impl ProxyRuntime {
    pub(crate) fn set_official_address_for_test(
        &self,
        provider: crate::models::DataProvider,
        address: String,
    ) {
        let mut addresses = self.official_addresses.write().unwrap();
        addresses.retain(|(key, _)| *key != provider);
        addresses.push((provider, address));
    }

    pub(crate) fn replace_for_test(&self, config: GitHubProxyConfig) {
        self.current.write().unwrap().groups = config;
    }
}
