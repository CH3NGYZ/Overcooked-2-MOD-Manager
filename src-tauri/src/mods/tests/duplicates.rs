use super::*;

#[test]
fn both_duplicate_copies_get_the_duplicate_label() {
    // 装了两个同 GUID 的版本（1.8.7 + 1.9.7）：
    // 两条都应显示「重复」，旧的那条额外显示「低版本」。
    let mut mods = vec![
        duplicate_configuration_manager("1.8.7", "1.9.7"),
        duplicate_configuration_manager("1.9.7", "1.9.7"),
    ];
    super::mark_duplicate_guids(&mut mods);
    super::refresh_status_labels(&mut mods);

    use crate::models::InstalledModStatusLabel as Label;
    assert!(mods[0].labels.contains(&Label::Duplicate));
    assert!(mods[0].labels.contains(&Label::DuplicateLowerVersion));
    assert!(mods[1].labels.contains(&Label::Duplicate));
    assert!(!mods[1].labels.contains(&Label::DuplicateLowerVersion));
    // 高版本副本保留它自己的更新状态（这里已是最新）
    assert!(mods[1].labels.contains(&Label::Latest));
}

#[test]
fn shadowed_old_configuration_manager_is_not_an_available_update() {
    let mut installed = vec![
        duplicate_configuration_manager("18.3", "19.0"),
        duplicate_configuration_manager("19.0", "19.0"),
    ];
    assert!(installed[0].update_available);
    super::mark_duplicate_guids(&mut installed);
    assert_eq!(installed[0].status, "duplicate_lower_version");
    assert_eq!(
        installed[0].load_warning.as_deref(),
        Some("mods.higherVersionPresent")
    );
    assert_eq!(
        installed
            .iter()
            .filter(|item| item.update_available)
            .count(),
        0
    );
}

#[test]
fn newer_online_configuration_manager_keeps_the_effective_local_copy_update() {
    let mut installed = vec![
        duplicate_configuration_manager("18.3", "20.0"),
        duplicate_configuration_manager("19.0", "20.0"),
    ];
    super::mark_duplicate_guids(&mut installed);
    assert!(!installed[0].update_available);
    assert!(installed[1].update_available);
    assert_eq!(
        installed
            .iter()
            .filter(|item| item.update_available)
            .count(),
        1
    );
}

#[test]
fn disabled_higher_copy_does_not_hide_the_loaded_mod_update() {
    let mut installed = vec![
        duplicate_configuration_manager("18.3", "19.0"),
        duplicate_configuration_manager("19.0", "19.0"),
    ];
    installed[1].enabled = false;
    super::mark_duplicate_guids(&mut installed);
    assert!(installed[0].update_available);
    assert!(installed[0].load_warning.is_none());
    assert_ne!(installed[0].status, "duplicate_lower_version");
}

#[test]
fn same_version_duplicates_keep_updates_and_raw_keeps_its_source_status() {
    let mut same_version = vec![
        duplicate_configuration_manager("19.0", "20.0"),
        duplicate_configuration_manager("19.0", "20.0"),
    ];
    super::mark_duplicate_guids(&mut same_version);
    assert!(
        same_version
            .iter()
            .all(|item| item.duplicate_version && item.update_available)
    );
    let mut raw = duplicate_configuration_manager("18.3", "19.0");
    raw.status = "not_release_asset".into();
    raw.update_available = false;
    let mut raw_and_release = vec![raw, duplicate_configuration_manager("19.0", "19.0")];
    super::mark_duplicate_guids(&mut raw_and_release);
    assert_eq!(raw_and_release[0].status, "not_release_asset");
    assert!(!raw_and_release[0].update_available);
}

