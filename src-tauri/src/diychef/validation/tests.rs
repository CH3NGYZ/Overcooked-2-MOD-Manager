use super::*;

fn chef(id: &str, name: &str, custom_id: Option<u8>, head: &str, state: &str) -> ChefItem {
    ChefItem {
        id: id.into(),
        source_kind: if custom_id.is_some() {
            "custom"
        } else {
            "native"
        }
        .into(),
        display_name: name.into(),
        config_name: name.into(),
        head_name: head.into(),
        custom_id,
        resource_state: state.into(),
        preview_state: "pending".into(),
    }
}

#[test]
fn historical_invalid_chef_does_not_reserve_id_or_head_for_new_usable_chef() {
    for head_conflict in [false, true] {
        let c = Catalog {
            chefs: vec![
                chef(
                    "native:Chef_Black_cat",
                    "Chef_Black_cat",
                    None,
                    "Chef_Black_cat",
                    "known_name",
                ),
                chef(
                    "custom:Broken",
                    "Broken",
                    Some(0),
                    if head_conflict {
                        "Chef_Fox"
                    } else {
                        "Chef_Broken"
                    },
                    "invalid",
                ),
                chef(
                    "custom:Fox",
                    "Fox",
                    Some(if head_conflict { 1 } else { 0 }),
                    "Chef_Fox",
                    "valid",
                ),
            ],
            hats: vec![ChefHat {
                id: "hat:".into(),
                kind: "auto".into(),
                config_value: String::new(),
                display_name: "Auto".into(),
                resource_state: "known_name".into(),
                preview_state: "pending".into(),
            }],
            revision: "fixture".into(),
        };
        let doc = Document::parse(b"Broken\nChef_Black_cat\n").unwrap();
        let selection = doc.selection(&c);
        let mut d = ChefDraft {
            base_revision: "fixture".into(),
            catalog_revision: c.revision.clone(),
            target_generation: 1,
            entries: selection.entries,
            hat_bindings: selection.hat_bindings,
            lobby_switch: false,
            removed_problem_row_ids: vec![],
            acknowledged_change_ids: vec![],
            reset_intent: None,
        };
        d.entries.push(ChefEntry {
            entry_id: "added:fox".into(),
            row_id: None,
            chef_ref: ChefReference::Known {
                chef_id: "custom:Fox".into(),
            },
            hat_ref: HatReference::Known {
                hat_id: "hat:".into(),
            },
        });
        let result = validate(&doc, &c, &d, false).unwrap();
        assert!(
            !result.save_blocking,
            "plugin skips broken registration first: {:?}",
            result.diagnostics
        );
        assert!(
            result.diagnostics.is_empty(),
            "unsupported historical rows are discarded"
        );
        let (bytes, _) = prefer::plan(&doc, &d, &c).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            "Chef_Black_cat\nFox\n",
            "unsupported historical record is discarded"
        );
        // A usable earlier chef really does reserve the ID/head and must still
        // block a newly introduced competing registration.
        let mut usable = c.clone();
        usable.chefs[1].resource_state = "valid".into();
        let mut conflicting = d.clone();
        conflicting.entries.insert(
            0,
            ChefEntry {
                entry_id: "broken-now-usable".into(),
                row_id: None,
                chef_ref: ChefReference::Known {
                    chef_id: "custom:Broken".into(),
                },
                hat_ref: HatReference::Known {
                    hat_id: "hat:".into(),
                },
            },
        );
        let actual_conflict = validate(&doc, &usable, &conflicting, false).unwrap();
        assert!(actual_conflict.save_blocking);
        assert!(
            actual_conflict
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code == "diychef.identityConflict"
                    && diagnostic.save_blocking)
        );
    }
}

fn native_default_fixture(text: &[u8]) -> (Document, Catalog, ChefDraft) {
    let c = Catalog {
        chefs: vec![
            chef(
                "native:cat",
                "Chef_Black_cat",
                None,
                "Chef_Black_cat",
                "known_name",
            ),
            chef("custom:Fox", "Fox", Some(0), "Chef_Fox", "valid"),
            chef("custom:Broken", "Broken", Some(1), "Chef_Broken", "invalid"),
        ],
        hats: vec![ChefHat {
            id: "hat:".into(),
            kind: "auto".into(),
            config_value: String::new(),
            display_name: "Auto".into(),
            resource_state: "known_name".into(),
            preview_state: "pending".into(),
        }],
        revision: "fixture".into(),
    };
    let doc = Document::parse(text).unwrap();
    let selection = doc.selection(&c);
    let d = ChefDraft {
        base_revision: "fixture".into(),
        catalog_revision: c.revision.clone(),
        target_generation: 1,
        entries: selection.entries,
        hat_bindings: selection.hat_bindings,
        lobby_switch: selection.lobby_switch,
        removed_problem_row_ids: vec![],
        acknowledged_change_ids: vec![],
        reset_intent: None,
    };
    (doc, c, d)
}

