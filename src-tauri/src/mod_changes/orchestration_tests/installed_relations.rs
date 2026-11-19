//! Real DLL scans must preserve the same bundled-member declarations as staging.
use super::*;
use std::io::Write;

fn package(secondary_enabled: bool) -> Fixture {
    let mut primary = repository(
        "a",
        "fixture-a",
        &["1.0.0", "2.0.0"],
        vec![requires("fixture-d", ">=99.0.0")],
    );
    primary.release_relations = vec![ReleaseRelationOverride {
        version: "<3.0.0".into(),
        relations: vec![requires("fixture-d", ">=1.0.0")],
    }];
    primary.asset_rules[0].value = "a.zip".into();
    for release in &mut primary.cached_releases {
        release.assets[0].name = "a.zip".into();
    }
    let mut secondary = repository(
        "b",
        "fixture-b",
        &[],
        vec![requires("fixture-c", ">=99.0.0")],
    );
    secondary.release_relations = vec![ReleaseRelationOverride {
        version: "<2.0.0".into(),
        relations: vec![requires("fixture-c", ">=1.0.0")],
    }];
    let f = fixture(vec![
        primary,
        secondary,
        repository("c", "fixture-c", &["1.0.0"], vec![]),
        repository("d", "fixture-d", &["1.0.0"], vec![]),
    ]);
    let a = install(&f, "a", "fixture-a", "1.0.0", true);
    install(&f, "c", "fixture-c", "1.0.0", secondary_enabled);
    install(&f, "d", "fixture-d", "1.0.0", true);
    let b = "BepInEx/plugins/a/b.dll";
    let physical_b = if secondary_enabled {
        b.into()
    } else {
        mods::disabled_dll_file(b)
    };
    write(
        &f.game.join(&physical_b),
        &mods::plugin_blob("fixture-b", "B", "1.5.0"),
    );
    let mut manifest = archive::read_manifests(&f.state.config.manifests_dir())
        .into_iter()
        .find(|manifest| manifest.repository_id == "a")
        .unwrap();
    let config = f.state.config.load_or_default();
    manifest.files = vec![a, physical_b];
    manifest.relations = mods::effective_relations(&config.repositories[0], Some("v1.0.0"));
    archive::write_manifest(&f.state.config.manifests_dir(), &manifest).unwrap();
    f
}

fn assert_relations(inventory: &Inventory, secondary_enabled: bool) {
    let node = inventory
        .nodes
        .iter()
        .find(|node| node.id == "mod:a")
        .unwrap();
    let candidate = node.current.as_ref().unwrap();
    assert!(candidate.relations_known);
    let primary = candidate
        .relations
        .iter()
        .find(|owned| owned.relation.guid == "fixture-d")
        .unwrap();
    assert_eq!(primary.relation.version, ">=1.0.0");
    assert_eq!(primary.owner_guid.as_deref(), Some("fixture-a"));
    let secondary = candidate
        .relations
        .iter()
        .filter(|owned| owned.relation.guid == "fixture-c")
        .collect::<Vec<_>>();
    assert_eq!(secondary.len(), 1);
    assert_eq!(secondary[0].relation.version, ">=1.0.0");
    assert_eq!(secondary[0].owner_guid.as_deref(), Some("fixture-b"));
    assert_eq!(
        secondary[0].owner_member_key.as_deref(),
        Some(inventory::key("BepInEx/plugins/a/b.dll").as_str())
    );
    let evaluation = super::super::evaluate(inventory)
        .into_iter()
        .find(|entry| entry.node_id == node.id)
        .unwrap();
    let runtime = evaluation
        .runtime
        .iter()
        .filter(|entry| entry.relation.guid == "fixture-c")
        .collect::<Vec<_>>();
    assert_eq!(runtime.len(), usize::from(secondary_enabled));
    if secondary_enabled {
        assert_eq!(runtime[0].state, EvaluationState::Satisfied);
    }
    let readiness = evaluation
        .readiness
        .iter()
        .find(|entry| entry.relation.guid == "fixture-c")
        .unwrap();
    assert_eq!(
        readiness.state,
        if secondary_enabled {
            EvaluationState::Satisfied
        } else {
            EvaluationState::Disabled
        }
    );
    let target = inventory
        .nodes
        .iter()
        .find(|node| node.id == "mod:c")
        .unwrap();
    assert_eq!(target.enabled, secondary_enabled);
}

