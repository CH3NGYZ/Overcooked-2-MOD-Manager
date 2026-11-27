use super::ConfigService;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

impl ConfigService {
    pub(crate) fn for_test(root: PathBuf) -> Self {
        Self {
            root,
            storage_lock: Default::default(),
            cache: Arc::new(Mutex::new(None)),
            proxies: Default::default(),
        }
    }

    pub(crate) fn set_official_address_for_test(
        &self,
        provider: crate::models::DataProvider,
        address: String,
    ) {
        self.proxies
            .set_official_address_for_test(provider, address);
    }
}
