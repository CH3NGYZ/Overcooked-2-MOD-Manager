//! Exercise the ownership exception through verified repository ZIPs and the real transaction.
use super::{bundled_maps_tests::stage_zip_payload, *};
use crate::{mod_changes::transaction, models::ModRepositoryBinding};
use std::cell::Cell;

const FW_DLL: &str = "BepInEx/plugins/OC2DIYLevel/LevelEditorStub.dll";
const WEB_DLL: &str = "BepInEx/plugins/OC2DIYLevelRuntimeWLoader/Loader.dll";
const WEB_GUID: &str = "oc2.oc2diylevelruntimewloader";
const WEB_RESOURCE_4: &str = "BepInEx/plugins/OC2DIYLevelRuntimeWLoader/commonW4";
const WEB_RESOURCE_5: &str = "BepInEx/plugins/OC2DIYLevelRuntimeWLoader/commonW5";
const WEB_RESOURCE_6: &str = "BepInEx/plugins/OC2DIYLevelRuntimeWLoader/commonW6";
const OLD_MAP: &str = "BepInEx/plugins/OC2DIYLevel/levels/oc1_story/info";
const NEW_MAP: &str = "BepInEx/plugins/OC2DIYLevel/levels/oc1_dlc/info";

fn set_old_binding(f: &mut Fixture, id: &str, primary: &str, tag: &str) {
    let repo = f
        .session
        .trusted
        .config
        .repositories
        .iter()
        .find(|repo| repo.id == id)
        .unwrap();
    let trusted_node = f
        .session
        .trusted
        .inventory
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
        .unwrap();
    let installed = trusted_node.current.as_mut().unwrap();
    installed.source_id = Some(id.into());
    installed.tag = Some(tag.into());
    let version = tag.trim_start_matches('v');
    installed.version = Some(version.into());
    installed.provides[0].version = Some(version.into());

    let files = f.session.trusted.files.get_mut(id).unwrap();
    files.summary.repository_id = Some(id.into());
    files.summary.subject_tag = Some(tag.into());
    files.summary.dll_files[0] = primary.into();
    files.summary.members[0].dll_file = primary.into();
    files.manifests[0].release_tag = tag.into();
    files.manifests[0].repository_binding = ModRepositoryBinding::from_release(repo, tag);
    archive::write_manifest(&f.roots.data_root.join("manifests"), &files.manifests[0]).unwrap();
}

fn upgrades(web_digest: bool) -> Fixture {
    upgrades_with_extra(web_digest, &[])
}

fn upgrades_with_extra(web_digest: bool, extra: &[(&str, &[u8])]) -> Fixture {
    let mut f = fixture();
    let fw_guid = mods::plugin_target::DIYLEVEL_GUID;
    write(
        &f.roots.game_root,
        FW_DLL,
        &mods::plugin_blob(fw_guid, "DIYLevel", "0.10.0"),
    );
    write(
        &f.roots.game_root,
        WEB_DLL,
        &mods::plugin_blob(WEB_GUID, "Web", "3.10.2"),
    );
    write(&f.roots.game_root, OLD_MAP, b"player-map");
    write(&f.roots.game_root, WEB_RESOURCE_4, b"old-runtime");
    write(&f.roots.game_root, WEB_RESOURCE_5, b"new-runtime-5");
    write(&f.roots.game_root, WEB_RESOURCE_6, b"new-runtime-6");
    add_mod(
        &mut f,
        "framework",
        vec![member(FW_DLL, true, fw_guid)],
        vec![FW_DLL.into(), OLD_MAP.into()],
    );
    add_mod(
        &mut f,
        "web",
        vec![member(WEB_DLL, true, WEB_GUID)],
        vec![WEB_DLL.into(), WEB_RESOURCE_4.into()],
    );
    f.session.trusted.level_roots = crate::level_sets::LevelSetRoots::new(
        f.roots.game_root.clone(),
        Some(f.roots.game_root.join("BepInEx/plugins/OC2DIYLevel")),
    );
    let new_framework = mods::plugin_blob(fw_guid, "DIYLevel", "0.11.1");
    stage_zip_payload(
        &mut f,
        "framework",
        fw_guid,
        "0.11.1",
        &[
            ("OC2DIYLevel/LevelEditorStub.dll", &new_framework),
            ("OC2DIYLevel/levels/oc1_story/info", b"shipped-map"),
            ("OC2DIYLevel/levels/oc1_dlc/info", b"new-map"),
        ],
        true,
    );
    let new_web = mods::plugin_blob(WEB_GUID, "Web", "4.0.0");
    let mut web_entries: Vec<(&str, &[u8])> = vec![
        ("plugins/OC2DIYLevelRuntimeWLoader/Loader.dll", &new_web),
        (
            "plugins/OC2DIYLevelRuntimeWLoader/commonW4",
            b"new-runtime-4",
        ),
        (
            "plugins/OC2DIYLevelRuntimeWLoader/commonW5",
            b"new-runtime-5",
        ),
        (
            "plugins/OC2DIYLevelRuntimeWLoader/commonW6",
            b"new-runtime-6",
        ),
    ];
    web_entries.extend_from_slice(extra);
    stage_zip_payload(&mut f, "web", WEB_GUID, "4.0.0", &web_entries, web_digest);
    set_old_binding(&mut f, "framework", FW_DLL, "v0.10.0");
    set_old_binding(&mut f, "web", WEB_DLL, "v3.10.2");
    f
}

