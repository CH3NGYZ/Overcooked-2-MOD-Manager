use super::*;
use crate::archive::release_download::tests::{Server, reuse_tests::served};
use std::io::{Cursor, Write};

const FRAMEWORK_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const SPLITTER_GUID: &str = "dev.gua.overcooked.diylevel.ingredientsplittermod";

#[tokio::test]
async fn local_diylevel_zip_reports_upgrade_using_the_main_plugin_version() {
    let fixture = fixture(vec![]);
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, guid, name, version) in [
        (
            "OC2DIYLevel/modules/IngredientSplitter/IngredientSplitterMod.dll",
            SPLITTER_GUID,
            "IngredientSplitterMod",
            "0.1.0",
        ),
        (
            "OC2DIYLevel/OC2DIYLevel.dll",
            FRAMEWORK_GUID,
            "DIYLevel",
            "0.11.1",
        ),
    ] {
        zip.start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&mods::plugin_blob(guid, name, version))
            .unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    let old = fixture
        .game
        .join("BepInEx/plugins/OC2DIYLevel/OC2DIYLevel.dll");
    let previous = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.10.0");
    write(&old, &previous);
    let preview = stage_preview(&fixture, "arbitrary-name.zip", &bytes);
    let plan = planning::create(&fixture.state, manual_request(preview.clone())).unwrap();
    assert_framework_upgrade(&plan);
    let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
    assert_eq!(outcome.error, None);
    let prepared = outcome.plan.unwrap();
    assert_framework_upgrade(&prepared);
    assert_eq!(fs::read(&old).unwrap(), previous);
    let mut session = take(&fixture, &prepared);
    let candidate = &session.payloads[&preview].candidate;
    assert_eq!(candidate.version.as_deref(), Some("0.11.1"));
    assert_eq!(
        candidate
            .provides
            .iter()
            .find(|member| member.guid == SPLITTER_GUID)
            .and_then(|member| member.version.as_deref()),
        Some("0.1.0")
    );
    super::super::execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
    let splitter = fixture
        .game
        .join("BepInEx/plugins/OC2DIYLevel/modules/IngredientSplitter/IngredientSplitterMod.dll");
    fs::rename(&splitter, splitter.with_extension("dll.disabled")).unwrap();
    let scanned = inventory::build(&fixture.state).unwrap();
    let installed = scanned
        .inventory
        .nodes
        .iter()
        .filter_map(|node| node.current.as_ref())
        .find(|candidate| {
            candidate
                .provides
                .iter()
                .any(|member| member.guid == FRAMEWORK_GUID)
        })
        .unwrap();
    assert!(installed.source_id.is_none() && installed.tag.is_none());
    assert_eq!(installed.version.as_deref(), Some("0.11.1"));
    assert!(installed.provides.iter().any(|member| {
        member.guid == SPLITTER_GUID
            && member.version.as_deref() == Some("0.1.0")
            && !member.enabled
    }));
}

fn assert_framework_upgrade(plan: &ChangePlan) {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let [action] = plan.result.actions.as_slice() else {
        panic!("expected one package action: {:?}", plan.result.actions);
    };
    assert!(action.node_id.starts_with("preview:"));
    assert_eq!(
        (
            &action.kind,
            action.from_tag.as_deref(),
            action.to_tag.as_deref()
        ),
        (&ActionKind::Upgrade, Some("0.10.0"), Some("0.11.1"))
    );
}

