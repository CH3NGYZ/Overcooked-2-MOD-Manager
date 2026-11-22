//! A map import and a framework reinstall share the tree composer in one batch.
use super::*;

#[test]
fn same_root_framework_bundle_and_map_update_keep_disjoint_tree_endpoints() {
    let mut f = fixture();
    let package = crate::framework::DIYLEVEL_DIR;
    let dll = format!("{package}/DIYLevel.dll");
    let sample = format!("{package}/levels/BundledExample/info");
    let replaced_sample = format!("{package}/levels/KeepDirectory/info");
    let old_manifest = archive::new_manifest(
        "framework",
        "0.10",
        BTreeMap::new(),
        BTreeMap::new(),
        vec![dll.clone()],
        vec![],
    );
    write(
        &f.roots.data_root,
        "manifests/framework.toml",
        toml::to_string(&old_manifest).unwrap().as_bytes(),
    );
    f.session.trusted.files.insert(
        "framework".into(),
        super::super::super::inventory::NodeFiles {
            summary: super::super::super::inventory::NodeSummary {
                node_id: "framework".into(),
                label: "DIYLevel".into(),
                name_i18n: Default::default(),
                map_name_zh: None,
                kind: NodeKind::Mod,
                presentation_kind: None,
                enabled: true,
                repository_id: None,
                package_id: Some("framework".into()),
                dll_files: vec![dll.clone()],
                members: vec![super::super::super::inventory::MemberSummary {
                    key: key(&dll),
                    dll_file: dll.clone(),
                    enabled: true,
                    guid: Some(crate::mods::plugin_target::DIYLEVEL_GUID.into()),
                    role: None,
                }],
                map_dir_name: None,
                map_id: None,
                subject_tag: None,
                candidates: vec![],
            },
            manifests: vec![old_manifest],
            map_path: None,
            map_metadata: None,
        },
    );
    let stage = f.session.staging.join("framework");
    write(&stage, &dll, b"new framework");
    write(&stage, &sample, b"new bundled sample");
    write(
        &stage,
        &replaced_sample,
        b"old bundled sample must not overwrite imported map",
    );
    let node = f
        .session
        .solved
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| node.id == "framework")
        .unwrap();
    let mut next = node.current.clone().unwrap();
    next.id = "verified:framework-reinstall".into();
    next.provides[0].member_key = Some(key(&dll));
    node.current = Some(next.clone());
    f.session.payloads.insert(
        "framework".into(),
        super::super::super::session::Payload {
            candidate: next,
            notices: Vec::new(),
            member_roles: Default::default(),
            stage_fingerprint: Fingerprint::capture(&stage).unwrap(),
            stage_root: stage,
            release_files: None,
            downloaded: false,
            files: vec![dll.clone(), sample, replaced_sample],
            manifest: Some(archive::new_manifest(
                "framework",
                "0.10",
                BTreeMap::new(),
                BTreeMap::new(),
                vec![dll],
                vec![],
            )),
        },
    );
    let changes = super::super::super::mutations::assemble(&f.session, f.roots.clone()).unwrap();
    let tree_targets = changes
        .iter()
        .filter_map(|change| match &change.mutation {
            Mutation::Tree(tree) => Some(key(&tree.target.relative)),
            _ => None,
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        tree_targets,
        BTreeSet::from([
            key(&format!("{package}/levels/KeepDirectory")),
            key(&format!("{package}/levels/BundledExample")),
        ])
    );
    super::super::super::transaction::prepare(
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
                .join(format!("{package}/levels/KeepDirectory/new-level"))
        )
        .unwrap(),
        b"new-level"
    );
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join(format!("{package}/levels/BundledExample/info"))
        )
        .unwrap(),
        b"new bundled sample"
    );
    assert!(
        !f.roots
            .game_root
            .join(format!("{package}/levels/KeepDirectory/old-level"))
            .exists()
    );
}
