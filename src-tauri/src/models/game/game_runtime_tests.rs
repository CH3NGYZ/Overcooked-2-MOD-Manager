use super::*;

/// 旧配置里没有 `runtime` 字段时必须落到原生：macOS 新装用户默认就是
/// 原生框架，而缺字段说明这份配置来自更早的版本，不该反过来把它
/// 推进 CrossOver 分支（那会去 Wine 容器里找游戏与 BepInEx）。
#[test]
fn missing_and_fresh_game_settings_both_default_to_native() {
    let missing: GameConfig = serde_json::from_str(r#"{"path":""}"#).unwrap();
    assert_eq!(missing.runtime, GameRuntime::Native);
    // 旧配置里的 `auto_detect` 字段已删除：多出来的键被忽略，不影响迁移。
    let legacy: GameConfig = serde_json::from_str(r#"{"path":"","auto_detect":false}"#).unwrap();
    assert_eq!(legacy.runtime, GameRuntime::Native);
    let fresh = GameConfig {
        path: String::new(),
        runtime: GameRuntime::default(),
    };
    assert_eq!(fresh.runtime, GameRuntime::Native);
}

#[test]
fn switching_runtime_clears_the_selected_path() {
    let mut settings = GameConfig {
        path: "old Windows game directory".into(),
        runtime: GameRuntime::Wine,
    };
    settings.switch_runtime(GameRuntime::Wine);
    assert_eq!(settings.path, "old Windows game directory");
    settings.switch_runtime(GameRuntime::Native);
    assert!(settings.path.is_empty());
    let persisted = serde_json::to_string(&settings).unwrap();
    assert_eq!(
        serde_json::from_str::<GameConfig>(&persisted).unwrap(),
        settings
    );
    settings.path = "native game directory".into();
    settings.switch_runtime(GameRuntime::Wine);
    assert!(settings.path.is_empty());
}