fn manifest(f: &Fixture, id: &str) -> archive::InstallManifest {
    archive::read_manifest(&f.roots.data_root.join("manifests"), id).unwrap()
}

fn fingerprint(f: &Fixture) -> (Fingerprint, Fingerprint) {
    (
        Fingerprint::capture(&f.roots.game_root).unwrap(),
        Fingerprint::capture(&f.roots.data_root.join("manifests")).unwrap(),
    )
}

#[test]
fn matching_verified_web_resources_join_new_manifest_as_both_packages_upgrade() {
    let f = upgrades(true);
    let before = fingerprint(&f);
    assert!(
        !manifest(&f, "web")
            .files
            .iter()
            .any(|path| path == WEB_RESOURCE_5 || path == WEB_RESOURCE_6)
    );
    let changes = assemble(&f.session, f.roots.clone())
        .expect("the online package already contains the identical loose files");
    assert_eq!(fingerprint(&f), before);
    transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply()
    .unwrap();

    assert_eq!(
        fs::read(f.roots.game_root.join(FW_DLL)).unwrap(),
        mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.11.1")
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(WEB_DLL)).unwrap(),
        mods::plugin_blob(WEB_GUID, "Web", "4.0.0")
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(WEB_RESOURCE_4)).unwrap(),
        b"new-runtime-4"
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(WEB_RESOURCE_5)).unwrap(),
        b"new-runtime-5"
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(WEB_RESOURCE_6)).unwrap(),
        b"new-runtime-6"
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(OLD_MAP)).unwrap(),
        b"player-map"
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(NEW_MAP)).unwrap(),
        b"new-map"
    );
    let framework = manifest(&f, "framework");
    assert_eq!(framework.release_tag, "v0.11.1");
    assert_eq!(framework.files, [FW_DLL]);
    let web = manifest(&f, "web");
    assert_eq!(web.release_tag, "v4.0.0");
    assert_eq!(web.repository_binding.unwrap().tag, "v4.0.0");
    assert_eq!(
        web.files
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([WEB_DLL, WEB_RESOURCE_4, WEB_RESOURCE_5, WEB_RESOURCE_6])
    );
}

fn assert_resource_rejected(f: &Fixture, path: &str) {
    let before = fingerprint(f);
    let error = assemble(&f.session, f.roots.clone()).unwrap_err();
    assert_eq!(
        error,
        format!("modChanges.fileOwnershipConflict?path={path}")
    );
    assert_eq!(
        fingerprint(f),
        before,
        "a rejected plan leaves files and manifests intact"
    );
}

#[test]
fn matching_bytes_without_a_release_digest_do_not_authorize_adoption() {
    assert_resource_rejected(&upgrades(false), WEB_RESOURCE_5);
}

#[test]
fn verified_asset_cannot_adopt_same_size_different_content() {
    let f = upgrades(true);
    write(&f.roots.game_root, WEB_RESOURCE_6, b"new-runtime-X");
    assert_resource_rejected(&f, WEB_RESOURCE_6);
}

#[test]
fn verified_asset_cannot_adopt_a_file_claimed_by_another_manifest() {
    let mut f = upgrades(true);
    add_mod(
        &mut f,
        "another-package",
        vec![],
        vec![WEB_RESOURCE_6.into()],
    );
    assert_resource_rejected(&f, WEB_RESOURCE_6);
}

