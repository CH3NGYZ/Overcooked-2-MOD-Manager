//! An empty installation database must not strip explicit local package identity.
use super::*;
use std::io::Write;

#[path = "declared_mod_bundle/legacy_library.rs"]
mod legacy_library;

const CORE: &str = "oc2.oc2diylevelruntimewloader";
const HELPER: &str = concat!("oc2", ".oc2diylevel.debuglog");
const CORE_DLL: &str = "BepInEx/plugins/Web/Loader.dll";
const HELPER_DLL: &str = "BepInEx/plugins/Web/debugLog.dll";
const PEER_GUID: &str = "fixture.package-consumer";
const NODE: &str = "mod:Web";

fn setup(core_enabled: bool, helper_enabled: bool, relations: Vec<RepositoryRelation>) -> Fixture {
    // Mirror the built-in Web declaration without depending on the resource pin.
    let mut web = repository("Web", CORE, &["4.0.0"], vec![]);
    web.bundled_plugin_guids = vec![HELPER.into()];
    web.asset_rules[0].value = "Web.zip".into();
    web.cached_releases[0].assets[0].name = "Web.zip".into();
    let f = fixture(vec![web, repository("Peer", PEER_GUID, &[], relations)]);
    let path = |dll: &str, enabled: bool| {
        f.game.join(if enabled {
            dll.into()
        } else {
            mods::disabled_dll_file(dll)
        })
    };
    write(
        &path(CORE_DLL, core_enabled),
        &mods::plugin_blob(CORE, "Loader", "3.10.2"),
    );
    write(
        &path(HELPER_DLL, helper_enabled),
        &mods::plugin_blob(HELPER, "Debug log", "1.0.0"),
    );
    install(&f, "Peer", PEER_GUID, "1.0.0", false);
    f
}

fn prepared(f: &Fixture, plan: &ChangePlan) -> PlanSession {
    let mut session = take(f, plan);
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;
    session
}

fn assert_atomic_action(plan: &ChangePlan, kind: ActionKind) {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let mut bundle_actions = plan
        .result
        .actions
        .iter()
        .filter(|action| action.node_id == NODE);
    let action = bundle_actions.next().unwrap();
    assert_eq!(action.kind, kind);
    assert!(action.member_key.is_none() && bundle_actions.next().is_none());
}

#[test]
fn dependency_enable_uses_the_whole_scanned_bundle_and_no_install_manifest() {
    for guid in [CORE, HELPER] {
        let f = setup(false, false, vec![requires(guid, ">=1.0.0")]);
        let scan = inventory::build(&f.state).unwrap();
        let bundle = &scan.files[NODE];
        assert_eq!(bundle.summary.dll_files.len(), 2);
        assert!(bundle.manifests.is_empty() && bundle.summary.package_id.is_none());
        assert!(scan.atomic_mod_package(NODE));
        let current = scan
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == NODE)
            .unwrap()
            .current
            .as_ref()
            .unwrap();
        assert_eq!(current.version.as_deref(), Some("3.10.2"));
        assert!(
            current
                .provides
                .iter()
                .any(|member| member.guid == HELPER && member.version.as_deref() == Some("1.0.0"))
        );
        let plan = planning::create(&f.state, request("Peer", None, true)).unwrap();
        assert_atomic_action(&plan, ActionKind::Enable);
        assert!(
            plan.nodes
                .iter()
                .find(|node| node.node_id == NODE)
                .unwrap()
                .members
                .iter()
                .all(|member| member.enabled)
        );
        let mut session = prepared(&f, &plan);
        assert!(f.game.join(mods::disabled_dll_file(CORE_DLL)).exists());
        assert!(f.game.join(mods::disabled_dll_file(HELPER_DLL)).exists());
        super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
        assert!(f.game.join(CORE_DLL).is_file() && f.game.join(HELPER_DLL).is_file());
        assert!(!f.state.config.manifests_dir().join("Web.toml").exists());
    }
}

#[test]
fn conflict_stops_the_whole_bundle_and_a_member_switch_cannot_bypass_it() {
    let mut conflict = requires(HELPER, "");
    conflict.kind = RelationKind::Conflicts;
    let f = setup(true, true, vec![conflict]);
    let mut member_only = request("Web", None, false);
    member_only.roots[0].intent.member_key = Some(inventory::key(HELPER_DLL));
    let blocked = planning::create(&f.state, member_only).unwrap();
    assert_eq!(blocked.result.status, SolveStatus::Incomplete);
    assert_eq!(blocked.result.blockers[0].code, "root_ownership_unsafe");
    assert!(blocked.result.actions.is_empty());
    let plan = planning::create(&f.state, request("Peer", None, true)).unwrap();
    assert_atomic_action(&plan, ActionKind::Disable);
    let mut session = prepared(&f, &plan);
    assert!(f.game.join(CORE_DLL).exists() && f.game.join(HELPER_DLL).exists());
    super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    for path in [CORE_DLL, HELPER_DLL] {
        assert!(f.game.join(mods::disabled_dll_file(path)).is_file());
    }
}

