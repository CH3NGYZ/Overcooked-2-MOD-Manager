//! Real ZIP preview -> private install -> batch mutations, including DIYLevel's bundled maps.
use super::*;
use crate::{app_state::AppState, config::ConfigService, models::*};
use std::io::Write;

pub(super) fn zip_payload(
    f: &mut Fixture,
    id: &str,
    guid: &str,
    version: &str,
    entries: &[(&str, &[u8])],
) {
    let state = AppState::with_config(ConfigService::for_test(f.roots.data_root.clone()));
    let mut repo =
        crate::community::repository_from_coordinates(&Provider::Github, "Fixture", id, "", "")
            .unwrap();
    repo.id = id.into();
    repo.plugin_guid = guid.into();
    repo.install_target = "BepInEx/plugins".into();
    repo.asset_rules = vec![AssetRule {
        rule_type: AssetRuleType::Exact,
        value: format!("{id}.zip"),
    }];
    repo.cached_releases = vec![ReleaseInfo {
        release_id: None,
        tag_name: format!("v{version}"),
        version: Some(version.into()),
        name: id.into(),
        body: String::new(),
        published_at: None,
        api_prerelease: Some(false),
        prerelease: false,
        draft: false,
        assets: vec![ReleaseAsset {
            name: format!("{id}.zip"),
            size: 0,
            download_count: None,
            download_url: format!("https://example.invalid/{id}.zip"),
            digest: None,
        }],
    }];
    f.session.trusted.config.repositories.push(repo);
    let preview_id = uuid::Uuid::new_v4().to_string();
    let preview = crate::mod_preview::staging::preview_root(&state).join(&preview_id);
    fs::create_dir_all(&preview).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(preview.join(format!("{id}.zip"))).unwrap());
    for (path, bytes) in entries {
        zip.start_file(*path, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
    fs::write(
        preview.join(crate::mod_preview::staging::ORIGIN_FILE),
        serde_json::to_vec(&crate::mod_preview::staging::PreviewOriginRecord {
            repository_id: id.into(),
            release_tag: format!("v{version}"),
            prerelease: false,
        })
        .unwrap(),
    )
    .unwrap();
    let mut selected = candidate(&format!("release:{id}:v{version}"), vec![]);
    selected.source_id = Some(id.into());
    selected.tag = Some(format!("v{version}"));
    let payload = crate::mod_changes::payload::from_preview(
        &state,
        &f.session.trusted,
        &preview_id,
        id,
        &selected,
        None,
        &f.session.staging.join(format!("verified-{id}")),
    )
    .unwrap();
    let node = f
        .session
        .solved
        .as_mut()
        .unwrap()
        .nodes
        .iter_mut()
        .find(|node| node.id == id)
        .unwrap();
    node.current = Some(payload.candidate.clone());
    f.session.payloads.insert(id.into(), payload);
}

pub(super) fn two_zip_replacements() -> Fixture {
    let mut f = fixture();
    let framework = "BepInEx/plugins/OC2DIYLevel/LevelEditorStub.dll";
    let bundled = "BepInEx/plugins/OC2DIYLevel/levels/oc1_story/info";
    let web = "BepInEx/plugins/OC2DIYLevelRuntimeWLoader/Loader.dll";
    write(
        &f.roots.game_root,
        framework,
        &mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.10.0"),
    );
    write(&f.roots.game_root, bundled, b"user-preserved-map");
    write(
        &f.roots.game_root,
        "BepInEx/plugins/OC2DIYLevel/levels/custom/user.txt",
        b"custom",
    );
    write(
        &f.roots.game_root,
        web,
        &mods::plugin_blob("fixture-web", "Web", "4.0.0"),
    );
    add_mod(
        &mut f,
        "framework",
        vec![member(framework, true, mods::plugin_target::DIYLEVEL_GUID)],
        vec![framework.into(), bundled.into()],
    );
    add_mod(
        &mut f,
        "web",
        vec![member(web, true, "fixture-web")],
        vec![web.into()],
    );
    f.session.trusted.level_roots = crate::level_sets::LevelSetRoots::new(
        f.roots.game_root.clone(),
        Some(f.roots.game_root.join("BepInEx/plugins/OC2DIYLevel")),
    );
    let framework_bytes =
        mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.10.0");
    zip_payload(
        &mut f,
        "framework",
        mods::plugin_target::DIYLEVEL_GUID,
        "0.10.0",
        &[
            ("OC2DIYLevel/LevelEditorStub.dll", &framework_bytes),
            ("OC2DIYLevel/levels/oc1_story/info", b"shipped-old-map"),
            (
                "OC2DIYLevel/levels/oc1_story/new-file",
                b"must-not-partially-merge",
            ),
            ("OC2DIYLevel/levels/oc1_dlc/info", b"new-bundled-map"),
        ],
    );
    let web_bytes = mods::plugin_blob("fixture-web", "Web", "3.10.2");
    zip_payload(
        &mut f,
        "web",
        "fixture-web",
        "3.10.2",
        &[
            ("OC2DIYLevelRuntimeWLoader/Loader.dll", &web_bytes),
            ("OC2DIYLevelRuntimeWLoader/version.txt", b"3.10.2"),
        ],
    );
    f
}

#[test]
fn removing_only_bundled_levels_from_two_real_zips_avoids_collision() {
    let mut f = two_zip_replacements();
    f.session
        .payloads
        .get_mut("framework")
        .unwrap()
        .files
        .retain(|path| !path.replace('\\', "/").contains("/levels/"));
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    super::super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes,
    )
    .unwrap()
    .apply()
    .unwrap();
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join("BepInEx/plugins/OC2DIYLevelRuntimeWLoader/version.txt")
        )
        .unwrap(),
        b"3.10.2"
    );
}

