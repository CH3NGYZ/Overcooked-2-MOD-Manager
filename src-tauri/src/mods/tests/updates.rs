use super::*;

#[test]
fn same_release_tag_does_not_hide_a_manually_downgraded_dll() {
    let mut repo = binding_repository();
    repo.cached_releases = vec![high_cached_release()];
    let binding = crate::models::ModRepositoryBinding::from_release(&repo, "v99.0.0").unwrap();
    // 清单 tag 就是线上最新版，DLL 被替换成更旧的 1.2.0
    assert_eq!(
        super::binding_update_state(Some(&repo), &binding, Some("1.2.0")),
        (true, "update_available")
    );
    assert_eq!(
        super::binding_update_state(Some(&repo), &binding, None),
        (false, "unknown")
    );
}

/*
 * 用户手动升级 DLL（高于清单 tag）时，本地其实已经是线上那个版本，
 * 不能因为清单 tag 旧就一直提示更新。
 */
#[test]
fn manual_upgrade_suppresses_update_without_hiding_real_one() {
    let mut repo = binding_repository();
    repo.cached_releases = vec![high_cached_release()];
    // 清单 tag 是旧版本 v1.0.0，DLL 已经是线上的 99.0.0
    let binding = crate::models::ModRepositoryBinding::from_release(&repo, "v1.0.0").unwrap();
    assert_eq!(
        super::binding_update_state(Some(&repo), &binding, Some("99.0.0")),
        (false, "latest")
    );
    // 线上再发新版本时仍然要提示
    repo.cached_releases[0].tag_name = "v100.0.0".into();
    repo.cached_releases[0].version = Some("100.0.0".into());
    assert_eq!(
        super::binding_update_state(Some(&repo), &binding, Some("99.0.0")),
        (true, "update_available")
    );
}

#[test]
fn comparison_uses_the_install_baseline_but_does_not_invent_one_for_legacy_manifests() {
    let installed = "1.5.0";
    for (baseline, disk, expected) in [
        (Some(installed), Some("1.5.0"), Some("v2.0.0")),
        (Some(installed), Some("1.5"), Some("v2.0.0")),
        (Some(installed), Some("1.2.0"), Some("1.2.0")),
        (Some(installed), Some("2.1.0"), Some("2.1.0")),
        (Some(installed), None, None),
        (Some(""), None, Some("v2.0.0")),
        (Some(""), Some("1.2.0"), Some("1.2.0")),
        (None, Some("1.2.0"), Some("1.2.0")),
        (None, Some("2.1.0"), Some("2.1.0")),
        (None, Some("2.0"), Some("v2.0.0")),
        (None, Some("nightly"), Some("v2.0.0")),
        (None, None, Some("v2.0.0")),
    ] {
        assert_eq!(
            super::effective_local_version("v2.0.0", disk, baseline),
            expected,
            "baseline={baseline:?}, disk={disk:?}"
        );
    }
}

#[test]
fn downgraded_dll_shows_update_label_without_losing_its_binding() {
    let mut repo = binding_repository();
    repo.cached_releases = vec![high_cached_release()];
    let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
        "repository_id": "repo-1", "repository_ids": ["repo-1"],
        "name": "Example", "dll_file": "BepInEx/plugins/Example.dll",
        "enabled": true, "version": "1.2.0", "version_source": "BepInPlugin",
        "guid": "com.example.guid", "repository_identity_match": "guid",
        "update_available": false, "duplicate_guid": false, "status": "latest",
        "repository_binding": {
            "provider": "gitee", "owner": "example", "repository": "mod",
            "tag": "v99.0.0", "channel": "all"
        }
    }))
    .unwrap();
    crate::mods::status::apply_mod_update_state(Some(&repo), &mut item);
    super::refresh_status_labels(std::slice::from_mut(&mut item));
    assert!(item.update_available);
    assert_eq!(item.status, "update_available");
    assert_eq!(item.repository_binding.as_ref().unwrap().tag, "v99.0.0");
    assert_eq!(
        item.labels,
        [crate::models::InstalledModStatusLabel::UpdateAvailable]
    );
}

