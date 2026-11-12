use super::{
    catalog::Catalog,
    prefer::{self, Document},
};
use crate::models::*;
use std::collections::{HashMap, HashSet};
pub(crate) const NATIVE_DEFAULT_REQUIRED: &str = "diychef.nativeDefaultRequired";
pub(crate) fn diagnostic(code: &str, row: Option<String>, blocking: bool) -> ChefDiagnostic {
    ChefDiagnostic {
        id: format!("{code}:{}", row.as_deref().unwrap_or("")),
        severity: if blocking { "error" } else { "warning" }.into(),
        code: code.into(),
        chef_id: None,
        hat_id: None,
        row_id: row,
        save_blocking: blocking,
    }
}
pub(crate) fn validate(
    doc: &Document,
    c: &Catalog,
    draft: &ChefDraft,
    supports_custom_default: bool,
) -> Result<ChefValidation, String> {
    let projected = prefer::project(draft);
    let draft = &projected;
    for (chef_id, hat) in &draft.hat_bindings {
        if !c.chefs.iter().any(|chef| chef.id == *chef_id) {
            return Err("diychef.invalidDraft".into());
        }
        if !matches!(hat, HatReference::Known { hat_id } if c.hats.iter().any(|item| item.id == *hat_id && item.resource_state != "invalid"))
        {
            return Err("diychef.invalidDraft".into());
        }
    }
    let baseline = doc.selection(c);
    let old: HashMap<_, _> = baseline
        .entries
        .iter()
        .filter_map(|e| e.row_id.as_ref().map(|r| (r, e)))
        .collect();
    let mut diagnostics = vec![];
    let mut first_usable = None;
    let mut seen = HashSet::new();
    let mut ids = HashMap::new();
    let mut heads = HashMap::new();
    for e in &draft.entries {
        let unchanged = e
            .row_id
            .as_ref()
            .and_then(|r| old.get(r))
            .is_some_and(|x| x.chef_ref == e.chef_ref && x.hat_ref == e.hat_ref);
        let row = e.row_id.clone();
        match &e.chef_ref {
            ChefReference::Unresolved { raw_key, row_id } => {
                if !old.get(row_id).is_some_and(
                    |x| matches!(&x.chef_ref,ChefReference::Unresolved{raw_key:k,..}if k==raw_key),
                ) {
                    return Err("diychef.invalidDraft".into());
                }
                diagnostics.push(diagnostic("diychef.unknownChef", row.clone(), !unchanged));
            }
            ChefReference::Known { chef_id } => {
                let item = c
                    .chefs
                    .iter()
                    .find(|x| x.id == *chef_id)
                    .ok_or("diychef.invalidDraft")?;
                if !seen.insert(chef_id) {
                    diagnostics.push(diagnostic("diychef.duplicateEntry", row.clone(), true));
                }
                if item.resource_state == "invalid" {
                    diagnostics.push(diagnostic(
                        "diychef.invalidResource",
                        row.clone(),
                        !unchanged,
                    ));
                } else {
                    first_usable.get_or_insert(item);
                }
                // Registration skips missing/invalid resources before reserving
                // either identity. Historical broken entries must not block a
                // different usable chef that can actually register.
                if item.resource_state != "invalid" {
                    let conflict = item
                        .custom_id
                        .filter(|_| item.source_kind == "custom")
                        .is_some_and(|id| ids.insert(id, chef_id).is_some())
                        || heads.insert(&item.head_name, chef_id).is_some();
                    if conflict {
                        diagnostics.push(diagnostic(
                            "diychef.identityConflict",
                            row.clone(),
                            group_sequence(c, &baseline.entries, item)
                                != group_sequence(c, &draft.entries, item)
                                || !unchanged,
                        ));
                    }
                }
            }
        }
        match &e.hat_ref {
            HatReference::Unresolved { raw_value, row_id } => {
                if !old.get(row_id).is_some_and(|x|matches!(&x.hat_ref,HatReference::Unresolved{raw_value:v,..}if v==raw_value)){return Err("diychef.invalidDraft".into())}
                diagnostics.push(diagnostic("diychef.unknownHat", row, !unchanged));
            }
            HatReference::Known { hat_id } => {
                let hat = c
                    .hats
                    .iter()
                    .find(|x| x.id == *hat_id)
                    .ok_or("diychef.invalidDraft")?;
                if hat.resource_state == "invalid" {
                    diagnostics.push(diagnostic("diychef.invalidResource", row, !unchanged));
                }
            }
        }
    }
    if first_usable.is_none() {
        diagnostics.push(diagnostic("diychef.emptySelection", None, true));
    }
    // The selected DLL determines default eligibility; unresolved/broken rows
    // never occupy the opening slot. DIYChef 2.2 added custom opening chefs.
    if !supports_custom_default && first_usable.is_none_or(|item| item.source_kind != "native") {
        let mut problem = diagnostic(NATIVE_DEFAULT_REQUIRED, None, true);
        problem.chef_id = first_usable.map(|item| item.id.clone());
        diagnostics.push(problem);
    }
    let (_, changes) = prefer::plan(doc, draft, c)?;
    let save_blocking = diagnostics.iter().any(|d| d.save_blocking);
    Ok(ChefValidation {
        diagnostics,
        changes,
        save_blocking,
    })
}

fn group_sequence(c: &Catalog, entries: &[ChefEntry], item: &ChefItem) -> Vec<String> {
    entries
        .iter()
        .filter_map(|e| match &e.chef_ref {
            ChefReference::Known { chef_id } => c.chefs.iter().find(|x| x.id == *chef_id),
            _ => None,
        })
        .filter(|x| {
            x.head_name == item.head_name
                || (item.custom_id.is_some()
                    && x.source_kind == "custom"
                    && x.custom_id == item.custom_id)
        })
        .map(|x| x.id.clone())
        .collect()
}

#[cfg(test)]
mod tests;
