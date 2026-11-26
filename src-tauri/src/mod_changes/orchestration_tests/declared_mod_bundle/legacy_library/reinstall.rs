use super::*;
use crate::archive::release_download::tests::{Server, reuse_tests::served};
use crate::mod_changes::{execution, mutations, transaction};
use std::{
    cell::Cell,
    io::{Cursor, Read, Write},
};

fn fixture_zip() -> Vec<u8> {
    let core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.1");
    release_zip(&[(CORE_PATH, &core), (STUB_PATH, b"new stub")])
}

fn release_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, bytes) in entries {
        archive
            .start_file(
                path.strip_prefix("BepInEx/plugins/").unwrap(),
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(bytes).unwrap();
    }
    archive.finish().unwrap().into_inner()
}

fn online_fixture(server: &Server, bytes: &[u8]) -> Fixture {
    let mut source = repository(FRAMEWORK, FRAMEWORK_GUID, &["0.11.1"], vec![]);
    source.username = "gua248".into();
    source.install_target = mods::file_state::PLUGINS_DIR.into();
    let asset = "OC2DIYLevel.zip";
    source.asset_rules[0].value = asset.into();
    source.cached_releases[0].assets = vec![served(server, 0, asset, bytes)];
    let fixture = fixture(vec![source]);
    write(
        &fixture.game.join("BepInEx/core/BepInEx.dll"),
        include_bytes!("../../duplicate_release_target/fastinit_version.pe"),
    );
    fixture
}

async fn prepare_reinstall(fixture: &Fixture) -> ChangePlan {
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let plan = planning::create(&fixture.state, request(FRAMEWORK, Some("v0.11.1"), true)).unwrap();
    assert!(!plan.prepared);
    let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    assert!(!fixture.state.config.backups_dir().exists());
    assert!(archive::read_manifests(&fixture.state.config.manifests_dir()).is_empty());
    assert_eq!(outcome.error, None);
    let prepared = outcome.plan.unwrap();
    assert!(prepared.prepared);
    prepared
}

#[tokio::test]
async fn diylevel_reinstall_replaces_only_the_confirmed_stub_and_keeps_a_backup() {
    verify_reinstall(fixture_zip(), b"old stub", b"new stub").await;
}

#[tokio::test]
#[ignore = "requires OC2_LOCAL_DIYLEVEL_ZIP and OC2_LOCAL_DIYLEVEL_OLD_STUB"]
async fn real_diylevel_reinstall_backs_up_the_old_stub_and_preserves_maps() {
    let bytes = fs::read(std::env::var_os("OC2_LOCAL_DIYLEVEL_ZIP").unwrap()).unwrap();
    let old = fs::read(std::env::var_os("OC2_LOCAL_DIYLEVEL_OLD_STUB").unwrap()).unwrap();
    let mut archive = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut incoming = Vec::new();
    archive
        .by_name("OC2DIYLevel/LevelEditorStub.dll")
        .unwrap()
        .read_to_end(&mut incoming)
        .unwrap();
    assert_ne!(old, incoming);
    drop(archive);
    verify_reinstall(bytes, &old, &incoming).await;
}

async fn verify_reinstall(bytes: Vec<u8>, old: &[u8], incoming: &[u8]) {
    let server = Server::new(vec![bytes.clone()], false);
    let fixture = online_fixture(&server, &bytes);
    let map = fixture
        .game
        .join("BepInEx/plugins/OC2DIYLevel/levels/oc1_story/user-map");
    write(&map, b"user map content");
    write(&fixture.game.join(STUB_PATH), old);
    write(&fixture.game.join(UNRELATED_PATH), b"unrelated DLL");
    let prepared = prepare_reinstall(&fixture).await;
    let [overwrite] = prepared.untracked_overwrites.as_slice() else {
        panic!("expected only the stub as a conflict");
    };
    assert_eq!(overwrite.path, STUB_PATH);
    assert!(overwrite.unverified_source);
    let mut session = take(&fixture, &prepared);
    let expected_path = format!(
        "backups/mod-reinstall/{}/{STUB_PATH}",
        session.operation_id()
    );
    let backup = fixture
        .state
        .config
        .config_path()
        .parent()
        .unwrap()
        .join(&expected_path);
    assert!(!backup.exists());
    execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
    let wire = serde_json::to_value(overwrite).unwrap();
    assert_eq!(wire["backup_path"], expected_path);
    assert_eq!(fs::read(backup).unwrap(), old);
    assert_eq!(fs::read(fixture.game.join(STUB_PATH)).unwrap(), incoming);
    assert_eq!(fs::read(map).unwrap(), b"user map content");
    assert_eq!(
        fs::read(fixture.game.join(UNRELATED_PATH)).unwrap(),
        b"unrelated DLL"
    );
    let manifests = archive::read_manifests(&fixture.state.config.manifests_dir());
    let [manifest] = manifests.as_slice() else {
        panic!("expected one installed package");
    };
    assert!(manifest.files.iter().any(|file| file == CORE_PATH));
    assert!(manifest.files.iter().any(|file| file == STUB_PATH));
    assert!(!manifest.files.iter().any(|file| file == UNRELATED_PATH));
    assert_eq!(server.request_count(), 1);
}

