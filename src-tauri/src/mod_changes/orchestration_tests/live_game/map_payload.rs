use super::*;

const MEMBER: &str = "nested/Inner.dll";
const DISABLED: &str = "nested/Inner.dll.disabled";
const INNER_GUID: &str = "com.example.inner";

fn replacing_disabled_member(f: &Fixture, incoming: &[u8]) -> (PathBuf, PlanRequest) {
    let old_map = installed_map(f);
    write(
        &old_map.join("info"),
        &live_game_info::bundle(MAP_NAME, AUTHOR, "0.9.0"),
    );
    write(&old_map.join("scene"), b"previous map data");
    write(
        &old_map.join(DISABLED),
        &mods::plugin_blob(INNER_GUID, "Inner", "0.9.0"),
    );
    let entry = format!(
        "{}/levels/{MAP_NAME}/{MEMBER}",
        crate::framework::DIYLEVEL_DIR
    );
    let request = import(f, &[(&entry, incoming)]);
    (old_map, request)
}

#[tokio::test]
async fn map_update_preserves_a_disabled_inner_dll_without_changing_the_verified_inputs() {
    let f = setup(vec![], vec![]);
    let incoming = mods::plugin_blob(INNER_GUID, "Inner", "0.8.0");
    let (old_map, request) = replacing_disabled_member(&f, &incoming);
    let zip = PathBuf::from(&request.map_import.as_ref().unwrap().zip_path);
    let zip_before = Fingerprint::capture(&zip).unwrap();
    let live_before = snapshot(&f);

    let plan = planning::create(&f.state, request).unwrap();
    assert_map_only(&plan);
    assert_eq!(plan.result.actions[0].kind, ActionKind::Upgrade);
    let prepared = ready(prepare(&f, &plan).await);
    let [bundled] = prepared.bundled_dlls[MAP_ID].as_slice() else {
        panic!("the changed map must list its verified inner DLL");
    };
    assert_eq!(bundled.kind, Some(ActionKind::Downgrade));
    assert_eq!(bundled.from_version.as_deref(), Some("0.9.0"));
    assert_eq!(bundled.to_version.as_deref(), Some("0.8.0"));
    assert!(bundled.dll_file.ends_with(MEMBER), "{}", bundled.dll_file);
    assert_eq!(bundled.node_id, MAP_ID);
    assert_eq!(bundled.parent_node_id, MAP_ID);
    assert_eq!(snapshot(&f), live_before);
    let mut session = take(&f, &prepared);
    let batch = session.map_payload.as_ref().unwrap();
    let staged_map = batch.stage_root.join(format!(
        "{}/levels/{MAP_NAME}",
        crate::framework::DIYLEVEL_DIR
    ));
    assert_eq!(fs::read(staged_map.join(MEMBER)).unwrap(), incoming);
    assert!(!staged_map.join(DISABLED).exists());

    execution::commit(&f.state, &mut session, |_| {}).unwrap();
    assert_eq!(Fingerprint::capture(&zip).unwrap(), zip_before);
    assert_eq!(fs::read(old_map.join("scene")).unwrap(), SCENE);
    assert_eq!(fs::read(old_map.join(DISABLED)).unwrap(), incoming);
    assert!(!old_map.join(MEMBER).exists());
}

#[test]
fn fresh_map_plan_lists_a_new_bundled_dll_as_an_install() {
    let f = setup(vec![], vec![]);
    let entry = format!(
        "{}/levels/{MAP_NAME}/{MEMBER}",
        crate::framework::DIYLEVEL_DIR
    );
    let incoming = mods::plugin_blob(INNER_GUID, "Inner", "2.1.0");
    let request = import(&f, &[(&entry, &incoming)]);
    let plan = planning::create(&f.state, request).unwrap();
    let [bundled] = plan.bundled_dlls[MAP_ID].as_slice() else {
        panic!("expected exactly one inner DLL");
    };
    assert_eq!(bundled.kind, Some(ActionKind::Install));
    assert_eq!(bundled.from_version, None);
    assert_eq!(bundled.to_version.as_deref(), Some("2.1.0"));
    assert!(bundled.dll_file.ends_with(MEMBER), "{}", bundled.dll_file);
}

#[tokio::test]
async fn changes_to_a_map_zip_or_its_verified_stage_stop_preparation() {
    for input in ["zip", "map", "registry"] {
        let f = setup(vec![], vec![]);
        let incoming = mods::plugin_blob(INNER_GUID, "Inner", VERSION);
        let (_, request) = replacing_disabled_member(&f, &incoming);
        let plan = planning::create(&f.state, request).unwrap();
        assert_map_only(&plan);
        let forked_path = {
            let manager = f.state.mod_changes.lock().unwrap();
            let batch = manager.plans[&plan.plan_id].map_payload.as_ref().unwrap();
            match input {
                "zip" => batch.source_path.clone(),
                "map" => batch.stage_root.join(format!(
                    "{}/levels/{MAP_NAME}/{MEMBER}",
                    crate::framework::DIYLEVEL_DIR
                )),
                "registry" => batch
                    .manifests_root
                    .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
                _ => unreachable!(),
            }
        };
        let live_before = snapshot(&f);
        fs::write(forked_path, b"external edit after planning").unwrap();
        let outcome = prepare(&f, &plan).await;
        assert_eq!(
            outcome.error.as_deref(),
            Some("modChanges.payloadChanged"),
            "{input}"
        );
        assert!(outcome.plan.is_none(), "{input}");
        assert_eq!(snapshot(&f), live_before, "{input}");
    }
}

#[tokio::test]
async fn changing_the_prepared_map_tree_prevents_commit() {
    let f = setup(vec![], vec![]);
    let incoming = mods::plugin_blob(INNER_GUID, "Inner", VERSION);
    let (_, request) = replacing_disabled_member(&f, &incoming);
    let plan = planning::create(&f.state, request).unwrap();
    let prepared = ready(prepare(&f, &plan).await);
    let mut session = take(&f, &prepared);
    let batch = session.map_payload.as_ref().unwrap();
    let output = session
        .payload_fingerprints
        .keys()
        .find(|path| !path.starts_with(&batch.stage_root) && path.join(DISABLED).is_file())
        .expect("the prepared tree has its own fingerprint");
    let live_before = snapshot(&f);
    fs::write(output.join(DISABLED), b"external edit after approval").unwrap();
    let error =
        execution::commit(&f.state, &mut session, |_| panic!("live write started")).unwrap_err();
    assert_eq!(error, "modChanges.payloadChanged");
    assert_eq!(snapshot(&f), live_before);
}
