//! Consume the same physical bundle anchors as the local MOD cards.
use super::*;

fn standalone_key(item: &InstalledMod) -> String {
    item.package_id
        .as_ref()
        .map(|id| format!("package:{id}"))
        .unwrap_or_else(|| format!("dll:{}", key(&item.dll_file)))
}

pub(super) fn group_mods(items: Vec<InstalledMod>) -> BTreeMap<String, Vec<InstalledMod>> {
    // A loose declaration may join a real manifest group via its core DLL, but
    // the loose DLL must not acquire that manifest ID or removal authority.
    let anchors = items
        .iter()
        .filter(|item| item.bundled_with_dll.is_none())
        .map(|item| (key(&item.dll_file), standalone_key(item)))
        .collect::<BTreeMap<_, _>>();
    let mut groups: BTreeMap<String, Vec<InstalledMod>> = BTreeMap::new();
    for mut item in items {
        let owner = item
            .bundled_with_dll
            .as_deref()
            .and_then(|dll| anchors.get(&key(dll)))
            .filter(|_| item.package_id.is_none());
        let group = if let Some(owner) = owner {
            owner.clone()
        } else {
            item.bundled_with_dll = None;
            standalone_key(&item)
        };
        groups.entry(group).or_default().push(item);
    }
    groups
}

impl TrustedInventory {
    pub(in crate::mod_changes) fn verify_declared_bundle_overlaps(
        &self,
        id: &str,
        repo: Option<&ModRepositoryConfig>,
        incoming: &[ProvidedPlugin],
    ) -> Result<(), String> {
        use crate::mod_changes::evaluate::same_guid;
        for old in self.declared_bundle_members(id) {
            let Some(path) = old.member_key.as_deref() else {
                return Err("modChanges.ambiguousOwnership".into());
            };
            let mut overlaps = incoming.iter().filter(|new| {
                // GUID-less libraries only identify themselves by path.
                new.member_key.as_deref() == Some(path)
                    || (!old.guid.trim().is_empty() && same_guid(&new.guid, &old.guid))
            });
            let Some(new) = overlaps.next() else { continue };
            let legacy_diylevel = old.guid.trim().is_empty()
                && repo.is_some_and(|repo| {
                    same_guid(&repo.plugin_guid, mods::plugin_target::DIYLEVEL_GUID)
                        && self.files.get(id).is_some_and(|files| {
                            files.summary.repository_id.as_ref() == Some(&repo.id)
                                && files.summary.members.iter().any(|member| {
                                    member
                                        .guid
                                        .as_deref()
                                        .is_some_and(|guid| same_guid(guid, &repo.plugin_guid))
                                        && mods::legacy_diylevel_stub(&member.key, path)
                                })
                        })
                });
            let explicitly_declared = repo.is_some_and(|repo| {
                self.files
                    .get(id)
                    .and_then(|files| files.summary.repository_id.as_ref())
                    == Some(&repo.id)
                    && repo
                        .bundled_plugin_guids
                        .iter()
                        .any(|guid| same_guid(guid, &old.guid))
            });
            if !(explicitly_declared || legacy_diylevel)
                || overlaps.next().is_some()
                || new.member_key.as_deref() != Some(path)
                || !same_guid(&new.guid, &old.guid)
            {
                return Err(crate::mod_changes::mutations::file_conflict(id, path));
            }
        }
        Ok(())
    }

    pub(in crate::mod_changes) fn atomic_mod_package(&self, id: &str) -> bool {
        self.files.get(id).is_some_and(|node| {
            node.summary.kind == NodeKind::Mod
                && node.summary.members.len() > 1
                && (!node.manifests.is_empty()
                    || self.declared_bundle_switch_members.contains_key(id))
        })
    }

    pub(in crate::mod_changes) fn declared_bundle_members(
        &self,
        id: &str,
    ) -> impl Iterator<Item = &ProvidedPlugin> {
        let keys = self.declared_bundle_switch_members.get(id);
        self.inventory
            .nodes
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.current.as_ref())
            .into_iter()
            .flat_map(|candidate| &candidate.provides)
            .filter(move |member| {
                keys.is_some_and(|keys| {
                    member
                        .member_key
                        .as_ref()
                        .is_some_and(|key| keys.contains(key))
                })
            })
    }
}
