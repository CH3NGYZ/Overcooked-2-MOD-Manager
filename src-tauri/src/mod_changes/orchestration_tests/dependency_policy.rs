use super::super::{execution, solve::DependencyPolicy};
use super::*;
use crate::archive::release_download::tests::{Server, reuse_tests::served};
use std::io::Write;

const DIY: &str = "DIYLevel";
const DIY_GUID: &str = mods::plugin_target::DIYLEVEL_GUID;
const WEB: &str = "WebRuntime";
const WEB_GUID: &str = "oc2.oc2diylevelruntimewloader";
const MOD: &str = "ExplicitMod";
const MOD_GUID: &str = "fixture.explicit-mod";
const MAP: &str = "FixtureMap";
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
    let path = fixture.game.parent().unwrap().join("map.zip");
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
            selected: vec![MAP.into()],
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

fn mod_request(fixture: &Fixture, online: bool) -> PlanRequest {
    if online {
        return request(MOD, Some("v1.0.0"), true);
    }
    let id = uuid::Uuid::new_v4();
    let path = crate::mod_preview::staging::preview_root(&fixture.state)
        .join(id.to_string())
        .join("ExplicitMod.dll");
    write(&path, &mods::plugin_blob(MOD_GUID, MOD, "1.0.0"));
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
async fn local_and_online_map_keep_the_old_web_chain_but_mod_install_updates_it() {
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

        let map = apply(&fixture, map_request(&fixture, online), |plan| {
            assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
            let [action] = plan.result.actions.as_slice() else {
                panic!("{:?}", plan.result.actions)
            };
            assert_eq!(action.kind, ActionKind::Upgrade);
            assert_eq!(action.node_id, format!("map:{MAP}"));
        })
        .await;
        assert_eq!(map.dependency_policy, DependencyPolicy::MapInstall);
        assert_eq!(fs::read(old_map.join("scene")).unwrap(), b"new map");
        assert_eq!(fs::read(&diy).unwrap(), diy_before);
        assert_eq!(fs::read(&web).unwrap(), web_before);
        assert_eq!(server.request_count(), 0);

        let installed = apply(&fixture, mod_request(&fixture, online), |plan| {
            assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
            assert_eq!(plan.result.actions.len(), 3, "{:?}", plan.result.actions);
            for (id, tag) in [("mod:DIYLevel", "v0.11.1"), ("mod:WebRuntime", "v4.0.0")] {
                assert!(
                    plan.result.actions.iter().any(|action| action.node_id == id
                        && action.kind == ActionKind::Upgrade
                        && action.to_tag.as_deref() == Some(tag)),
                    "{:?}",
                    plan.result.actions
                );
            }
            assert!(plan.result.actions.iter().any(|action| {
                action.kind == ActionKind::Install
                    && action
                        .to_tag
                        .as_deref()
                        .is_some_and(|tag| tag.trim_start_matches('v') == "1.0.0")
            }));
        })
        .await;
        assert_eq!(installed.dependency_policy, DependencyPolicy::PreferNewest);
        assert_eq!(server.request_count(), if online { 3 } else { 2 });
        assert_eq!(fs::read(diy).unwrap(), downloads[0]);
        assert_eq!(fs::read(web).unwrap(), downloads[1]);
        assert_eq!(fs::read(old_map.join("scene")).unwrap(), b"new map");
    }
}