#[tokio::test]
#[ignore = "requires OC2_LOCAL_DIYLEVEL_ZIP pointing to DIYLevel v0.11.1"]
async fn real_local_diylevel_zip_installs_mod_and_preserves_the_existing_map_tree() {
    let zip = std::env::var_os("OC2_LOCAL_DIYLEVEL_ZIP")
        .map(std::path::PathBuf::from)
        .expect("set OC2_LOCAL_DIYLEVEL_ZIP");
    let contents = crate::level_sets::classify_zip(&zip).unwrap();
    assert!(contents.mod_bundled_maps);
    assert_eq!(contents.map_sets, 2);
    assert_eq!(contents.plugins, 4);
    assert_eq!(
        crate::level_sets::require_map_only_zip(&zip).unwrap_err(),
        "levelSet.zipContainsModAndMaps"
    );

    let bytes = fs::read(&zip).unwrap();
    let asset = zip.file_name().unwrap().to_str().unwrap();
    for (online, keep_old) in [(false, false), (false, true), (true, true)] {
        let server = online.then(|| Server::new(vec![bytes.clone()], false));
        let repositories = server.as_ref().map_or_else(Vec::new, |server| {
            let mut source = repository(
                "FixtureDIYLevel",
                mods::plugin_target::DIYLEVEL_GUID,
                &["0.11.1"],
                vec![],
            );
            source.asset_rules[0].value = asset.into();
            source.install_target = "BepInEx/plugins".into();
            source.cached_releases[0].assets = vec![served(server, 0, asset, &bytes)];
            vec![source]
        });
        let fixture = fixture(repositories);
        let package = fixture.game.join("BepInEx/plugins/OC2DIYLevel");
        let core = package.join("OC2DIYLevel.dll");
        let story = package.join("levels/oc1_story");
        let test_level = package.join("levels/test_level");
        let original_marker = story.join("local-marker");
        if keep_old {
            write(&original_marker, b"user content");
        }
        let old_core = (!online && keep_old).then(|| {
            let bytes = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.10.0");
            write(&core, &bytes);
            bytes
        });
        let assert_uncommitted = || match &old_core {
            Some(bytes) => assert_eq!(fs::read(&core).unwrap(), *bytes),
            None => assert!(!core.exists()),
        };
        let request = if online {
            write(
                &fixture.game.join("BepInEx/core/BepInEx.dll"),
                include_bytes!("duplicate_release_target/fastinit_version.pe"),
            );
            let mut selected = request("FixtureDIYLevel", Some("v0.11.1"), true);
            selected.roots[0].asset_name = Some(asset.into());
            selected
        } else {
            manual_request(stage_preview(&fixture, asset, &bytes))
        };
        let plan = planning::create(&fixture.state, request).unwrap();
        if old_core.is_some() {
            assert_framework_upgrade(&plan);
        }
        assert_uncommitted();
        assert!(!test_level.exists());
        let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
        assert_eq!(outcome.error, None);
        let prepared = outcome.plan.unwrap();
        if old_core.is_some() {
            assert_framework_upgrade(&prepared);
        }
        let mut session = take(&fixture, &prepared);
        assert!(session.plan.prepared);
        assert_eq!(
            session.dependency_policy,
            super::super::solve::DependencyPolicy::PreferNewest,
        );
        let payload = session.payloads.values().next().unwrap();
        assert_eq!(payload.candidate.version.as_deref(), Some("0.11.1"));
        assert_eq!(payload.candidate.provides.len(), 4);
        assert!(payload.candidate.provides.iter().any(|member| {
            member.guid == mods::plugin_target::DIYLEVEL_GUID
                && member.version.as_deref() == Some("0.11.1")
        }));
        assert_uncommitted();
        super::super::execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
        assert_eq!(
            mods::read_dll_details(&core, "OC2DIYLevel.dll")
                .version
                .as_deref(),
            Some("0.11.1")
        );
        assert!(package.join("LevelEditorStub.dll").is_file());
        assert!(package.join("common04").is_file());
        assert!(
            package
                .join("modules/IngredientSplitter/IngredientSplitterMod.dll")
                .is_file()
        );
        assert!(test_level.join("info_test_level").is_file());
        assert!(test_level.join("s_test_level_5").is_file());
        if keep_old {
            assert_eq!(fs::read(original_marker).unwrap(), b"user content");
            assert!(!story.join("info_oc1_story").exists());
        } else {
            assert!(story.join("info_oc1_story").is_file());
            assert!(story.join("s_oc1_story_5_5").is_file());
        }
        let manifests = archive::read_manifests(&fixture.state.config.manifests_dir());
        let manifest = manifests
            .iter()
            .find(|manifest| {
                manifest
                    .files
                    .iter()
                    .any(|path| path.ends_with("/OC2DIYLevel.dll"))
            })
            .unwrap();
        assert!(manifest.files.iter().all(|path| !path.contains("/levels/")));
        assert_eq!(manifest.repository_binding.is_some(), online);
        if let Some(server) = server {
            assert_eq!(server.request_count(), 1);
        }
        assert_eq!(fs::metadata(&zip).unwrap().len(), bytes.len() as u64);
    }
}

fn stage_preview(fixture: &Fixture, asset: &str, bytes: &[u8]) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let preview = crate::mod_preview::staging::preview_root(&fixture.state)
        .join(&id)
        .join(asset);
    write(&preview, bytes);
    format!("preview:{id}")
}

fn manual_request(node_id: String) -> PlanRequest {
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id,
                candidate_id: None,
                enabled: true,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    }
}

const MANUAL_GUID: &str = "example.manual-mod";
const DEPENDENCY_GUID: &str = "example.shared-dependency";

fn stage_local(fixture: &Fixture, version: &str) -> PlanRequest {
    manual_request(stage_preview(
        fixture,
        "FixtureMod.dll",
        &mods::plugin_blob(MANUAL_GUID, "FixtureMod", version),
    ))
}

