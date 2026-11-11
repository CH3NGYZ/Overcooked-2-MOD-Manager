use super::*;
use std::path::PathBuf;

#[test]
fn scene_fingerprint_is_stable_across_sessions_and_tracks_selected_hat_files() {
    let root = std::env::temp_dir().join(format!("chef-scene-key-{}", uuid::Uuid::new_v4()));
    let chef = root.join("Resources/Fox");
    let hat = root.join("Resources/HATS/Test");
    std::fs::create_dir_all(&chef).unwrap();
    std::fs::create_dir_all(&hat).unwrap();
    std::fs::write(chef.join("arbitrary-unread.bin"), [0xff, 0, 0xff]).unwrap();
    std::fs::write(hat.join("Test.obj"), b"deliberately not a valid OBJ").unwrap();
    let context = crate::diychef::session::PreviewContext {
        session_id: "first".into(),
        game_root: root.clone(),
        chef_name: "Fox".into(),
        chef_directory: Some(chef),
        hat_value: "Test".into(),
        hat_directory: Some(hat.clone()),
    };
    let first = context_fingerprint(&context).unwrap();
    let mut reopened = context.clone();
    reopened.session_id = "another-session".into();
    assert_eq!(first, context_fingerprint(&reopened).unwrap());
    std::fs::write(hat.join("extra-not-rendered.txt"), b"x").unwrap();
    assert_ne!(first, context_fingerprint(&reopened).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn model_selection_keeps_body_and_target_hands_but_excludes_other_chefs_and_props() {
    use assembly::{IDENTITY, Node, Tree};
    use native::reader::Ref;
    let node = |name: &str, parent, active| Node {
        name: name.into(),
        parent,
        active,
        world: IDENTITY,
        renderer: Some(Ref { file: 0, id: 1 }),
    };
    let tree = Tree {
        nodes: vec![
            node("Prefab", None, false),
            node("Mesh", Some(0), true),
            node("Body", Some(1), true),
            node("Chef_Selected", Some(1), false),
            node("Hand_Open_L", Some(3), true),
            node("Tail", Some(3), true),
            node("Chef_Other", Some(1), true),
            node("Hand_Grip_R", Some(6), true),
            node("Knife", Some(1), true),
            node("Hat_Fancy", Some(1), true),
            node("Props", Some(0), true),
            node("m_Plate", Some(10), true),
            node("Eyes2_Blinks", Some(3), true),
            node("Hidden", Some(3), false),
            node("Accessory", Some(13), true),
            node("Wheelchair", Some(3), true),
            node("Accessories", Some(3), true),
        ],
        by_ref: Default::default(),
    };
    let selected: Vec<_> = tree
        .nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| native::model_node(&tree, *index, 3))
        .map(|(_, node)| node.name.as_str())
        .collect();
    assert_eq!(
        selected,
        [
            "Body",
            "Chef_Selected",
            "Hand_Open_L",
            "Tail",
            "Wheelchair",
            "Accessories"
        ]
    );
}

#[test]
#[ignore = "requires installed real DIYChef and game assets; never writes game data"]
fn real_chef_previews() {
    let game = PathBuf::from("E:/SteamLibrary/steamapps/common/Overcooked! 2");
    let root = game.join("BepInEx/plugins/OC2DIYChef-FastInit");
    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/diychef-native-acceptance");
    std::fs::create_dir_all(&out).unwrap();
    for (name, hat, custom) in [
        ("00-Fox", "None", true),
        ("00-Fox", "Wizardhat", true),
        ("00-Fox", "Hardhat", true),
        ("Chef_Black_cat", "None", false),
        ("Chef_Black_cat", "Santa", false),
        ("Chef_Black_cat", "Fancy", false),
        ("Chef_Black_cat", "Baseballcap", false),
        ("Chef_Black_cat", "Wizardhat", false),
        ("Chef_alaskan_malamute", "None", false),
        ("Chef_Pig", "None", false),
        ("Chef_Racoon", "None", false),
        ("Chef_Ghost", "None", false),
    ] {
        let start = std::time::Instant::now();
        let context = crate::diychef::session::PreviewContext {
            session_id: "fixture".into(),
            game_root: game.clone(),
            chef_name: name.into(),
            chef_directory: custom.then(|| root.join("Resources").join(name)),
            hat_value: hat.into(),
            hat_directory: (["Wizardhat", "Hardhat"].contains(&hat))
                .then(|| root.join("Resources/HATS").join(hat)),
        };
        let first = context_fingerprint(&context).unwrap();
        let scene = cache::get(&context).unwrap();
        assert_eq!(first, scene.fingerprint);
        eprintln!(
            "{name} {hat}: status={} meshes={} bounds={:?} diagnostics={:?} elapsed={:?}",
            scene.status,
            scene.meshes.len(),
            scene.head_bounds,
            scene.diagnostics,
            start.elapsed()
        );
        assert_ne!(scene.status, "unavailable");
        assert!(
            scene.diagnostics.is_empty(),
            "{}",
            scene.diagnostics.join(",")
        );
        assert!(
            scene.meshes.iter().any(|mesh| mesh.name == "Body"),
            "{name} body"
        );
        assert!(
            scene
                .meshes
                .iter()
                .any(|mesh| mesh.name.starts_with("Hand_Open_R")),
            "{name} right hand"
        );
        assert!(
            scene
                .meshes
                .iter()
                .any(|mesh| mesh.name.starts_with("Hand_Open_L")),
            "{name} left hand"
        );
        if custom {
            assert!(scene.meshes.iter().any(|mesh| mesh.name == "Tail"));
            assert!(
                !scene
                    .meshes
                    .iter()
                    .any(|mesh| mesh.name.starts_with("Hand_Grip"))
            );
        }
        assert!(
            !scene
                .meshes
                .iter()
                .any(|mesh| mesh.name.starts_with("Props") || mesh.name.starts_with("Knife"))
        );
        if name == "Chef_Racoon" {
            assert!(
                scene
                    .meshes
                    .iter()
                    .any(|mesh| mesh.name.starts_with("Wheelchair")),
                "Racoon keeps its wheelchair"
            );
        }
        if name == "Chef_Ghost" {
            assert!(
                scene.meshes.iter().any(|mesh| mesh.name == "Accessories"),
                "Ghost keeps its chain"
            );
        }
        let body = scene
            .meshes
            .iter()
            .find(|mesh| mesh.name == "Body")
            .unwrap();
        assert_eq!(body.material_floats.get("_MaskColorR"), Some(&0.));
        assert!(body.material_floats["_MaskColorB"] > body.material_floats["_MaskColorG"]);
        let folder = out.join(format!("{name}-{hat}"));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            folder.join("scene.json"),
            serde_json::to_vec_pretty(&scene).unwrap(),
        )
        .unwrap();
        for mesh in &scene.meshes {
            for id in std::iter::once(&mesh.geometry_asset).chain(mesh.texture_asset.iter()) {
                std::fs::write(
                    folder.join(id.replace(':', "_")),
                    cache::read_asset("fixture", id).unwrap(),
                )
                .unwrap();
            }
        }
        let repeat = std::time::Instant::now();
        let second = cache::get(&context).unwrap();
        assert_eq!(scene.fingerprint, second.fingerprint);
        eprintln!("cache-hit {:?}", repeat.elapsed());
    }
}