/*
 * 仓库文件（RAW）模式的 MOD 永远显示「已安装」，不能显示「版本未知」。
 *
 * 这类仓库装的是一份固定文件，没有 Release 可比较；但安装清单里可能
 * 留下 repository_binding，此时若照常走 binding_update_state，
 * bound_release 找不到 Release 就返回 unknown —— 即使仓库刷新早已完成，
 * MOD 管理页也会一直同时显示「版本未知」和「已安装」。
 */
#[test]
fn raw_repository_mods_never_report_unknown_version() {
    let repo: crate::models::ModRepositoryConfig = serde_json::from_value(serde_json::json!({
        "id": "raw-1", "name": "Raw", "provider": "github", "username": "u", "repository": "r",
        "download_mode": "repository_file", "branch": "main", "file_path": "mod.dll",
        "channel": "all", "asset_rules": [], "install_target": "BepInEx/plugins",
        "note": "", "plugin_guid": "", "local_product_name": "", "local_original_filename": "",
        "version_regex": "", "max_release_count": 10, "download_threads": 4,
        "enabled": true, "builtin": false, "cached_releases": []
    }))
    .unwrap();
    // RAW 仓库没有 cached_releases，但绑定存在
    let (update, status) = super::binding_update_state(
        Some(&repo),
        &crate::models::ModRepositoryBinding {
            provider: crate::models::Provider::Github,
            owner: "u".into(),
            repository: "r".into(),
            tag: "v1.0.0".into(),
            channel: crate::models::Channel::All,
        },
        None,
    );
    // 直接调绑定判定仍会返回 unknown —— 这是它本身的语义
    assert!(!update);
    assert_eq!(status, "unknown");
    // 但扫描路径必须先按仓库模式分流，不能让 RAW 走到这里
    assert_eq!(
        super::repository_update_state(Some(&repo), Some("1.0.0")).1,
        "not_release_asset"
    );
}

#[test]
fn disabled_mod_keeps_update_label_and_legacy_json_defaults_labels() {
    let mut item: crate::models::InstalledMod = serde_json::from_value(serde_json::json!({
        "repository_id": "configuration-manager", "repository_ids": ["configuration-manager"],
        "name": "Configuration Manager", "dll_file": "BepInEx/plugins/ConfigurationManager.dll",
        "enabled": false, "version": "18.3", "version_source": "BepInPlugin",
        "update_available": true, "duplicate_guid": false, "status": "update_available"
    }))
    .unwrap();
    assert!(item.labels.is_empty());
    super::refresh_status_labels(std::slice::from_mut(&mut item));
    assert_eq!(
        item.labels,
        [
            crate::models::InstalledModStatusLabel::UpdateAvailable,
            crate::models::InstalledModStatusLabel::Disabled
        ]
    );
    assert_eq!(
        serde_json::to_value(&item).unwrap()["labels"],
        serde_json::json!(["update_available", "disabled"])
    );
}

#[test]
fn unknown_version_is_not_update() {
    assert!(!matches!(
        crate::releases::parse_version("unknown"),
        Some(_)
    ));
}

#[test]
fn comparable_bound_versions_use_the_same_rules_for_every_source_type() {
    for source in [Some("official"), Some(crate::community::SOURCE_ID), None] {
        let mut repo = binding_repository();
        repo.source_id = source.map(str::to_string);
        repo.builtin = source == Some("official");
        repo.cached_releases = vec![high_cached_release()];
        let binding = crate::models::ModRepositoryBinding::from_release(&repo, "v98.0.0").unwrap();
        assert_eq!(
            super::binding_update_state(Some(&repo), &binding, Some(&binding.tag)),
            (true, "update_available")
        );
        let mut current = binding.clone();
        current.tag = "v99.0.0".into();
        assert_eq!(
            super::binding_update_state(Some(&repo), &current, Some(&current.tag)),
            (false, "latest")
        );
        current.tag = "v100.0.0".into();
        assert_eq!(
            super::binding_update_state(Some(&repo), &current, Some(&current.tag)),
            (false, "latest")
        );
        repo.last_error = Some("request failed".into());
        assert_eq!(
            super::binding_update_state(Some(&repo), &binding, Some(&binding.tag)),
            (false, "repository_unavailable")
        );
    }
}

