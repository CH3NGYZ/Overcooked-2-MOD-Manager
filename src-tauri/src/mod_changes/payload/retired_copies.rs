use super::{ProvidedPlugin, TrustedInventory, mods};
use crate::mod_changes::{evaluate::same_guid, inventory::RetiredModCopy};
use std::cmp::Ordering;

pub(super) const INVALID: &str = "modChanges.duplicateReleasePayload";

pub(super) fn contains(copies: Option<&[RetiredModCopy]>, plugin: &ProvidedPlugin) -> bool {
    copies.is_some_and(|copies| {
        copies
            .iter()
            .any(|copy| plugin.member_key.as_ref() == Some(&copy.member_key))
    })
}

/// Advertised tags do not authorize retiring a DLL. Check the real core and
/// reject a package that would reinstall a GUID it is meant to retire.
pub(super) fn verify(
    trusted: &TrustedInventory,
    node_id: &str,
    copies: &[RetiredModCopy],
    guid: &str,
    actual: &[ProvidedPlugin],
) -> Result<(), String> {
    let mut core = actual.iter().filter(|plugin| same_guid(&plugin.guid, guid));
    let first = core.next().ok_or(INVALID)?;
    let version = first.version.as_deref().ok_or(INVALID)?;
    if !first.enabled || core.next().is_some() {
        return Err(INVALID.into());
    }
    let old = trusted
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .and_then(|node| node.current.as_ref())
        .ok_or(INVALID)?;
    let previous = old.version.as_deref().ok_or(INVALID)?;
    let is_newer_than =
        |previous| mods::compare_mod_versions(version, previous) == Some(Ordering::Greater);
    if !is_newer_than(previous) || copies.iter().any(|copy| !is_newer_than(&copy.version)) {
        return Err(INVALID.into());
    }
    for plugin in &old.provides {
        if contains(Some(copies), plugin)
            && !same_guid(&plugin.guid, guid)
            && actual.iter().any(|new| same_guid(&new.guid, &plugin.guid))
        {
            return Err(INVALID.into());
        }
    }
    Ok(())
}
