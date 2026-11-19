use super::{binding_repository, plugin_blob, scan_mods_with_manifests};
use crate::models::{InstalledMod, ModRepositoryConfig, RepositoryIdentityMatch};
use crate::mods::{binding::rebind_installed_mods, test_support::config};
use std::path::Path;

const CORE_DLL: &str = "BepInEx/plugins/Core/Main.dll";
const HELPER_DLL: &str = "BepInEx/plugins/External/Helper.dll";
const DISABLED_HELPER_DLL: &str = "BepInEx/plugins/External/Helper.dll.disabled";
const CORE: &str = "com.example.core";
const HELPER: &str = "com.example.helper";

fn source(id: &str, guid: &str, bundled: &[&str]) -> ModRepositoryConfig {
    let mut repo = binding_repository();
    repo.id = id.into();
    repo.repository = id.into();
    repo.plugin_guid = guid.into();
    repo.bundled_plugin_guids = bundled.iter().map(|guid| (*guid).into()).collect();
    repo.local_product_name.clear();
    repo.local_original_filename.clear();
    repo
}

fn write_plugin(root: &Path, relative: &str, guid: &str, version: &str) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, plugin_blob(guid, guid, version)).unwrap();
}

fn plugin<'a>(installed: &'a [InstalledMod], guid: &str) -> &'a InstalledMod {
    installed
        .iter()
        .find(|item| item.guid.as_deref() == Some(guid))
        .unwrap_or_else(|| panic!("expected scanned GUID {guid}"))
}

fn assert_loose_helper(installed: &[InstalledMod], expected_anchor: Option<&str>) {
    let helper = plugin(installed, HELPER);
    assert_eq!(helper.version.as_deref(), Some("99.0.0"));
    assert_eq!(helper.bundled_with_dll.as_deref(), expected_anchor);
    assert!(helper.repository_id.is_none());
    assert!(helper.repository_ids.is_empty());
    assert!(helper.repository_binding.is_none());
    assert!(helper.repository_identity_match.is_none());
    assert!(helper.package_id.is_none());
    assert!(!helper.duplicate_guid && !helper.update_available);
}

#[test]
fn scan_and_rebind_group_a_disabled_helper_across_directories_without_owning_it() {
    let root = tempfile::tempdir().unwrap();
    write_plugin(root.path(), CORE_DLL, CORE, "1.4.2");
    write_plugin(root.path(), DISABLED_HELPER_DLL, HELPER, "99.0.0");
    let declaration = source("core", CORE, &[" COM.EXAMPLE.HELPER "]);
    let mut config = config(vec![declaration.clone()]);

    let installed = scan_mods_with_manifests(root.path(), &config, None);
    assert_eq!(installed.len(), 2);
    let core = plugin(&installed, CORE);
    assert_eq!(core.repository_id.as_deref(), Some("core"));
    assert_eq!(
        core.repository_identity_match,
        Some(RepositoryIdentityMatch::Guid)
    );
    assert_eq!(core.version.as_deref(), Some("1.4.2"));
    assert!(core.enabled && core.bundled_with_dll.is_none());
    let anchor = core.dll_file.clone();
    assert_loose_helper(&installed, Some(&anchor));
    assert!(!plugin(&installed, HELPER).enabled);

    config.repositories[0].bundled_plugin_guids.clear();
    let mut snapshot = scan_mods_with_manifests(root.path(), &config, None);
    assert_loose_helper(&snapshot, None);
    for path in [CORE_DLL, DISABLED_HELPER_DLL] {
        std::fs::remove_file(root.path().join(path)).unwrap();
    }

    config.repositories[0] = declaration;
    rebind_installed_mods(&config, &mut snapshot);
    assert_loose_helper(&snapshot, Some(&anchor));
    assert!(!plugin(&snapshot, HELPER).enabled);
    config.repositories[0].bundled_plugin_guids.clear();
    rebind_installed_mods(&config, &mut snapshot);
    assert_loose_helper(&snapshot, None);
    assert_eq!(
        plugin(&snapshot, CORE).repository_id.as_deref(),
        Some("core")
    );
}

