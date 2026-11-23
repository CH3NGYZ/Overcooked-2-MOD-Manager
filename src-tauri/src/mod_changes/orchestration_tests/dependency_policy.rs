use super::super::execution;
use super::*;
use crate::archive::release_download::tests::{Server, reuse_tests::served};
use std::io::Write;

const DIY: &str = "DIYLevel";
const DIY_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const WEB: &str = "WebRuntime";
const WEB_GUID: &str = mods::plugin_target::RUNTIME_WLOADER_GUID;
const MOD: &str = "ExplicitMod";
const MOD_GUID: &str = "fixture.explicit-mod";
const HELPER: &str = "Helper";
const HELPER_GUID: &str = "fixture-helper";
const MAP: &str = "FixtureMap";
const MAP_ARCHIVE_STEM: &str = "fixture-map-package";
const AUTHOR: &str = "FixtureAuthor";
const PE: &[u8] = include_bytes!("duplicate_release_target/fastinit_version.pe");

fn release_dll(guid: &str, name: &str, version: &str) -> Vec<u8> {
    let mut bytes = PE.to_vec();
    bytes.extend(mods::plugin_blob(guid, name, version));
    bytes
}

fn sources(server: &Server, downloads: &[Vec<u8>]) -> Vec<ModRepositoryConfig> {
    let mut diy = repository(DIY, DIY_GUID, &["0.10.0", "0.11.1"], vec![]);
    diy.cached_releases[1].assets = vec![served(server, 0, "DIYLevel.dll", &downloads[0])];
    let mut web = repository(
        WEB,
        WEB_GUID,
        &["3.10.2", "4.0.0"],
        vec![requires(DIY_GUID, ">=0.10.0,<0.11.0")],
    );
    web.release_relations = vec![ReleaseRelationOverride {
        version: ">=4.0.0".into(),
        relations: vec![requires(DIY_GUID, ">=0.11.0,<0.12.0")],
    }];
    web.cached_releases[1].assets = vec![served(server, 1, "WebRuntime.dll", &downloads[1])];
    let mut root = repository(
        MOD,
        MOD_GUID,
        &["1.0.0", "2.0.0"],
        vec![requires(WEB_GUID, ">=3.8.8")],
    );
    root.cached_releases[0].assets = vec![served(server, 2, "ExplicitMod.dll", &downloads[2])];
    vec![diy, web, root]
}

