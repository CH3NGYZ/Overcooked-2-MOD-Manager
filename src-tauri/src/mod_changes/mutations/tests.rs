use super::*;
use crate::mod_changes::{
    inventory::{MemberSummary, NodeFiles, NodeSummary, TrustedInventory},
    session::{ChangePlan, Payload, PlanRequest},
};
use std::{collections::HashMap, time::Instant};

#[path = "bundled_maps_tests.rs"]
mod bundled_maps_tests;
#[path = "bundled_members_tests.rs"]
mod bundled_members_tests;
#[path = "physical_tests.rs"]
mod physical_tests;
#[path = "release_asset_adoption_tests.rs"]
mod release_asset_adoption_tests;

struct Fixture {
    _temp: tempfile::TempDir,
    roots: TransactionRoots,
    session: PlanSession,
}

fn member(path: &str, enabled: bool, guid: &str) -> ProvidedPlugin {
    ProvidedPlugin {
        member_key: Some(inventory::key(path)),
        guid: guid.into(),
        version: Some("1.0.0".into()),
        prerelease: None,
        enabled,
    }
}

fn candidate(id: &str, provides: Vec<ProvidedPlugin>) -> Candidate {
    Candidate {
        id: id.into(),
        source_id: None,
        tag: None,
        version: Some("1.0.0".into()),
        provides,
        relations: vec![],
        relations_known: true,
        installable: true,
        raw: false,
        verification_pending: false,
    }
}

fn fixture() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let game_root = root.join("game");
    let data_root = root.join("data");
    fs::create_dir(&game_root).unwrap();
    fs::create_dir(&data_root).unwrap();
    let staging = data_root.join("stage");
    fs::create_dir(&staging).unwrap();
    let inventory = Inventory::default();
    let trusted = TrustedInventory {
        inventory: inventory.clone(),
        files: BTreeMap::new(),
        revision: "fixture".into(),
        game_root: game_root.clone(),
        level_roots: crate::level_sets::LevelSetRoots::new(game_root.clone(), None),
        config: crate::config::default_config(),
        managed_mods: Default::default(),
        toggle_unbound_nodes: Default::default(),
        manual_declaration_notices: Default::default(),
        installed_mods: Default::default(),
        duplicate_releases: Default::default(),
        duplicate_release_safe_nodes: Default::default(),
        declared_bundle_switch_members: Default::default(),
    };
    let session = PlanSession {
        trusted,
        request: ChangeRequest::default(),
        wire_request: PlanRequest::default(),
        dependency_policy: Default::default(),
        plan: ChangePlan {
            plan_id: "fixture".into(),
            result: SolveResult {
                status: SolveStatus::Ready,
                actions: vec![],
                evaluations: vec![],
                blockers: vec![],
                notices: vec![],
                impacts: vec![],
            },
            roots: vec![],
            nodes: vec![],
            relation_labels: Default::default(),
            previous_enabled_members: Default::default(),
            retired_mod_copies: Default::default(),
            untracked_overwrites: Vec::new(),
            install_notes: vec![],
            prepared: true,
        },
        solved: Some(inventory),
        payloads: HashMap::new(),
        verified_payloads: HashMap::new(),
        map_payload: None,
        staging,
        payload_fingerprints: HashMap::new(),
        mutations: None,
        release_target_claims: Default::default(),
        created: Instant::now(),
    };
    Fixture {
        _temp: temp,
        roots: TransactionRoots {
            game_root,
            data_root,
        },
        session,
    }
}