#[tokio::test]
async fn cancel_before_confirm_keeps_the_stub_uninstalled_and_does_not_back_it_up() {
    let bytes = fixture_zip();
    let server = Server::new(vec![bytes.clone()], false);
    let fixture = online_fixture(&server, &bytes);
    write(&fixture.game.join(STUB_PATH), b"old stub");
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let prepared = prepare_reinstall(&fixture).await;
    execution::cancel(&fixture.state, &prepared.plan_id).unwrap();
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    assert!(!fixture.state.config.backups_dir().exists());
    assert!(archive::read_manifests(&fixture.state.config.manifests_dir()).is_empty());
}

#[tokio::test]
async fn a_new_file_at_the_reviewed_stub_path_needs_new_confirmation() {
    for incoming in [b"changed bytes".as_slice(), b"old stub"] {
        let bytes = fixture_zip();
        let server = Server::new(vec![bytes.clone()], false);
        let fixture = online_fixture(&server, &bytes);
        let path = fixture.game.join(STUB_PATH);
        write(&path, b"old stub");
        let prepared = prepare_reinstall(&fixture).await;
        let mut session = take(&fixture, &prepared);
        fs::rename(&path, fixture._temp.path().join("original-stub.dll")).unwrap();
        write(&path, incoming);
        let error = execution::commit(&fixture.state, &mut session, |_| {}).unwrap_err();
        assert_eq!(
            error,
            crate::locale_tag::with_params(
                "modChanges.installTargetVerificationChanged",
                &[("path", STUB_PATH)],
            )
        );
        assert_eq!(fs::read(path).unwrap(), incoming);
        assert!(!fixture.game.join(CORE_PATH).exists());
        assert!(!fixture.state.config.backups_dir().exists());
        assert!(archive::read_manifests(&fixture.state.config.manifests_dir()).is_empty());
    }
}

#[tokio::test]
async fn rollback_restores_disabled_stub_and_removes_the_uncommitted_backup() {
    let bytes = fixture_zip();
    let server = Server::new(vec![bytes.clone()], false);
    let fixture = online_fixture(&server, &bytes);
    let old_path = mods::disabled_dll_file(STUB_PATH);
    write(&fixture.game.join(&old_path), b"old stub");
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let prepared = prepare_reinstall(&fixture).await;
    let mut session = take(&fixture, &prepared);
    let roots = mutations::transaction_roots(&fixture.state, &session).unwrap();
    let backup = roots.data_root.join(
        prepared.untracked_overwrites[0]
            .backup_path
            .as_deref()
            .unwrap(),
    );
    let transaction = transaction::prepare_for_operation(
        &fixture.state.config.mod_change_transactions_dir(),
        &roots,
        session
            .mutations
            .take()
            .unwrap()
            .into_iter()
            .map(|owned| owned.mutation)
            .collect(),
        None,
        session.release_target_claims.guarded_original_files(),
    )
    .unwrap();
    let interrupted = Cell::new(false);
    let result = transaction.apply_with_cancel(
        || interrupted.get(),
        |event| {
            if matches!(event, transaction::RecordProgress::Processed(_))
                && !fixture.game.join(&old_path).is_file()
            {
                assert_eq!(fs::read(&backup).unwrap(), b"old stub");
                interrupted.set(true);
            }
        },
    );
    assert!(interrupted.get());
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    assert!(!backup.exists());
    assert!(archive::read_manifests(&fixture.state.config.manifests_dir()).is_empty());
}

#[tokio::test]
async fn reinstall_refuses_to_replace_an_existing_backup() {
    let bytes = fixture_zip();
    let server = Server::new(vec![bytes.clone()], false);
    let fixture = online_fixture(&server, &bytes);
    write(&fixture.game.join(STUB_PATH), b"old stub");
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let prepared = prepare_reinstall(&fixture).await;
    let mut session = take(&fixture, &prepared);
    let roots = mutations::transaction_roots(&fixture.state, &session).unwrap();
    let backup = roots.data_root.join(
        prepared.untracked_overwrites[0]
            .backup_path
            .as_deref()
            .unwrap(),
    );
    write(&backup, b"external file");
    assert!(execution::commit(&fixture.state, &mut session, |_| {}).is_err());
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    assert_eq!(fs::read(backup).unwrap(), b"external file");
    assert!(archive::read_manifests(&fixture.state.config.manifests_dir()).is_empty());
}

