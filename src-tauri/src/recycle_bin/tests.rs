use crate::archive;
use crate::archive::InstallManifest;
use crate::recycle_bin::manifests::read_active_manifest;
use std::fs;
use std::path::PathBuf;
use uuid::Uuid;

struct Workspace {
    directory: PathBuf,
    game: PathBuf,
    manifests: PathBuf,
    recycle: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let directory = std::env::temp_dir().join(format!("oc2-recycle-test-{}", Uuid::new_v4()));
        let game = directory.join("game");
        let manifests = directory.join("manager/manifests");
        let recycle = directory.join("manager/recycle-bin");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&manifests).unwrap();
        fs::write(game.join("Overcooked2.exe"), b"test fixture only").unwrap();
        Self {
            directory,
            game,
            manifests,
            recycle,
        }
    }

    fn write(&self, relative: &str, content: &[u8]) {
        let path = self.game.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn manifest(&self, id: &str, files: &[&str]) -> InstallManifest {
        let manifest = InstallManifest {
            repository_id: id.into(),
            release_tag: "v1.2.3".into(),
            repository_binding: None,
            package_name_i18n: crate::archive::single_language("Fixture package"),
            note_i18n: crate::archive::single_language("original note"),
            group_i18n: Default::default(),
            prerelease: false,
            relations: Vec::new(),
            installed_at: "2026-10-07T00:00:00Z".into(),
            files: files.iter().map(|file| (*file).into()).collect(),
        };
        archive::write_manifest(&self.manifests, &manifest).unwrap();
        manifest
    }

    fn active(&self, id: &str) -> Option<InstallManifest> {
        read_active_manifest(&self.manifests, id)
            .unwrap()
            .map(|(manifest, _)| manifest)
    }
}

/// 构造一个只含 BepInPlugin 头的假 DLL，够 `parse_bepin_plugin_metadata` 解析。
fn plugin_blob(guid: &str, name: &str, version: &str) -> Vec<u8> {
    let mut bytes = b"BepInPlugin".to_vec();
    bytes.extend_from_slice(&[1, 0]);
    for value in [guid, name, version] {
        bytes.push(value.len() as u8);
        bytes.extend_from_slice(value.as_bytes());
    }
    bytes
}

impl Drop for Workspace {
    fn drop(&mut self) {
        // Only the fresh UUID-owned test directory can be removed.
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        let target = fs::canonicalize(&self.directory).unwrap();
        assert!(target.starts_with(&temp) && target != temp);
        assert!(
            self.directory
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("oc2-recycle-test-")
        );
        let _ = fs::remove_dir_all(&self.directory);
    }
}

const A: &str = "BepInEx/plugins/Bundle/A.dll";

const B: &str = "BepInEx/plugins/Bundle/B.dll";

const DATA: &str = "BepInEx/plugins/Bundle/data/settings.json";

fn duplicate_plugin(file: &str, version: &str) -> crate::models::InstalledMod {
    serde_json::from_value(serde_json::json!({
        "name": "ConfigurationManager fixture", "dll_file": file,
        "guid": "fixture.configurationmanager", "enabled": true,
        "version": version, "version_source": "BepInPlugin",
        "update_available": false, "duplicate_guid": true,
        "duplicate_version": true, "status": "installed"
    }))
    .unwrap()
}

mod delete;
mod duplicates;
mod file_io;
mod links;
mod repository;
mod rollback;
mod roundtrip;
mod safety;
mod selection;
