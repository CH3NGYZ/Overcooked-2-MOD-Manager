//! Disk/ZIP regressions through the production scan, preview and install services.
use crate::archive;
use crate::models::{Channel, ModDownloadMode, ModRepositoryBinding, ModRepositoryConfig};
use crate::mods::test_support::{config, plugin_blob, repository};
use crate::{mods, recycle_bin};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

struct Workspace {
    root: PathBuf,
    game: PathBuf,
    manifests: PathBuf,
    recycle: PathBuf,
}

impl Workspace {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("oc2-guid-repository-{}", uuid::Uuid::new_v4()));
        let value = Self {
            game: root.join("game"),
            manifests: root.join("manifests"),
            recycle: root.join("recycle"),
            root,
        };
        fs::create_dir_all(&value.game).unwrap();
        fs::write(value.game.join("Overcooked2.exe"), b"test fixture only").unwrap();
        value
    }

    fn dll(&self, path: &str, guid: &str, version: &str) {
        let target = self.game.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, plugin_blob(guid, "DIYLevel Fast Init", version)).unwrap();
    }

    fn manifest(&self, repo: &ModRepositoryConfig, tag: &str, files: &[&str]) {
        let mut manifest = archive::new_manifest(
            &repo.id,
            tag,
            archive::single_language("Fast Init"),
            Default::default(),
            files.iter().map(|file| (*file).into()).collect(),
            Vec::new(),
        );
        manifest.repository_binding = ModRepositoryBinding::from_release(repo, tag);
        archive::write_manifest(&self.manifests, &manifest).unwrap();
    }

    fn zip(&self) -> PathBuf {
        let path = self.root.join("DIYLevelFastInit.zip");
        let mut writer = zip::ZipWriter::new(fs::File::create(&path).unwrap());
        writer
            .start_file(
                "DIYLevelFastInit.dll",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        writer
            .write_all(&plugin_blob(
                "oc2.diylevel.fastinit",
                "DIYLevel Fast Init",
                "1.9.2",
            ))
            .unwrap();
        writer.finish().unwrap();
        path
    }

    fn install(&self, asset: &std::path::Path, repo: &ModRepositoryConfig) {
        mods::install::install_downloaded_mod_repository(
            asset,
            "DIYLevelFastInit.zip",
            &self.game,
            &self.manifests,
            &self.recycle,
            repo,
            "v1.9.2",
            false,
            true,
        )
        .unwrap();
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let target = fs::canonicalize(&self.root).unwrap();
        let temp = fs::canonicalize(std::env::temp_dir()).unwrap();
        assert!(target.starts_with(&temp) && target != temp);
        assert!(
            self.root
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("oc2-guid-repository-")
        );
        let _ = fs::remove_dir_all(target);
    }
}

fn fast_init_repo(id: &str, guid: &str) -> ModRepositoryConfig {
    let mut value = repository(id, ModDownloadMode::Release);
    value.plugin_guid = guid.into();
    value.channel = Channel::Stable;
    value.username = "CH3NGYZ".into();
    value.repository = "DIYLevelFastInit".into();
    value
}

const OLD: &str = "BepInEx/plugins/legacy/DIYLevelFastInit.dll";
const NEW: &str = "BepInEx/plugins/current/DIYLevelFastInit.dll";

#[test]
fn changed_guid_same_package_scan_zip_preview_and_update_recycles_both_versions() {
    let workspace = Workspace::new();
    let repo = fast_init_repo("fast-init", "oc2.diylevel.fastinit");
    let config = config(vec![repo.clone()]);
    workspace.dll(OLD, "yang.oc2.diylevel.fastinit", "1.3");
    workspace.dll(NEW, "oc2.diylevel.fastinit", "1.9.2");
    // One repository ID can cover historical copies at different paths.
    workspace.manifest(&repo, "v1.3", &[OLD, NEW]);
    let installed =
        mods::scan_mods_with_manifests(&workspace.game, &config, Some(&workspace.manifests));
    assert_eq!(installed.len(), 2);
    assert!(installed.iter().all(|item| item.duplicate_guid));
    assert_eq!(
        installed
            .iter()
            .find(|item| item.version.as_deref() == Some("1.3"))
            .unwrap()
            .status,
        "duplicate_lower_version"
    );

    let asset = workspace.zip();
    let preview = super::super::analysis::analyze_zip(&asset, "DIYLevelFastInit.zip", 1).unwrap();
    assert_eq!(
        preview.entries[0].guid.as_deref(),
        Some("oc2.diylevel.fastinit")
    );
    let binding = ModRepositoryBinding::from_release(&repo, "v1.9.2").unwrap();
    let conflicts =
        super::find_install_conflicts(&installed, &preview.entries, Some(&binding), &config);
    assert_eq!(
        conflicts.len(),
        1,
        "one conflict per actual manifest removal"
    );
    assert!(
        installed
            .iter()
            .any(|item| item.dll_file == conflicts[0].handle)
    );

    workspace.install(&asset, &repo);
    assert!(!workspace.game.join(OLD).exists());
    assert!(!workspace.game.join(NEW).exists());
    let refreshed =
        mods::scan_mods_with_manifests(&workspace.game, &config, Some(&workspace.manifests));
    assert_eq!(refreshed.len(), 1);
    assert_eq!(refreshed[0].guid.as_deref(), Some("oc2.diylevel.fastinit"));
    assert_eq!(refreshed[0].version.as_deref(), Some("1.9.2"));
    assert!(!refreshed[0].duplicate_guid);
    let recycled = recycle_bin::list(&workspace.recycle).unwrap();
    assert_eq!(recycled.len(), 1);
    assert_eq!(recycled[0].file_count, 2);
    let snapshot = fs::read_to_string(
        workspace
            .recycle
            .join(&recycled[0].id)
            .join("manifest.toml"),
    )
    .unwrap();
    let snapshot: archive::InstallManifest = toml::from_str(&snapshot).unwrap();
    assert_eq!(snapshot.files.len(), 2);
    assert_eq!(snapshot.repository_binding.unwrap().tag, "v1.3");
}