fn write(root: &Path, path: &str, bytes: &[u8]) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn add_mod(fixture: &mut Fixture, id: &str, provides: Vec<ProvidedPlugin>, owned: Vec<String>) {
    let candidate = candidate(&format!("installed:{id}"), provides.clone());
    let node = ChangeNode {
        id: id.into(),
        kind: NodeKind::Mod,
        current: Some(candidate),
        enabled: provides.iter().any(|member| member.enabled),
        mutable: true,
        candidates: vec![],
        candidates_complete: true,
    };
    fixture.session.trusted.inventory.nodes.push(node.clone());
    fixture
        .session
        .solved
        .as_mut()
        .unwrap()
        .nodes
        .push(node.clone());
    let manifest =
        crate::archive::new_manifest(id, "1", BTreeMap::new(), BTreeMap::new(), owned, vec![]);
    write(
        &fixture.roots.data_root,
        &format!("manifests/{id}.toml"),
        toml::to_string(&manifest).unwrap().as_bytes(),
    );
    fixture.session.trusted.files.insert(
        id.into(),
        NodeFiles {
            summary: NodeSummary {
                node_id: id.into(),
                label: id.into(),
                name_i18n: Default::default(),
                map_name_zh: None,
                kind: NodeKind::Mod,
                presentation_kind: None,
                enabled: node.enabled,
                repository_id: None,
                package_id: Some(id.into()),
                dll_files: provides
                    .iter()
                    .filter_map(|member| member.member_key.clone())
                    .collect(),
                members: provides
                    .iter()
                    .map(|member| MemberSummary {
                        key: member.member_key.clone().unwrap(),
                        dll_file: member.member_key.clone().unwrap(),
                        enabled: member.enabled,
                        guid: Some(member.guid.clone()),
                        role: None,
                    })
                    .collect(),
                map_dir_name: None,
                map_id: None,
                subject_tag: None,
                candidates: vec![],
            },
            manifests: vec![manifest],
            map_path: None,
            map_metadata: None,
        },
    );
}

fn payload(fixture: &mut Fixture, id: &str, path: &str, enabled: bool) {
    let stage = fixture.session.staging.join(id);
    fs::create_dir_all(&stage).unwrap();
    write(&stage, path, b"new");
    let actual = candidate(&format!("new:{id}"), vec![member(path, enabled, "fixture")]);
    let node = fixture
        .session
        .solved
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
        .unwrap();
    node.current = Some(actual.clone());
    node.enabled = enabled;
    fixture.session.payloads.insert(
        id.into(),
        Payload {
            candidate: actual,
            notices: Vec::new(),
            member_roles: Default::default(),
            stage_fingerprint: Fingerprint::capture(&stage).unwrap(),
            stage_root: stage,
            release_files: None,
            downloaded: false,
            files: vec![path.into()],
            manifest: Some(crate::archive::new_manifest(
                id,
                "2",
                BTreeMap::new(),
                BTreeMap::new(),
                vec![path.into()],
                vec![],
            )),
        },
    );
}

#[test]
fn replacement_deletes_disabled_old_and_batches_manifest_with_final_flags() {
    let mut f = fixture();
    let dll = "BepInEx/plugins/old.dll";
    write(&f.roots.game_root, &mods::disabled_dll_file(dll), b"old");
    add_mod(
        &mut f,
        "mod",
        vec![member(dll, false, "fixture")],
        vec![dll.into()],
    );
    payload(&mut f, "mod", "BepInEx/plugins/new.dll", false);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    assert_eq!(
        fs::read(f.roots.game_root.join(mods::disabled_dll_file(dll))).unwrap(),
        b"old"
    );
    let prepared = super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap();
    prepared.apply().unwrap();
    assert!(
        !f.roots
            .game_root
            .join(mods::disabled_dll_file(dll))
            .exists()
    );
    assert_eq!(
        fs::read(f.roots.game_root.join("BepInEx/plugins/new.dll.disabled")).unwrap(),
        b"new"
    );
    let manifest: archive::InstallManifest =
        toml::from_str(&fs::read_to_string(f.roots.data_root.join("manifests/mod.toml")).unwrap())
            .unwrap();
    assert_eq!(manifest.files, vec!["BepInEx/plugins/new.dll.disabled"]);
}

#[test]
fn unowned_destination_or_disabled_counterpart_is_never_overwritten() {
    for suffix in ["", ".disabled"] {
        let mut f = fixture();
        let old = "BepInEx/plugins/old.dll";
        write(&f.roots.game_root, old, b"old");
        add_mod(
            &mut f,
            "mod",
            vec![member(old, true, "fixture")],
            vec![old.into()],
        );
        payload(&mut f, "mod", "BepInEx/plugins/new.dll", true);
        let path = format!("BepInEx/plugins/new.dll{suffix}");
        write(&f.roots.game_root, &path, b"user");
        assert_eq!(
            assemble(&f.session, f.roots.clone()).unwrap_err(),
            format!("modChanges.installTargetUnrecognized?path={path}")
        );
        assert_eq!(fs::read(f.roots.game_root.join(path)).unwrap(), b"user");
    }
}

