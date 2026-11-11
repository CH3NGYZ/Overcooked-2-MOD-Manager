//! Read-only real-model profile; exports remain in a fresh UUID workspace folder.
use super::*;
use crate::{
    diychef::{
        preview::{self, cache, native, native_disk_cache},
        session::PreviewContext,
    },
    models::ChefPreviewScene,
};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fs, path::Path};

const CHEFS: [&str; 12] = [
    "Chef_Black_cat",
    "Chef_Beard",
    "Chef_Racoon",
    "Chef_Ghost",
    "Chef_Beaver",
    "Chef_Owl",
    "Chef_Box",
    "Chef_Wizard_Boy",
    "Chef_Mike",
    "Chef_Platypus",
    "Chef_Mermaid",
    "Chef_dragon",
];
const HAT: &str = "Fancy";
const SESSION: &str = "twelve-real-model-export";

fn context(game: &Path, name: &str) -> PreviewContext {
    PreviewContext {
        session_id: SESSION.into(),
        game_root: game.to_owned(),
        chef_name: name.into(),
        chef_directory: None,
        hat_value: HAT.into(),
        hat_directory: None,
    }
}

fn pool_profile() -> Value {
    let state = shared()
        .state
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    json!({
        "workers": state.workers, "active": state.active, "peak": state.peak,
        "seed": state.seed.as_ref().map(|store| store.memory_profile()),
        "idle": state.idle.as_ref().map(|store| store.memory_profile()),
    })
}

fn phase(root: &Path, name: &str, detail: Value) {
    eprintln!(
        "memory_profile={}",
        json!({
            "pid": std::process::id(), "phase": name,
            "output_directory": root, "pool": pool_profile(),
            "cache": cache::memory_profile(), "detail": detail,
        })
    );
    // Let the external process observer sample each stable ownership boundary.
    std::thread::sleep(Duration::from_millis(250));
}

fn cold_scene(game: &Path, root: &Path, name: &str, epoch: u64) -> ChefPreviewScene {
    let context = context(game, name);
    cache::get_or_prepare(&context, epoch, &root.join("diychef-assets"), || {
        native_disk_cache::scene_at(
            game,
            name,
            HAT,
            epoch,
            Ok(root.join("diychef-native")),
            || {
                native::with_store(game, epoch, |store| {
                    // Match the existing isolated native-disk real test: parts
                    // are baked normally, but only full scenes persist in this
                    // UUID directory; never touch the user's real part cache.
                    store.disk_stamp.clear();
                    let tree = native::tree(store, name)?;
                    let mut scene = native::model_parts(store, &tree, name)?;
                    scene.parts.extend(native::hat(store, &tree, HAT)?);
                    Ok(scene)
                })
            },
        )
    })
    .unwrap()
}