#[test]
fn two_real_zips_replace_framework_with_bundled_maps_and_downgrade_web() {
    let f = two_zip_replacements();
    let before = Fingerprint::capture(&f.roots.game_root).unwrap();
    let changes = assemble(&f.session, f.roots.clone())
        .expect("bundled levels must not block two-package replacement");
    assert_eq!(Fingerprint::capture(&f.roots.game_root).unwrap(), before);
    super::super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes,
    )
    .unwrap()
    .apply()
    .unwrap();
    let levels = f.roots.game_root.join("BepInEx/plugins/OC2DIYLevel/levels");
    assert_eq!(
        fs::read(levels.join("oc1_story/info")).unwrap(),
        b"user-preserved-map"
    );
    assert!(!levels.join("oc1_story/new-file").exists());
    assert_eq!(
        fs::read(levels.join("oc1_dlc/info")).unwrap(),
        b"new-bundled-map"
    );
    assert_eq!(fs::read(levels.join("custom/user.txt")).unwrap(), b"custom");
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join("BepInEx/plugins/OC2DIYLevelRuntimeWLoader/version.txt")
        )
        .unwrap(),
        b"3.10.2"
    );
    let manifest: archive::InstallManifest = toml::from_str(
        &fs::read_to_string(f.roots.data_root.join("manifests/framework.toml")).unwrap(),
    )
    .unwrap();
    assert!(
        manifest
            .files
            .iter()
            .all(|path| !path.replace('\\', "/").contains("/levels/"))
    );
}

#[test]
fn bundled_maps_never_revive_disabled_map_directories() {
    let f = two_zip_replacements();
    let disabled = crate::level_sets::disabled_root(&f.roots.game_root).join("oc1_dlc/info");
    fs::create_dir_all(disabled.parent().unwrap()).unwrap();
    fs::write(&disabled, b"disabled-user-map").unwrap();
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    super::super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes,
    )
    .unwrap()
    .apply()
    .unwrap();
    assert_eq!(fs::read(disabled).unwrap(), b"disabled-user-map");
    assert!(
        !f.roots
            .game_root
            .join("BepInEx/plugins/OC2DIYLevel/levels/oc1_dlc")
            .exists()
    );
}

#[test]
fn bundled_maps_seed_first_framework_install_without_live_changes_during_prepare() {
    let mut f = fixture();
    add_mod(&mut f, "framework", vec![], vec![]);
    f.session.trusted.inventory.nodes.clear();
    f.session.trusted.files.clear();
    fs::remove_file(f.roots.data_root.join("manifests/framework.toml")).unwrap();
    let bytes = mods::plugin_blob(mods::plugin_target::DIYLEVEL_GUID, "DIYLevel", "0.10.0");
    zip_payload(
        &mut f,
        "framework",
        mods::plugin_target::DIYLEVEL_GUID,
        "0.10.0",
        &[
            ("OC2DIYLevel/LevelEditorStub.dll", &bytes),
            ("OC2DIYLevel/levels/oc1_story/info", b"first-map"),
        ],
    );
    let before = Fingerprint::capture(&f.roots.game_root).unwrap();
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    assert_eq!(Fingerprint::capture(&f.roots.game_root).unwrap(), before);
    super::super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes,
    )
    .unwrap()
    .apply()
    .unwrap();
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join("BepInEx/plugins/OC2DIYLevel/levels/oc1_story/info")
        )
        .unwrap(),
        b"first-map"
    );
}