#[test]
fn scan_and_rebind_do_not_guess_an_absent_duplicate_or_disputed_core() {
    const SECOND_CORE: &str = "com.example.second";
    let root = tempfile::tempdir().unwrap();
    write_plugin(root.path(), HELPER_DLL, HELPER, "99.0.0");
    let config = config(vec![
        source("core", CORE, &[HELPER]),
        source("second", SECOND_CORE, &[HELPER]),
    ]);
    let assert_unclaimed = |mut installed: Vec<InstalledMod>, expected_count| {
        assert_eq!(installed.len(), expected_count);
        assert_loose_helper(&installed, None);
        rebind_installed_mods(&config, &mut installed);
        assert_loose_helper(&installed, None);
    };
    assert_unclaimed(scan_mods_with_manifests(root.path(), &config, None), 1);
    write_plugin(root.path(), CORE_DLL, CORE, "1.4.2");
    let one_claim = scan_mods_with_manifests(root.path(), &config, None);
    assert_eq!(one_claim.len(), 2);
    assert_loose_helper(&one_claim, Some(&plugin(&one_claim, CORE).dll_file));

    // A disabled copy still makes the installed core GUID ambiguous.
    const OTHER_DLL: &str = "BepInEx/plugins/Other/Core.dll.disabled";
    write_plugin(root.path(), OTHER_DLL, CORE, "1.0.0");
    let duplicate = scan_mods_with_manifests(root.path(), &config, None);
    let copies: Vec<_> = duplicate
        .iter()
        .filter(|item| item.guid.as_deref() == Some(CORE))
        .collect();
    assert_eq!(copies.len(), 2);
    assert_eq!(copies.iter().filter(|item| item.enabled).count(), 1);
    assert_unclaimed(duplicate, 3);

    // Two distinct cores each bind independently, but both claim the helper.
    std::fs::remove_file(root.path().join(OTHER_DLL)).unwrap();
    write_plugin(
        root.path(),
        "BepInEx/plugins/Other/Core.dll",
        SECOND_CORE,
        "2.0.0",
    );
    let disputed = scan_mods_with_manifests(root.path(), &config, None);
    assert_eq!(
        plugin(&disputed, CORE).repository_id.as_deref(),
        Some("core")
    );
    assert_eq!(
        plugin(&disputed, SECOND_CORE).repository_id.as_deref(),
        Some("second")
    );
    assert_unclaimed(disputed, 3);
}

#[test]
fn scan_and_rebind_keep_independent_identity_and_manifest_ownership() {
    let root = tempfile::tempdir().unwrap();
    write_plugin(root.path(), CORE_DLL, CORE, "1.4.2");
    write_plugin(root.path(), HELPER_DLL, HELPER, "99.0.0");
    let mut config = config(vec![
        source("core", CORE, &[HELPER]),
        source("helper", HELPER, &[]),
    ]);

    let mut independent = scan_mods_with_manifests(root.path(), &config, None);
    assert_eq!(independent.len(), 2);
    let assert_independent = |items: &[InstalledMod]| {
        let helper = plugin(items, HELPER);
        assert_eq!(helper.repository_id.as_deref(), Some("helper"));
        assert_eq!(
            helper.repository_identity_match,
            Some(RepositoryIdentityMatch::Guid)
        );
        assert!(helper.package_id.is_none() && helper.bundled_with_dll.is_none());
    };
    assert_independent(&independent);
    rebind_installed_mods(&config, &mut independent);
    assert_independent(&independent);
    config.repositories[1].enabled = false;
    let mut unavailable = scan_mods_with_manifests(root.path(), &config, None);
    assert_loose_helper(&unavailable, None);
    rebind_installed_mods(&config, &mut unavailable);
    assert_loose_helper(&unavailable, None);
    config.repositories.pop();

    // Even without any matching repository, a manifest still owns its DLL.
    let manifests = root.path().join("manifests");
    let manifest = crate::archive::new_manifest(
        "local-helper",
        "",
        crate::archive::single_language("Local helper package"),
        Default::default(),
        vec![HELPER_DLL.into()],
        vec![],
    );
    crate::archive::write_manifest(&manifests, &manifest).unwrap();
    let mut owned = scan_mods_with_manifests(root.path(), &config, Some(&manifests));
    assert_eq!(owned.len(), 2);
    let assert_owned = |items: &[InstalledMod]| {
        let helper = plugin(items, HELPER);
        assert_eq!(helper.package_id.as_deref(), Some("local-helper"));
        assert!(helper.repository_id.is_none() && helper.bundled_with_dll.is_none());
    };
    assert_owned(&owned);
    rebind_installed_mods(&config, &mut owned);
    assert_owned(&owned);
}