fn map_request(fixture: &Fixture, online: bool) -> PlanRequest {
    let path = fixture
        .game
        .parent()
        .unwrap()
        .join(format!("{MAP_ARCHIVE_STEM}.zip"));
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, bytes) in [
        ("info", live_game_info::bundle(MAP, AUTHOR, "2.24")),
        ("requires.txt", b"3.8.8".to_vec()),
        ("scene", b"new map".to_vec()),
    ] {
        zip.start_file(
            format!("{MAP}/{name}"),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();
    PlanRequest {
        map_import: Some(MapImport {
            zip_path: path.to_string_lossy().into_owned(),
            download_id: None,
            selected: vec![MAP_ARCHIVE_STEM.into()],
            origin: online.then(|| MapInstallOrigin {
                repository_id: Some("fixture-map-source".into()),
                level_set_name: MAP.into(),
                level_set_name_zh: String::new(),
                author: AUTHOR.into(),
                mapset_version: "v2.24".into(),
            }),
        }),
        ..Default::default()
    }
}

#[tokio::test]
async fn online_mod_update_backtracks_ordinary_dependencies_to_keep_installed_runtimes() {
    let mut downloads = vec![
        release_dll(DIY_GUID, DIY, "0.11.1"),
        release_dll(WEB_GUID, WEB, "4.0.0"),
        release_dll(MOD_GUID, MOD, "1.0.0"),
    ];
    downloads.extend([
        release_dll(MOD_GUID, MOD, "2.0.0"),
        release_dll(HELPER_GUID, HELPER, "2.0.0"),
        release_dll(HELPER_GUID, HELPER, "3.0.0"),
    ]);
    let server = Server::new(downloads.clone(), false);
    let mut repos = sources(&server, &downloads);
    repos[2].cached_releases[1].assets = vec![served(&server, 3, "ExplicitMod.dll", &downloads[3])];
    repos[2].relations.push(requires(HELPER_GUID, ">=1.0.0"));
    let mut helper = repository(
        HELPER,
        HELPER_GUID,
        &["1.0.0", "2.0.0", "3.0.0"],
        vec![requires(WEB_GUID, ">=3.8.8")],
    );
    helper.release_relations = vec![ReleaseRelationOverride {
        version: ">=3.0.0".into(),
        relations: vec![requires(WEB_GUID, ">=4.0.0")],
    }];
    helper.cached_releases[1].assets = vec![served(&server, 4, "Helper.dll", &downloads[4])];
    helper.cached_releases[2].assets = vec![served(&server, 5, "Helper.dll", &downloads[5])];
    repos.push(helper);
    let fixture = fixture(repos);
    write(&fixture.game.join("BepInEx/core/BepInEx.dll"), PE);
    let diy = fixture
        .game
        .join(install(&fixture, DIY, DIY_GUID, "0.10.0", true));
    let web = fixture
        .game
        .join(install(&fixture, WEB, WEB_GUID, "3.10.2", true));
    let root = fixture
        .game
        .join(install(&fixture, MOD, MOD_GUID, "1.0.0", true));
    let helper = fixture
        .game
        .join(install(&fixture, HELPER, HELPER_GUID, "1.0.0", true));
    let diy_before = fs::read(&diy).unwrap();
    let web_before = fs::read(&web).unwrap();

    apply(&fixture, request(MOD, Some("v2.0.0"), true), |plan| {
        assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
        assert_eq!(plan.result.actions.len(), 2, "{:?}", plan.result.actions);
        for id in ["mod:ExplicitMod", "mod:Helper"] {
            assert!(
                plan.result.actions.iter().any(|action| {
                    action.node_id == id
                        && action.kind == ActionKind::Upgrade
                        && action.to_tag.as_deref() == Some("v2.0.0")
                }),
                "{:?}",
                plan.result.actions
            );
        }
    })
    .await;
    assert_eq!(server.request_count(), 2);
    assert_eq!(fs::read(diy).unwrap(), diy_before);
    assert_eq!(fs::read(web).unwrap(), web_before);
    assert_eq!(fs::read(root).unwrap(), downloads[3]);
    assert_eq!(fs::read(helper).unwrap(), downloads[4]);
}

#[tokio::test]
async fn enabling_mod_reuses_disabled_runtimes_and_explicit_web_upgrade_still_works() {
    let downloads = vec![
        release_dll(DIY_GUID, DIY, "0.11.1"),
        release_dll(WEB_GUID, WEB, "4.0.0"),
        release_dll(MOD_GUID, MOD, "1.0.0"),
    ];
    let server = Server::new(downloads.clone(), false);
    let fixture = fixture(sources(&server, &downloads));
    write(&fixture.game.join("BepInEx/core/BepInEx.dll"), PE);
    let diy = install(&fixture, DIY, DIY_GUID, "0.10.0", false);
    let web = install(&fixture, WEB, WEB_GUID, "3.10.2", false);
    let root = install(&fixture, MOD, MOD_GUID, "1.0.0", false);
    let diy_before = fs::read(fixture.game.join(mods::disabled_dll_file(&diy))).unwrap();
    let web_before = fs::read(fixture.game.join(mods::disabled_dll_file(&web))).unwrap();

    apply(&fixture, request(MOD, None, true), |plan| {
        assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
        assert_eq!(plan.result.actions.len(), 3, "{:?}", plan.result.actions);
        assert!(plan.result.actions.iter().all(|action| {
            action.kind == ActionKind::Enable && action.from_candidate_id == action.to_candidate_id
        }));
    })
    .await;
    assert_eq!(server.request_count(), 0);
    assert!(fixture.game.join(root).is_file());
    assert_eq!(fs::read(fixture.game.join(&diy)).unwrap(), diy_before);
    assert_eq!(fs::read(fixture.game.join(&web)).unwrap(), web_before);

    apply(&fixture, request(WEB, Some("v4.0.0"), true), |plan| {
        assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
        assert_eq!(plan.result.actions.len(), 2, "{:?}", plan.result.actions);
        for (id, version) in [("mod:DIYLevel", "v0.11.1"), ("mod:WebRuntime", "v4.0.0")] {
            assert!(plan.result.actions.iter().any(|action| {
                action.node_id == id
                    && action.kind == ActionKind::Upgrade
                    && action.to_tag.as_deref() == Some(version)
            }));
        }
    })
    .await;
    assert_eq!(server.request_count(), 2);
    assert_eq!(fs::read(fixture.game.join(diy)).unwrap(), downloads[0]);
    assert_eq!(fs::read(fixture.game.join(web)).unwrap(), downloads[1]);
}

fn mod_request(fixture: &Fixture, online: bool, dll: &[u8]) -> PlanRequest {
    if online {
        return request(MOD, Some("v1.0.0"), true);
    }
    let id = uuid::Uuid::new_v4();
    let path = crate::mod_preview::staging::preview_root(&fixture.state)
        .join(id.to_string())
        .join("ExplicitMod.zip");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file(
        "ExplicitMod/ExplicitMod.dll",
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    zip.write_all(dll).unwrap();
    zip.finish().unwrap();
    PlanRequest {
        roots: vec![RootRequest {
            intent: RootIntent {
                node_id: format!("preview:{id}"),
                candidate_id: None,
                enabled: true,
                member_key: None,
            },
            asset_name: None,
        }],
        ..Default::default()
    }
}

async fn apply(
    fixture: &Fixture,
    request: PlanRequest,
    assert_plan: impl Fn(&ChangePlan),
) -> PlanSession {
    let before = Fingerprint::capture(&fixture.game).unwrap();
    let plan = planning::create(&fixture.state, request).unwrap();
    assert_plan(&plan);
    let outcome = preparation::prepare(fixture.state.clone(), plan.plan_id.clone(), |_| {}).await;
    assert_eq!(outcome.error, None);
    let prepared = outcome.plan.unwrap();
    assert!(prepared.prepared);
    assert_plan(&prepared);
    assert_eq!(Fingerprint::capture(&fixture.game).unwrap(), before);
    let mut session = take(fixture, &prepared);
    execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
    session
}

#[tokio::test]
async fn local_and_online_map_and_mod_installs_keep_the_old_web_chain() {
    let downloads = vec![
        release_dll(DIY_GUID, DIY, "0.11.1"),
        release_dll(WEB_GUID, WEB, "4.0.0"),
        release_dll(MOD_GUID, MOD, "1.0.0"),
    ];
    for online in [false, true] {
        let server = Server::new(downloads.clone(), false);
        let fixture = fixture(sources(&server, &downloads));
        write(&fixture.game.join("BepInEx/core/BepInEx.dll"), PE);
        let diy = fixture
            .game
            .join(install(&fixture, DIY, DIY_GUID, "0.10.0", true));
        let web = fixture
            .game
            .join(install(&fixture, WEB, WEB_GUID, "3.10.2", true));
        let mut config = fixture.state.config.load_or_default();
        config.map_repositories = vec![
            serde_json::from_value(serde_json::json!({
                "id":"fixture-map-source", "level_set_name":MAP, "author":AUTHOR,
                "relations":[requires(DIY_GUID, ">=0.10.0"), requires(WEB_GUID, ">=3.8.8")],
            }))
            .unwrap(),
        ];
        fixture.state.config.save(&config).unwrap();
        let maps = diy.parent().unwrap().join("levels");
        let old_map = maps.join(MAP);
        write(
            &old_map.join("info"),
            &live_game_info::bundle(MAP, AUTHOR, "2.21"),
        );
        let diy_before = fs::read(&diy).unwrap();
        let web_before = fs::read(&web).unwrap();

        apply(&fixture, map_request(&fixture, online), |plan| {
            assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
            let [action] = plan.result.actions.as_slice() else {
                panic!("{:?}", plan.result.actions)
            };
            assert_eq!(action.kind, ActionKind::Upgrade);
            assert_eq!(action.node_id, format!("map:{MAP}"));
        })
        .await;
        assert_eq!(fs::read(old_map.join("scene")).unwrap(), b"new map");
        assert!(!maps.join(MAP_ARCHIVE_STEM).exists());
        assert_eq!(fs::read(&diy).unwrap(), diy_before);
        assert_eq!(fs::read(&web).unwrap(), web_before);
        assert_eq!(server.request_count(), 0);

        let request = mod_request(&fixture, online, &downloads[2]);
        let root = request.roots[0].intent.node_id.clone();
        apply(&fixture, request, |plan| {
            assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
            let [action] = plan.result.actions.as_slice() else {
                panic!(
                    "online={online}: expected only the requested MOD: {:?}",
                    plan.result.actions
                );
            };
            assert_eq!(action.kind, ActionKind::Install);
            assert_eq!(action.node_id, root);
            assert_eq!(
                action
                    .to_tag
                    .as_deref()
                    .map(|tag| tag.trim_start_matches('v')),
                Some("1.0.0")
            );
        })
        .await;
        assert_eq!(server.request_count(), usize::from(online));
        assert_eq!(
            fs::read(
                fixture
                    .game
                    .join("BepInEx/plugins/ExplicitMod/ExplicitMod.dll")
            )
            .unwrap(),
            downloads[2]
        );
        assert_eq!(fs::read(diy).unwrap(), diy_before);
        assert_eq!(fs::read(web).unwrap(), web_before);
        assert_eq!(fs::read(old_map.join("scene")).unwrap(), b"new map");
    }
}

#[tokio::test]
async fn disabled_map_update_does_not_require_diy_level_or_move_the_map() {
    for online in [false, true] {
        let fixture = fixture(vec![]);
        let mut config = fixture.state.config.load_or_default();
        config.map_repositories = vec![
            serde_json::from_value(serde_json::json!({
                "id": "fixture-map-source", "level_set_name": MAP, "author": AUTHOR,
                "relations": [requires(DIY_GUID, ">=0.10.0"), requires(WEB_GUID, ">=3.8.8")]
            }))
            .unwrap(),
        ];
        fixture.state.config.save(&config).unwrap();
        let map = crate::level_sets::disabled_root(&fixture.game).join(MAP);
        write(
            &map.join("info"),
            &live_game_info::bundle(MAP, AUTHOR, "2.21"),
        );
        write(&map.join("old-scene"), b"old map");

        apply(&fixture, map_request(&fixture, online), |plan| {
            assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
            let [action] = plan.result.actions.as_slice() else {
                panic!("{:?}", plan.result.actions);
            };
            assert_eq!(action.kind, ActionKind::Upgrade);
            assert_eq!(action.node_id, format!("map:{MAP}"));
            assert_eq!(action.from_tag.as_deref(), Some("2.21"));
            assert_eq!(action.to_tag.as_deref(), Some("2.24"));
            assert!(
                plan.nodes
                    .iter()
                    .any(|node| node.node_id == action.node_id && !node.enabled)
            );
        })
        .await;

        assert_eq!(fs::read(map.join("scene")).unwrap(), b"new map");
        assert!(!map.join("old-scene").exists());
        assert!(!map.with_file_name(MAP_ARCHIVE_STEM).exists());
        assert!(!fixture.game.join(crate::framework::DIYLEVEL_DIR).exists());

        let enable = planning::create(
            &fixture.state,
            PlanRequest {
                roots: vec![RootRequest {
                    intent: RootIntent {
                        node_id: format!("map:{MAP}"),
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
        assert_ne!(enable.result.status, SolveStatus::Ready);
        assert!(map.join("scene").is_file());
    }
}

#[tokio::test]
async fn disabled_map_updates_active_external_without_fixing_existing_missing_preconditions() {
    let fixture = fixture(vec![repository(
        MOD,
        MOD_GUID,
        &["1.0.0", "2.0.0"],
        vec![requires(WEB_GUID, ">=3.8.8")],
    )]);
    write(&fixture.game.join("BepInEx/core/BepInEx.dll"), PE);
    let map = crate::level_sets::disabled_root(&fixture.game).join(MAP);
    let external = format!("BepInEx/plugins/{MOD}/{MOD}.dll");
    let previous = release_dll(MOD_GUID, MOD, "1.0.0");
    let updated = release_dll(MOD_GUID, MOD, "2.0.0");
    write(&fixture.game.join(&external), &previous);
    write(
        &map.join("info"),
        &live_game_info::bundle(MAP, AUTHOR, "2.21"),
    );
    write(&map.join("old-scene"), b"old map");
    write(
        &fixture
            .state
            .config
            .manifests_dir()
            .join(crate::level_sets::LEVEL_SET_PLUGIN_REGISTRY),
        &serde_json::to_vec(&serde_json::json!({
            "version": 1,
            "entries": [{"dll": external, "owner": {"name": MAP, "author": AUTHOR}}]
        }))
        .unwrap(),
    );

    let zip_path = fixture.game.parent().unwrap().join("layer-with-plugin.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&zip_path).unwrap());
    for (path, bytes) in [
        (
            format!("BepInEx/plugins/OC2DIYLevel/levels/{MAP}/info"),
            live_game_info::bundle(MAP, AUTHOR, "2.24"),
        ),
        (
            format!("BepInEx/plugins/OC2DIYLevel/levels/{MAP}/scene"),
            b"new map".to_vec(),
        ),
        (external.clone(), updated.clone()),
    ] {
        zip.start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&bytes).unwrap();
    }
    zip.finish().unwrap();

    apply(
        &fixture,
        PlanRequest {
            map_import: Some(MapImport {
                zip_path: zip_path.to_string_lossy().into_owned(),
                download_id: None,
                selected: vec![MAP.into()],
                origin: None,
            }),
            ..Default::default()
        },
        |plan| {
            assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
            assert_eq!(plan.result.actions.len(), 2, "{:?}", plan.result.actions);
            assert!(
                plan.result
                    .actions
                    .iter()
                    .all(|action| matches!(action.kind, ActionKind::Upgrade | ActionKind::Replace))
            );
            assert!(
                plan.nodes
                    .iter()
                    .any(|node| { node.node_id.starts_with("map-dll:") && node.enabled })
            );
        },
    )
    .await;
    assert_eq!(fs::read(map.join("scene")).unwrap(), b"new map");
    assert!(!map.join("old-scene").exists());
    assert_eq!(fs::read(fixture.game.join(&external)).unwrap(), updated);
    assert!(
        !fixture
            .game
            .join(mods::disabled_dll_file(&external))
            .exists()
    );
    assert!(!fixture.game.join(crate::framework::DIYLEVEL_DIR).exists());
}