#[test]
fn a_manifest_claim_with_different_case_still_blocks_release_adoption() {
    let mut f = upgrades(true);
    add_mod(
        &mut f,
        "another-package",
        vec![],
        vec![WEB_RESOURCE_6.replace("commonW6", "CommonW6")],
    );
    assert_resource_rejected(&f, WEB_RESOURCE_6);
}

#[test]
fn an_unmatched_manifest_claiming_a_hard_link_blocks_release_adoption() {
    let f = upgrades(true);
    let alias = "BepInEx/plugins/another-package/shared-data";
    let path = f.roots.game_root.join(alias);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::hard_link(f.roots.game_root.join(WEB_RESOURCE_6), &path).unwrap();
    let manifest = archive::new_manifest(
        "missing-plugin",
        "1",
        BTreeMap::new(),
        BTreeMap::new(),
        vec![alias.into()],
        vec![],
    );
    archive::write_manifest(&f.roots.data_root.join("manifests"), &manifest).unwrap();

    assert_resource_rejected(&f, WEB_RESOURCE_6);
}

#[test]
fn a_hard_link_to_a_map_file_without_a_manifest_claim_blocks_release_adoption() {
    let f = upgrades(true);
    let map_file = f.roots.game_root.join(OLD_MAP).with_file_name("map-data");
    fs::hard_link(f.roots.game_root.join(WEB_RESOURCE_6), &map_file).unwrap();
    assert_resource_rejected(&f, WEB_RESOURCE_6);
}

#[test]
fn release_files_can_share_a_directory_with_mixed_case_zip_paths() {
    let first = "plugins/OC2DIYLevelRuntimeWLoader/assets/a.png";
    let second = "plugins/OC2DIYLevelRuntimeWLoader/ASSETS/b.png";
    let f = upgrades_with_extra(true, &[(first, b"first"), (second, b"second")]);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    for (name, bytes) in [(first, b"first".as_slice()), (second, b"second")] {
        let staged = changes
            .iter()
            .find_map(|change| match &change.mutation {
                Mutation::File(change)
                    if path_key(Path::new(&change.target.relative))
                        == path_key(&Path::new("BepInEx").join(name)) =>
                {
                    change.source.as_ref()
                }
                _ => None,
            })
            .expect("both ZIP entries become verified writes");
        assert_eq!(fs::read(&staged.path).unwrap(), bytes);
    }
}

#[test]
fn verified_asset_cannot_adopt_when_the_installed_repository_binding_differs() {
    let mut f = upgrades(true);
    let old = &mut f.session.trusted.files.get_mut("web").unwrap().manifests[0];
    old.repository_binding.as_mut().unwrap().owner = "other-owner".into();
    archive::write_manifest(&f.roots.data_root.join("manifests"), old).unwrap();
    assert_resource_rejected(&f, WEB_RESOURCE_5);
}

#[test]
fn verified_asset_cannot_adopt_a_file_in_a_prefix_matching_sibling_folder() {
    let f = upgrades_with_extra(
        true,
        &[(
            "plugins/OC2DIYLevelRuntimeWLoaderArchive/commonW6",
            b"loose",
        )],
    );
    let unrelated = "BepInEx/plugins/OC2DIYLevelRuntimeWLoaderArchive/commonW6";
    write(&f.roots.game_root, unrelated, b"loose");
    assert_resource_rejected(&f, unrelated);
}

#[test]
fn verified_regular_file_cannot_replace_a_directory_at_the_resource_path() {
    let f = upgrades(true);
    let path = f.roots.game_root.join(WEB_RESOURCE_6);
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    write(&path, "preserve", b"user data");
    let before = fingerprint(&f);
    let result = assemble(&f.session, f.roots.clone());
    assert!(
        result.is_err(),
        "a directory is not an adoptable regular file"
    );
    assert_eq!(fingerprint(&f), before);
}

#[test]
fn matching_live_and_staged_edits_cannot_impersonate_the_verified_release() {
    let f = upgrades(true);
    let changed = b"new-runtime-X";
    write(&f.roots.game_root, WEB_RESOURCE_6, changed);
    write(
        &f.session.payloads["web"].stage_root,
        WEB_RESOURCE_6,
        changed,
    );
    let before = fingerprint(&f);
    let result = assemble(&f.session, f.roots.clone());
    assert!(
        result.is_err(),
        "matching copies cannot prove these bytes came from the ZIP"
    );
    assert_eq!(fingerprint(&f), before);
}

