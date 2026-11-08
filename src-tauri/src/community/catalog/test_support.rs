use super::{SearchHit, classify_search_item};
use crate::models::RepositoryConfig;
use serde_json::Value;

pub(in crate::community) fn repository_from_search(item: &Value) -> Option<RepositoryConfig> {
    match classify_search_item(item) {
        SearchHit::Mod(repository) => Some(repository),
        _ => None,
    }
}
