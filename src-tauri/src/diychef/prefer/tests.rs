use super::*;
use crate::{
    diychef::{
        catalog::{Catalog, NATIVE_NAMES},
        validation,
    },
    models::*,
};
fn catalog() -> Catalog {
    Catalog {
        chefs: NATIVE_NAMES
            .lines()
            .map(|n| ChefItem {
                id: format!("native:{n}"),
                source_kind: "native".into(),
                display_name: n.into(),
                config_name: n.into(),
                head_name: n.into(),
                custom_id: None,
                resource_state: "known_name".into(),
                preview_state: "pending".into(),
            })
            .collect(),
        hats: ["", "None", "Santa", "Fancy"]
            .iter()
            .map(|n| ChefHat {
                id: format!("hat:{n}"),
                kind: "native".into(),
                config_value: (*n).into(),
                display_name: (*n).into(),
                resource_state: "known_name".into(),
                preview_state: "pending".into(),
            })
            .collect(),
        revision: "catalog".into(),
    }
}
fn draft(d: &Document, c: &Catalog) -> ChefDraft {
    let s = d.selection(c);
    ChefDraft {
        base_revision: "r".into(),
        catalog_revision: c.revision.clone(),
        target_generation: 1,
        entries: s.entries,
        lobby_switch: s.lobby_switch,
        removed_problem_row_ids: vec![],
        acknowledged_change_ids: vec![],
        reset_intent: None,
    }
}
#[test]
fn unrelated_switch_preserves_unknown_and_duplicate_bytes() {
    let c = catalog();
    let original = "Chef_Black_cat HAT=Alien extra\r\nUNKNOWN\r\nChef_Black_cat HAT=None\r\n";
    let doc = Document::parse(original.as_bytes()).unwrap();
    let mut d = draft(&doc, &c);
    d.lobby_switch = true;
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert!(String::from_utf8(bytes).unwrap().starts_with(original));
    let v = validation::validate(&doc, &c, &d).unwrap();
    assert!(!v.save_blocking);
    assert_eq!(v.diagnostics.len(), 2);
}
#[test]
fn disabling_does_not_reactivate_duplicate() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat\nChef_Beard\nChef_Black_cat HAT=None\n").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.remove(0);
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(String::from_utf8(bytes).unwrap(), "Chef_Beard\n");
}
#[test]
fn all_true_records_removed_and_abnormal_change_is_explicit() {
    let c = catalog();
    let doc =
        Document::parse(b"Chef_Black_cat\nLOBBYSWITCHCHEF=TRUE\n# LOBBYSWITCHCHEF=TRUE extra\n")
            .unwrap();
    let mut d = draft(&doc, &c);
    d.lobby_switch = false;
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(String::from_utf8(bytes).unwrap(), "Chef_Black_cat\n");
}
#[test]
fn reset_cleans_problem_records_but_uses_final_edited_selection() {
    let c = catalog();
    let doc = Document::parse(
        b"UNKNOWN\nChef_Black_cat\nChef_Black_cat HAT=None\nLOBBYSWITCHCHEF=TRUE\n",
    )
    .unwrap();
    let mut d = draft(&doc, &c);
    d.entries
        .retain(|e| matches!(&e.chef_ref, ChefReference::Known { .. }));
    d.reset_intent = Some("restore_default".into());
    d.lobby_switch = false;
    d.entries[0].hat_ref = HatReference::Known {
        hat_id: "hat:None".into(),
    };
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Chef_Black_cat HAT=None\n"
    );
    assert_eq!(changes[0].id, "reset");
}
#[test]
fn second_unknown_token_is_preserved_when_hat_inserted() {
    let c = catalog();
    let doc = Document::parse(b"  Chef_Black_cat  EXTRA tail\n").unwrap();
    let mut d = draft(&doc, &c);
    d.entries[0].hat_ref = HatReference::Known {
        hat_id: "hat:None".into(),
    };
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "  Chef_Black_cat HAT=None  EXTRA tail\n"
    );
}
#[test]
fn reorder_preserves_no_final_newline_and_separates_records() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat\nChef_Beard").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.reverse();
    let (bytes, _) = plan(&doc, &d, &c).unwrap();
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Chef_Beard\nChef_Black_cat"
    );
}
#[test]
fn empty_and_forged_unknown_refs_block() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.clear();
    assert!(validation::validate(&doc, &c, &d).unwrap().save_blocking);
    d.entries.push(ChefEntry {
        entry_id: "fake".into(),
        row_id: None,
        chef_ref: ChefReference::Unresolved {
            raw_key: "injected".into(),
            row_id: "fake".into(),
        },
        hat_ref: HatReference::Known {
            hat_id: "hat:".into(),
        },
    });
    assert!(validation::validate(&doc, &c, &d).is_err());
}

#[test]
fn append_without_final_newline_separates_new_entry_and_switch() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.push(ChefEntry {
        entry_id: "new".into(),
        row_id: None,
        chef_ref: ChefReference::Known {
            chef_id: "native:Chef_Beard".into(),
        },
        hat_ref: HatReference::Known {
            hat_id: "hat:".into(),
        },
    });
    d.lobby_switch = true;
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        format!(
            "Chef_Black_cat{}Chef_Beard{}LOBBYSWITCHCHEF=TRUE",
            doc.newline, doc.newline
        )
    );
}
#[test]
fn unrelated_reordering_preserves_shadowed_duplicate() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat\nChef_Black_cat HAT=None\nChef_Beard\nChef_Box\n")
        .unwrap();
    let mut d = draft(&doc, &c);
    d.entries.swap(1, 2);
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Chef_Black_cat\nChef_Black_cat HAT=None\nChef_Box\nChef_Beard\n"
    );
}
#[test]
fn moving_first_record_past_duplicate_requires_its_removal() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat\nChef_Black_cat HAT=None\nChef_Beard\n").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.reverse();
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert_eq!(changes.len(), 1);
    assert!(changes[0].id.starts_with("duplicate:"));
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Chef_Beard\nChef_Black_cat\n"
    );
}
#[test]
fn unknown_deletion_cannot_resurrect_shadowed_unknown() {
    let c = catalog();
    let doc = Document::parse(b"UNKNOWN HAT=Alien\nUNKNOWN HAT=None\nChef_Black_cat").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.remove(0);
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(String::from_utf8(bytes).unwrap(), "Chef_Black_cat");
}