#[tokio::test]
async fn a_late_disabled_counterpart_rolls_back_the_reinstall() {
    for relative in [CORE_PATH, STUB_PATH] {
        let bytes = fixture_zip();
        let server = Server::new(vec![bytes.clone()], false);
        let fixture = online_fixture(&server, &bytes);
        let stub = fixture.game.join(STUB_PATH);
        let core = fixture.game.join(CORE_PATH);
        let counterpart = fixture.game.join(mods::disabled_dll_file(relative));
        write(&stub, b"old stub");
        let prepared = prepare_reinstall(&fixture).await;
        let mut session = take(&fixture, &prepared);
        let roots = mutations::transaction_roots(&fixture.state, &session).unwrap();
        let backup = roots.data_root.join(
            prepared.untracked_overwrites[0]
                .backup_path
                .as_deref()
                .unwrap(),
        );
        let mut appeared = false;
        let error = execution::commit(&fixture.state, &mut session, |_| {
            if !appeared
                && core.is_file()
                && fs::read(&stub).is_ok_and(|bytes| bytes == b"new stub")
            {
                assert_eq!(fs::read(&backup).unwrap(), b"old stub");
                write(&counterpart, b"external disabled DLL");
                appeared = true;
            }
        })
        .unwrap_err();
        assert!(appeared);
        assert_eq!(
            crate::locale_tag::diagnostic_tag(&error),
            "modChanges.installTargetHasBothStates"
        );
        assert_eq!(fs::read(counterpart).unwrap(), b"external disabled DLL");
        assert_eq!(fs::read(stub).unwrap(), b"old stub");
        assert!(!core.exists());
        assert!(!backup.exists());
        assert!(archive::read_manifests(&fixture.state.config.manifests_dir()).is_empty());
    }
}

#[tokio::test]
async fn the_reinstall_exception_keeps_existing_release_and_ownership_checks() {
    for (case, tag) in [
        ("unrelated DLL", "modChanges.installTargetResidueDiffers"),
        (
            "old plugin GUID",
            "modChanges.installTargetPluginIdentityUnknown",
        ),
        (
            "missing digest",
            "modChanges.installTargetNeedsVerifiedRelease",
        ),
        ("twins", "modChanges.installTargetHasBothStates"),
        ("hard link", "modChanges.installTargetAliasesOrLink"),
        ("other MOD", "modChanges.installTargetClaimedWithManifests"),
        ("map registry", "modChanges.installTargetMapProtected"),
    ] {
        let relative = if case == "unrelated DLL" {
            UNRELATED_PATH
        } else {
            STUB_PATH
        };
        let core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.1");
        let bytes = release_zip(&[(CORE_PATH, &core), (relative, b"new stub")]);
        let server = Server::new(vec![bytes.clone()], false);
        let fixture = online_fixture(&server, &bytes);
        write(&fixture.game.join(relative), b"old stub");
        match case {
            "old plugin GUID" => write(
                &fixture.game.join(relative),
                &mods::plugin_blob("com.example.other", "Other", "1.0.0"),
            ),
            "missing digest" => {
                let mut config = fixture.state.config.load().unwrap();
                config.repositories[0].cached_releases[0].assets[0].digest = None;
                fixture.state.config.save(&config).unwrap();
            }
            "twins" => write(
                &fixture.game.join(mods::disabled_dll_file(relative)),
                b"off",
            ),
            "hard link" => fs::hard_link(
                fixture.game.join(relative),
                fixture._temp.path().join("another-file.dll"),
            )
            .unwrap(),
            "other MOD" => {
                let manifest = archive::new_manifest(
                    "other",
                    "v1",
                    BTreeMap::new(),
                    BTreeMap::new(),
                    vec![relative.into()],
                    vec![],
                );
                archive::write_manifest(&fixture.state.config.manifests_dir(), &manifest).unwrap();
            }
            "map registry" => write(
                &fixture
                    .state
                    .config
                    .manifests_dir()
                    .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                &serde_json::to_vec(
                    &serde_json::json!({"version": 1, "entries": [{"dll": relative}]}),
                )
                .unwrap(),
            ),
            _ => {}
        }
        let before = Fingerprint::capture(&fixture.game).unwrap();
        let records = Fingerprint::capture(&fixture.state.config.manifests_dir()).unwrap();
        let plan =
            planning::create(&fixture.state, request(FRAMEWORK, Some("v0.11.1"), true)).unwrap();
        let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await;
        let error = outcome
            .error
            .as_deref()
            .unwrap_or_else(|| panic!("{case} unexpectedly prepared"));
        assert_eq!(
            crate::locale_tag::diagnostic_tag(error),
            tag,
            "{case}: {error}"
        );
        assert_eq!(
            Fingerprint::capture(&fixture.game).unwrap(),
            before,
            "{case}"
        );
        assert_eq!(
            Fingerprint::capture(&fixture.state.config.manifests_dir()).unwrap(),
            records,
            "{case}"
        );
        assert!(!fixture.state.config.backups_dir().exists(), "{case}");
    }
}
