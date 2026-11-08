//! Repository request-plan references and sparse, inheritable overrides.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RepositoryKind {
    #[default]
    Mod,
    Map,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum PlanStep {
    Official,
    Direct,
    Proxy { id: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RouteOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Vec<PlanStep>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub race: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallelism: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub download_threads: Option<u32>,
}
impl RouteOverride {
    fn inherit(&self, parent: &Self) -> Self {
        Self {
            route: self.route.clone().or_else(|| parent.route.clone()),
            race: self.race.or(parent.race),
            max_parallelism: self.max_parallelism.or(parent.max_parallelism),
            download_threads: self.download_threads.or(parent.download_threads),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<RouteOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release: Option<RouteOverride>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw: Option<RouteOverride>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PlanReference {
    Id(String),
    Override(PlanOverride),
}
impl PlanReference {
    pub fn override_value(&self) -> PlanOverride {
        match self {
            Self::Id(id) => PlanOverride {
                id: Some(id.clone()),
                ..Default::default()
            },
            Self::Override(value) => value.clone(),
        }
    }
    fn inherit(&self, parent: &Self) -> Self {
        let mut child = self.override_value();
        // An explicit ID selects a fresh plan, even when equal to the parent ID.
        if child.id.is_some() {
            return Self::Override(child);
        }
        let parent = parent.override_value();
        child.id = parent.id;
        let merge =
            |child: Option<RouteOverride>, parent: Option<RouteOverride>| match (child, parent) {
                (Some(child), Some(parent)) => Some(child.inherit(&parent)),
                (child, parent) => child.or(parent),
            };
        child.api = merge(child.api, parent.api);
        child.release = merge(child.release, parent.release);
        child.raw = merge(child.raw, parent.raw);
        Self::Override(child)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NetworkPlanSelection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mainland: Option<PlanReference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub international: Option<PlanReference>,
}
impl NetworkPlanSelection {
    pub fn is_empty(&self) -> bool {
        self.mainland.is_none() && self.international.is_none()
    }
    /// The receiver is the nearer declaration. Missing fields inherit.
    pub fn inherit(&self, parent: &Self) -> Self {
        let merge =
            |child: &Option<PlanReference>, parent: &Option<PlanReference>| match (child, parent) {
                (Some(child), Some(parent)) => Some(child.inherit(parent)),
                (child, parent) => child.clone().or_else(|| parent.clone()),
            };
        Self {
            mainland: merge(&self.mainland, &parent.mainland),
            international: merge(&self.international, &parent.international),
        }
    }
}
