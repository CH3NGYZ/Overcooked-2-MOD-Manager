use super::*;

#[derive(Default)]
pub(super) struct MapBundleSwitches {
    members: BTreeMap<usize, Vec<usize>>,
    owners: BTreeMap<usize, usize>,
}

impl MapBundleSwitches {
    pub(super) fn installed(inventory: &Inventory, trusted: Option<&TrustedInventory>) -> Self {
        let Some(trusted) = trusted else {
            return Self::default();
        };
        let maps: BTreeMap<_, _> = inventory
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| node.kind == NodeKind::Map && node.current.is_some())
            .map(|(index, node)| (node.id.as_str(), index))
            .collect();
        let mut links = Self::default();
        for (index, node) in inventory.nodes.iter().enumerate() {
            if !node.is_registered_map_plugin() || node.current.is_none() {
                continue;
            }
            if let Some(owner) = trusted
                .files
                .get(&node.id)
                .and_then(|files| files.summary.bundle_parent_node_id.as_deref())
                .and_then(|id| maps.get(id).copied())
            {
                links.members.entry(owner).or_default().push(index);
                links.owners.insert(index, owner);
            }
        }
        links
    }

    pub(super) fn owner(&self, index: usize) -> Option<usize> {
        self.owners.get(&index).copied()
    }

    pub(super) fn members(&self, owner: usize) -> &[usize] {
        self.members.get(&owner).map_or(&[], Vec::as_slice)
    }

    pub(super) fn synchronize(
        &self,
        state: &mut SearchState,
        owner: usize,
        source: DisableSource,
        locked: impl Fn(usize) -> bool,
    ) -> Option<()> {
        let enabled = state.inventory.nodes[owner].enabled;
        let reason = Arc::new(ChangeReason {
            kind: ReasonKind::RequiredBy,
            related_node_ids: vec![state.inventory.nodes[owner].id.clone()],
            target_guid: None,
            requirement: None,
            prerelease: None,
            condition_on_related: false,
        });
        for &index in self.members(owner) {
            let node = &state.inventory.nodes[index];
            let [member] = node.current.as_ref()?.provides.as_slice() else {
                return None;
            };
            if !node.mutable {
                return None;
            }
            if node.enabled == enabled && member.enabled == enabled {
                continue;
            }
            if locked(index) {
                return None;
            }
            let disabled =
                (!enabled && node.enabled && member.enabled && !member.guid.trim().is_empty())
                    .then(|| member.guid.trim().to_lowercase());
            let node = &mut state.inventory.nodes[index];
            let before = node.clone();
            node.enabled = enabled;
            node.current.as_mut()?.provides[0].enabled = enabled;
            state.reasons.record(index, &before, node, &reason);
            state.decided.insert(index);
            if let Some(guid) = disabled {
                source.guids(state).insert(guid);
            }
        }
        Some(())
    }
}