#[test]
fn adopted_file_changed_after_assembly_blocks_the_transaction() {
    let f = upgrades(true);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    write(&f.roots.game_root, WEB_RESOURCE_6, b"new-runtime-X");
    let edited = fingerprint(&f);
    let outcome = transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    );
    assert_eq!(outcome.err().as_deref(), Some("recycle.payloadModified"));
    assert_eq!(fingerprint(&f), edited);
}

#[test]
fn adopted_file_linked_after_assembly_blocks_the_transaction() {
    let f = upgrades(true);
    let before = fingerprint(&f);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    let alias = f.roots.data_root.join("linked-runtime");
    fs::hard_link(f.roots.game_root.join(WEB_RESOURCE_6), &alias).unwrap();
    let journal = f.roots.data_root.join("transactions");
    let outcome = transaction::prepare(
        &journal,
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    );
    assert_eq!(outcome.err().as_deref(), Some("recycle.payloadModified"));
    assert_eq!(fingerprint(&f), before);
    assert_eq!(fs::read(alias).unwrap(), b"new-runtime-6");
    assert!(!transaction::has_pending(&journal).unwrap());
}

#[test]
fn a_link_added_during_apply_rolls_back_previous_writes() {
    let f = upgrades(true);
    let before = fingerprint(&f);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    let alias = f.roots.data_root.join("linked-runtime");
    let upgraded_dll = mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.11.1");
    let linked = Cell::new(false);
    let journal = f.roots.data_root.join("transactions");
    let outcome = transaction::prepare(
        &journal,
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply_with_cancel(
        || {
            if !linked.get()
                && fs::read(f.roots.game_root.join(FW_DLL)).is_ok_and(|bytes| bytes == upgraded_dll)
            {
                fs::hard_link(f.roots.game_root.join(WEB_RESOURCE_6), &alias).unwrap();
                linked.set(true);
            }
            false
        },
        |_| {},
    );
    assert!(linked.get(), "the link must be added after a real write");
    assert_eq!(outcome.unwrap_err(), "recycle.payloadModified");
    assert_eq!(fingerprint(&f), before);
    assert_eq!(fs::read(alias).unwrap(), b"new-runtime-6");
    assert!(!transaction::has_pending(&journal).unwrap());
}

#[test]
fn staged_source_changed_after_assembly_blocks_the_transaction() {
    let f = upgrades(true);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    write(
        &f.session.payloads["web"].stage_root,
        WEB_RESOURCE_6,
        b"new-runtime-X",
    );
    let before = fingerprint(&f);
    let outcome = transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    );
    assert_eq!(outcome.err().as_deref(), Some("recycle.payloadModified"));
    assert_eq!(fingerprint(&f), before);
}

#[test]
fn cancelling_after_real_upgrade_writes_restores_both_packages_and_loose_files() {
    // Leave a later write pending so cancellation can observe both upgrades already on disk.
    let f = upgrades_with_extra(
        true,
        &[("plugins/OC2DIYLevelRuntimeWLoader/zz-last", b"last")],
    );
    let before = fingerprint(&f);
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    let saw_writes = Cell::new(false);
    let upgraded_dll = mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.11.1");
    let upgraded_web = mods::plugin_blob(WEB_GUID, "Web", "4.0.0");
    let outcome = transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes.into_iter().map(|owned| owned.mutation).collect(),
    )
    .unwrap()
    .apply_with_cancel(
        || {
            let wrote = fs::read(f.roots.game_root.join(FW_DLL))
                .is_ok_and(|bytes| bytes == upgraded_dll)
                && fs::read(f.roots.game_root.join(WEB_DLL))
                    .is_ok_and(|bytes| bytes == upgraded_web)
                && fs::read(f.roots.game_root.join(WEB_RESOURCE_4))
                    .is_ok_and(|bytes| bytes == b"new-runtime-4")
                && f.roots.game_root.join(NEW_MAP).exists()
                && manifest(&f, "framework").release_tag == "v0.11.1"
                && manifest(&f, "web").release_tag == "v4.0.0";
            saw_writes.set(wrote);
            wrote
        },
        |_| {},
    );
    assert_eq!(outcome.unwrap_err(), "download.cancelled");
    assert!(
        saw_writes.get(),
        "cancel after observing both upgraded DLLs, both manifests and the new map"
    );
    assert_eq!(fingerprint(&f), before);
}
