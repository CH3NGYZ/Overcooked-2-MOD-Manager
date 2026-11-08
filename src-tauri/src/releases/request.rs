use crate::models::{NetworkPlanSelection, Provider, RepositoryKind};

/// Only the inputs needed to read a hosted repository. No plugin identity,
/// install target, asset selection rules or mutable business cache belongs here.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct RepositoryRequest {
    pub id: String,
    pub provider: Provider,
    pub username: String,
    pub repository: String,
    #[serde(default)]
    pub kind: RepositoryKind,
    #[serde(default)]
    pub network_plan: NetworkPlanSelection,
    #[serde(default)]
    pub source_id: Option<String>,
}

impl From<&crate::models::ModRepositoryConfig> for RepositoryRequest {
    fn from(repo: &crate::models::ModRepositoryConfig) -> Self {
        Self {
            id: repo.id.clone(),
            provider: repo.provider.clone(),
            username: repo.username.clone(),
            repository: repo.repository.clone(),
            kind: repo.kind,
            network_plan: repo.network_plan.clone(),
            source_id: repo.source_id.clone(),
        }
    }
}

impl From<&crate::models::MapRepositoryConfig> for RepositoryRequest {
    fn from(map: &crate::models::MapRepositoryConfig) -> Self {
        Self {
            id: map.id.clone(),
            provider: map.provider.clone(),
            username: map.username.clone(),
            repository: map.repository.clone(),
            kind: RepositoryKind::Map,
            network_plan: map.network_plan.clone(),
            source_id: None,
        }
    }
}
