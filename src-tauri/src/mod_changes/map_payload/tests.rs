use super::super::session::{ChangePlan, PlanRequest};
use super::*;
use std::{collections::HashMap, io::Write, time::Instant};

#[path = "bundled_tests.rs"]
mod bundled_tests;

#[test]
fn metadata_copy_preserves_unrelated_snapshots_and_rejects_future_registry() {
    let temp = tempfile::tempdir().unwrap();
    let from = temp.path().join("from");
    let to = temp.path().join("to");
    fs::create_dir_all(&from).unwrap();
    fs::create_dir_all(&to).unwrap();
    let bytes = br#"{"schema_version":1,"entries":{"untouched":{"repository_id":"old","level_set_name":"Other","author":"Author","group":""}}}"#;
    fs::write(from.join("map-relations.json"), bytes).unwrap();
    copy_metadata(&from, &to).unwrap();
    assert_eq!(fs::read(to.join("map-relations.json")).unwrap(), bytes);
    fs::write(
        from.join(level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        br#"{"version":2,"entries":[]}"#,
    )
    .unwrap();
    assert_eq!(
        copy_metadata(&from, &to).unwrap_err(),
        "modChanges.ambiguousOwnership"
    );
}

#[test]
fn loose_loader_input_never_escapes_the_graph() {
    let temp = tempfile::tempdir().unwrap();
    let stage = fs::canonicalize(temp.path()).unwrap();
    fs::write(stage.join("stray.dll"), b"bytes").unwrap();
    let batch = MapBatch {
        stage_root: stage.clone(),
        manifests_root: stage.clone(),
        source_path: stage.clone(),
        source_fingerprint: Fingerprint::Missing,
        node_ids: BTreeSet::new(),
        maps: vec![],
        external: vec![],
        extra_files: vec![],
    };
    assert_eq!(
        loose_files(&batch).unwrap_err(),
        "modChanges.ambiguousOwnership"
    );
}

struct Fixture {
    _temp: tempfile::TempDir,
    roots: TransactionRoots,
    session: PlanSession,
    registry_other: String,
}

fn write(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn fixture() -> Fixture {
    fixture_with_release(None)
}

fn fixture_with_release(origin: Option<MapInstallOrigin>) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let game = root.join("game");
    let data = root.join("data");
    let staging = data.join("stage");
    fs::create_dir_all(&game).unwrap();
    fs::create_dir_all(&staging).unwrap();
    let stage = staging.join("map-game");
    let manifests = data.join("manifests");
    let stage_manifests = staging.join("map-manifests");
    fs::create_dir_all(&stage).unwrap();
    fs::create_dir_all(&stage_manifests).unwrap();
    let metadata = crate::asset_bundle::LevelSetMetadata {
        name: "Fixture".into(),
        author: "Tests".into(),
        version: "2.0".into(),
        ..Default::default()
    };
    let old_map = format!("{}/levels/KeepDirectory", crate::framework::DIYLEVEL_DIR);
    write(&game, &format!("{old_map}/info"), b"old-info");
    write(&game, &format!("{old_map}/old-level"), b"old");
    let mut old_meta = metadata.clone();
    old_meta.version = "1.0".into();
    level_sets::seed_info_metadata(&game.join(format!("{old_map}/info")), old_meta);
    let external = "BepInEx/plugins/External/Fix.dll";
    write(
        &game,
        &crate::mods::disabled_dll_file(external),
        &crate::mods::plugin_blob("fixture-fix", "Fix", "1.0"),
    );
    let registry_other = "BepInEx/plugins/Other/Unrelated.dll".to_string();
    let registry_bytes = serde_json::to_vec(&serde_json::json!({"version":1,"entries":[
        {"dll":external,"owner":{"name":"Fixture","author":"Tests"}},
        {"dll":registry_other,"owner":{"name":"Untouched","author":"Other"}}
    ]}))
    .unwrap();
    write(
        &data,
        &format!("manifests/{}", level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        &registry_bytes,
    );
    copy_metadata(&manifests, &stage_manifests).unwrap();
    let mut config = crate::config::default_config();
    if origin.is_some() {
        config.map_repositories = ["mirror", "selected"]
            .into_iter()
            .map(|id| {
                serde_json::from_value(serde_json::json!({
                    "id": id, "level_set_name": "Fixture", "author": "Tests",
                    "group": id,
                    "relations": [{"kind": "compatible", "guid": format!("fixture-{id}")}],
                }))
                .unwrap()
            })
            .collect();
    }
    let live_roots = level_sets::LevelSetRoots::new(
        game.clone(),
        Some(game.join(crate::framework::DIYLEVEL_DIR)),
    );
    let mut inventory = Inventory::default();
    let mut files = BTreeMap::new();
    super::super::inventory_maps::append_at(
        &manifests,
        &config,
        &game,
        &live_roots,
        &mut inventory,
        &mut files,
    )
    .unwrap();
    let fw_key = format!("{}/DIYLevel.dll", crate::framework::DIYLEVEL_DIR);
    write(&game, &fw_key, b"framework-fixture");
    let framework = Candidate {
        id: "installed:framework".into(),
        source_id: None,
        tag: None,
        version: Some("0.11".into()),
        provides: vec![ProvidedPlugin {
            member_key: Some(fw_key),
            guid: crate::mods::plugin_target::DIYLEVEL_GUID.into(),
            version: Some("0.11".into()),
            prerelease: None,
            enabled: true,
        }],
        relations: vec![],
        relations_known: true,
        installable: true,
        raw: false,
        verification_pending: false,
    };
    inventory.nodes.push(ChangeNode {
        id: "framework".into(),
        kind: NodeKind::Mod,
        current: Some(framework),
        enabled: true,
        mutable: true,
        candidates: vec![],
        candidates_complete: true,
    });
    let trusted = TrustedInventory {
        inventory,
        files,
        revision: "fixture".into(),
        game_root: game.clone(),
        level_roots: live_roots,
        config,
        managed_mods: Default::default(),
    };
    let zip_path = staging.join("map.zip");
    let mut writer = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    let info_entry = "BepInEx/plugins/OC2DIYLevel/levels/NewDirectory/info";
    for (name, bytes) in [
        (info_entry, b"new-info".as_slice()),
        (
            "BepInEx/plugins/OC2DIYLevel/levels/NewDirectory/new-level",
            b"new-level".as_slice(),
        ),
        (
            external,
            crate::mods::plugin_blob("fixture-fix", "Fix", "2.0").as_slice(),
        ),
    ] {
        writer
            .start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
    let source_fingerprint = Fingerprint::capture(&zip_path).unwrap();
    let mut imported: level_sets::ImportedLevelSet = level_sets::ImportedLevelSet {
        dir_name: "NewDirectory".into(),
        layout: crate::models::LevelSetLayout::Bepinex,
        metadata: metadata.clone(),
        files: vec![(info_entry.into(), "info".into())],
        plugins: vec![],
    };
    let plugin = crate::mods::read_dll_details(
        &game.join(crate::mods::disabled_dll_file(external)),
        "Fix.dll",
    );
    imported.plugins.push(crate::models::ZipLevelSetPlugin {
        path: external.into(),
        file_name: "Fix.dll".into(),
        install_path: Some(external.into()),
        info: plugin.into(),
    });
    let roots = level_sets::LevelSetRoots::new(
        stage.clone(),
        Some(stage.join(crate::framework::DIYLEVEL_DIR)),
    );
    let metadata_refs = [&imported.metadata];
    let primary = validate_origin(&metadata_refs, origin.as_ref(), &trusted.config).unwrap();
    projection::seed_snapshots(&trusted.config, &metadata_refs, primary, &stage_manifests).unwrap();
    if primary.is_some() {
        assert!(verify_staged_source(&stage_manifests, primary).is_err());
    }
    level_sets::install_parsed_sets(
        std::slice::from_ref(&imported),
        &zip_path,
        &roots,
        &stage_manifests,
        &["NewDirectory".into()],
        origin.as_ref(),
    )
    .unwrap();
    verify_staged_source(&stage_manifests, primary).unwrap();
    level_sets::seed_info_metadata(&stage.join(info_entry), metadata);
    let mut projected = Inventory::default();
    let mut projected_files = BTreeMap::new();
    super::super::inventory_maps::append_at(
        &stage_manifests,
        &trusted.config,
        &stage,
        &roots,
        &mut projected,
        &mut projected_files,
    )
    .unwrap();
    let plan = ChangePlan {
        plan_id: "fixture".into(),
        result: super::super::solve(&trusted.inventory, &ChangeRequest::default()),
        roots: vec![],
        nodes: vec![],
        install_notes: vec![],
        prepared: false,
    };
    let mut session = PlanSession {
        trusted,
        request: ChangeRequest::default(),
        wire_request: PlanRequest::default(),
        plan,
        solved: None,
        payloads: HashMap::new(),
        verified_payloads: HashMap::new(),
        map_payload: None,
        staging,
        payload_fingerprints: HashMap::new(),
        mutations: None,
        created: Instant::now(),
    };
    let mut batch = MapBatch {
        stage_root: stage,
        manifests_root: stage_manifests,
        source_path: zip_path,
        source_fingerprint,
        node_ids: BTreeSet::new(),
        maps: vec![],
        external: vec![],
        extra_files: vec![],
    };
    projection::merge(
        &mut session,
        &mut batch,
        projected,
        projected_files,
        &level_sets::read_registered_plugins(&manifests),
        1,
    )
    .unwrap();
    batch.extra_files = loose_files(&batch).unwrap();
    session.map_payload = Some(batch);
    let (result, solved) =
        super::super::solve_with_inventory(&session.trusted.inventory, &session.request);
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    session.solved = solved;
    Fixture {
        _temp: temp,
        roots: TransactionRoots {
            game_root: game,
            data_root: data,
        },
        session,
        registry_other,
    }
}

#[test]
fn online_map_update_uses_the_chosen_source_when_a_mirror_has_the_same_identity() {
    let origin = serde_json::from_value(serde_json::json!({
        "repository_id": "selected", "level_set_name": "Fixture", "level_set_name_zh": "",
        "author": "Tests", "mapset_version": "2.0",
    }))
    .unwrap();
    let fixture = fixture_with_release(Some(origin));
    let planned = fixture
        .session
        .solved
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|node| node.id == "map:KeepDirectory")
        .unwrap()
        .current
        .as_ref()
        .unwrap();
    assert_eq!(planned.source_id.as_deref(), Some("selected"));
    assert!(
        planned
            .relations
            .iter()
            .any(|owned| owned.relation.guid == "fixture-selected")
    );
    assert!(
        !planned
            .relations
            .iter()
            .any(|owned| owned.relation.guid == "fixture-mirror")
    );
    let mutations = output::build_at(&fixture.roots.data_root, &fixture.session).unwrap();
    super::super::transaction::prepare(
        &fixture.roots.data_root.join("transactions"),
        &fixture.roots,
        mutations,
    )
    .unwrap()
    .apply()
    .unwrap();
    let snapshots =
        archive::read_map_relation_snapshots(&fixture.roots.data_root.join("manifests"));
    let staged_info = fixture
        .session
        .map_payload
        .as_ref()
        .unwrap()
        .stage_root
        .join(format!(
            "{}/levels/NewDirectory/info",
            crate::framework::DIYLEVEL_DIR
        ));
    let installed_info = fixture.roots.game_root.join(format!(
        "{}/levels/KeepDirectory/info",
        crate::framework::DIYLEVEL_DIR
    ));
    assert_eq!(fs::read(&installed_info).unwrap(), b"new-info");
    // The fixture's tiny synthetic info does not parse as UnityFS. Seed the
    // same parsed metadata after verifying the transaction replaced its bytes.
    level_sets::seed_info_metadata(
        &installed_info,
        level_sets::read_info_metadata(&staged_info).unwrap(),
    );
    let mut info = level_sets::scan_level_sets(&fixture.session.trusted.level_roots)
        .into_iter()
        .find(|set| set.dir_name == "KeepDirectory")
        .map(level_sets::view::level_set_to_info)
        .unwrap();
    level_sets::view::bind_map_relation_snapshot(
        &mut info,
        &fixture.session.trusted.config,
        &snapshots,
    );
    assert_eq!(info.map_id.as_deref(), Some("selected"));
    assert_eq!(info.installed_mapset_version, "2.0");
    assert_eq!(info.map_group.as_deref(), Some("selected"));
    assert_eq!(info.version, "2.0");
    assert_eq!(info.relations[0].guid, "fixture-selected");
}

#[test]
fn map_update_does_not_guess_missing_or_inconsistent_source_provenance() {
    let fixture = fixture_with_release(None);
    let mut config = fixture.session.trusted.config;
    let metadata = crate::asset_bundle::LevelSetMetadata {
        name: "Fixture".into(),
        author: "Tests".into(),
        ..Default::default()
    };
    let mut origin: MapInstallOrigin = serde_json::from_value(serde_json::json!({
        "level_set_name": "Fixture", "level_set_name_zh": "", "author": "Tests", "mapset_version": "2.0",
    })).unwrap();
    config.map_repositories = vec![
        serde_json::from_value(serde_json::json!({
            "id": "first", "author": "Tests", "level_set_name": "Fixture"
        }))
        .unwrap(),
    ];
    let validated = validate_origin(&[&metadata], Some(&origin), &config)
        .unwrap()
        .unwrap();
    assert_eq!(
        validated.1.id, "first",
        "legacy DTO must work for a unique source"
    );
    let mut duplicate = config.map_repositories[0].clone();
    duplicate.id = "second".to_owned();
    config.map_repositories.push(duplicate);
    assert!(validate_origin(&[&metadata], Some(&origin), &config).is_err());
    assert!(
        projection::seed_snapshots(
            &config,
            &[&metadata],
            None,
            fixture.roots.data_root.as_path()
        )
        .is_err()
    );
    origin.repository_id = Some("second".to_owned());
    assert!(validate_origin(&[&metadata, &metadata], Some(&origin), &config).is_err());
    config.map_repositories[1].level_set_name = "Different".to_owned();
    assert!(validate_origin(&[&metadata], Some(&origin), &config).is_err());
    config.map_repositories[1].id = "first".to_owned();
    origin.repository_id = Some("first".to_owned());
    assert!(validate_origin(&[&metadata], Some(&origin), &config).is_err());
    origin.repository_id = Some("gone".to_owned());
    assert!(validate_origin(&[&metadata], Some(&origin), &config).is_err());
}

#[test]
fn sandbox_map_update_is_one_tree_and_keeps_external_disabled_and_other_registry_owner() {
    let fixture = fixture();
    let mutations = output::build_at(&fixture.roots.data_root, &fixture.session).unwrap();
    let tree_targets = mutations
        .iter()
        .filter_map(|mutation| {
            if let Mutation::Tree(change) = mutation {
                Some(&change.target.relative)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        tree_targets,
        vec![&format!(
            "{}/levels/KeepDirectory",
            crate::framework::DIYLEVEL_DIR
        )]
    );
    let old = fixture.roots.game_root.join(&tree_targets[0]);
    assert!(old.join("old-level").is_file());
    assert!(!old.join("new-level").exists());
    let transaction = super::super::transaction::prepare(
        &fixture.roots.data_root.join("transactions"),
        &fixture.roots,
        mutations,
    )
    .unwrap();
    assert!(old.join("old-level").is_file(), "prepare不能碰live");
    transaction.apply().unwrap();
    assert!(!old.join("old-level").exists());
    assert_eq!(fs::read(old.join("new-level")).unwrap(), b"new-level");
    assert!(
        !fixture
            .roots
            .game_root
            .join("BepInEx/plugins/External/Fix.dll")
            .exists()
    );
    let details = crate::mods::read_dll_details(
        &fixture
            .roots
            .game_root
            .join("BepInEx/plugins/External/Fix.dll.disabled"),
        "Fix.dll",
    );
    assert_eq!(details.version.as_deref(), Some("2.0"));
    let registered =
        level_sets::read_registered_plugins(&fixture.roots.data_root.join("manifests"));
    assert!(
        registered
            .iter()
            .any(|entry| entry.dll == fixture.registry_other)
    );
    assert_eq!(registered.len(), 2);
}

#[test]
fn map_tree_endpoint_collisions_and_external_active_disabled_duplicates_block() {
    let fixture = fixture();
    write(
        &fixture.roots.game_root,
        "BepInEx/plugins/External/Fix.dll",
        b"unexpected second variant",
    );
    assert!(
        matches!(output::build_at(&fixture.roots.data_root, &fixture.session), Err(error) if error == "modChanges.ambiguousOwnership")
    );
    let mut result = Vec::new();
    let mut targets = BTreeSet::new();
    let parent = format!("{}/levels", crate::framework::DIYLEVEL_DIR);
    output::push_tree(&mut result, &mut targets, &fixture.roots, &parent, None).unwrap();
    assert!(
        output::push_tree(
            &mut result,
            &mut targets,
            &fixture.roots,
            &format!("{parent}/Child"),
            None
        )
        .is_err()
    );
}

#[test]
fn framework_directory_change_and_map_update_commit_with_all_map_and_member_switches() {
    let mut fixture = fixture();
    let old_package = crate::framework::DIYLEVEL_DIR;
    let old_dll = format!("{old_package}/DIYLevel.dll");
    let old_map_file = format!("{old_package}/levels/KeepDirectory/old-level");
    let other = format!("{old_package}/levels/Unrelated/keep.txt");
    write(&fixture.roots.game_root, &other, b"unrelated map");
    let registered_path = fixture
        .session
        .map_payload
        .as_ref()
        .unwrap()
        .manifests_root
        .join(level_sets::LEVEL_SET_PLUGIN_REGISTRY);
    let mut registry: serde_json::Value =
        serde_json::from_slice(&fs::read(&registered_path).unwrap()).unwrap();
    registry["entries"].as_array_mut().unwrap().push(serde_json::json!({"dll":format!("{old_package}/levels/Unrelated/Internal.dll"),"owner":{"name":"Untouched","author":"Other"}}));
    let disabled_root = relative(
        &fixture.roots.game_root,
        &level_sets::disabled_root(&fixture.roots.game_root),
    )
    .unwrap();
    for (name, was_enabled) in [("DisableExisting", true), ("EnableExisting", false)] {
        let before = if was_enabled {
            format!("{old_package}/levels/{name}")
        } else {
            format!("{disabled_root}/{name}")
        };
        let member = format!("{before}/nested/Plugin.dll");
        let physical = if was_enabled {
            member.clone()
        } else {
            crate::mods::disabled_dll_file(&member)
        };
        write(
            &fixture.roots.game_root,
            &physical,
            b"existing internal member",
        );
        write(
            &fixture.roots.game_root,
            &format!("{before}/keep.txt"),
            b"keep map files",
        );
        let current = Candidate {
            id: format!("map:{name}"),
            source_id: None,
            tag: None,
            version: Some("1".into()),
            provides: vec![ProvidedPlugin {
                member_key: Some(key(&member)),
                guid: format!("fixture-{name}"),
                version: Some("1".into()),
                prerelease: None,
                enabled: was_enabled,
            }],
            relations: vec![],
            relations_known: true,
            installable: true,
            raw: false,
            verification_pending: false,
        };
        let node = ChangeNode {
            id: format!("map:{name}"),
            kind: NodeKind::Map,
            current: Some(current),
            enabled: was_enabled,
            mutable: true,
            candidates: vec![],
            candidates_complete: true,
        };
        let files = NodeFiles {
            summary: NodeSummary {
                node_id: node.id.clone(),
                label: name.into(),
                kind: NodeKind::Map,
                enabled: was_enabled,
                repository_id: None,
                package_id: None,
                dll_files: vec![member.clone()],
                members: vec![MemberSummary {
                    key: key(&member),
                    dll_file: member.clone(),
                    enabled: was_enabled,
                    guid: Some(format!("fixture-{name}")),
                }],
                map_dir_name: Some(name.into()),
                map_id: None,
                subject_tag: None,
                candidates: vec![],
            },
            manifests: vec![],
            map_path: Some(before),
            map_metadata: None,
        };
        fixture.session.trusted.files.insert(node.id.clone(), files);
        fixture.session.trusted.inventory.nodes.push(node.clone());
        let mut switched = node;
        switched.enabled = !was_enabled;
        switched.current.as_mut().unwrap().provides[0].enabled = !was_enabled;
        fixture
            .session
            .solved
            .as_mut()
            .unwrap()
            .nodes
            .push(switched);
        registry["entries"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"dll":member,"owner":{"name":name,"author":"Tests"}}));
    }
    fs::write(&registered_path, serde_json::to_vec(&registry).unwrap()).unwrap();
    let old_manifest = archive::new_manifest(
        "framework",
        "0.11",
        BTreeMap::new(),
        BTreeMap::new(),
        vec![old_dll.clone(), old_map_file],
        vec![],
    );
    fixture.session.trusted.files.insert(
        "framework".into(),
        NodeFiles {
            summary: NodeSummary {
                node_id: "framework".into(),
                label: "Framework".into(),
                kind: NodeKind::Mod,
                enabled: true,
                repository_id: None,
                package_id: Some("framework".into()),
                dll_files: vec![old_dll.clone()],
                members: vec![MemberSummary {
                    key: key(&old_dll),
                    dll_file: old_dll,
                    enabled: true,
                    guid: Some(crate::mods::plugin_target::DIYLEVEL_GUID.into()),
                }],
                map_dir_name: None,
                map_id: None,
                subject_tag: None,
                candidates: vec![],
            },
            manifests: vec![old_manifest.clone()],
            map_path: None,
            map_metadata: None,
        },
    );
    write(
        &fixture.roots.data_root,
        "manifests/framework.toml",
        toml::to_string_pretty(&old_manifest).unwrap().as_bytes(),
    );
    let new_dll = "BepInEx/plugins/OlderFramework/DIYLevel.dll";
    let payload_root = fixture.session.staging.join("framework-asset");
    fs::create_dir(&payload_root).unwrap();
    write(&payload_root, new_dll, b"new framework");
    let node = fixture
        .session
        .solved
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| node.id == "framework")
        .unwrap();
    let mut next = node.current.clone().unwrap();
    next.id = "verified:older-framework".into();
    next.version = Some("0.10".into());
    next.provides[0].member_key = Some(key(new_dll));
    next.provides[0].version = Some("0.10".into());
    node.current = Some(next.clone());
    fixture.session.payloads.insert(
        "framework".into(),
        super::super::session::Payload {
            candidate: next,
            stage_fingerprint: Fingerprint::capture(&payload_root).unwrap(),
            stage_root: payload_root,
            release_files: None,
            files: vec![new_dll.into()],
            manifest: Some(archive::new_manifest(
                "framework",
                "0.10",
                BTreeMap::new(),
                BTreeMap::new(),
                vec![new_dll.into()],
                vec![],
            )),
        },
    );
    let mut mutations =
        super::super::mutations::assemble(&fixture.session, fixture.roots.clone()).unwrap();
    mutations.extend(output::build_at(&fixture.roots.data_root, &fixture.session).unwrap());
    let endpoints = mutations
        .iter()
        .filter_map(|mutation| {
            if let Mutation::Tree(change) = mutation {
                Some(change.target.relative.clone())
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        endpoints
            .iter()
            .map(|path| key(path))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            key(&format!("{old_package}/levels")),
            key("BepInEx/plugins/OlderFramework/levels"),
            key(&format!("{disabled_root}/EnableExisting")),
            key(&format!("{disabled_root}/DisableExisting")),
        ])
    );
    let transaction = super::super::transaction::prepare(
        &fixture.roots.data_root.join("transactions"),
        &fixture.roots,
        mutations,
    )
    .unwrap();
    assert!(fixture.roots.game_root.join(&other).is_file());
    transaction.apply().unwrap();
    let new_levels = fixture
        .roots
        .game_root
        .join("BepInEx/plugins/OlderFramework/levels");
    assert_eq!(
        fs::read(new_levels.join("Unrelated/keep.txt")).unwrap(),
        b"unrelated map"
    );
    assert_eq!(
        fs::read(new_levels.join("KeepDirectory/new-level")).unwrap(),
        b"new-level"
    );
    assert!(!new_levels.join("DisableExisting").exists());
    assert!(
        new_levels
            .join("EnableExisting/nested/Plugin.dll")
            .is_file()
    );
    assert!(
        !new_levels
            .join("EnableExisting/nested/Plugin.dll.disabled")
            .exists()
    );
    assert!(
        fixture
            .roots
            .game_root
            .join(format!(
                "{disabled_root}/DisableExisting/nested/Plugin.dll.disabled"
            ))
            .is_file()
    );
    assert!(
        !fixture
            .roots
            .game_root
            .join(format!("{disabled_root}/EnableExisting"))
            .exists()
    );
    assert!(
        !fixture
            .roots
            .game_root
            .join(format!("{old_package}/levels"))
            .exists()
    );
    assert_eq!(
        fs::read(fixture.roots.game_root.join(new_dll)).unwrap(),
        b"new framework"
    );
    let registry = level_sets::read_registered_plugins(&fixture.roots.data_root.join("manifests"));
    assert!(registry.iter().any(|entry| key(&entry.dll)
        == key(&format!(
            "{disabled_root}/DisableExisting/nested/Plugin.dll"
        ))));
    assert!(registry.iter().any(|entry| key(&entry.dll)
        == key("BepInEx/plugins/OlderFramework/levels/EnableExisting/nested/Plugin.dll")));
    assert!(
        registry.iter().any(|entry| key(&entry.dll)
            == key("BepInEx/plugins/OlderFramework/levels/Unrelated/Internal.dll")),
        "unrelated registry member must follow the normalized framework root: {registry:?}"
    );
    assert!(
        registry
            .iter()
            .any(|entry| entry.dll == fixture.registry_other)
    );
}