#[test]
fn mod_version_comparison_fills_missing_components_with_zero() {
    assert_eq!(
        super::compare_mod_versions("1.8.5.03", "1.8.5"),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(
        super::compare_mod_versions("1.8.5", "1.8.5.0"),
        Some(std::cmp::Ordering::Equal)
    );
    assert_eq!(
        super::compare_mod_versions("1.10.0", "1.9.99.99"),
        Some(std::cmp::Ordering::Greater)
    );
    /*
     * 本地副本去重用的是**纯数字段**比较，不是 semver。
     *
     * BepInEx 加载同 GUID 多份 DLL 时比程序集版本号的数字大小，
     * `1.9.8.1` 严格大于 `1.9.8`。若这里改成 semver（beta 低于
     * 同号正式版），管理器会建议卸载实际被加载的那个文件。
     *
     * 线上更新判定走另一套（releases::compare_release_versions），
     * 那里 `1.9.8 > 1.9.8-beta.1` 是对的 —— 两套规则服务两个目的。
     */
    assert_eq!(
        super::compare_mod_versions("1.9.8.1", "1.9.8"),
        Some(std::cmp::Ordering::Greater),
        "本地副本：四段 beta 的数字更大，与实际加载一致"
    );
    assert_eq!(
        super::compare_mod_versions("1.9.8.21", "1.9.8.1"),
        Some(std::cmp::Ordering::Greater)
    );
}

#[test]
fn lower_duplicate_guid_is_marked_as_not_loaded() {
    let mut mods = vec![
        crate::models::InstalledMod {
            repository_id: None,
            repository_ids: Vec::new(),
            manual_install: false,
            is_dependency: false,
            repository_binding: None,
            package_id: None,
            package_name: None,
            package_name_i18n: Default::default(),
            package_mod_count: 0,
            group_i18n: Default::default(),
            group: String::new(),
            prerelease: false,
            relations: Vec::new(),
            name: "Old".into(),
            has_config: false,
            has_logs: false,
            title: None,
            description: None,
            product_name: Some("Old".into()),
            original_filename: Some("old.dll".into()),
            guid: Some("com.example.mod".into()),
            plugin_name: None,
            company: None,
            copyright: None,
            dll_file: "BepInEx/plugins/old.dll".into(),
            enabled: true,
            version: Some("1.0.0".into()),
            version_source: "BepInPlugin".into(),
            update_available: false,
            labels: Vec::new(),
            duplicate_guid: false,
            duplicate_version: false,
            load_warning: None,
            note: None,
            status: "installed".into(),
        },
        crate::models::InstalledMod {
            repository_id: None,
            repository_ids: Vec::new(),
            manual_install: false,
            is_dependency: false,
            repository_binding: None,
            package_id: None,
            package_name: None,
            package_name_i18n: Default::default(),
            package_mod_count: 0,
            group_i18n: Default::default(),
            group: String::new(),
            prerelease: false,
            relations: Vec::new(),
            name: "New".into(),
            has_config: false,
            has_logs: false,
            title: None,
            description: None,
            product_name: Some("New".into()),
            original_filename: Some("new.dll".into()),
            guid: Some("com.example.mod".into()),
            plugin_name: None,
            company: None,
            copyright: None,
            dll_file: "BepInEx/plugins/new.dll".into(),
            enabled: true,
            version: Some("2.0.0".into()),
            version_source: "BepInPlugin".into(),
            update_available: false,
            labels: Vec::new(),
            duplicate_guid: false,
            duplicate_version: false,
            load_warning: None,
            note: None,
            status: "installed".into(),
        },
    ];
    super::mark_duplicate_guids(&mut mods);
    assert_eq!(
        mods[0].load_warning.as_deref(),
        Some("mods.higherVersionPresent")
    );
    assert!(mods[1].load_warning.is_none());
    assert!(mods[0].duplicate_guid && mods[1].duplicate_guid);
    assert!(!mods[0].duplicate_version && !mods[1].duplicate_version);

    mods[1].version = Some("1.0.0".into());
    mods[0].load_warning = None;
    mods[0].status = "installed".into();
    mods[0].duplicate_guid = false;
    mods[1].duplicate_guid = false;
    super::mark_duplicate_guids(&mut mods);
    assert!(mods[0].duplicate_version && mods[1].duplicate_version);
}