#[test]
fn sharing_owned_files_blocks_replacement() {
    let mut f = fixture();
    let path = "BepInEx/plugins/shared.dll";
    write(&f.roots.game_root, path, b"old");
    add_mod(
        &mut f,
        "one",
        vec![member(path, true, "fixture")],
        vec![path.into()],
    );
    add_mod(
        &mut f,
        "two",
        vec![member("BepInEx/plugins/two.dll", true, "other")],
        vec![path.into()],
    );
    payload(&mut f, "one", path, true);
    assert_eq!(
        assemble(&f.session, f.roots.clone()).unwrap_err(),
        format!("modChanges.installTargetClaimed?path={path}")
    );
    assert_eq!(fs::read(f.roots.game_root.join(path)).unwrap(), b"old");
}

#[test]
fn member_toggle_uses_final_inventory_without_replaying_actions() {
    let mut f = fixture();
    let path = "BepInEx/plugins/member.dll";
    write(&f.roots.game_root, path, b"member");
    add_mod(
        &mut f,
        "mod",
        vec![member(path, true, "fixture")],
        vec![path.into()],
    );
    let node = &mut f.session.solved.as_mut().unwrap().nodes[0];
    node.enabled = false;
    node.current.as_mut().unwrap().provides[0].enabled = false;
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    assert_eq!(changes.len(), 1);
    super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply()
    .unwrap();
    assert!(
        f.roots
            .game_root
            .join(mods::disabled_dll_file(path))
            .is_file()
    );
}

#[test]
fn framework_root_migration_keeps_unknown_user_tree_and_rewrites_registry() {
    let mut f = fixture();
    let old = "BepInEx/plugins/old/framework.dll";
    write(&f.roots.game_root, old, b"old");
    write(
        &f.roots.game_root,
        "BepInEx/plugins/old/levels/unrecognized/user.txt",
        b"user",
    );
    add_mod(
        &mut f,
        "framework",
        vec![member(old, true, mods::plugin_target::DIYLEVEL_GUID)],
        vec![
            old.into(),
            "BepInEx/plugins/old/levels/unrecognized/user.txt".into(),
        ],
    );
    f.session.trusted.level_roots = crate::level_sets::LevelSetRoots::new(
        f.roots.game_root.clone(),
        Some(f.roots.game_root.join("BepInEx/plugins/old")),
    );
    payload(
        &mut f,
        "framework",
        "BepInEx/plugins/new/framework.dll",
        true,
    );
    f.session.solved.as_mut().unwrap().nodes[0]
        .current
        .as_mut()
        .unwrap()
        .provides[0]
        .guid = mods::plugin_target::DIYLEVEL_GUID.into();
    write(&f.roots.data_root, "manifests/level-set-plugins.json", br#"{"version":1,"entries":[{"dll":"BepInEx/plugins/old/levels/unrecognized/plugin.dll","owner":{"name":"map"}}]}"#);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply()
    .unwrap();
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join("BepInEx/plugins/new/levels/unrecognized/user.txt")
        )
        .unwrap(),
        b"user"
    );
    assert!(
        fs::read_to_string(f.roots.data_root.join("manifests/level-set-plugins.json"))
            .unwrap()
            .contains("new/levels/unrecognized/plugin.dll")
    );
}

#[test]
fn root_destination_collision_blocks_before_journal_or_live_write() {
    let mut f = fixture();
    let old = "BepInEx/plugins/old/framework.dll";
    write(&f.roots.game_root, old, b"old");
    write(
        &f.roots.game_root,
        "BepInEx/plugins/old/levels/user.txt",
        b"old user",
    );
    add_mod(
        &mut f,
        "framework",
        vec![member(old, true, mods::plugin_target::DIYLEVEL_GUID)],
        vec![old.into()],
    );
    f.session.trusted.level_roots = crate::level_sets::LevelSetRoots::new(
        f.roots.game_root.clone(),
        Some(f.roots.game_root.join("BepInEx/plugins/old")),
    );
    payload(
        &mut f,
        "framework",
        "BepInEx/plugins/new/framework.dll",
        true,
    );
    f.session.solved.as_mut().unwrap().nodes[0]
        .current
        .as_mut()
        .unwrap()
        .provides[0]
        .guid = mods::plugin_target::DIYLEVEL_GUID.into();
    write(
        &f.roots.game_root,
        "BepInEx/plugins/new/levels/user.txt",
        b"new user",
    );
    assert!(assemble(&f.session, f.roots.clone()).is_err());
    assert!(!f.roots.data_root.join("transactions").exists());
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join("BepInEx/plugins/new/levels/user.txt")
        )
        .unwrap(),
        b"new user"
    );
}

