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
    pub excluded_map_identities: Vec<ExcludedMapIdentity>,
}

/// A declaration already read, but not in the available map list.
#[derive(Clone, Serialize)]
pub struct ExcludedMapIdentity {
    pub source_id: String,
    pub level_set_name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub level_set_name_zh: String,
    pub author: String,
}

pub struct SourceObserver {
    nodes: Mutex<Vec<SourceNode>>,
    excluded_map_identities: Mutex<Vec<ExcludedMapIdentity>>,
    registry: Arc<SourceRegistry>,
    namespace: String,
    region: String,
    send: Box<dyn Fn(MapTreeProgress) + Send + Sync>,
}

impl SourceObserver {
    #[cfg(test)]
    pub fn new(region: &str, send: impl Fn(MapTreeProgress) + Send + Sync + 'static) -> Self {
        Self {
            nodes: Mutex::new(Vec::new()),
            excluded_map_identities: Mutex::default(),
            registry: Arc::default(),
            namespace: String::new(),
            region: region.into(),
            send: Box::new(send),
        }
    }

    pub fn with_registry(
        send: impl Fn(MapTreeProgress) + Send + Sync + 'static,
        registry: Arc<SourceRegistry>,
        namespace: String,
        region: &str,
    ) -> Self {
        Self {
            nodes: Mutex::new(Vec::new()),
            excluded_map_identities: Mutex::default(),
            send: Box::new(send),
            registry,
            namespace,
            region: region.into(),
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

    /// A parsed root owns the next tree; failed reads must keep old retry targets.
    pub(super) fn commit_root(&self, id: &str) {
        let mut nodes = self.nodes.lock().unwrap_or_else(|e| e.into_inner());
        nodes.retain(|node| node.id == id);
        if let Some(entries) = self
            .registry
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&self.namespace)
        {
            entries.retain(|key, _| key == id);
        }
    }

    pub(super) fn begin(
        &self,
        id: &str,
    ) -> Option<(SourceTarget, std::collections::HashSet<String>)> {
        let mut all = self
            .registry
            .entries
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let entries = all.entry(self.namespace.clone()).or_default();
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

    pub(super) fn record_excluded_identity(
        &self,
        source_id: &str,
        level_set_name: &str,
        level_set_name_zh: &str,
        author: &str,
    ) {
        self.excluded_map_identities
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(ExcludedMapIdentity {
                source_id: source_id.into(),
                level_set_name: level_set_name.trim().into(),
                level_set_name_zh: level_set_name_zh.trim().into(),
                author: author.trim().into(),
            });
    }

    /// Assemble the authoritative reply after saving, without depending on Channel delivery.
    pub(crate) fn finish(&self, maps: Vec<MapInfo>) -> MapTreeProgress {
        let nodes = self.nodes.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let excluded_map_identities = std::mem::take(
            &mut *self
                .excluded_map_identities
                .lock()
                .unwrap_or_else(|e| e.into_inner()),
        );
        MapTreeProgress {
            nodes,
            maps,
            excluded_map_identities,
        }
    }

    /// Scheduling may prioritize installed repositories; display and storage use tree order.
    pub(super) fn sort_by_source<T>(
        &self,
        items: &mut [T],
        source_id: impl Fn(&T) -> Option<&str>,
    ) {
        let order: BTreeMap<_, _> = {
            let all = self
                .registry
                .entries
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let Some(entries) = all.get(&self.namespace) else {
                return;
            };
            entries
                .iter()
                .filter_map(|(id, target)| {
                    let mut path = Vec::new();
                    let mut seen = std::collections::HashSet::new();
                    let mut current = target;
                    loop {
                        if !seen.insert(&current.node.id) {
                            return None;
                        }
                        path.push(current.sequence);
                        let Some(parent) = &current.node.parent_id else {
                            break;
                        };
                        // An orphan or malformed cycle cannot establish a declaration rank.
                        current = entries.get(parent)?;
                    }
                    path.reverse();
                    Some((id.clone(), path))
                })
                .collect()
        };
        // Stable sorting preserves map order within a source and unknown/community tails.
        items.sort_by_cached_key(|item| {
            let path = source_id(item).and_then(|id| order.get(id));
            (path.is_none(), path.cloned())
        });
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
        let mut maps: Vec<_> = maps
            .iter()
            .filter(|map| super::state::map_is_available(map, &self.region))
            .collect();
        self.sort_by_source(&mut maps, |map| map.source_id.as_deref());
        (self.send)(MapTreeProgress {
            nodes,
            maps: maps.into_iter().map(super::view::map_to_info).collect(),
            excluded_map_identities: Vec::new(),
        });
    }
}
