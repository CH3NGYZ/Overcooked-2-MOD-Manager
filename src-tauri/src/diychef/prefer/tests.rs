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
        hat_bindings: s.hat_bindings,
        lobby_switch: s.lobby_switch,
        removed_problem_row_ids: vec![],
        acknowledged_change_ids: vec![],
        reset_intent: None,
    }
}
#[test]
fn unsupported_and_duplicate_rows_are_discarded_on_explicit_save() {
    let c = catalog();
    let original =
        b"Chef_Black_cat HAT=Alien extra\r\nUNKNOWN\r\nChef_Black_cat HAT=None\r\nChef_Beard\r\n";
    let doc = Document::parse(original).unwrap();
    let mut d = draft(&doc, &c);
    assert_eq!(d.entries.len(), 1);
    d.lobby_switch = true;
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(bytes, b"Chef_Beard\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n");
    assert!(
        !validation::validate(&doc, &c, &d, false)
            .unwrap()
            .save_blocking
    );
    assert_eq!(
        doc.encode(&doc.text()),
        original,
        "reading and planning never rewrite the source"
    );
}
#[test]
fn disabling_does_not_reactivate_duplicate() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat\nChef_Beard\nChef_Black_cat HAT=None\n").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.remove(0);
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
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
    d.hat_bindings.insert(
        "native:Chef_Black_cat".into(),
        HatReference::Known {
            hat_id: "hat:None".into(),
        },
    );
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
    d.hat_bindings.insert(
        "native:Chef_Black_cat".into(),
        HatReference::Known {
            hat_id: "hat:None".into(),
        },
    );
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
    assert!(
        validation::validate(&doc, &c, &d, false)
            .unwrap()
            .save_blocking
    );
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
    assert!(validation::validate(&doc, &c, &d, false).is_err());
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
            "Chef_Black_cat{}Chef_Beard{}{}LOBBYSWITCHCHEF=TRUE",
            doc.newline, doc.newline, doc.newline
        )
    );
}
#[test]
fn reordering_discards_shadowed_duplicate() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat\nChef_Black_cat HAT=None\nChef_Beard\nChef_Box\n")
        .unwrap();
    let mut d = draft(&doc, &c);
    d.entries.swap(1, 2);
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Chef_Black_cat\nChef_Box\nChef_Beard\n"
    );
}
#[test]
fn moving_first_record_past_duplicate_requires_its_removal() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat\nChef_Black_cat HAT=None\nChef_Beard\n").unwrap();
    let mut d = draft(&doc, &c);
    d.entries.reverse();
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(
        String::from_utf8(bytes).unwrap(),
        "Chef_Beard\nChef_Black_cat\n"
    );
}
#[test]
fn unknown_deletion_cannot_resurrect_shadowed_unknown() {
    let c = catalog();
    let doc = Document::parse(b"UNKNOWN HAT=Alien\nUNKNOWN HAT=None\nChef_Black_cat").unwrap();
    let d = draft(&doc, &c);
    assert_eq!(d.entries.len(), 1, "unknown rows were already discarded");
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(String::from_utf8(bytes).unwrap(), "Chef_Black_cat");
}