#[test]
#[ignore = "requires installed game; writes only fresh UUID .tmp model exports/caches"]
fn real_twelve_models_cache_transport_and_idle_memory_profile() {
    let _guard = preview::TEST_CACHE_ACCESS
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2")
        .canonicalize()
        .unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../.tmp")
        .join(format!("diychef-12-models-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let root = root.canonicalize().unwrap();
    let started = Instant::now();
    let epoch = preview::cache_epoch();
    preview::configure_concurrency(0);
    cache::clear();
    native::clear(epoch);
    phase(
        &root,
        "before_twelve_cold",
        json!({"chefs": CHEFS, "hat": HAT}),
    );

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let scenes = runtime.block_on(async {
        let mut jobs = tokio::task::JoinSet::new();
        for name in CHEFS {
            let (game, root) = (game.clone(), root.clone());
            jobs.spawn(async move {
                let permit = preview::scene_permit().await.unwrap();
                tokio::task::spawn_blocking(move || {
                    let _permit = permit;
                    let start = Instant::now();
                    let scene = cold_scene(&game, &root, name, epoch);
                    assert_eq!(scene.status, "complete", "{name}: {:?}", scene.diagnostics);
                    assert!(scene.diagnostics.is_empty());
                    (name, scene, start.elapsed().as_millis())
                })
                .await
                .unwrap()
            });
        }
        let mut scenes = BTreeMap::new();
        while let Some(result) = jobs.join_next().await {
            let (name, scene, elapsed) = result.unwrap();
            eprintln!(
                "model_complete={}",
                json!({"name": name, "elapsed_ms": elapsed})
            );
            scenes.insert(name, scene);
        }
        scenes
    });
    let cold_ms = started.elapsed().as_millis();
    phase(
        &root,
        "twelve_cold_complete",
        json!({"elapsed_ms": cold_ms}),
    );
    assert_eq!(scenes.len(), 12);
    assert_eq!(cache::memory_profile()["scene_entries"], 12);
    assert_eq!(cache::memory_profile()["retained_raw_asset_bytes"], 0);

    let export = root.join("export");
    fs::create_dir(&export).unwrap();
    let mut manifest = vec![];
    for (name, scene) in &scenes {
        let directory = export.join(name);
        fs::create_dir(&directory).unwrap();
        fs::write(
            directory.join("scene.json"),
            serde_json::to_vec_pretty(scene).unwrap(),
        )
        .unwrap();
        let mut assets = BTreeMap::new();
        let mut bytes = 0usize;
        for mesh in &scene.meshes {
            for id in std::iter::once(&mesh.geometry_asset).chain(mesh.texture_asset.iter()) {
                // Read only after all 12 descriptors return: other cold scenes
                // must not have evicted an earlier descriptor's raw resources.
                let asset = cache::read_asset(SESSION, id).unwrap();
                bytes += asset.len();
                let filename = format!("{}.bin", id.replace(':', "_"));
                fs::write(directory.join(&filename), &asset).unwrap();
                assets.insert(id, format!("export/{name}/{filename}"));
            }
        }
        let id = &scene.meshes[0].geometry_asset;
        assert!(cache::read_asset("unauthorized-editor", id).is_err());
        assert!(cache::read_asset(SESSION, &format!("{id}:suffix")).is_err());
        manifest.push(json!({
            "name": name, "chef_id": format!("native:{name}"), "hat_id": format!("hat:{HAT}"),
            "scene": format!("export/{name}/scene.json"), "assets": assets,
            "asset_bytes": bytes, "estimated_bytes": scene.estimated_bytes,
        }));
    }
    fs::write(root.join("manifest.json"), serde_json::to_vec_pretty(&json!({
        "game_root": game, "session": SESSION, "hat": HAT, "cold_ms": cold_ms,
        "models": manifest, "evidence": "real game files via production native/cache/transport; session authorization bypassed by test contexts",
    })).unwrap()).unwrap();
    phase(
        &root,
        "twelve_assets_exported",
        json!({"models": 12, "cold_ms": cold_ms}),
    );

    // Exercise the production reclaimer's actual ten-second inactivity deadline.
    std::thread::sleep(Duration::from_secs(11));
    phase(&root, "after_eleven_seconds_idle", Value::Null);
    assert!(shared().state.lock().unwrap().seed.is_none());
    assert!(shared().state.lock().unwrap().idle.is_none());
    cache::clear();
    let warm = Instant::now();
    for (name, original) in &scenes {
        let hit = cache::get_or_prepare(
            &context(&game, name),
            epoch,
            &root.join("diychef-assets"),
            || {
                native_disk_cache::scene_at(
                    &game,
                    name,
                    HAT,
                    epoch,
                    Ok(root.join("diychef-native")),
                    || panic!("warm native disk scene must not recreate a Store"),
                )
            },
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&hit).unwrap(),
            serde_json::to_value(original).unwrap()
        );
        assert!(cache::read_asset(SESSION, &hit.meshes[0].geometry_asset).is_ok());
    }
    phase(
        &root,
        "twelve_warm_disk_complete",
        json!({"elapsed_ms": warm.elapsed().as_millis()}),
    );
    assert!(shared().state.lock().unwrap().seed.is_none());
    cache::clear();
    phase(
        &root,
        "after_scene_index_clear",
        json!({"export_retained": true}),
    );
    assert_eq!(cache::memory_profile()["scene_entries"], 0);
}
