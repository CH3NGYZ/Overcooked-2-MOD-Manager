use super::ConfigService;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

impl ConfigService {
    pub(crate) fn for_test(root: PathBuf) -> Self {
        Self {
            root,
            cache: Arc::new(Mutex::new(None)),
            proxies: Default::default(),
        }
    }
}