#[test]
fn normal_saves_move_every_old_switch_position_to_one_final_separated_record() {
    let c = catalog();
    for newline in ["\n", "\r\n", "\r"] {
        for final_newline in [false, true] {
            let ending = if final_newline { newline } else { "" };
            for old_lines in [
                vec![SWITCH_LINE, "Chef_Black_cat", "Chef_Beard"],
                vec!["Chef_Black_cat", SWITCH_LINE, "Chef_Beard"],
                vec!["Chef_Black_cat", "Chef_Beard", SWITCH_LINE],
                vec!["Chef_Black_cat", "", "\t", SWITCH_LINE, "Chef_Beard"],
                vec![
                    SWITCH_LINE,
                    "Chef_Black_cat",
                    SWITCH_LINE,
                    "Chef_Beard",
                    SWITCH_LINE,
                ],
            ] {
                let original = old_lines.join(newline) + ending;
                let doc = Document::parse(original.as_bytes()).unwrap();
                let (bytes, changes) = plan(&doc, &draft(&doc, &c), &c).unwrap();
                let expected = format!(
                    "Chef_Black_cat{newline}Chef_Beard{newline}{newline}{SWITCH_LINE}{ending}"
                );
                assert_eq!(decode(&bytes).unwrap().0, expected, "{original:?}");
                assert!(changes.is_empty(), "ordinary switch relocation is lossless");
                assert_eq!(doc.text(), original, "planning never mutates the source");
                let readback = Document::parse(&bytes).unwrap();
                assert!(matches!(
                    readback.lines.last().unwrap().kind,
                    LineKind::Switch
                ));
                assert_eq!(
                    readback
                        .lines
                        .iter()
                        .filter(|line| matches!(line.kind, LineKind::Switch))
                        .count(),
                    1
                );
                assert_eq!(
                    plan(&readback, &draft(&readback, &c), &c).unwrap().0,
                    bytes,
                    "the normalized layout is stable"
                );
            }
        }
    }
}

#[test]
fn switch_footer_discards_unsupported_rows_and_keeps_mixed_supported_endings() {
    let c = catalog();
    let original = concat!(
        "LOBBYSWITCHCHEF=TRUE\r\n",
        "  Chef_Black_cat HAT=Alien extra \n",
        "UNKNOWN HAT=Alien\r",
        "Chef_Black_cat HAT=None\r\n",
        "Chef_Beard HAT=None\r",
        " \t\r\n\n"
    );
    let doc = Document::parse(original.as_bytes()).unwrap();
    let (bytes, changes) = plan(&doc, &draft(&doc, &c), &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(
        decode(&bytes).unwrap().0,
        concat!("Chef_Beard HAT=None\r", "\rLOBBYSWITCHCHEF=TRUE\r\n")
    );
    let mixed = Document::parse(b"Chef_Black_cat\nChef_Beard\r").unwrap();
    let mut d = draft(&mixed, &c);
    d.lobby_switch = true;
    assert_eq!(
        decode(&plan(&mixed, &d, &c).unwrap().0).unwrap().0,
        "Chef_Black_cat\nChef_Beard\r\rLOBBYSWITCHCHEF=TRUE\n",
        "CR followed by LF must not collapse the intended empty separator line"
    );
}

#[test]
fn switch_footer_preserves_encoding_bom_newline_and_unterminated_final_record() {
    let c = catalog();
    for encoding in ["utf8", "utf8bom", "utf16le", "utf16be"] {
        for newline in ["\n", "\r\n", "\r"] {
            let mut encoder = Document::parse(b"").unwrap();
            encoder.encoding = encoding.into();
            for final_newline in [false, true] {
                let ending = if final_newline { newline } else { "" };
                let original = encoder.encode(&format!(
                    "{SWITCH_LINE}{newline}Chef_Black_cat{newline}UNKNOWN HAT=Alien{ending}"
                ));
                let doc = Document::parse(&original).unwrap();
                let (bytes, _) = plan(&doc, &draft(&doc, &c), &c).unwrap();
                assert_eq!(
                    bytes,
                    encoder.encode(&format!(
                        "Chef_Black_cat{newline}{newline}{SWITCH_LINE}{ending}"
                    ))
                );
                assert_eq!(Document::parse(&bytes).unwrap().encoding, encoding);
                assert_eq!(doc.encode(&doc.text()), original);
            }
        }
    }
}

#[test]
fn abnormal_switch_records_need_explicit_confirmation_when_canonicalized_or_removed() {
    let c = catalog();
    let doc =
        Document::parse(b"# LOBBYSWITCHCHEF=TRUE extra\nChef_Black_cat\nLOBBYSWITCHCHEF=TRUE\n")
            .unwrap();
    let d = draft(&doc, &c);
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].before, "# LOBBYSWITCHCHEF=TRUE extra");
    assert_eq!(changes[0].after, SWITCH_LINE);
    assert_eq!(
        decode(&bytes).unwrap().0,
        "Chef_Black_cat\n\nLOBBYSWITCHCHEF=TRUE\n"
    );
}

