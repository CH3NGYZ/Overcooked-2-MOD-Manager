use super::*;
use std::fs;

/// 配置里历史写法的路径（结尾斜杠、`.`/`..` 段）与预期根收敛到同一字符串，
/// 输出给前端时再统一成 display_path 形态（无 verbatim 前缀）。
#[test]
fn configured_path_variants_converge_on_one_game_root() {
    let fixture = std::env::temp_dir().join(format!("oc2-path-variants-{}", uuid::Uuid::new_v4()));
    let root = fixture.join("Overcooked! 2");
    fs::create_dir_all(root.join("sub")).unwrap();
    fs::write(root.join("Overcooked2.exe"), b"fixture").unwrap();
    let canonical = fs::canonicalize(&root).unwrap();
    let expected = display_path(&canonical);
    for variant in [
        root.to_string_lossy().into_owned(),
        format!("{}/", root.to_string_lossy()),
        format!("{}/sub/..", root.to_string_lossy()),
    ] {
        let config = GameConfig {
            path: variant.clone(),
            runtime: crate::models::GameRuntime::Wine,
        };
        let detected = find_game_path(&config).expect(&variant);
        assert_eq!(detected.game, canonical, "{variant}");
        let info = detect_game_from_settings(&config);
        assert_eq!(info.path.as_deref(), Some(expected.as_str()), "{variant}");
        assert!(
            !info
                .path
                .as_deref()
                .unwrap_or_default()
                .starts_with(r"\\?\")
        );
    }
    fs::remove_dir_all(fixture).unwrap();
}
