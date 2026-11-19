//! Map DLLs in real framework ZIPs must never advertise unwritten plugin versions.
use super::bundled_maps_tests::{two_zip_replacements, zip_payload};
use super::*;
use crate::models::{Provider, RelationKind, RepositoryRelation};

#[test]
fn bundled_zip_existing_map_dll_keeps_only_real_inventory_member_and_bytes() {
    let mut f = two_zip_replacements();
    let map_dll = "BepInEx/plugins/OC2DIYLevel/levels/oc1_story/Map.dll";
    let old_dll = mods::plugin_blob("fixture-map-dll", "Map", "1.0.0");
    write(&f.roots.game_root, map_dll, &old_dll);
    crate::level_sets::seed_info_metadata(
        &f.roots
            .game_root
            .join("BepInEx/plugins/OC2DIYLevel/levels/oc1_story/info"),
        crate::asset_bundle::LevelSetMetadata {
            name: "Bundled".into(),
            author: "Fixture".into(),
            version: "1.0.0".into(),
            ..Default::default()
        },
    );
    let mut repo = crate::community::repository_from_coordinates(
        &Provider::Github,
        "Fixture",
        "map-member",
        "",
        "",
    )
    .unwrap();
    repo.plugin_guid = "fixture-map-dll".into();
    repo.relations = vec![RepositoryRelation {
        kind: RelationKind::Requires,
        guid: mods::plugin_target::DIYLEVEL_GUID.into(),
        version: ">=0.10".into(),
        prerelease: None,
        optional: false,
    }];
    f.session.trusted.config.repositories.push(repo);
    let mut maps = Inventory::default();
    let mut map_files = BTreeMap::new();
    super::super::super::inventory_maps::append_at(
        &f.roots.data_root.join("manifests"),
        &f.session.trusted.config,
        &f.roots.game_root,
        &f.session.trusted.level_roots,
        &mut maps,
        &mut map_files,
    )
    .unwrap();
    let map = maps
        .nodes
        .into_iter()
        .find(|map| map.id == "map:oc1_story")
        .unwrap();
    assert_eq!(
        map.current.as_ref().unwrap().provides[0].version.as_deref(),
        Some("1.0.0")
    );
    f.session
        .trusted
        .files
        .insert(map.id.clone(), map_files.remove(&map.id).unwrap());
    f.session.trusted.inventory.nodes.push(map.clone());
    f.session.solved.as_mut().unwrap().nodes.push(map);
    let framework_dll = mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.10.0");
    let new_map_dll = mods::plugin_blob("fixture-map-dll", "Map", "2.0.0");
    // Replace the private preview, retaining the same actual framework node.
    f.session
        .trusted
        .config
        .repositories
        .retain(|repo| repo.id != "framework");
    let stage = f.session.payloads.remove("framework").unwrap().stage_root;
    fs::remove_dir_all(stage).unwrap();
    zip_payload(
        &mut f,
        "framework",
        mods::plugin_target::DIYLEVEL_GUID,
        "0.10.0",
        &[
            ("OC2DIYLevel/LevelEditorStub.dll", &framework_dll),
            ("OC2DIYLevel/levels/oc1_story/info", b"unwritten-map"),
            ("OC2DIYLevel/levels/oc1_story/Map.dll", &new_map_dll),
        ],
    );
    let verified = &f.session.payloads["framework"].candidate;
    assert_eq!(
        verified.provides.len(),
        1,
        "framework must exclude bundled map DLL"
    );
    assert!(
        verified
            .relations
            .iter()
            .all(|relation| relation.owner_guid.as_deref() != Some("fixture-map-dll"))
    );
    let (result, solved) = super::super::super::solve::solve_with_inventory(
        f.session.solved.as_ref().unwrap(),
        &ChangeRequest::default(),
    );
    assert_eq!(result.status, SolveStatus::Ready, "{result:?}");
    let solved = solved.unwrap();
    let map_members = solved
        .nodes
        .iter()
        .filter_map(|node| node.current.as_ref())
        .flat_map(|candidate| &candidate.provides)
        .filter(|member| member.guid == "fixture-map-dll")
        .collect::<Vec<_>>();
    assert_eq!(map_members.len(), 1);
    assert_eq!(map_members[0].version.as_deref(), Some("1.0.0"));
    f.session.solved = Some(solved);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    super::super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply()
    .unwrap();
    assert_eq!(fs::read(f.roots.game_root.join(map_dll)).unwrap(), old_dll);
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join("BepInEx/plugins/OC2DIYLevel/levels/oc1_story/info")
        )
        .unwrap(),
        b"user-preserved-map"
    );
}

#[test]
fn bundled_zip_missing_map_with_dll_is_rejected_before_live_writes() {
    for name in ["Map.dll", "Map.dll.disabled"] {
        let mut f = two_zip_replacements();
        f.session
            .trusted
            .config
            .repositories
            .retain(|repo| repo.id != "framework");
        let stage = f.session.payloads.remove("framework").unwrap().stage_root;
        fs::remove_dir_all(stage).unwrap();
        let framework = mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.10.0");
        let map = mods::plugin_blob("fixture-map-dll", "Map", "2.0.0");
        let path = format!("OC2DIYLevel/levels/new-map/nested/{name}");
        zip_payload(
            &mut f,
            "framework",
            mods::plugin_target::DIYLEVEL_GUID,
            "0.10.0",
            &[
                ("OC2DIYLevel/LevelEditorStub.dll", &framework),
                ("OC2DIYLevel/levels/new-map/info", b"map-info"),
                (&path, &map),
            ],
        );
        assert_eq!(f.session.payloads["framework"].candidate.provides.len(), 1);
        let before = Fingerprint::capture(&f.roots.game_root).unwrap();
        assert_eq!(
            assemble(&f.session, f.roots.clone()).unwrap_err(),
            "modChanges.ambiguousOwnership"
        );
        assert_eq!(Fingerprint::capture(&f.roots.game_root).unwrap(), before);
    }
}
