use super::{SearchHit, classify_search_item};
use crate::models::ModRepositoryConfig;
use serde_json::Value;

pub(in crate::community) fn repository_from_search(item: &Value) -> Option<ModRepositoryConfig> {
    match classify_search_item(item) {
        SearchHit::Mod(repository) => Some(repository),
        _ => None,
    }
}
