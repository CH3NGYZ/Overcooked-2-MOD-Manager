use super::*;
use crate::mods::test_support::{config, repository};
use std::fs;
use std::io::Write;

pub(super) struct InstallWorkspace {
    pub(super) root: PathBuf,
    pub(super) game: PathBuf,
    pub(super) manifests: PathBuf,
    pub(super) recycle: PathBuf,
}

impl InstallWorkspace {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!("oc2-reinstall-test-{}", Uuid::new_v4()));
        let game = root.join("game");
        let manifests = root.join("manager/manifests");
        let recycle = root.join("manager/recycle-bin");
        fs::create_dir_all(&game).unwrap();
        fs::create_dir_all(&manifests).unwrap();
        Self {
            root,
            game,
            manifests,
            recycle,
        }
    }

    pub(super) fn write(&self, relative: &str, contents: &[u8]) {
        let path = self.game.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    pub(super) fn zip(&self, entries: &[(&str, &[u8])]) -> PathBuf {
        let path = self.root.join("update.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        for (name, contents) in entries {
            writer
                .start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(contents).unwrap();
        }
        writer.finish().unwrap();
        path
    }

    pub(super) fn original(&self) -> Vec<u8> {
        self.write("BepInEx/plugins/Main.dll", b"old main");
        self.write("BepInEx/plugins/Hidden.dll.disabled", b"old hidden");
        self.write("BepInEx/plugins/data/settings.json", b"old settings");
        self.write("BepInEx/plugins/Shared.dll", b"shared");
        self.write("BepInEx/plugins/Unrelated.dll", b"unrelated");
        let files = ["Main.dll", "Hidden.dll", "data/settings.json", "Shared.dll"]
            .map(|file| format!("BepInEx/plugins/{file}"));
        archive::write_manifest(
            &self.manifests,
            &archive::new_manifest(
                "package",
                "v1",
                archive::single_language("Package"),
                Default::default(),
                files.to_vec(),
                Vec::new(),
            ),
        )
        .unwrap();
        archive::write_manifest(
            &self.manifests,
            &archive::new_manifest(
                "peer",
                "v1",
                archive::single_language("Peer"),
                Default::default(),
                vec!["BepInEx/plugins/Shared.dll".into()],
                Vec::new(),
            ),
        )
        .unwrap();
        fs::read(self.manifests.join("package.toml")).unwrap()
    }
}

impl Drop for InstallWorkspace {
    fn drop(&mut self) {
        let target = fs::canonicalize(&self.root).unwrap();
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(target.starts_with(&temp) && target != temp);
        let _ = fs::remove_dir_all(target);
    }
}

mod identity;
mod transaction;
