use crate::asset_bundle::LevelSetMetadata;
use crate::framework::DIYLEVEL_DIR;
use crate::level_sets::bepinex_layer::bepinex_level_set_dir_name;
use crate::level_sets::bepinex_layer::is_bepinex_layer_entry;
use crate::level_sets::bepinex_layer::is_bepinex_layer_zip;
use crate::level_sets::flatten::flatten_level_sets_with;
use crate::level_sets::install::install_from_zip;
use crate::level_sets::operations::remove_level_set;
use crate::level_sets::paths::LEGACY_DISABLED_ROOT;
use crate::level_sets::paths::LEVELS_DIR;
use crate::level_sets::paths::disabled_root;
use crate::level_sets::paths::levels_root;
use crate::level_sets::paths::migrate_legacy_disabled_root;
use crate::level_sets::plugin_assignment::package_unit_root;
use crate::level_sets::plugin_assignment::pick_level_set_for_plugin;
use crate::level_sets::plugin_assignment::plugin_unit_root;
use crate::level_sets::plugin_registry::LevelSetPluginOwner;
use crate::level_sets::plugin_registry::RegisteredLevelSetPlugin;
use crate::level_sets::plugin_registry::merge_registered_plugins;
use crate::level_sets::plugin_registry::read_registered_plugins;
use crate::level_sets::plugin_registry::registered_plugin_paths;
use crate::level_sets::plugin_registry::registry_path;
use crate::level_sets::plugin_registry::write_registered_plugins;
use crate::level_sets::scan::LevelSetTarget;
use crate::level_sets::scan::assemble_level_set;
use crate::level_sets::scan::scan_level_sets_with_progress;
use crate::level_sets::zip_info::is_info_file;
use crate::level_sets::zip_inspect::ZipContents;
use crate::level_sets::zip_inspect::classify_zip;
use crate::level_sets::zip_inspect::inspect_zip;
use crate::models::DllRole;
use crate::models::LevelSetLayout;
use std::path::Path;
use std::path::PathBuf;

use std::io::{Read, Write as _};

/// 造一个 ZIP：`entries` 是 (条目名, 内容) 列表。
fn write_test_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = std::fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();
    for (name, bytes) in entries {
        writer.start_file(*name, options).unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap();
}

fn metadata(name: &str, name_zh: Option<&str>) -> LevelSetMetadata {
    LevelSetMetadata {
        name: name.to_string(),
        name_zh: name_zh.map(str::to_string),
        ..Default::default()
    }
}

/// 真实 `info` 的字节（从 `.tmp/rw.zip` 里取一份），没有就跳过。
fn real_info_bytes() -> Option<Vec<u8>> {
    let zip = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.tmp/rw.zip");
    if !zip.is_file() {
        return None;
    }
    let file = std::fs::File::open(zip).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).ok()?;
        if is_info_file(entry.name()) {
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).ok()?;
            return Some(bytes);
        }
    }
    None
}

/// 每个用例独立根目录，避免并行测试互相踩；用例结束自己删掉。
fn workspace(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
    let root = std::env::temp_dir().join(format!("oc2-lv-{tag}-{}", uuid::Uuid::new_v4()));
    let game = root.join("game");
    let recycle = root.join("recycle");
    let manifests = root.join("manifests");
    std::fs::create_dir_all(game.join(DIYLEVEL_DIR).join(LEVELS_DIR)).unwrap();
    std::fs::create_dir_all(disabled_root(&game)).unwrap();
    std::fs::create_dir_all(&recycle).unwrap();
    std::fs::create_dir_all(&manifests).unwrap();
    (root, game, recycle)
}

mod paths;

mod assignment;

mod scan;

mod zip_cases;

mod bepinex_layer;

mod registry;

mod flatten;
