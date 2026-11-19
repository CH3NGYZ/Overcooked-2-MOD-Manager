//! Exercise the production plan, actual-DLL verification, frozen mutations and
//! commit boundary together. Every path lives in a fresh temporary game/data root.
use super::{inventory, model::*, planning, preparation, session::*, transaction::*};
use crate::{app_state::AppState, archive, config::ConfigService, models::*, mods};
use std::{collections::BTreeMap, fs, path::Path};

#[path = "orchestration_tests/disable_fallback.rs"]
mod disable_fallback;
#[path = "orchestration_tests/install_notes.rs"]
mod install_notes;
#[path = "orchestration_tests/installed_relations.rs"]
mod installed_relations;
#[path = "orchestration_tests/map_runtime_duplicate.rs"]
mod map_runtime_duplicate;
#[path = "orchestration_tests/retired_members.rs"]
mod retired_members;
#[path = "orchestration_tests/toggle_scope.rs"]
mod toggle_scope;
#[path = "orchestration_tests/unbound_toggle.rs"]
mod unbound_toggle;

struct Fixture {
    _temp: tempfile::TempDir,
    state: AppState,
    game: std::path::PathBuf,
}

fn fixture(repositories: Vec<ModRepositoryConfig>) -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(temp.path()).unwrap();
    let game = root.join("game");
    fs::create_dir_all(game.join("BepInEx/plugins")).unwrap();
    write(&game.join("BepInEx/core/BepInEx.dll"), b"fixture loader");
    let state = AppState::with_config(ConfigService::for_test(root.join("data")));
    let mut config = crate::config::default_config();
    config.game.path = game.to_string_lossy().into_owned();
    config.repositories = repositories;
    state.config.save(&config).unwrap();
    Fixture {
        _temp: temp,
        state,
        game,
    }
}

fn requires(guid: &str, version: &str) -> RepositoryRelation {
    RepositoryRelation {
        kind: RelationKind::Requires,
        guid: guid.into(),
        version: version.into(),
        prerelease: None,
        optional: false,
    }
}

fn repository(
    name: &str,
    guid: &str,
    versions: &[&str],
    relations: Vec<RepositoryRelation>,
) -> ModRepositoryConfig {
    let mut repo =
        crate::community::repository_from_coordinates(&Provider::Github, "Fixture", name, "", "")
            .unwrap();
    repo.id = name.into();
    repo.name = name.into();
    repo.plugin_guid = guid.into();
    repo.source_id = None;
    repo.install_target = format!("BepInEx/plugins/{name}");
    repo.relations = relations;
    repo.releases_cached_at = Some(1);
    repo.cached_releases_complete = true;
    repo.asset_rules = vec![AssetRule {
        rule_type: AssetRuleType::Exact,
        value: format!("{name}.dll"),
    }];
    repo.cached_releases = versions
        .iter()
        .map(|version| ReleaseInfo {
            release_id: None,
            tag_name: format!("v{version}"),
            version: Some((*version).into()),
            name: name.into(),
            body: String::new(),
            published_at: None,
            api_prerelease: Some(false),
            prerelease: false,
            draft: false,
            assets: vec![ReleaseAsset {
                name: format!("{name}.dll"),
                size: 0,
                download_count: None,
                download_url: format!("https://example.invalid/{name}/{version}.dll"),
                digest: None,
            }],
        })
        .collect();
    repo
}

fn install(f: &Fixture, name: &str, guid: &str, version: &str, enabled: bool) -> String {
    let file = format!("BepInEx/plugins/{name}/{name}.dll");
    let physical = if enabled {
        file.clone()
    } else {
        mods::disabled_dll_file(&file)
    };
    write(
        &f.game.join(&physical),
        &mods::plugin_blob(guid, name, version),
    );
    let config = f.state.config.load_or_default();
    let repo = config
        .repositories
        .iter()
        .find(|repo| repo.id == name)
        .unwrap();
    let mut manifest = archive::new_manifest(
        name,
        &format!("v{version}"),
        BTreeMap::new(),
        BTreeMap::new(),
        vec![physical],
        repo.relations.clone(),
    );
    manifest.repository_binding = ModRepositoryBinding::from_release(repo, &format!("v{version}"));
    archive::write_manifest(&f.state.config.manifests_dir(), &manifest).unwrap();
    file
}

fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

fn request(id: &str, tag: Option<&str>, enabled: bool) -> PlanRequest {
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: format!("mod:{id}"),
                candidate_id: tag.map(|tag| format!("release:{id}:{tag}")),
                enabled,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    }
}

fn take(f: &Fixture, plan: &ChangePlan) -> PlanSession {
    f.state
        .mod_changes
        .lock()
        .unwrap()
        .plans
        .remove(&plan.plan_id)
        .unwrap()
}

/// Use the same staged preview format as the existing downloader, with real PE
/// metadata and the selected exact source/tag. No HTTP request is needed here.
fn verify_selected(f: &Fixture, session: &mut PlanSession) {
    for _ in 0..16 {
        planning::resolve(&f.state, session).unwrap();
        assert_eq!(
            session.plan.result.status,
            SolveStatus::Ready,
            "{:?}",
            session.plan.result
        );
        let selected = session
            .solved
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .find_map(|node| {
                node.current
                    .as_ref()
                    .filter(|candidate| candidate.verification_pending)
                    .map(|candidate| (node.id.clone(), candidate.clone()))
            });
        let Some((node_id, candidate)) = selected else {
            return;
        };
        let repo = session
            .trusted
            .config
            .repositories
            .iter()
            .find(|repo| Some(&repo.id) == candidate.source_id.as_ref())
            .unwrap();
        let preview_id = uuid::Uuid::new_v4().to_string();
        let directory = crate::mod_preview::staging::preview_root(&f.state).join(&preview_id);
        write(
            &directory.join(format!("{}.dll", repo.id)),
            &mods::plugin_blob(
                &repo.plugin_guid,
                &repo.name,
                candidate.tag.as_deref().unwrap().trim_start_matches('v'),
            ),
        );
        write(
            &directory.join(crate::mod_preview::staging::ORIGIN_FILE),
            &serde_json::to_vec(&crate::mod_preview::staging::PreviewOriginRecord {
                repository_id: repo.id.clone(),
                release_tag: candidate.tag.clone().unwrap(),
                prerelease: false,
            })
            .unwrap(),
        );
        let payload = super::payload::from_preview(
            &f.state,
            &session.trusted,
            &preview_id,
            &node_id,
            &candidate,
            None,
            &session.staging.join(format!("verified-{preview_id}")),
        )
        .unwrap();
        let node = session
            .trusted
            .inventory
            .nodes
            .iter_mut()
            .find(|node| node.id == node_id)
            .unwrap();
        *node
            .candidates
            .iter_mut()
            .find(|entry| entry.id == candidate.id)
            .unwrap() = payload.candidate.clone();
        session.payloads.insert(node_id, payload);
    }
    panic!("fixture exceeded asset verification bound");
}