#[test]
#[ignore = "requires installed Windows game resources; only reads game files"]
fn real_all_native_previews() {
    let game = PathBuf::from("E:/SteamLibrary/steamapps/common/Overcooked! 2");
    let started = std::time::Instant::now();
    let mut report = vec![];
    for name in crate::diychef::catalog::NATIVE_NAMES.lines() {
        let start = std::time::Instant::now();
        let result = native::native_scene(&game, name, "None");
        match result {
            Ok(scene)=>report.push(serde_json::json!({"name":name,"meshes":scene.parts.len(),"triangles":scene.parts.iter().map(|p|p.geometry.indices.len()/3).sum::<usize>(),"diagnostics":scene.diagnostics,"elapsed_ms":start.elapsed().as_millis()})),
            Err(error)=>report.push(serde_json::json!({"name":name,"error":error,"elapsed_ms":start.elapsed().as_millis()})),
        }
    }
    let ok = report
        .iter()
        .filter(|r| r.get("error").is_none() && r["diagnostics"].as_array().unwrap().is_empty())
        .count();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/diychef-native-acceptance");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("native-coverage.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    eprintln!(
        "NATIVE COVERAGE complete={ok}/{} elapsed={:?}",
        report.len(),
        started.elapsed()
    );
    for item in report.iter().filter(|r| {
        r.get("error").is_some() || !r["diagnostics"].as_array().is_some_and(Vec::is_empty)
    }) {
        eprintln!("{item}");
    }
    assert_eq!(ok, 61);
}
