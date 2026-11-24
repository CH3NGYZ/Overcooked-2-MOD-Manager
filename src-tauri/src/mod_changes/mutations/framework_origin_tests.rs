use super::*;
use release_asset_adoption_tests::{freeze_writes, upgrades};

const FRAMEWORK_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const FRAMEWORK: &str = "bepinex/plugins/framework/main.dll";
const INTERNAL: &str = "BepInEx/cache/modmanager_disabled_maps/map/main.dll";
const EXTERNAL: &str = "BepInEx/plugins/external/main.dll";

fn provider(id: &str, kind: NodeKind, path: &str, version: Option<&str>) -> ChangeNode {
    let mut plugin = member(path, false, FRAMEWORK_GUID);
    plugin.version = version.map(str::to_owned);
    ChangeNode {
        id: id.into(),
        kind,
        current: Some(candidate(id, vec![plugin])),
        enabled: false,
        mutable: true,
        candidates: vec![],
        candidates_complete: true,
    }
}

#[test]
fn map_providers_never_supply_or_displace_the_framework_root() {
    for version in [Some("0.11.1"), Some("0.12.0"), None] {
        let external_id = format!("map-dll:{}", inventory::key(EXTERNAL));
        let mut solved = Inventory {
            nodes: vec![
                provider("map:fixture", NodeKind::Map, INTERNAL, version),
                provider(&external_id, NodeKind::Mod, EXTERNAL, version),
            ],
        };
        assert_eq!(final_framework_package(&solved), Ok(None));
        solved.nodes.push(provider(
            "mod:framework",
            NodeKind::Mod,
            FRAMEWORK,
            Some("0.11.1"),
        ));
        assert_eq!(
            final_framework_package(&solved),
            Ok(Some(("mod:framework", "bepinex/plugins/framework".into())))
        );
    }
}

#[test]
fn real_framework_mod_copies_keep_highest_version_and_ambiguity_rules() {
    for (version, ambiguous) in [
        (Some("0.10.0"), false),
        (Some("0.11.1"), true),
        (None, true),
    ] {
        let solved = Inventory {
            nodes: vec![
                provider("mod:framework", NodeKind::Mod, FRAMEWORK, Some("0.11.1")),
                provider("mod:second", NodeKind::Mod, EXTERNAL, version),
            ],
        };
        let expected = if ambiguous {
            Err("framework.diyLevelAmbiguous".into())
        } else {
            Ok(Some(("mod:framework", "bepinex/plugins/framework".into())))
        };
        assert_eq!(final_framework_package(&solved), expected);
    }
}

#[test]
fn framework_migration_keeps_the_map_and_its_disabled_same_guid_member() {
    let mut fixture = fixture();
    framework_at(&mut fixture, true);
    let original = "BepInEx/plugins/old/levels/fixture";
    let new = "BepInEx/plugins/new/levels/fixture";
    map_with_member(&mut fixture, "map:fixture", original, true, false);
    for inventory in [
        &mut fixture.session.trusted.inventory,
        fixture.session.solved.as_mut().unwrap(),
    ] {
        inventory.nodes[1].current.as_mut().unwrap().provides[0].guid = FRAMEWORK_GUID.into();
    }
    let changes = assemble(&fixture.session, fixture.roots.clone()).unwrap();
    assert!(fixture.roots.game_root.join(original).exists());
    crate::mod_changes::transaction::prepare(
        &fixture.roots.data_root.join("transactions"),
        &fixture.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply()
    .unwrap();
    assert!(!fixture.roots.game_root.join(original).exists());
    assert_eq!(
        fs::read(
            fixture
                .roots
                .game_root
                .join(new)
                .join("nested/plugin.dll.disabled")
        )
        .unwrap(),
        b"plugin"
    );
}

#[test]
fn same_guid_inside_map_does_not_block_verified_mod_resource_adoption() {
    let mut fixture = upgrades(true);
    map_with_member(
        &mut fixture,
        "map:fixture",
        "BepInEx/plugins/OC2DIYLevel/levels/fixture",
        true,
        false,
    );
    fixture.session.trusted.inventory.nodes[2]
        .current
        .as_mut()
        .unwrap()
        .provides[0]
        .guid = mods::plugin_target::RUNTIME_WLOADER_GUID.into();
    assert!(!freeze_writes(&mut fixture).unwrap().is_empty());
}

#[test]
fn loose_dll_proof_ignores_map_members_but_keeps_mod_and_registered_path_checks() {
    let mut fixture = fixture();
    let path = "BepInEx/plugins/Framework/main.dll";
    write(
        &fixture.roots.game_root,
        path,
        &mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.10.0"),
    );
    let incoming = member(path, true, FRAMEWORK_GUID);
    add_mod(&mut fixture, "framework", vec![incoming.clone()], vec![]);
    fixture
        .session
        .trusted
        .files
        .get_mut("framework")
        .unwrap()
        .manifests
        .clear();
    fs::remove_file(fixture.roots.data_root.join("manifests/framework.toml")).unwrap();
    fixture.session.trusted.inventory.nodes.push(provider(
        "map:fixture",
        NodeKind::Map,
        INTERNAL,
        Some("0.10.0"),
    ));
    let prove = |fixture: &Fixture| {
        prove_loose_dll(
            &fixture.session,
            fixture.roots.clone(),
            "framework",
            path,
            &incoming,
        )
    };
    assert!(prove(&fixture).is_ok());

    for other in ["mod:duplicate", "map-dll:registered"] {
        fixture.session.trusted.inventory.nodes.push(provider(
            other,
            NodeKind::Mod,
            EXTERNAL,
            Some("0.10.0"),
        ));
        assert_eq!(
            prove(&fixture).unwrap_err(),
            format!("modChanges.installBundlePluginMismatch?path={path}")
        );
        fixture.session.trusted.inventory.nodes.pop();
    }

    write(
        &fixture.roots.data_root,
        "manifests/level-set-plugins.json",
        br#"{"version":1,"entries":[{"dll":"BepInEx/plugins/Framework/main.dll","owner":{"name":"fixture"}}]}"#,
    );
    assert_eq!(
        prove(&fixture).unwrap_err(),
        format!("modChanges.installTargetMapProtected?path={path}")
    );
}
