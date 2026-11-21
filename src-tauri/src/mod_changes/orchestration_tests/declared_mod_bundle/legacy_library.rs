use super::*;

const FRAMEWORK: &str = "Overcooked2-LevelEditor";
const FRAMEWORK_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const CORE_PATH: &str = "BepInEx/plugins/OC2DIYLevel/OC2DIYLevel.dll";
const STUB_PATH: &str = "BepInEx/plugins/OC2DIYLevel/LevelEditorStub.dll";
const UNRELATED_PATH: &str = "BepInEx/plugins/OC2DIYLevel/UnrelatedLibrary.dll";
const SPLITTER: &str = "dev.gua.overcooked.diylevel.ingredientsplittermod";
const SPLITTER_PATH: &str =
    "BepInEx/plugins/OC2DIYLevel/modules/IngredientSplitter/IngredientSplitterMod.dll";
const SPLITTER_STUB_PATH: &str =
    "BepInEx/plugins/OC2DIYLevel/modules/IngredientSplitter/IngredientSplitterStub.dll";
const RESOURCE_PATH: &str = "BepInEx/plugins/OC2DIYLevel/common01";

fn framework_fixture() -> Fixture {
    let mut framework = repository(FRAMEWORK, FRAMEWORK_GUID, &["0.11.1"], vec![]);
    framework.username = "gua248".into();
    framework.install_target = mods::file_state::PLUGINS_DIR.into();
    framework.asset_rules[0].value = "OC2DIYLevel.zip".into();
    framework.cached_releases[0].assets[0].name = "OC2DIYLevel.zip".into();
    fixture(vec![framework])
}

