use super::*;

fn temp_dir(label: &str) -> PathBuf {
    let path =
        std::env::temp_dir().join(format!("oc2-user-paths-{label}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&path).unwrap();
    path
}

/// 数据根必须在可执行文件之外：macOS 的可执行文件与数据目录同名，
/// 这份断言守的就是那次 `Not a directory` 启动崩溃。
#[test]
fn data_root_is_a_directory_beside_the_user_data_parent() {
    let root = root().unwrap();
    assert!(root.is_absolute());
    assert_eq!(root.file_name().unwrap(), DATA_DIRECTORY);
    let executable = std::env::current_exe().unwrap();
    assert_ne!(root, executable);
    assert!(!root.starts_with(executable.parent().unwrap()));
}

#[test]
fn data_directories_stay_inside_the_root() {
    let root = Path::new("/data/oc2").join(DATA_DIRECTORY);
    for sub in ["logs", "backups", "cache/updates"] {
        assert!(root.join(sub).starts_with(&root), "{sub}");
    }
}

/// 旧版本把数据放在安装目录里，升级后必须搬过来，否则用户的配置、
/// 安装清单和回收站会凭空消失。
#[test]
fn migrates_legacy_data_into_an_empty_target() {
    let base = temp_dir("migrate-empty");
    let legacy = base.join("install").join(DATA_DIRECTORY);
    let target = base.join("documents").join(DATA_DIRECTORY);
    fs::create_dir_all(legacy.join("logs")).unwrap();
    fs::write(legacy.join("config.toml"), "ui_zoom = 1.5").unwrap();
    fs::write(legacy.join("logs/rust.log"), "started").unwrap();

    assert!(migrate_between(&legacy, &target).unwrap());
    assert_eq!(
        fs::read_to_string(target.join("config.toml")).unwrap(),
        "ui_zoom = 1.5"
    );
    assert!(target.join("logs/rust.log").is_file());
    assert!(!legacy.exists(), "搬空的旧目录应被删掉");
    fs::remove_dir_all(base).unwrap();
}

/// 安装器会先在新数据根写安装语言标记，迁移不能把它盖掉。
#[test]
fn keeps_target_entries_and_still_moves_the_rest() {
    let base = temp_dir("migrate-merge");
    let legacy = base.join("install").join(DATA_DIRECTORY);
    let target = base.join("documents").join(DATA_DIRECTORY);
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::write(legacy.join("install-language.txt"), "en").unwrap();
    fs::write(legacy.join("config.toml"), "ui_zoom = 1.0").unwrap();
    fs::write(target.join("install-language.txt"), "zh-CN").unwrap();

    assert!(migrate_between(&legacy, &target).unwrap());
    assert_eq!(
        fs::read_to_string(target.join("install-language.txt")).unwrap(),
        "zh-CN"
    );
    assert!(target.join("config.toml").is_file());
    // 被跳过的条目还留在旧目录里，所以旧目录不能被删。
    assert!(legacy.join("install-language.txt").is_file());
    fs::remove_dir_all(base).unwrap();
}

/// 新位置已经写过配置 → 旧目录只是残留，绝不覆盖正在用的数据。
#[test]
fn skips_when_the_target_is_already_initialized() {
    let base = temp_dir("migrate-initialized");
    let legacy = base.join("install").join(DATA_DIRECTORY);
    let target = base.join("documents").join(DATA_DIRECTORY);
    fs::create_dir_all(&legacy).unwrap();
    fs::create_dir_all(&target).unwrap();
    fs::write(legacy.join("config.toml"), "old").unwrap();
    fs::write(target.join("config.toml"), "current").unwrap();

    assert!(!migrate_between(&legacy, &target).unwrap());
    assert_eq!(
        fs::read_to_string(target.join("config.toml")).unwrap(),
        "current"
    );
    assert!(legacy.join("config.toml").is_file());
    fs::remove_dir_all(base).unwrap();
}

/// macOS 的旧路径上是一个**文件**（可执行文件本身），不能被当成目录搬走。
#[test]
fn ignores_a_legacy_path_that_is_a_file() {
    let base = temp_dir("migrate-file");
    let legacy = base.join("install").join(DATA_DIRECTORY);
    let target = base.join("documents").join(DATA_DIRECTORY);
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, b"executable").unwrap();

    assert!(!migrate_between(&legacy, &target).unwrap());
    assert!(!target.exists());
    fs::remove_dir_all(base).unwrap();
}