#[test]
fn opaque_tags_and_channel_provider_boundaries_are_preserved() {
    let mut repo = binding_repository();
    repo.channel = crate::models::Channel::Stable;
    let binding = crate::models::ModRepositoryBinding::from_release(&repo, "old-nightly").unwrap();
    let mut beta = high_cached_release();
    beta.tag_name = "beta-nightly".into();
    beta.prerelease = true;
    let mut stable = high_cached_release();
    stable.tag_name = "new-nightly".into();
    repo.channel = crate::models::Channel::All;
    repo.cached_releases = vec![beta, stable];
    assert_eq!(
        super::bound_release(&repo, &binding).unwrap().tag_name,
        "new-nightly"
    );
    assert_eq!(
        super::binding_update_state(Some(&repo), &binding, Some(&binding.tag)),
        (true, "update_available")
    );
    assert_eq!(
        super::binding_update_state(Some(&repo), &binding, Some("1.2.0")),
        (false, "unknown")
    );
    let mut current = binding.clone();
    current.tag = "new-nightly".into();
    assert_eq!(
        super::binding_update_state(Some(&repo), &current, Some(&current.tag)),
        (false, "latest")
    );
    repo.provider = crate::models::Provider::Github;
    assert!(!binding.matches_repository(&repo));
    repo.provider = binding.provider.clone();
    repo.channel = crate::models::Channel::Beta;
    assert!(!binding.matches_repository(&repo));
    repo.download_mode = crate::models::ModDownloadMode::RepositoryFile;
    assert!(crate::models::ModRepositoryBinding::from_release(&repo, "tag").is_none());
    assert_eq!(
        super::binding_update_state(None, &binding, None),
        (false, "bound_repository_missing")
    );
}

#[test]
fn failed_release_refresh_does_not_confirm_a_cached_version() {
    for cached_version in ["1.2.3", "99.0.0"] {
        let mut repo = binding_repository();
        let mut release = high_cached_release();
        release.version = Some(cached_version.into());
        release.tag_name = format!("v{cached_version}");
        repo.cached_releases = vec![release];
        repo.last_error = Some("Release request failed".into());
        assert_eq!(super::repository_remote_version(&repo), None);
        assert_eq!(
            super::repository_update_state(Some(&repo), Some("1.2.3")),
            (false, "repository_unavailable")
        );
    }
}

#[test]
fn release_state_requires_comparable_local_and_remote_versions() {
    let mut repo = binding_repository();
    assert_eq!(
        super::repository_update_state(Some(&repo), Some("1.2.3")),
        (false, "unknown")
    );
    let mut release = high_cached_release();
    release.version = Some("not-a-version".into());
    repo.cached_releases = vec![release];
    assert_eq!(
        super::repository_update_state(Some(&repo), Some("1.2.3")),
        (false, "unknown")
    );
    repo.cached_releases = vec![high_cached_release()];
    for local_version in [None, Some("not-a-version")] {
        assert_eq!(
            super::repository_update_state(Some(&repo), local_version),
            (false, "unknown")
        );
    }
}

#[test]
fn successful_release_retry_restores_version_comparison() {
    let mut repo = binding_repository();
    repo.last_error = Some("Release request failed".into());
    repo.cached_releases = vec![high_cached_release()];
    assert_eq!(
        super::repository_update_state(Some(&repo), Some("1.2.3")),
        (false, "repository_unavailable")
    );
    repo.last_error = None;
    assert_eq!(
        super::repository_update_state(Some(&repo), Some("1.2.3")),
        (true, "update_available")
    );
    assert_eq!(
        super::repository_update_state(Some(&repo), Some("99.0.0")),
        (false, "latest")
    );
    repo.cached_releases.clear();
    assert_eq!(
        super::repository_update_state(Some(&repo), Some("1.2.3")),
        (false, "unknown")
    );
}