#[test]
fn installed_secondary_declarations_survive_actual_prepare_commit_and_rescan() {
    for secondary_enabled in [true, false] {
        let f = package(secondary_enabled);
        let before = inventory::build(&f.state).unwrap();
        assert_relations(&before.inventory, secondary_enabled);
        let plan = planning::create(&f.state, request("a", Some("v2.0.0"), true)).unwrap();
        assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
        let mut session = take(&f, &plan);
        let candidate = session
            .solved
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find(|node| node.id == "mod:a")
            .unwrap()
            .current
            .clone()
            .unwrap();
        let preview_id = uuid::Uuid::new_v4().to_string();
        let directory = crate::mod_preview::staging::preview_root(&f.state).join(&preview_id);
        fs::create_dir_all(&directory).unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(directory.join("a.zip")).unwrap());
        for (file, guid, version) in [
            ("a.dll", "fixture-a", "9.0.0"),
            ("b.dll", "fixture-b", "1.5.0"),
        ] {
            zip.start_file(file, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&mods::plugin_blob(guid, file, version))
                .unwrap();
        }
        zip.finish().unwrap();
        write(
            &directory.join(crate::mod_preview::staging::ORIGIN_FILE),
            &serde_json::to_vec(&crate::mod_preview::staging::PreviewOriginRecord {
                repository_id: "a".into(),
                release_tag: "v2.0.0".into(),
                prerelease: false,
            })
            .unwrap(),
        );
        let payload = super::super::payload::from_preview(
            &f.state,
            &session.trusted,
            &preview_id,
            "mod:a",
            &candidate,
            None,
            &session.staging.join("verified-a"),
        )
        .unwrap();
        let node = session
            .trusted
            .inventory
            .nodes
            .iter_mut()
            .find(|node| node.id == "mod:a")
            .unwrap();
        *node
            .candidates
            .iter_mut()
            .find(|entry| entry.id == candidate.id)
            .unwrap() = payload.candidate.clone();
        session.payloads.insert("mod:a".into(), payload);
        planning::resolve(&f.state, &mut session).unwrap();
        assert_eq!(
            session.plan.result.status,
            SolveStatus::Ready,
            "{:?}",
            session.plan.result
        );
        assert_relations(session.solved.as_ref().unwrap(), secondary_enabled);
        preparation::finish(&f.state, &mut session).unwrap();
        assert_relations(
            &inventory::build(&f.state).unwrap().inventory,
            secondary_enabled,
        );
        session.plan.prepared = true;
        super::super::execution::commit(&f.state, &mut session).unwrap();
        let actual = inventory::build(&f.state).unwrap();
        assert_relations(&actual.inventory, secondary_enabled);
        let current = actual
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == "mod:a")
            .unwrap()
            .current
            .as_ref()
            .unwrap();
        assert_eq!(current.tag.as_deref(), Some("v2.0.0"));
        assert_eq!(
            current
                .provides
                .iter()
                .find(|member| member.guid == "fixture-a")
                .unwrap()
                .version
                .as_deref(),
            Some("9.0.0")
        );
    }
}

#[test]
fn contradictory_installed_secondary_sources_remain_unknown_instead_of_choosing_one() {
    let f = package(true);
    let mut config = f.state.config.load_or_default();
    config.repositories.push(repository(
        "b-other",
        "fixture-b",
        &[],
        vec![requires("fixture-c", ">=2.0.0")],
    ));
    f.state.config.save(&config).unwrap();
    let actual = inventory::build(&f.state).unwrap();
    let current = actual
        .inventory
        .nodes
        .iter()
        .find(|node| node.id == "mod:a")
        .unwrap()
        .current
        .as_ref()
        .unwrap();
    assert!(!current.relations_known);
    assert!(
        !current
            .relations
            .iter()
            .any(|owned| owned.owner_guid.as_deref() == Some("fixture-b"))
    );
    let plan = planning::create(
        &f.state,
        PlanRequest {
            roots: vec![RootRequest {
                intent: RootIntent {
                    node_id: "mod:a".into(),
                    candidate_id: Some(current.id.clone()),
                    enabled: true,
                    member_key: None,
                },
                asset_name: None,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        plan.result.status,
        SolveStatus::Incomplete,
        "{:?}",
        plan.result
    );
}
