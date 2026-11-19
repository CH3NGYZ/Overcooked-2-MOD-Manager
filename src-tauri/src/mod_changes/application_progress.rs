//! Associate journal records with visible actions without persisting UI state.
use super::transaction::{Mutation, RecordProgress};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub(super) struct ItemKey {
    node_id: String,
    member_key: Option<String>,
}

impl ItemKey {
    pub(super) fn new(node_id: &str, member_key: Option<&str>) -> Self {
        Self {
            node_id: node_id.into(),
            member_key: member_key.map(str::to_owned),
        }
    }
}

#[derive(Debug)]
pub(super) struct OwnedMutation {
    pub mutation: Mutation,
    pub items: BTreeSet<ItemKey>,
}

impl OwnedMutation {
    pub(super) fn new(mutation: Mutation, items: impl IntoIterator<Item = ItemKey>) -> Self {
        Self {
            mutation,
            items: items.into_iter().collect(),
        }
    }

    pub(super) fn for_node(mutation: Mutation, node_id: &str) -> Self {
        Self::new(mutation, [ItemKey::new(node_id, None)])
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Stage {
    Applying,
    Processed,
    Applied,
    Failed,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct ItemProgress {
    #[serde(flatten)]
    key: ItemKey,
    stage: Stage,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(crate) struct ChangeApplicationProgress {
    plan_id: String,
    items: Vec<ItemProgress>,
}

pub(super) struct Recorder<'a, F> {
    plan_id: &'a str,
    report: F,
    records: Vec<BTreeSet<ItemKey>>,
    remaining: BTreeMap<ItemKey, usize>,
    touched: BTreeSet<ItemKey>,
}

impl<'a, F: FnMut(ChangeApplicationProgress)> Recorder<'a, F> {
    pub(super) fn new(
        plan_id: &'a str,
        visible: impl IntoIterator<Item = ItemKey>,
        mut records: Vec<BTreeSet<ItemKey>>,
        report: F,
    ) -> Self {
        let visible = visible.into_iter().collect::<BTreeSet<_>>();
        let mut remaining = BTreeMap::new();
        for items in &mut records {
            items.retain(|key| visible.contains(key));
            for key in items.iter() {
                *remaining.entry(key.clone()).or_default() += 1;
            }
        }
        Self {
            plan_id,
            report,
            records,
            remaining,
            touched: BTreeSet::new(),
        }
    }

    pub(super) fn observe(&mut self, event: RecordProgress) {
        match event {
            RecordProgress::Applying(index) => {
                let keys = self.records[index]
                    .iter()
                    .filter(|key| self.touched.insert((*key).clone()))
                    .cloned()
                    .collect();
                self.emit(Stage::Applying, keys);
            }
            RecordProgress::Processed(index) => {
                let keys = self.records[index]
                    .iter()
                    .filter(|key| {
                        let remaining = self.remaining.get_mut(*key).expect("indexed record owner");
                        *remaining -= 1;
                        *remaining == 0
                    })
                    .cloned()
                    .collect();
                self.emit(Stage::Processed, keys);
            }
            RecordProgress::Committed => self.finish(Stage::Applied),
        }
    }

    pub(super) fn failed(&mut self) {
        self.finish(Stage::Failed);
    }

    fn finish(&mut self, stage: Stage) {
        let keys = std::mem::take(&mut self.touched).into_iter().collect();
        self.emit(stage, keys);
    }

    fn emit(&mut self, stage: Stage, keys: Vec<ItemKey>) {
        if !keys.is_empty() {
            (self.report)(ChangeApplicationProgress {
                plan_id: self.plan_id.into(),
                items: keys
                    .into_iter()
                    .map(|key| ItemProgress { key, stage })
                    .collect(),
            });
        }
    }
}
