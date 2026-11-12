//! The only prefer parser and lossless editor.
mod document;
mod edit;
pub(crate) use document::{Document, LineKind, decode};
pub(crate) use edit::plan;

/// Known-chef hats are always derived from the complete binding map.
/// A missing binding deliberately means automatic, never an old row value.
pub(crate) fn project(draft: &crate::models::ChefDraft) -> crate::models::ChefDraft {
    use crate::models::{ChefReference, HatReference};
    let mut projected = draft.clone();
    for entry in &mut projected.entries {
        if let ChefReference::Known { chef_id } = &entry.chef_ref {
            entry.hat_ref = projected
                .hat_bindings
                .get(chef_id)
                .cloned()
                .unwrap_or_else(|| HatReference::Known {
                    hat_id: "hat:".into(),
                });
        }
    }
    projected
}

pub(super) const SWITCH_LINE: &str = "LOBBYSWITCHCHEF=TRUE";
pub(crate) const DEFAULT_CONTENT: &[u8] = b"Chef_Black_cat\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n";

#[cfg(test)]
mod tests;