#[test]
fn default_and_both_reset_modes_share_the_single_switch_footer() {
    assert_eq!(DEFAULT_CONTENT.len(), 40);
    assert_eq!(
        DEFAULT_CONTENT,
        b"Chef_Black_cat\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n"
    );
    let c = catalog();
    for original in [
        "LOBBYSWITCHCHEF=TRUE\r\nUNKNOWN\r\nChef_Black_cat\r\nChef_Black_cat HAT=None",
        "Chef_Black_cat\r\n\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n",
    ] {
        let doc = Document::parse(original.as_bytes()).unwrap();
        for intent in ["keep_black_cat", "restore_default"] {
            let mut d = draft(&doc, &c);
            d.entries.retain(|entry| {
                matches!(&entry.chef_ref, ChefReference::Known { chef_id } if chef_id == "native:Chef_Black_cat")
            });
            d.lobby_switch = true;
            d.reset_intent = Some(intent.into());
            let (bytes, changes) = plan(&doc, &d, &c).unwrap();
            assert_eq!(bytes, DEFAULT_CONTENT);
            assert_eq!(changes.len(), 1);
            assert_eq!(changes[0].id, "reset");
        }
    }
    let doc = Document::parse(DEFAULT_CONTENT).unwrap();
    let (bytes, changes) = plan(&doc, &draft(&doc, &c), &c).unwrap();
    assert_eq!(bytes, DEFAULT_CONTENT);
    assert!(changes.is_empty());
}

#[test]
fn appending_after_the_previous_switch_moves_its_separator_with_the_footer() {
    let c = catalog();
    let doc = Document::parse(DEFAULT_CONTENT).unwrap();
    let mut d = draft(&doc, &c);
    d.entries.push(ChefEntry {
        entry_id: "appended-after-switch".into(),
        row_id: None,
        chef_ref: ChefReference::Known {
            chef_id: "native:Chef_Beard".into(),
        },
        hat_ref: HatReference::Known {
            hat_id: "hat:".into(),
        },
    });
    let (bytes, changes) = plan(&doc, &d, &c).unwrap();
    assert!(changes.is_empty());
    assert_eq!(
        bytes,
        b"Chef_Black_cat\r\nChef_Beard\r\n\r\nLOBBYSWITCHCHEF=TRUE\r\n"
    );
    d.lobby_switch = false;
    assert_eq!(
        plan(&doc, &d, &c).unwrap().0,
        b"Chef_Black_cat\r\nChef_Beard\r\n",
        "disabling removes the old switch separator as well"
    );
}

#[test]
fn binding_map_overrides_stale_rows_and_clearing_means_automatic() {
    let c = catalog();
    let doc = Document::parse(b"Chef_Black_cat HAT=Santa\n").unwrap();
    let mut d = draft(&doc, &c);
    d.hat_bindings.insert(
        "native:Chef_Black_cat".into(),
        HatReference::Known {
            hat_id: "hat:None".into(),
        },
    );
    assert_eq!(plan(&doc, &d, &c).unwrap().0, b"Chef_Black_cat HAT=None\n");
    d.hat_bindings.clear();
    assert_eq!(plan(&doc, &d, &c).unwrap().0, b"Chef_Black_cat HAT=\n");
}
#[test]
fn native_is_the_physical_first_line_after_leading_blanks_and_switches() {
    let c = catalog();
    for prefix in ["\r\n \t\r\n", "\r\nLOBBYSWITCHCHEF=TRUE\r\n\r\n"] {
        for lobby in [false, true] {
            let doc = Document::parse(format!("{prefix}Chef_Black_cat\r\n").as_bytes()).unwrap();
            let mut d = draft(&doc, &c);
            d.lobby_switch = lobby;
            assert!(
                !validation::validate(&doc, &c, &d, false)
                    .unwrap()
                    .save_blocking
            );
            let output = decode(&plan(&doc, &d, &c).unwrap().0).unwrap().0;
            assert_eq!(output.lines().next(), Some("Chef_Black_cat"));
        }
    }
}