#[test]
fn same_guid_copies_and_dual_file_states_never_gain_switch_authority() {
    for duplicate in [
        "BepInEx/plugins/Other/debugLog.dll".into(),
        mods::disabled_dll_file(HELPER_DLL),
    ] {
        let f = setup(true, true, vec![requires(CORE, "")]);
        write(
            &f.game.join(&duplicate),
            &mods::plugin_blob(HELPER, "Debug log", "0.1.0"),
        );
        let scan = inventory::build(&f.state).unwrap();
        let bundle = scan
            .inventory
            .nodes
            .iter()
            .find(|node| node.id == NODE)
            .unwrap();
        if duplicate.ends_with(".disabled") {
            assert!(!bundle.mutable);
            let blocked = planning::create(&f.state, request("Web", None, false)).unwrap();
            assert_eq!(blocked.result.status, SolveStatus::Incomplete);
            assert!(blocked.result.actions.is_empty());
        } else {
            assert!(!scan.atomic_mod_package(NODE));
            assert_eq!(bundle.current.as_ref().unwrap().provides.len(), 1);
            assert!(!scan.files[NODE].summary.dll_files.contains(&duplicate));
        }
        assert!(f.game.join(duplicate).is_file());
    }
}

fn payload(
    f: &Fixture,
    session: &PlanSession,
    member: Option<(&str, &str)>,
) -> Result<Payload, String> {
    let preview_id = uuid::Uuid::new_v4().to_string();
    let dir = crate::mod_preview::staging::preview_root(&f.state).join(&preview_id);
    fs::create_dir_all(&dir).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(dir.join("Web.zip")).unwrap());
    for (file, guid) in std::iter::once(("Loader.dll", CORE)).chain(member) {
        zip.start_file(file, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&mods::plugin_blob(guid, file, "4.0.0"))
            .unwrap();
    }
    zip.finish().unwrap();
    write(
        &dir.join(crate::mod_preview::staging::ORIGIN_FILE),
        &serde_json::to_vec(&crate::mod_preview::staging::PreviewOriginRecord {
            repository_id: "Web".into(),
            release_tag: "v4.0.0".into(),
            prerelease: false,
        })
        .unwrap(),
    );
    let candidate = session
        .solved
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|node| node.id == NODE)
        .unwrap()
        .current
        .as_ref()
        .unwrap();
    super::super::payload::from_preview(
        &f.state,
        &session.trusted,
        &preview_id,
        NODE,
        candidate,
        None,
        None,
        &session.staging.join(format!("verified-{preview_id}")),
    )
}

#[test]
fn dependency_can_upgrade_the_core_and_enable_an_omitted_declared_member() {
    // The helper GUID sorts first, so it must be considered as a provider
    // before the solver sees that the same package also needs a newer core.
    let f = setup(
        true,
        false,
        vec![requires(HELPER, ">=1.0.0"), requires(CORE, ">=4.0.0")],
    );
    let disabled_helper = f.game.join(mods::disabled_dll_file(HELPER_DLL));
    let helper_bytes = fs::read(&disabled_helper).unwrap();
    let plan = planning::create(&f.state, request("Peer", None, true)).unwrap();
    assert_atomic_action(&plan, ActionKind::Upgrade);
    let mut session = take(&f, &plan);
    let payload = payload(&f, &session, None).unwrap();
    let web = session
        .trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == NODE)
        .unwrap();
    *web.candidates
        .iter_mut()
        .find(|candidate| candidate.id == payload.candidate.id)
        .unwrap() = payload.candidate.clone();
    session.payloads.insert(NODE.into(), payload);
    planning::resolve(&f.state, &mut session).unwrap();
    assert_atomic_action(&session.plan, ActionKind::Upgrade);
    preparation::finish(&f.state, &mut session).unwrap();
    assert!(disabled_helper.is_file() && !f.game.join(HELPER_DLL).exists());
    session.plan.prepared = true;
    super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    assert!(!disabled_helper.exists());
    assert_eq!(fs::read(f.game.join(HELPER_DLL)).unwrap(), helper_bytes);
    assert_eq!(
        mods::read_dll_details(&f.game.join(CORE_DLL), CORE_DLL)
            .version
            .as_deref(),
        Some("4.0.0")
    );
}