#[test]
fn framework_migration_and_bundled_map_seed_share_one_tree_composition() {
    let mut f = two_zip_replacements();
    let old = "BepInEx/plugins/OldDIYLevel";
    let new = "BepInEx/plugins/OC2DIYLevel";
    fs::rename(f.roots.game_root.join(new), f.roots.game_root.join(old)).unwrap();
    let trusted = &mut f.session.trusted;
    trusted.level_roots = crate::level_sets::LevelSetRoots::new(
        f.roots.game_root.clone(),
        Some(f.roots.game_root.join(old)),
    );
    for member in &mut trusted.inventory.nodes[0]
        .current
        .as_mut()
        .unwrap()
        .provides
    {
        member.member_key = member.member_key.as_ref().map(|path| {
            path.replace("oc2diylevel/", "olddiylevel/")
                .replace("OC2DIYLevel/", "OldDIYLevel/")
        });
    }
    let node = trusted.files.get_mut("framework").unwrap();
    for path in &mut node.summary.dll_files {
        *path = path
            .replace("oc2diylevel/", "olddiylevel/")
            .replace("OC2DIYLevel/", "OldDIYLevel/");
    }
    for member in &mut node.summary.members {
        member.key = member
            .key
            .replace("oc2diylevel/", "olddiylevel/")
            .replace("OC2DIYLevel/", "OldDIYLevel/");
        member.dll_file = member
            .dll_file
            .replace("oc2diylevel/", "olddiylevel/")
            .replace("OC2DIYLevel/", "OldDIYLevel/");
    }
    for path in &mut node.manifests[0].files {
        *path = path.replace(new, old);
    }
    fs::write(
        f.roots.data_root.join("manifests/framework.toml"),
        toml::to_string(&node.manifests[0]).unwrap(),
    )
    .unwrap();
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    super::super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes,
    )
    .unwrap()
    .apply()
    .unwrap();
    assert!(!f.roots.game_root.join(format!("{old}/levels")).exists());
    assert_eq!(
        fs::read(
            f.roots
                .game_root
                .join(format!("{new}/levels/oc1_story/info"))
        )
        .unwrap(),
        b"user-preserved-map"
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(format!("{new}/levels/oc1_dlc/info"))).unwrap(),
        b"new-bundled-map"
    );
}

#[test]
fn cancellation_after_tree_and_package_writes_rolls_back_both_real_zips() {
    use std::cell::Cell;
    let f = two_zip_replacements();
    let before_game = Fingerprint::capture(&f.roots.game_root).unwrap();
    let before_manifests = Fingerprint::capture(&f.roots.data_root.join("manifests")).unwrap();
    let changes = assemble(&f.session, f.roots.clone()).unwrap();
    let calls = Cell::new(0);
    let observed_write = Cell::new(false);
    let outcome = super::super::super::transaction::prepare(
        &f.roots.data_root.join("transactions"),
        &f.roots,
        changes,
    )
    .unwrap()
    .apply_with_cancel(|| {
        calls.set(calls.get() + 1);
        if calls.get() == 5 {
            observed_write.set(
                f.roots
                    .game_root
                    .join("BepInEx/plugins/OC2DIYLevel/levels/oc1_dlc/info")
                    .exists(),
            );
            true
        } else {
            false
        }
    });
    assert_eq!(outcome.unwrap_err(), "download.cancelled");
    assert!(
        observed_write.get(),
        "cancel must occur after actual tree application"
    );
    assert_eq!(
        Fingerprint::capture(&f.roots.game_root).unwrap(),
        before_game
    );
    assert_eq!(
        Fingerprint::capture(&f.roots.data_root.join("manifests")).unwrap(),
        before_manifests
    );
}

#[test]
fn unrelated_package_cannot_write_framework_bundled_maps() {
    let mut f = two_zip_replacements();
    let payload = f.session.payloads.get_mut("web").unwrap();
    let path = "BepInEx/plugins/OC2DIYLevel/levels/oc1_story/info";
    write(&payload.stage_root, path, b"foreign-overwrite");
    payload.files.push(path.into());
    assert_eq!(
        assemble(&f.session, f.roots.clone()).unwrap_err(),
        "modChanges.pathCollision"
    );
    assert_eq!(
        fs::read(f.roots.game_root.join(path)).unwrap(),
        b"user-preserved-map"
    );
}