#[test]
fn framework_upgrade_is_selectable_after_manifest_loss_with_a_guidless_stub() {
    let f = framework_fixture();
    write(
        &f.game.join(CORE_PATH),
        &mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.10.0"),
    );
    write(&f.game.join(STUB_PATH), b"fixture dependency (no GUID)");
    write(&f.game.join(UNRELATED_PATH), b"fixture unrelated library");
    assert!(archive::read_manifests(&f.state.config.manifests_dir()).is_empty());

    let plan = planning::create(&f.state, request(FRAMEWORK, Some("v0.11.1"), true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    let id = format!("mod:{FRAMEWORK}");
    assert_eq!(plan.result.actions.len(), 1);
    assert_eq!(plan.result.actions[0].node_id, id);
    assert_eq!(plan.result.actions[0].kind, ActionKind::Upgrade);
    let current = inventory::build(&f.state).unwrap();
    assert!(current.atomic_mod_package(&id));
    let members = &current.files[&id].summary.dll_files;
    assert_eq!(members.len(), 2);
    assert!(
        members
            .iter()
            .any(|path| inventory::key(path) == inventory::key(STUB_PATH))
    );
    assert!(current.files[&id].manifests.is_empty());
    assert!(f.game.join(UNRELATED_PATH).is_file());
}

#[test]
fn exact_release_replaces_changed_guidless_and_ungrouped_companions_without_a_manifest() {
    let f = framework_fixture();
    let old_core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.0");
    let new_core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.1");
    let old_splitter = mods::plugin_blob(SPLITTER, "IngredientSplitter", "0.1.0");
    let new_splitter = mods::plugin_blob(SPLITTER, "IngredientSplitter", "0.1.1");
    let other_stub = "BepInEx/plugins/MOD/LevelEditorStub.dll";
    for (path, bytes) in [
        (CORE_PATH, old_core.as_slice()),
        (STUB_PATH, b"old level stub"),
        (SPLITTER_STUB_PATH, b"old splitter stub"),
        (SPLITTER_PATH, old_splitter.as_slice()),
        (RESOURCE_PATH, b"old asset"),
        (other_stub, b"unrelated same-name stub"),
    ] {
        write(&f.game.join(path), bytes);
    }
    let incoming = [
        (CORE_PATH, new_core.as_slice()),
        (STUB_PATH, b"new level stub"),
        (SPLITTER_STUB_PATH, b"new splitter stub"),
        (SPLITTER_PATH, new_splitter.as_slice()),
        (RESOURCE_PATH, b"new asset"),
    ];
    let preview = exact_preview(&f, incoming, true);
    let before = Fingerprint::capture(&f.game).unwrap();
    let plan = planning::create(&f.state, request(FRAMEWORK, Some("v0.11.1"), true)).unwrap();
    assert_eq!(plan.result.actions[0].kind, ActionKind::Upgrade);
    let mut session = take(&f, &plan);
    inject_release(&f, &mut session, &preview).expect("empty GUIDs are not shared identities");
    planning::resolve(&f.state, &mut session).unwrap();
    preparation::finish(&f.state, &mut session).unwrap();
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert!(archive::read_manifests(&f.state.config.manifests_dir()).is_empty());
    session.plan.prepared = true;
    super::super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    let manifest = archive::read_manifest(&f.state.config.manifests_dir(), FRAMEWORK).unwrap();
    for (path, bytes) in incoming {
        assert_eq!(fs::read(f.game.join(path)).unwrap(), bytes, "{path}");
        assert!(
            manifest
                .files
                .iter()
                .any(|file| inventory::key(file) == inventory::key(path))
        );
    }
    assert_eq!(
        fs::read(f.game.join(other_stub)).unwrap(),
        b"unrelated same-name stub"
    );
}

#[test]
fn missing_core_reinstalls_with_disabled_identical_loose_members_and_resources() {
    let f = framework_fixture();
    let stubs = [
        (STUB_PATH, b"level stub".as_slice()),
        (SPLITTER_STUB_PATH, b"splitter stub"),
    ];
    let core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.1");
    let splitter = mods::plugin_blob(SPLITTER, "IngredientSplitter", "0.1.0");
    let loose = stubs
        .into_iter()
        .chain([(SPLITTER_PATH, splitter.as_slice())]);
    for (path, bytes) in loose.clone() {
        let existing = if path == SPLITTER_STUB_PATH {
            path.to_string()
        } else {
            mods::disabled_dll_file(path)
        };
        write(&f.game.join(existing), bytes);
    }
    write(&f.game.join(RESOURCE_PATH), b"fixture common asset");
    write(&f.game.join(UNRELATED_PATH), b"fixture unrelated library");
    let preview = exact_preview(
        &f,
        loose.clone().chain([
            (CORE_PATH, core.as_slice()),
            (RESOURCE_PATH, b"fixture common asset"),
        ]),
        true,
    );
    // A local dependent enables the missing package through the usual solver.
    let mut config = f.state.config.load_or_default();
    config.repositories.push(repository(
        "Peer",
        PEER_GUID,
        &[],
        vec![requires(FRAMEWORK_GUID, ">=0.11.1")],
    ));
    f.state.config.save(&config).unwrap();
    let peer = "BepInEx/plugins/Peer/Peer.dll";
    write(
        &f.game.join(mods::disabled_dll_file(peer)),
        &mods::plugin_blob(PEER_GUID, "Peer", "1.0.0"),
    );
    let before = Fingerprint::capture(&f.game).unwrap();
    let plan = planning::create(&f.state, request("Peer", None, true)).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready);
    let mut session = take(&f, &plan);
    inject_release(&f, &mut session, &preview).unwrap();
    planning::resolve(&f.state, &mut session)
        .expect("the verified release owns its identical loose files");
    preparation::finish(&f.state, &mut session).unwrap();
    assert!(!f.game.join(CORE_PATH).exists());
    assert!(archive::read_manifests(&f.state.config.manifests_dir()).is_empty());
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    session.plan.prepared = true;
    super::super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap();
    let manifest = archive::read_manifest(&f.state.config.manifests_dir(), FRAMEWORK).unwrap();
    for (path, bytes) in loose.chain([
        (CORE_PATH, core.as_slice()),
        (RESOURCE_PATH, b"fixture common asset"),
    ]) {
        assert_eq!(fs::read(f.game.join(path)).unwrap(), bytes);
        assert!(!f.game.join(mods::disabled_dll_file(path)).exists());
        assert!(
            manifest
                .files
                .iter()
                .any(|entry| inventory::key(entry) == inventory::key(path))
        );
    }
    assert!(f.game.join(peer).is_file());
    assert_eq!(
        fs::read(f.game.join(UNRELATED_PATH)).unwrap(),
        b"fixture unrelated library"
    );
}

#[test]
fn missing_core_refuses_unsafe_release_residues_with_their_exact_paths() {
    for case in [
        "different",
        "no digest",
        "twins",
        "hard link",
        "manifest",
        "map registry",
        "separate GUID",
        "different resource",
    ] {
        let f = framework_fixture();
        let core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.1");
        let splitter = mods::plugin_blob(SPLITTER, "IngredientSplitter", "0.1.0");
        let disabled = mods::disabled_dll_file(SPLITTER_PATH);
        let path = if case == "different resource" {
            RESOURCE_PATH
        } else if case == "twins" {
            SPLITTER_PATH
        } else {
            &disabled
        };
        let bytes = if case == "different" {
            b"different DLL".as_slice()
        } else {
            splitter.as_slice()
        };
        write(&f.game.join(&disabled), bytes);
        match case {
            "twins" => write(&f.game.join(SPLITTER_PATH), bytes),
            "hard link" => fs::hard_link(
                f.game.join(&disabled),
                f._temp.path().join("second-link.dll"),
            )
            .unwrap(),
            "manifest" => {
                let manifest = archive::new_manifest(
                    "other",
                    "v1",
                    BTreeMap::new(),
                    BTreeMap::new(),
                    vec![disabled.clone()],
                    vec![],
                );
                archive::write_manifest(&f.state.config.manifests_dir(), &manifest).unwrap();
            }
            "map registry" => write(
                &f.state
                    .config
                    .manifests_dir()
                    .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                &serde_json::to_vec(
                    &serde_json::json!({ "version": 1, "entries": [{"dll": SPLITTER_PATH}] }),
                )
                .unwrap(),
            ),
            "separate GUID" => {
                let mut config = f.state.config.load_or_default();
                config
                    .repositories
                    .push(repository("IndependentSplitter", SPLITTER, &[], vec![]));
                f.state.config.save(&config).unwrap();
            }
            "different resource" => write(&f.game.join(RESOURCE_PATH), b"locally modified asset"),
            _ => {}
        }
        let preview = exact_preview(
            &f,
            [
                (CORE_PATH, core.as_slice()),
                (SPLITTER_PATH, splitter.as_slice()),
                (RESOURCE_PATH, b"release asset"),
            ],
            case != "no digest",
        );
        let before = Fingerprint::capture(&f.game).unwrap();
        let plan = planning::create(&f.state, request(FRAMEWORK, Some("v0.11.1"), true)).unwrap();
        let mut session = take(&f, &plan);
        inject_release(&f, &mut session, &preview).unwrap();
        let error = planning::resolve(&f.state, &mut session).unwrap_err();
        assert_eq!(
            error,
            crate::locale_tag::with_params("modChanges.fileOwnershipConflict", &[("path", path)]),
            "{case}"
        );
        assert_eq!(Fingerprint::capture(&f.game).unwrap(), before, "{case}");
        assert!(!f.game.join(CORE_PATH).exists());
    }
}

#[test]
fn missing_core_approval_keeps_a_changed_residue_and_refuses_to_write() {
    let f = framework_fixture();
    let core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.1");
    let disabled = mods::disabled_dll_file(STUB_PATH);
    write(&f.game.join(&disabled), b"stub");
    let preview = exact_preview(
        &f,
        [(CORE_PATH, core.as_slice()), (STUB_PATH, b"stub")],
        true,
    );
    let plan = planning::create(&f.state, request(FRAMEWORK, Some("v0.11.1"), true)).unwrap();
    let mut session = take(&f, &plan);
    inject_release(&f, &mut session, &preview).unwrap();
    planning::resolve(&f.state, &mut session).unwrap();
    preparation::finish(&f.state, &mut session).unwrap();
    session.plan.prepared = true;

    write(&f.game.join(&disabled), b"stub changed after approval");
    let before = Fingerprint::capture(&f.game).unwrap();
    assert_eq!(
        super::super::super::execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
        crate::locale_tag::with_params("modChanges.fileOwnershipConflict", &[("path", &disabled)]),
    );
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert!(archive::read_manifests(&f.state.config.manifests_dir()).is_empty());
}

#[test]
fn missing_core_cancellation_restores_the_disabled_residue_and_removes_the_install() {
    use std::cell::Cell;

    let f = framework_fixture();
    let core = mods::plugin_blob(FRAMEWORK_GUID, "DIYLevel", "0.11.1");
    let disabled = mods::disabled_dll_file(STUB_PATH);
    write(&f.game.join(&disabled), b"stub");
    let preview = exact_preview(
        &f,
        [(CORE_PATH, core.as_slice()), (STUB_PATH, b"stub")],
        true,
    );
    let plan = planning::create(&f.state, request(FRAMEWORK, Some("v0.11.1"), true)).unwrap();
    let mut session = take(&f, &plan);
    inject_release(&f, &mut session, &preview).unwrap();
    planning::resolve(&f.state, &mut session).unwrap();
    preparation::finish(&f.state, &mut session).unwrap();
    let before = Fingerprint::capture(&f.game).unwrap();
    let data_root = f.state.config.config_path().parent().unwrap().to_path_buf();
    let roots = TransactionRoots {
        game_root: f.game.clone(),
        data_root: data_root.clone(),
    };
    let transaction = super::super::super::transaction::prepare(
        &data_root.join("mod-change-transactions"),
        &roots,
        session
            .mutations
            .unwrap()
            .into_iter()
            .map(|owned| owned.mutation)
            .collect(),
    )
    .unwrap();
    let interrupted = Cell::new(false);
    let result = transaction.apply_with_cancel(
        || interrupted.get(),
        |event| {
            if matches!(
                event,
                super::super::super::transaction::RecordProgress::Processed(_)
            ) && !f.game.join(&disabled).is_file()
            {
                interrupted.set(true);
            }
        },
    );
    assert!(
        interrupted.get(),
        "the old file was removed before cancellation"
    );
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert_eq!(Fingerprint::capture(&f.game).unwrap(), before);
    assert!(archive::read_manifests(&f.state.config.manifests_dir()).is_empty());
}

fn exact_preview<'a>(
    f: &Fixture,
    entries: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    with_digest: bool,
) -> String {
    let id = uuid::Uuid::new_v4().to_string();
    let directory = crate::mod_preview::staging::preview_root(&f.state).join(&id);
    fs::create_dir_all(&directory).unwrap();
    let zip_path = directory.join("OC2DIYLevel.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    for (path, bytes) in entries {
        zip.start_file(
            path.strip_prefix("BepInEx/plugins/").unwrap(),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    let mut config = f.state.config.load_or_default();
    let release = &mut config.repositories[0].cached_releases[0];
    let Fingerprint::File { size, sha256, .. } = Fingerprint::capture(&zip_path).unwrap() else {
        unreachable!()
    };
    release.assets[0].size = size;
    release.assets[0].digest = with_digest.then(|| format!("sha256:{sha256}"));
    f.state.config.save(&config).unwrap();
    write(
        &directory.join(crate::mod_preview::staging::ORIGIN_FILE),
        &serde_json::to_vec(&crate::mod_preview::staging::PreviewOriginRecord {
            repository_id: FRAMEWORK.into(),
            release_tag: "v0.11.1".into(),
            prerelease: false,
        })
        .unwrap(),
    );
    id
}

fn inject_release(f: &Fixture, session: &mut PlanSession, preview: &str) -> Result<(), String> {
    let node_id = format!("mod:{FRAMEWORK}");
    let chosen = session
        .solved
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|node| node.id == node_id)
        .unwrap()
        .current
        .as_ref()
        .unwrap();
    let payload = super::super::super::payload::from_preview(
        &f.state,
        &session.trusted,
        preview,
        &node_id,
        chosen,
        None,
        None,
        &session.staging.join("verified-release"),
    )?;
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
        .find(|candidate| candidate.id == payload.candidate.id)
        .unwrap() = payload.candidate.clone();
    session.payloads.insert(node_id, payload);
    Ok(())
}