#[test]
fn release_overwrites_only_a_verified_same_path_member_and_leaves_omitted_dll_alone() {
    for member in [None, Some(("debugLog.dll", HELPER))] {
        let f = setup(true, true, vec![requires(HELPER, "")]);
        let helper_bytes = fs::read(f.game.join(HELPER_DLL)).unwrap();
        let plan = planning::create(&f.state, request("Web", Some("v4.0.0"), true)).unwrap();
        assert_atomic_action(&plan, ActionKind::Upgrade);
        let mut session = take(&f, &plan);
        let payload = payload(&f, &session, member).unwrap();
        let trusted = session
            .trusted
            .inventory
            .nodes
            .iter_mut()
            .find(|node| node.id == NODE)
            .unwrap();
        *trusted
            .candidates
            .iter_mut()
            .find(|candidate| candidate.id == payload.candidate.id)
            .unwrap() = payload.candidate.clone();
        session.payloads.insert(NODE.into(), payload);
        planning::resolve(&f.state, &mut session).unwrap();
        assert_atomic_action(&session.plan, ActionKind::Upgrade);
        assert_eq!(
            session
                .solved
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .find(|node| node.id == NODE)
                .unwrap()
                .current
                .as_ref()
                .unwrap()
                .provides
                .iter()
                .find(|entry| entry.guid == HELPER)
                .unwrap()
                .version
                .as_deref(),
            Some(if member.is_some() { "4.0.0" } else { "1.0.0" })
        );
        preparation::finish(&f.state, &mut session).unwrap();
        assert_eq!(fs::read(f.game.join(HELPER_DLL)).unwrap(), helper_bytes);
        session.plan.prepared = true;
        super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
        let manifests = archive::read_manifests(&f.state.config.manifests_dir());
        let installed = manifests
            .iter()
            .find(|manifest| manifest.repository_id == "Web")
            .unwrap();
        assert_eq!(
            installed
                .files
                .iter()
                .any(|file| inventory::key(file) == inventory::key(HELPER_DLL)),
            member.is_some()
        );
        assert_eq!(
            fs::read(f.game.join(HELPER_DLL)).unwrap() == helper_bytes,
            member.is_none()
        );
    }
}

#[test]
fn release_refuses_a_relocated_member_or_a_different_guid_at_its_path() {
    for member in [
        ("relocated.dll", HELPER),
        ("debugLog.dll", "fixture.other-guid"),
    ] {
        let f = setup(true, true, vec![requires(HELPER, "")]);
        let original = fs::read(f.game.join(HELPER_DLL)).unwrap();
        let plan = planning::create(&f.state, request("Web", Some("v4.0.0"), true)).unwrap();
        let session = take(&f, &plan);
        let err = payload(&f, &session, Some(member)).err().unwrap();
        assert!(err.starts_with("modChanges.fileOwnershipConflict"), "{err}");
        assert_eq!(fs::read(f.game.join(HELPER_DLL)).unwrap(), original);
        assert!(!f.state.config.manifests_dir().join("Web.toml").exists());
    }
}

#[test]
fn release_refuses_to_overwrite_a_shared_physical_member() {
    let f = setup(true, true, vec![requires(HELPER, "")]);
    let active = f.game.join(HELPER_DLL);
    let other = f.game.join("SharedDebugLibrary.dll");
    fs::hard_link(&active, &other).unwrap();
    let original = fs::read(&active).unwrap();
    let plan = planning::create(&f.state, request("Web", Some("v4.0.0"), true)).unwrap();
    let mut session = take(&f, &plan);
    let payload = payload(&f, &session, Some(("debugLog.dll", HELPER))).unwrap();
    let trusted = session
        .trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == NODE)
        .unwrap();
    *trusted
        .candidates
        .iter_mut()
        .find(|candidate| candidate.id == payload.candidate.id)
        .unwrap() = payload.candidate.clone();
    session.payloads.insert(NODE.into(), payload);
    planning::resolve(&f.state, &mut session).unwrap();
    assert_atomic_action(&session.plan, ActionKind::Upgrade);
    let err = preparation::finish(&f.state, &mut session).unwrap_err();
    assert!(err.starts_with("modChanges.fileOwnershipConflict"), "{err}");
    assert_eq!(fs::read(active).unwrap(), original);
    assert_eq!(fs::read(other).unwrap(), original);
    assert!(!f.state.config.manifests_dir().join("Web.toml").exists());
}