#[test]
fn custom_defaults_are_allowed_only_for_capable_targets_and_empty_still_blocks() {
    for input in [b"Fox\n".as_slice(), b"Fox\nChef_Black_cat\n"] {
        let (doc, c, d) = native_default_fixture(input);
        assert!(!validate(&doc, &c, &d, true).unwrap().save_blocking);
        assert!(validate(&doc, &c, &d, false).unwrap().save_blocking);
        assert_eq!(prefer::plan(&doc, &d, &c).unwrap().0, input);
    }
    for input in [b"".as_slice(), b"Broken\n", b"UNKNOWN\n"] {
        let (doc, c, d) = native_default_fixture(input);
        let result = validate(&doc, &c, &d, true).unwrap();
        assert!(result.save_blocking);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|item| item.code == "diychef.emptySelection")
        );
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|item| item.code == NATIVE_DEFAULT_REQUIRED)
        );
    }
}

#[test]
fn custom_opening_chef_blocks_even_when_an_enabled_native_follows() {
    let (doc, c, d) = native_default_fixture(b"Fox\nChef_Black_cat\n");
    let result = validate(&doc, &c, &d, false).unwrap();
    assert!(result.save_blocking);
    let problem = result
        .diagnostics
        .iter()
        .find(|p| p.code == NATIVE_DEFAULT_REQUIRED)
        .unwrap();
    assert!(problem.save_blocking);
    assert_eq!(problem.chef_id.as_deref(), Some("custom:Fox"));
    assert!(
        problem.row_id.is_none(),
        "the fix belongs to the selection, not deleting one problem row"
    );
    let mut corrected = d.clone();
    corrected.entries.swap(0, 1);
    assert!(!validate(&doc, &c, &corrected, false).unwrap().save_blocking);
}

#[test]
fn no_usable_native_or_empty_selection_requires_a_native_default() {
    for input in [b"Fox\n".as_slice(), b"Broken\n", b"UNKNOWN\n", b""] {
        let (doc, c, d) = native_default_fixture(input);
        let result = validate(&doc, &c, &d, false).unwrap();
        assert!(result.save_blocking, "{input:?}");
        assert!(
            result
                .diagnostics
                .iter()
                .any(|p| p.code == NATIVE_DEFAULT_REQUIRED && p.save_blocking)
        );
    }
    let (doc, mut c, d) = native_default_fixture(b"Chef_Black_cat\nFox\n");
    c.chefs[0].resource_state = "invalid".into();
    assert!(
        validate(&doc, &c, &d, false).is_err(),
        "a newly invalid catalog requires a fresh snapshot"
    );
}

#[test]
fn historical_unknown_and_broken_rows_do_not_displace_the_first_usable_native() {
    let (doc, c, d) = native_default_fixture(b"UNKNOWN\nBroken\nChef_Black_cat\nFox\n");
    let result = validate(&doc, &c, &d, false).unwrap();
    assert!(!result.save_blocking, "{:?}", result.diagnostics);
    assert!(
        !result
            .diagnostics
            .iter()
            .any(|p| p.code == NATIVE_DEFAULT_REQUIRED)
    );
}

#[test]
fn validation_of_a_historical_custom_default_preserves_its_order_and_original_bytes() {
    let original = b"  Fox  EXTRA\r\nUNKNOWN\r\nChef_Black_cat\r\n";
    let (doc, c, d) = native_default_fixture(original);
    let before = d.clone();
    let result = validate(&doc, &c, &d, false).unwrap();
    assert!(result.save_blocking);
    assert!(
        matches!(&d.entries[0].chef_ref, ChefReference::Known { chef_id } if chef_id == "custom:Fox")
    );
    assert_eq!(
        serde_json::to_value(&d).unwrap(),
        serde_json::to_value(&before).unwrap()
    );
    let (bytes, changes) = prefer::plan(&doc, &d, &c).unwrap();
    assert_eq!(bytes, b"  Fox  EXTRA\r\nChef_Black_cat\r\n");
    assert_eq!(
        doc.encode(&doc.text()),
        original,
        "the original source remains untouched"
    );
    assert!(
        changes.is_empty(),
        "opening an old custom-first file does not silently repair it"
    );
}

#[test]
fn disabled_invalid_catalog_chefs_accept_bulk_bindings_without_generating_rows() {
    let (doc, mut c, mut d) = native_default_fixture(b"Chef_Black_cat\n");
    let mut none = c.hats[0].clone();
    none.id = "hat:None".into();
    none.config_value = "None".into();
    c.hats.push(none);
    assert!(
        c.chefs
            .iter()
            .any(|item| item.id == "custom:Broken" && item.resource_state == "invalid")
    );
    for chef in &c.chefs {
        d.hat_bindings.insert(
            chef.id.clone(),
            HatReference::Known {
                hat_id: "hat:None".into(),
            },
        );
    }
    let result = validate(&doc, &c, &d, false).unwrap();
    assert!(!result.save_blocking);
    let (bytes, _) = prefer::plan(&doc, &d, &c).unwrap();
    assert_eq!(bytes, b"Chef_Black_cat HAT=None\n");
    assert_eq!(d.hat_bindings.len(), c.chefs.len());
}