fn map_with_member(f: &mut Fixture, id: &str, path: &str, enabled: bool, member_enabled: bool) {
    let key = format!("{path}/nested/plugin.dll");
    write(&f.roots.game_root, &format!("{path}/info"), b"map info");
    write(
        &f.roots.game_root,
        &format!("{path}/unknown.txt"),
        b"user data",
    );
    write(
        &f.roots.game_root,
        &if member_enabled {
            key.clone()
        } else {
            mods::disabled_dll_file(&key)
        },
        b"plugin",
    );
    let current = candidate(
        &format!("installed:{id}"),
        vec![member(&key, member_enabled, "fixture-map")],
    );
    let node = ChangeNode {
        id: id.into(),
        kind: NodeKind::Map,
        current: Some(current),
        enabled,
        mutable: true,
        candidates: vec![],
        candidates_complete: true,
    };
    f.session.trusted.inventory.nodes.push(node.clone());
    f.session.solved.as_mut().unwrap().nodes.push(node);
    f.session.trusted.files.insert(
        id.into(),
        NodeFiles {
            summary: NodeSummary {
                node_id: id.into(),
                label: id.into(),
                name_i18n: Default::default(),
                map_name_zh: None,
                kind: NodeKind::Map,
                presentation_kind: None,
                enabled,
                repository_id: None,
                package_id: None,
                dll_files: vec![key.clone()],
                members: vec![MemberSummary {
                    key: inventory::key(&key),
                    dll_file: key,
                    enabled: member_enabled,
                    guid: Some("fixture-map".into()),
                    role: None,
                }],
                map_dir_name: Some(path.rsplit('/').next().unwrap().into()),
                map_id: None,
                subject_tag: None,
                candidates: vec![],
            },
            manifests: vec![],
            map_path: Some(path.into()),
            map_metadata: None,
        },
    );
}

fn framework_at(f: &mut Fixture, changing_root: bool) {
    let file = "BepInEx/plugins/old/framework.dll";
    write(&f.roots.game_root, file, b"framework");
    add_mod(
        f,
        "framework",
        vec![member(file, true, mods::plugin_target::DIYLEVEL_GUID)],
        vec![file.into()],
    );
    f.session.trusted.level_roots = crate::level_sets::LevelSetRoots::new(
        f.roots.game_root.clone(),
        Some(f.roots.game_root.join("BepInEx/plugins/old")),
    );
    if changing_root {
        payload(f, "framework", "BepInEx/plugins/new/framework.dll", true);
        f.session.solved.as_mut().unwrap().nodes[0]
            .current
            .as_mut()
            .unwrap()
            .provides[0]
            .guid = mods::plugin_target::DIYLEVEL_GUID.into();
    }
}

