//! A plan's proofs authorize only the selected Release, never a switch or a
//! different candidate on its temporarily consolidated node.
use super::{Inventory, MULTIPLE_INSTALLED_COPIES, ProvidedPlugin, RetiredModCopy};
use std::collections::BTreeMap;

#[derive(Clone, Default)]
pub(in crate::mod_changes) struct DuplicateReleases {
    pub(in crate::mod_changes) upgrades: BTreeMap<String, DuplicateReleaseUpgrade>,
    pub(in crate::mod_changes) blocked: Vec<ProvidedPlugin>,
}

#[derive(Clone)]
pub(in crate::mod_changes) struct DuplicateReleaseUpgrade {
    pub(in crate::mod_changes) candidate_id: String,
    pub(in crate::mod_changes) copies: Vec<RetiredModCopy>,
}

impl DuplicateReleases {
    pub(in crate::mod_changes) fn copies_for(
        &self,
        node_id: &str,
        candidate_id: &str,
    ) -> Option<&[RetiredModCopy]> {
        self.upgrades
            .get(node_id)
            .filter(|upgrade| upgrade.candidate_id == candidate_id)
            .map(|upgrade| upgrade.copies.as_slice())
    }

    pub(in crate::mod_changes) fn selected(
        &self,
        original: &Inventory,
        solved: &Inventory,
    ) -> Result<BTreeMap<String, Vec<RetiredModCopy>>, String> {
        let mut selected = BTreeMap::new();
        for id in self.upgrades.keys() {
            let before = original.nodes.iter().find(|node| &node.id == id);
            let after = solved.nodes.iter().find(|node| &node.id == id);
            let (Some(before), Some(after)) = (before, after) else {
                return Err("modChanges.targetChanged".into());
            };
            if before.current == after.current && before.enabled == after.enabled {
                continue;
            }
            let copies = after
                .current
                .as_ref()
                .filter(|_| after.enabled)
                .and_then(|candidate| self.copies_for(id, &candidate.id))
                .ok_or(MULTIPLE_INSTALLED_COPIES)?;
            selected.insert(id.clone(), copies.to_vec());
        }
        Ok(selected)
    }
}
