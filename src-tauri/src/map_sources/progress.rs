//! Source declarations survive an empty/failed document so it remains retryable.
use super::{document::MapSourceDeclaration, state::SourceLabel};
use crate::models::{MapInfo, MapRepositoryConfig};
use crate::repository_sources::SourceLocation;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct SourceTarget {
    pub node: SourceNode,
    pub sequence: usize,
    pub origin: SourceLocation,
    pub label: SourceLabel,
    pub builtin: bool,
    pub depth: usize,
    pub declaration: Option<MapSourceDeclaration>,
}

#[derive(Default)]
pub(crate) struct SourceRegistry {
    entries: Mutex<BTreeMap<String, BTreeMap<String, SourceTarget>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceNode {
    pub id: String,
    pub parent_id: Option<String>,
    pub label: String,
    pub label_i18n: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub note_i18n: BTreeMap<String, String>,
    pub url: String,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct MapTreeProgress {
    pub nodes: Vec<SourceNode>,
    pub maps: Vec<MapInfo>,
}

pub struct SourceObserver {
    nodes: Mutex<Vec<SourceNode>>,
    registry: Arc<SourceRegistry>,
    namespace: String,
    send: Box<dyn Fn(MapTreeProgress) + Send + Sync>,
}

impl SourceObserver {
    #[cfg(test)]
    pub fn new(send: impl Fn(MapTreeProgress) + Send + Sync + 'static) -> Self {
        Self {
            nodes: Mutex::new(Vec::new()),
            registry: Arc::default(),
            namespace: String::new(),
            send: Box::new(send),
        }
    }

    pub fn with_registry(
        send: impl Fn(MapTreeProgress) + Send + Sync + 'static,
        registry: Arc<SourceRegistry>,
        namespace: String,
    ) -> Self {
        Self {
            nodes: Mutex::new(Vec::new()),
            send: Box::new(send),
            registry,
            namespace,
        }
    }

    pub(super) fn register(&self, mut target: SourceTarget) {
        let mut all = self
            .registry
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let entries = all.entry(self.namespace.clone()).or_default();
        target.sequence = entries
            .get(&target.node.id)
            .map(|entry| entry.sequence)
            .unwrap_or_else(|| {
                entries
                    .values()
                    .map(|entry| entry.sequence)
                    .max()
                    .unwrap_or(0)
                    + 1
            });
        entries.insert(target.node.id.clone(), target);
    }

    pub(super) fn begin(
        &self,
        id: Option<&str>,
    ) -> Option<(SourceTarget, std::collections::HashSet<String>)> {
        let mut all = self
            .registry
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let entries = all.entry(self.namespace.clone()).or_default();
        let Some(id) = id else {
            entries.clear();
            return None;
        };
        let target = entries.get(id)?.clone();
        let mut ids = std::collections::HashSet::from([id.to_owned()]);
        loop {
            let previous = ids.len();
            for entry in entries.values() {
                if entry
                    .node
                    .parent_id
                    .as_ref()
                    .is_some_and(|parent| ids.contains(parent))
                {
                    ids.insert(entry.node.id.clone());
                }
            }
            if ids.len() == previous {
                break;
            }
        }
        entries.retain(|key, _| key == id || !ids.contains(key));
        let mut targets: Vec<_> = entries.values().collect();
        targets.sort_by_key(|target| target.sequence);
        let nodes = targets.into_iter().map(|t| t.node.clone()).collect();
        drop(all);
        *self.nodes.lock().unwrap_or_else(|e| e.into_inner()) = nodes;
        Some((target, ids))
    }

    pub(super) fn remove(&self, ids: &std::collections::HashSet<String>) {
        self.nodes
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|node| !ids.contains(&node.id));
        if let Some(entries) = self
            .registry
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&self.namespace)
        {
            entries.retain(|id, _| !ids.contains(id));
        }
    }

    pub(super) fn declare(&self, node: SourceNode) {
        let mut nodes = self.nodes.lock().unwrap_or_else(|e| e.into_inner());
        if !nodes.iter().any(|n| n.id == node.id) {
            nodes.push(node);
        }
    }

    pub(super) fn publish(
        &self,
        id: &str,
        status: &str,
        error: Option<String>,
        maps: &[MapRepositoryConfig],
    ) {
        let nodes = {
            let mut nodes = self.nodes.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(node) = nodes.iter_mut().find(|n| n.id == id) {
                node.status = status.into();
                node.error = error;
            }
            let snapshot = nodes.clone();
            if let Some(target) = self
                .registry
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get_mut(&self.namespace)
                .and_then(|entries| entries.get_mut(id))
            {
                if let Some(node) = snapshot.iter().find(|node| node.id == id) {
                    target.node = node.clone();
                }
            }
            snapshot
        };
        (self.send)(MapTreeProgress {
            nodes,
            maps: maps
                .iter()
                .filter(|map| {
                    super::state::map_is_available(map, crate::config::current_user_region())
                })
                .map(super::view::map_to_info)
                .collect(),
        });
    }
}