#[test]
fn manual_zip_preview_uses_existing_repository_matcher_for_changed_guids() {
    let workspace = Workspace::new();
    let old = fast_init_repo("legacy-source", "yang.oc2.diylevel.fastinit");
    let current = fast_init_repo("current-source", "oc2.diylevel.fastinit");
    let config = config(vec![old, current]);
    // Manual files have no persisted binding; resolve both through normal GUID/PE matching.
    workspace.dll(OLD, "yang.oc2.diylevel.fastinit", "1.3");
    let installed = mods::scan_mods_with_manifests(&workspace.game, &config, None);
    assert!(installed[0].repository_binding.is_none());
    assert_eq!(installed[0].repository_id.as_deref(), Some("legacy-source"));
    let asset = workspace.zip();
    let preview = super::super::analysis::analyze_zip(&asset, "DIYLevelFastInit.zip", 1).unwrap();
    let conflicts = super::find_install_conflicts(&installed, &preview.entries, None, &config);
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].handle, OLD);
    assert!(!conflicts[0].same_version);
}

#[test]
fn inherited_repository_grouping_cannot_create_a_plugin_identity() {
    let workspace = Workspace::new();
    let repo = fast_init_repo("target-repository", "oc2.diylevel.fastinit");
    let config = config(vec![repo.clone()]);
    workspace.dll(OLD, "com.example.unrelated", "1.3");
    let mut installed = mods::scan_mods_with_manifests(&workspace.game, &config, None);
    assert!(installed[0].repository_id.is_none());
    // Same-directory grouping inherits these IDs, but no metadata binds this
    // plugin to the target. It must not become a removable predecessor.
    installed[0].repository_id = Some(repo.id.clone());
    installed[0].repository_ids = vec![repo.id.clone()];
    let asset = workspace.zip();
    let preview = super::super::analysis::analyze_zip(&asset, "DIYLevelFastInit.zip", 1).unwrap();
    let binding = ModRepositoryBinding::from_release(&repo, "v1.9.2").unwrap();
    assert!(
        super::find_install_conflicts(&installed, &preview.entries, Some(&binding), &config)
            .is_empty()
    );
    let mut unrelated = binding.clone();
    unrelated.repository = "OtherProject".into();
    installed[0].repository_binding = Some(unrelated);
    assert!(
        super::find_install_conflicts(&installed, &preview.entries, Some(&binding), &config)
            .is_empty(),
        "real install binding wins over inherited grouping and download origin"
    );
    installed[0].repository_binding = Some(binding.clone());
    assert_eq!(
        super::find_install_conflicts(&installed, &preview.entries, Some(&binding), &config).len(),
        1,
        "matching real binding proves the source even when the GUID changed"
    );
}

#[test]
fn release_channel_switch_still_recycles_the_old_bound_package() {
    let workspace = Workspace::new();
    let repo = fast_init_repo("new-source-id", "oc2.diylevel.fastinit");
    let mut old = repo.clone();
    old.id = "old-source-id".into();
    old.channel = Channel::Beta;
    workspace.dll(OLD, "yang.oc2.diylevel.fastinit", "1.3");
    workspace.manifest(&old, "v1.3-beta.1", &[OLD]);
    let config = config(vec![repo.clone()]);
    let installed =
        mods::scan_mods_with_manifests(&workspace.game, &config, Some(&workspace.manifests));
    let asset = workspace.zip();
    let preview = super::super::analysis::analyze_zip(&asset, "DIYLevelFastInit.zip", 1).unwrap();
    let binding = ModRepositoryBinding::from_release(&repo, "v1.9.2").unwrap();
    assert!(
        super::find_install_conflicts(&installed, &preview.entries, Some(&binding), &config)
            .is_empty()
    );
    // Presentation isolates channels; repository replacement retains existing cross-channel cleanup.
    workspace.install(&asset, &repo);
    assert!(!workspace.game.join(OLD).exists());
    assert!(archive::read_manifest(&workspace.manifests, &old.id).is_none());
    assert_eq!(recycle_bin::list(&workspace.recycle).unwrap().len(), 1);
    let installed =
        mods::scan_mods_with_manifests(&workspace.game, &config, Some(&workspace.manifests));
    assert_eq!(installed.len(), 1);
    assert_eq!(installed[0].guid.as_deref(), Some("oc2.diylevel.fastinit"));
}