#[test]
fn manual_mod_selects_same_version_declarations_and_honors_the_recorded_channel() {
    let mut same = repository(
        "SameVersion",
        MANUAL_GUID,
        &["1.8.7.23"],
        vec![requires(DEPENDENCY_GUID, ">=1.0")],
    );
    same.channel = Channel::Stable;
    let lagging = repository(
        "LaggingSource",
        MANUAL_GUID,
        &["1.8.6"],
        vec![requires("example.unavailable-mod", ">=9.0.0")],
    );
    let fixture = fixture(vec![same, lagging]);
    let plan = planning::create(&fixture.state, stage_local(&fixture, "1.8.7.23")).unwrap();
    assert_eq!(
        plan.result.status,
        SolveStatus::Unsatisfiable,
        "{:?}",
        plan.result
    );
    assert_eq!(
        plan.result.blockers[0].target_guid.as_deref(),
        Some(DEPENDENCY_GUID)
    );
    assert!(plan.result.notices.is_empty());
    let session = take(&fixture, &plan);
    let payload = session.payloads.values().next().unwrap();
    assert!(payload.candidate.source_id.is_none());
    assert!(payload.candidate.provides[0].prerelease.is_none());
}

#[tokio::test]
async fn manual_mod_uses_common_requirements_and_preserves_its_provenance_on_rescan() {
    let shared = requires(DEPENDENCY_GUID, ">=1.0");
    let first = repository(
        "FixtureMod",
        MANUAL_GUID,
        &["1.0.0", "2.0.0"],
        vec![shared.clone(), requires("example.first-only", "")],
    );
    let mut second = repository("OtherSource", MANUAL_GUID, &["1.0.0"], vec![]);
    second.release_relations = vec![ReleaseRelationOverride {
        version: ">=2.0".into(),
        relations: vec![shared.clone(), requires("example.second-only", "")],
    }];
    let dependency = repository("Dependency", DEPENDENCY_GUID, &["1.0.0"], vec![]);
    let fixture = fixture(vec![first, second, dependency]);
    let old = fixture
        .game
        .join(install(&fixture, "FixtureMod", MANUAL_GUID, "1.0.0", true));
    let plan = planning::create(&fixture.state, stage_local(&fixture, "2.0.0")).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let notice = plan
        .result
        .notices
        .first()
        .expect("source disagreement must be visible")
        .clone();
    assert!(notice.starts_with("modChanges.localModSourceConflict?"));
    assert!(notice.contains("FixtureMod") && notice.contains("OtherSource"));
    let dependency_dll = fixture
        .game
        .join("BepInEx/plugins/Dependency/Dependency.dll");
    assert!(
        plan.result
            .actions
            .iter()
            .any(|action| action.node_id == "mod:Dependency")
    );
    assert!(!dependency_dll.exists());

    take(&fixture, &plan);
    install(&fixture, "Dependency", DEPENDENCY_GUID, "1.0.0", true);
    let plan = planning::create(&fixture.state, stage_local(&fixture, "2.0.0")).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
    assert_eq!(outcome.error, None);
    let prepared = outcome.plan.unwrap();
    assert_eq!(prepared.result.notices.len(), 1);
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    let mut session = take(&fixture, &prepared);
    super::super::execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
    assert!(!old.exists());
    let manifest =
        archive::read_manifest(&fixture.state.config.manifests_dir(), "FixtureMod").unwrap();
    assert!(manifest.is_manual());
    assert!(manifest.repository_binding.is_none());

    let scanned = inventory::build(&fixture.state).unwrap();
    let local = scanned
        .inventory
        .nodes
        .iter()
        .find(|node| {
            node.current.as_ref().is_some_and(|candidate| {
                candidate
                    .provides
                    .iter()
                    .any(|member| member.guid == MANUAL_GUID)
            })
        })
        .unwrap();
    let current = local.current.as_ref().unwrap();
    assert!(!current.relations_known);
    assert_eq!(current.relations.len(), 1);
    assert_eq!(current.relations[0].relation, shared);
    let toggle = |enabled| PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: local.id.clone(),
                candidate_id: None,
                enabled,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    };
    for enabled in [false, true] {
        let plan = planning::create(&fixture.state, toggle(enabled)).unwrap();
        assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
        assert_eq!(plan.result.notices.len(), usize::from(enabled));
        let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
        assert_eq!(outcome.error, None);
        super::super::execution::commit(
            &fixture.state,
            &mut take(&fixture, &outcome.plan.unwrap()),
            |_| {},
        )
        .unwrap();
        assert_eq!(
            fixture
                .game
                .join("BepInEx/plugins/FixtureMod.dll")
                .is_file(),
            enabled
        );
    }
}

#[test]
fn manual_mod_still_blocks_when_two_physical_install_targets_match() {
    let fixture = fixture(vec![]);
    for path in ["CopyA/A.dll", "CopyB/B.dll"] {
        write(
            &fixture.game.join("BepInEx/plugins").join(path),
            &mods::plugin_blob(MANUAL_GUID, "FixtureMod", "1.0.0"),
        );
    }
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let error = planning::create(&fixture.state, stage_local(&fixture, "2.0.0")).unwrap_err();
    assert!(
        error.starts_with("modChanges.localModMultipleTargets?"),
        "{error}"
    );
    assert!(error.to_ascii_lowercase().contains("copya"));
    assert!(error.to_ascii_lowercase().contains("copyb"));
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
}