#[test]
fn actual_chain_downgrades_framework_and_reverse_web_atomically() {
    let fw = mods::plugin_target::DIYLEVEL_GUID;
    let mut web = repository(
        "web",
        "fixture-web",
        &["4.0.0", "3.10.2"],
        vec![requires(fw, ">=0.11.0")],
    );
    web.release_relations = vec![ReleaseRelationOverride {
        version: "<4.0.0".into(),
        relations: vec![requires(fw, ">=0.10.0")],
    }];
    let f = fixture(vec![
        repository("framework", fw, &["0.11.0", "0.10.0"], vec![]),
        web,
        repository(
            "fix",
            "fixture-fix",
            &["0.4.0"],
            vec![requires(fw, "==0.10.0")],
        ),
    ]);
    let fw_path = install(&f, "framework", fw, "0.11.0", true);
    let web_path = install(&f, "web", "fixture-web", "4.0.0", true);
    let before_fw = fs::read(f.game.join(&fw_path)).unwrap();
    let before_web = fs::read(f.game.join(&web_path)).unwrap();
    let plan = planning::create(&f.state, request("fix", Some("v0.4.0"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(
        plan.result
            .actions
            .iter()
            .any(|action| action.node_id == "mod:web" && action.kind == ActionKind::Downgrade)
    );
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(fs::read(f.game.join(&fw_path)).unwrap(), before_fw);
    assert_eq!(fs::read(f.game.join(&web_path)).unwrap(), before_web);
    assert!(!f.game.join("BepInEx/plugins/fix/fix.dll").exists());
    session.plan.prepared = true;
    super::execution::commit(&f.state, &mut session).unwrap();
    let actual = inventory::build(&f.state).unwrap();
    for (name, version) in [("framework", "0.10.0"), ("web", "3.10.2"), ("fix", "0.4.0")] {
        let node = actual
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == format!("mod:{name}"))
            .unwrap();
        assert_eq!(
            node.current.as_ref().unwrap().version.as_deref(),
            Some(version)
        );
        assert!(node.enabled);
    }
    assert!(!session.plan.prepared);
    assert!(
        !has_pending(
            &f.state
                .config
                .config_path()
                .parent()
                .unwrap()
                .join("mod-change-transactions")
        )
        .unwrap()
    );
}

#[test]
fn changed_target_blocks_prepared_batch_without_touching_other_files() {
    let f = fixture(vec![
        repository("one", "fixture-one", &["1.0.0"], vec![]),
        repository("two", "fixture-two", &["1.0.0"], vec![]),
    ]);
    let one = install(&f, "one", "fixture-one", "1.0.0", true);
    let two = install(&f, "two", "fixture-two", "1.0.0", true);
    let mut req = request("one", None, false);
    req.roots.extend(request("two", None, false).roots);
    let plan = planning::create(&f.state, req).unwrap();
    let mut session = take(&f, &plan);
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;
    fs::write(f.game.join(&two), b"user changed target after approval").unwrap();
    assert_eq!(
        super::execution::commit(&f.state, &mut session).unwrap_err(),
        "modChanges.targetChanged"
    );
    assert!(f.game.join(&one).is_file());
    assert!(f.game.join(&two).is_file());
    assert!(!f.game.join(mods::disabled_dll_file(&one)).exists());
}

#[test]
fn manual_replacement_inherits_disabled_member_and_preserves_original_preview() {
    let f = fixture(vec![repository(
        "manual",
        "fixture-manual",
        &["1.0.0"],
        vec![],
    )]);
    let file = install(&f, "manual", "fixture-manual", "1.0.0", false);
    let preview_id = uuid::Uuid::new_v4().to_string();
    let source = crate::mod_preview::staging::preview_root(&f.state)
        .join(&preview_id)
        .join("manual.dll");
    write(
        &source,
        &mods::plugin_blob("fixture-manual", "Manual", "2.0.0"),
    );
    let plan = planning::create(
        &f.state,
        PlanRequest {
            roots: vec![RootRequest {
                intent: RootIntent {
                    node_id: format!("preview:{preview_id}"),
                    candidate_id: None,
                    enabled: true,
                    member_key: None,
                },
                asset_name: None,
            }],
            ..Default::default()
        },
    )
    .unwrap();
    let session = take(&f, &plan);
    let payload = session.payloads.values().next().unwrap();
    assert!(!payload.candidate.provides[0].enabled);
    assert_eq!(payload.manifest.as_ref().unwrap().repository_id, "manual");
    assert!(f.game.join(mods::disabled_dll_file(&file)).is_file());
    assert!(source.is_file());
    super::execution::cleanup(&f.state, &session.staging).unwrap();
    assert!(source.is_file());
}

#[test]
fn staged_bytes_and_configuration_are_bound_to_plan() {
    let f = fixture(vec![repository("one", "fixture-one", &["1.0.0"], vec![])]);
    let plan = planning::create(&f.state, request("one", Some("v1.0.0"), true)).unwrap();
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    planning::remember_payloads(&mut session).unwrap();
    let payload = session.payloads.values().next().unwrap();
    fs::write(
        payload.stage_root.join(&payload.files[0]),
        b"tampered asset",
    )
    .unwrap();
    assert_eq!(
        preparation::finish(&f.state, &mut session).unwrap_err(),
        "modChanges.payloadChanged"
    );
    assert!(!f.game.join("BepInEx/plugins/one/one.dll").exists());
    let mut config = f.state.config.load_or_default();
    config.ui_zoom += 0.25;
    f.state.config.save(&config).unwrap();
    assert_eq!(
        preparation::ensure_fresh(&f.state, &session.trusted.revision).unwrap_err(),
        "modChanges.targetChanged"
    );
}

#[test]
fn online_subject_preserves_exact_candidate_for_shared_repair() {
    let f = fixture(vec![repository(
        "one",
        "fixture-one",
        &["2.0.0", "1.0.0"],
        vec![],
    )]);
    let snapshot = planning::evaluate(
        &f.state,
        vec![planning::EvaluationSubject {
            repository_id: Some("one".into()),
            tag: Some("v1.0.0".into()),
            ..Default::default()
        }],
    )
    .unwrap();
    let subject = snapshot
        .nodes
        .iter()
        .find(|node| node.subject_tag.as_deref() == Some("v1.0.0"))
        .unwrap();
    assert_eq!(subject.candidates[0].id, "release:one:v1.0.0");
    assert!(subject.members.is_empty());
}

#[test]
fn expired_session_removes_only_private_stage_and_busy_creation_is_rejected() {
    let f = fixture(vec![repository("one", "fixture-one", &["1.0.0"], vec![])]);
    install(&f, "one", "fixture-one", "1.0.0", true);
    let plan = planning::create(&f.state, request("one", None, false)).unwrap();
    let stage = {
        let mut sessions = f.state.mod_changes.lock().unwrap();
        let s = sessions.plans.get_mut(&plan.plan_id).unwrap();
        s.created = std::time::Instant::now() - std::time::Duration::from_secs(3601);
        s.staging.clone()
    };
    assert!(SessionLease::take(&f.state, &plan.plan_id).is_err());
    assert!(!stage.exists());
    let lease = SessionLease::creating(&f.state).unwrap();
    assert!(planning::create(&f.state, request("one", None, false)).is_err());
    drop(lease);
    assert!(f.game.join("BepInEx/plugins/one/one.dll").is_file());
}

#[test]
fn evaluation_recovers_pending_journal_before_reading_cached_inventory() {
    let f = fixture(vec![repository("one", "fixture-one", &["1.0.0"], vec![])]);
    let file = install(&f, "one", "fixture-one", "1.0.0", true);
    let cached = inventory::cached(&f.state).unwrap();
    let data = f.state.config.config_path().parent().unwrap().to_path_buf();
    let transaction_root = data.join("mod-change-transactions");
    let source = data.join("replacement.dll");
    write(&source, &mods::plugin_blob("fixture-one", "One", "2.0.0"));
    let transaction = prepare(
        &transaction_root,
        &TransactionRoots {
            game_root: f.game.clone(),
            data_root: data,
        },
        vec![Mutation::File(FileChange {
            target: TransactionPath {
                root: RootKind::Game,
                relative: file.clone(),
            },
            source: Some(StagedFile::capture(source).unwrap()),
            expected: Fingerprint::capture(&f.game.join(&file)).unwrap(),
            require_single_link: false,
        })],
    )
    .unwrap();
    assert!(has_pending(&transaction_root).unwrap());
    drop(transaction);
    let actual = planning::evaluate(&f.state, vec![]).unwrap();
    assert_eq!(actual.revision, cached.revision);
    assert!(!has_pending(&transaction_root).unwrap());
    assert!(f.game.join(file).is_file());
}

#[test]
fn verified_assets_survive_a_later_preparation_failure_and_remain_frozen() {
    let f = fixture(vec![repository("one", "fixture-one", &["1.0.0"], vec![])]);
    let plan = planning::create(&f.state, request("one", Some("v1.0.0"), true)).unwrap();
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    let payload = session.payloads.remove("mod:one").unwrap();
    let key = ("mod:one".into(), payload.candidate.id.clone());
    session.verified_payloads.insert(key, payload.clone());
    planning::remember_payloads(&mut session).unwrap();
    // This is the persisted state after a subsequent asset download failed.
    planning::resolve(&f.state, &mut session).unwrap();
    preparation::select_payloads(&mut session).unwrap();
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(session.payloads["mod:one"].stage_root, payload.stage_root);
    fs::write(
        payload.stage_root.join(&payload.files[0]),
        b"changed between attempts",
    )
    .unwrap();
    assert_eq!(
        preparation::verify_fingerprints(&session.payload_fingerprints).unwrap_err(),
        "modChanges.payloadChanged"
    );
    assert!(!f.game.join("BepInEx/plugins/one/one.dll").exists());
}

#[test]
fn missing_loader_blocks_online_local_prepare_and_commit_but_allows_disabling() {
    let f = fixture(vec![repository("one", "fixture-one", &["1.0.0"], vec![])]);
    let loader = f.game.join("BepInEx/core/BepInEx.dll");
    let expected = "framework.bepInExRequiredBeforeMods";
    fs::remove_file(&loader).unwrap();
    assert_eq!(
        planning::create(&f.state, request("one", Some("v1.0.0"), true))
            .err()
            .unwrap(),
        expected
    );
    let preview_id = uuid::Uuid::new_v4().to_string();
    let source = crate::mod_preview::staging::preview_root(&f.state)
        .join(&preview_id)
        .join("local.dll");
    write(
        &source,
        &mods::plugin_blob("fixture-local", "Local", "1.0.0"),
    );
    let local = PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: format!("preview:{preview_id}"),
                candidate_id: None,
                enabled: true,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    };
    assert_eq!(planning::create(&f.state, local).err().unwrap(), expected);
    assert!(source.is_file());
    write(&loader, b"fixture loader");
    let plan = planning::create(&f.state, request("one", Some("v1.0.0"), true)).unwrap();
    let mut session = take(&f, &plan);
    verify_selected(&f, &mut session);
    fs::remove_file(&loader).unwrap();
    assert_eq!(
        preparation::finish(&f.state, &mut session).unwrap_err(),
        expected
    );
    write(&loader, b"fixture loader");
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;
    fs::remove_file(&loader).unwrap();
    assert_eq!(
        super::execution::commit(&f.state, &mut session).unwrap_err(),
        expected
    );
    assert!(session.plan.prepared);
    assert!(session.mutations.is_some());
    assert!(!f.game.join("BepInEx/plugins/one/one.dll").exists());
    assert!(
        !has_pending(
            &f.state
                .config
                .config_path()
                .parent()
                .unwrap()
                .join("mod-change-transactions")
        )
        .unwrap()
    );
    install(&f, "one", "fixture-one", "1.0.0", true);
    let disable = planning::create(&f.state, request("one", None, false)).unwrap();
    assert_eq!(disable.result.status, SolveStatus::Ready);
}

#[test]
fn pending_recompression_recovery_blocks_prepared_commit_without_any_live_write() {
    let f = fixture(vec![repository("one", "fixture-one", &["1.0.0"], vec![])]);
    let dll = install(&f, "one", "fixture-one", "1.0.0", true);
    let scene = f.game.join("BepInEx/plugins/Map/levels/Fixture/scene");
    write(&scene, b"map bytes awaiting recovery");
    let backup = f.state.config.cache_dir().join("recovery/original-bundle");
    write(&backup, b"original map bytes");
    let plan = planning::create(&f.state, request("one", None, false)).unwrap();
    let mut session = take(&f, &plan);
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;
    let before = Fingerprint::capture(&f.game).unwrap();
    let manifests = Fingerprint::capture(&f.state.config.manifests_dir()).unwrap();
    let pending =
        crate::level_sets::recompress::PendingRecoveryFixture::new(scene.clone(), backup.clone());
    assert!(crate::level_sets::recompress::has_pending_recovery(&f.game));
    assert_eq!(
        super::execution::commit(&f.state, &mut session).unwrap_err(),
        "modChanges.recoveryRequired"
    );
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert_eq!(
        Fingerprint::capture(&f.state.config.manifests_dir()).unwrap(),
        manifests
    );
    assert!(session.plan.prepared);
    assert!(session.mutations.is_some());
    assert!(f.game.join(&dll).is_file());
    assert!(!f.game.join(mods::disabled_dll_file(&dll)).exists());
    let journals = f
        .state
        .config
        .config_path()
        .parent()
        .unwrap()
        .join("mod-change-transactions");
    assert!(!journals.exists());
    assert_eq!(fs::read(&scene).unwrap(), b"map bytes awaiting recovery");
    assert_eq!(fs::read(&backup).unwrap(), b"original map bytes");
    drop(pending);
    assert!(!crate::level_sets::recompress::has_pending_recovery(
        &f.game
    ));
    super::execution::commit(&f.state, &mut session).unwrap();
    assert!(!f.game.join(&dll).exists());
    assert!(f.game.join(mods::disabled_dll_file(&dll)).is_file());
    assert_eq!(fs::read(scene).unwrap(), b"map bytes awaiting recovery");
    assert_eq!(fs::read(backup).unwrap(), b"original map bytes");
}
