use super::*;
use crate::mods::plugin_target::{DIYLEVEL_GUID, RUNTIME_WLOADER_GUID};

const ROOT: &str = "consumer";

fn runtime_inventory(guid: &str, requirement: &str) -> Inventory {
    Inventory {
        nodes: vec![
            node(
                guid,
                Some(candidate(guid, "3.10.2", vec![])),
                vec![candidate(guid, "4.0.0", vec![])],
            ),
            node(
                ROOT,
                Some(candidate(
                    ROOT,
                    "1.0",
                    vec![relation(RelationKind::Requires, guid, requirement)],
                )),
                vec![],
            ),
        ],
    }
}

#[test]
fn mod_install_and_enable_keep_unreadable_runtime_versions() {
    for guid in [DIYLEVEL_GUID, RUNTIME_WLOADER_GUID] {
        let mut inventory = runtime_inventory(guid, ">=3.10.2");
        inventory.nodes[0].current.as_mut().unwrap().provides[0].version = None;
        inventory.nodes[1].enabled = false;

        for installed in [true, false] {
            let candidate_id = if installed {
                None
            } else {
                let source = inventory.nodes[1].current.take().unwrap();
                let id = source.id.clone();
                inventory.nodes[1].candidates.push(source);
                Some(id)
            };
            let result = solve(&inventory, &intent(ROOT, candidate_id, true));
            assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
            assert_eq!(result.actions.len(), 1, "{result:?}");
            action_for(
                &result,
                ROOT,
                if installed {
                    ActionKind::Enable
                } else {
                    ActionKind::Install
                },
            );
            assert_eq!(
                result.notices,
                [crate::locale_tag::with_params(
                    "modChanges.dependencyVersionUnconfirmed",
                    &[
                        ("subject", ROOT),
                        ("dependency", guid),
                        ("requirement", ">=3.10.2")
                    ],
                )]
            );
        }
    }
}

#[test]
fn mod_enable_keeps_disabled_runtime_with_unconfirmed_declarations() {
    let mut inventory = runtime_inventory(RUNTIME_WLOADER_GUID, ">=3.10.2");
    inventory.nodes[0].enabled = false;
    inventory.nodes[0].current.as_mut().unwrap().relations_known = false;
    inventory.nodes[1].enabled = false;

    let result = solve(&inventory, &intent(ROOT, None, true));
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    assert_eq!(result.actions.len(), 1, "{result:?}");
    action_for(&result, ROOT, ActionKind::Enable);
    assert!(result.notices.contains(&crate::locale_tag::with_params(
        "modChanges.dependencyDisabledUnconfirmed",
        &[
            ("subject", ROOT),
            ("dependency", RUNTIME_WLOADER_GUID),
            ("requirement", ">=3.10.2"),
        ],
    )));
    assert!(result.notices.contains(&crate::locale_tag::with_params(
        "modChanges.dependencyDeclarationsUnconfirmed",
        &[("subject", ROOT), ("dependency", RUNTIME_WLOADER_GUID)],
    )));
}

#[test]
fn mod_install_does_not_accept_ambiguous_runtime_providers_inside_one_package() {
    let mut inventory = runtime_inventory(RUNTIME_WLOADER_GUID, ">=3.10.2");
    let web = inventory.nodes[0].current.as_mut().unwrap();
    web.provides[0].version = None;
    let mut second = web.provides[0].clone();
    second.member_key = Some("another-web.dll".into());
    web.provides.push(second);
    inventory.nodes[1].enabled = false;

    let result = solve(&inventory, &intent(ROOT, None, true));
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.notices.is_empty());
}

#[test]
fn mod_install_does_not_disable_runtime_whose_conflict_is_unconfirmed() {
    let mut inventory = runtime_inventory(RUNTIME_WLOADER_GUID, ">=3.10.2");
    let web = inventory.nodes[0].current.as_mut().unwrap();
    web.relations = vec![relation(RelationKind::Conflicts, ROOT, ">=1.0")];
    let mut consumer = inventory.nodes[1].current.take().unwrap();
    consumer.relations.clear();
    consumer.provides[0].version = None;
    inventory.nodes[1].enabled = false;
    inventory.nodes[1].candidates = vec![consumer.clone()];

    let result = solve(&inventory, &intent(ROOT, Some(consumer.id), true));
    assert_eq!(result.status, SolveStatus::Incomplete, "{result:?}");
    assert!(result.actions.is_empty());
}