#[test]
fn framework_root_migration_combines_map_gates_and_internal_member_switches() {
    for changing_root in [false, true] {
        for target_enabled in [false, true] {
            let mut f = fixture();
            framework_at(&mut f, changing_root);
            let disabled = inventory::relative(
                &f.roots.game_root,
                &crate::level_sets::disabled_root(&f.roots.game_root),
            )
            .unwrap();
            let initial = if target_enabled {
                format!("{disabled}/Fixture")
            } else {
                "BepInEx/plugins/old/levels/Fixture".into()
            };
            let final_package = if changing_root {
                "BepInEx/plugins/new"
            } else {
                "BepInEx/plugins/old"
            };
            let destination = if target_enabled {
                format!("{final_package}/levels/Fixture")
            } else {
                format!("{disabled}/Fixture")
            };
            map_with_member(
                &mut f,
                "map:fixture",
                &initial,
                !target_enabled,
                !target_enabled,
            );
            let map = f
                .session
                .solved
                .as_mut()
                .unwrap()
                .nodes
                .iter_mut()
                .find(|node| node.id == "map:fixture")
                .unwrap();
            map.enabled = target_enabled;
            map.current.as_mut().unwrap().provides[0].enabled = target_enabled;
            f.session.request.roots.push(RootIntent {
                node_id: "map:fixture".into(),
                candidate_id: None,
                enabled: target_enabled,
                member_key: None,
            });
            write(&f.roots.data_root, "manifests/level-set-plugins.json", &serde_json::to_vec(&serde_json::json!({
                "version": 1, "entries": [{"dll": format!("{initial}/nested/plugin.dll"), "owner": {"name": "Fixture"}}]
            })).unwrap());
            let changes = assemble(&f.session, f.roots.clone()).unwrap();
            assert!(f.roots.game_root.join(&initial).is_dir());
            assert!(!f.roots.game_root.join(&destination).exists());
            let target_key = ItemKey::new("map:fixture", None);
            let target_index = changes
                .iter()
                .position(|record| record.items.contains(&target_key))
                .unwrap();
            let game = &f.roots.game_root;
            let mut saw_target = false;
            super::super::transaction::prepare(
                &f.roots.data_root.join("transactions"),
                &f.roots,
                changes.into_iter().map(|owned| owned.mutation).collect(),
            )
            .unwrap()
            .apply_with_cancel(
                || false,
                |event| {
                    if let RecordProgress::Applying(index) = event {
                        if index <= target_index {
                            assert!(game.join(&initial).is_dir());
                            assert!(!game.join(&destination).exists());
                        }
                        if index == target_index {
                            saw_target = true;
                            if changing_root {
                                assert_eq!(
                                    fs::read(game.join("BepInEx/plugins/new/framework.dll"))
                                        .unwrap(),
                                    b"new"
                                );
                                assert!(!game.join("BepInEx/plugins/old/framework.dll").exists());
                            }
                            let registry = crate::level_sets::read_registered_plugins(
                                &f.roots.data_root.join("manifests"),
                            );
                            assert_eq!(
                                inventory::key(&registry[0].dll),
                                inventory::key(&format!("{destination}/nested/plugin.dll"))
                            );
                        }
                    }
                },
            )
            .unwrap();
            assert!(saw_target);
            let dll = if target_enabled {
                format!("{destination}/nested/plugin.dll")
            } else {
                format!("{destination}/nested/plugin.dll.disabled")
            };
            assert_eq!(fs::read(f.roots.game_root.join(dll)).unwrap(), b"plugin");
            assert_eq!(
                fs::read(f.roots.game_root.join(format!("{destination}/unknown.txt"))).unwrap(),
                b"user data"
            );
            assert!(!f.roots.game_root.join(initial).exists());
            let registry: serde_json::Value = serde_json::from_slice(
                &fs::read(f.roots.data_root.join("manifests/level-set-plugins.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(
                inventory::key(registry["entries"][0]["dll"].as_str().unwrap()),
                inventory::key(&format!("{destination}/nested/plugin.dll"))
            );
        }
    }
}

#[test]
fn framework_root_migration_changes_internal_member_without_changing_map_gate() {
    let mut f = fixture();
    framework_at(&mut f, true);
    let initial = "BepInEx/plugins/old/levels/Fixture";
    map_with_member(&mut f, "map:fixture", initial, true, false);
    let map = f
        .session
        .solved
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| node.id == "map:fixture")
        .unwrap();
    map.current.as_mut().unwrap().provides[0].enabled = true;
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    let map_switch = ItemKey::new(
        "map:fixture",
        Some(&inventory::key(&format!("{initial}/nested/plugin.dll"))),
    );
    let tree_edits = changes
        .iter()
        .filter_map(|owned| match &owned.mutation {
            Mutation::Tree(edit) if owned.items.contains(&map_switch) => {
                Some(inventory::key(&edit.target.relative))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tree_edits,
        ["BepInEx/plugins/old/levels", "BepInEx/plugins/new/levels"].map(inventory::key)
    );
    super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply()
    .unwrap();
    assert!(
        f.roots
            .game_root
            .join("BepInEx/plugins/new/levels/Fixture/nested/plugin.dll")
            .is_file()
    );
    assert!(!f.roots.game_root.join(initial).exists());
}
