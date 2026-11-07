use super::*;
use crate::mods::test_support::{config, repository};

#[test]
fn update_summary_prefers_a_bound_tag_over_dll_metadata_and_supports_opaque_tags() {
    for tag in ["v1.0.0", "old-nightly"] {
        let mut repo = repository("release", RepositoryDownloadMode::Release);
        repo.cached_releases[0].tag_name = if tag == "old-nightly" {
            "new-nightly".into()
        } else {
            "v99.0.0".into()
        };
        let mut local = installed("release", "latest");
        // DLL 元数据版本故意与清单 tag 不同、但低于线上版本：
        // 更新结论按清单 tag 算，local_version 也显示清单 tag。
        local.version = Some("0.9.0".into());
        local.repository_binding = RepositoryBinding::from_release(&repo, tag);
        let status = collect_update_status(&config(vec![repo]), &[local]);
        assert_eq!(status[0].local_version.as_deref(), Some(tag));
        assert_eq!(status[0].state, "update_available");
    }
}

fn installed(repository_id: &str, status: &str) -> InstalledMod {
    serde_json::from_value(serde_json::json!({
        "repository_id": repository_id, "repository_ids": [repository_id],
        "name": "Example", "dll_file": "BepInEx/plugins/Example.dll",
        "enabled": true, "version": "1.2.3.4", "version_source": "BepInPlugin",
        "update_available": false, "duplicate_guid": false, "status": status
    }))
    .unwrap()
}

#[test]
fn raw_update_status_ignores_cached_releases_and_api_errors() {
    let mut raw = repository("raw", RepositoryDownloadMode::RepositoryFile);
    raw.last_error = Some("obsolete Release request failure".into());
    let statuses =
        collect_update_status(&config(vec![raw]), &[installed("raw", "not_release_asset")]);
    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses[0].local_version.as_deref(), Some("1.2.3.4"));
    assert_eq!(statuses[0].remote_version, None);
    assert_eq!(statuses[0].state, "not_release_asset");
}

#[test]
fn raw_installed_mod_cannot_supply_a_release_local_version() {
    let raw = repository("raw", RepositoryDownloadMode::RepositoryFile);
    let release = repository("release", RepositoryDownloadMode::Release);
    let mut local = installed("raw", "not_release_asset");
    local.repository_ids.push("release".into());
    let statuses = collect_update_status(&config(vec![raw, release]), &[local]);
    assert_eq!(statuses[0].state, "not_release_asset");
    assert_eq!(statuses[1].local_version, None);
    assert_eq!(statuses[1].remote_version.as_deref(), Some("99.0.0"));
    assert_eq!(statuses[1].state, "unknown");
}

#[test]
fn release_installed_mod_still_reports_updates() {
    let release = repository("release", RepositoryDownloadMode::Release);
    let statuses = collect_update_status(&config(vec![release]), &[installed("release", "latest")]);
    assert_eq!(statuses[0].local_version.as_deref(), Some("1.2.3.4"));
    assert_eq!(statuses[0].remote_version.as_deref(), Some("99.0.0"));
    assert_eq!(statuses[0].state, "update_available");
}

#[test]
fn failed_release_refresh_overrides_cached_update_summary() {
    for cached_version in ["1.2.3.4", "99.0.0"] {
        let mut release = repository("release", RepositoryDownloadMode::Release);
        release.cached_releases[0].version = Some(cached_version.into());
        release.cached_releases[0].tag_name = format!("v{cached_version}");
        release.last_error = Some("Release request failed".into());
        let statuses =
            collect_update_status(&config(vec![release]), &[installed("release", "latest")]);
        assert_eq!(statuses[0].state, "repository_unavailable");
        assert_eq!(statuses[0].local_version.as_deref(), Some("1.2.3.4"));
        assert_eq!(statuses[0].remote_version, None);
    }
}

#[test]
fn empty_release_or_invalid_versions_cannot_confirm_latest() {
    let mut release = repository("release", RepositoryDownloadMode::Release);
    release.cached_releases.clear();
    let local = installed("release", "latest");
    let statuses = collect_update_status(&config(vec![release]), std::slice::from_ref(&local));
    assert_eq!(statuses[0].state, "unknown");
    assert_eq!(statuses[0].remote_version, None);

    let mut release = repository("release", RepositoryDownloadMode::Release);
    release.cached_releases[0].version = Some("not-a-version".into());
    let statuses = collect_update_status(&config(vec![release]), std::slice::from_ref(&local));
    assert_eq!(statuses[0].state, "unknown");
}

#[test]
fn successful_release_retry_restores_update_summary() {
    let mut release = repository("release", RepositoryDownloadMode::Release);
    release.last_error = Some("Release request failed".into());
    let local = installed("release", "repository_unavailable");
    let failed =
        collect_update_status(&config(vec![release.clone()]), std::slice::from_ref(&local));
    assert_eq!(failed[0].state, "repository_unavailable");
    release.last_error = None;
    let recovered =
        collect_update_status(&config(vec![release.clone()]), std::slice::from_ref(&local));
    assert_eq!(recovered[0].state, "update_available");
    release.cached_releases[0].version = local.version.clone();
    let current = collect_update_status(&config(vec![release]), std::slice::from_ref(&local));
    assert_eq!(current[0].state, "latest");
}

#[test]
fn failed_release_does_not_borrow_a_successful_bound_repositories_state() {
    let mut failed = repository("failed", RepositoryDownloadMode::Release);
    failed.last_error = Some("Release request failed".into());
    let mut current = repository("current", RepositoryDownloadMode::Release);
    current.cached_releases[0].version = Some("1.2.3.4".into());
    let mut local = installed("failed", "repository_unavailable");
    local.repository_ids.push("current".into());
    let statuses = collect_update_status(&config(vec![failed, current]), &[local]);
    assert_eq!(statuses[0].state, "repository_unavailable");
    assert_eq!(statuses[0].remote_version, None);
    assert_eq!(statuses[1].state, "latest");
}

#[test]
fn release_update_summary_uses_the_loaded_copy_in_both_scan_orders() {
    for online_version in ["19.0", "20.0"] {
        let mut release = repository("release", RepositoryDownloadMode::Release);
        release.cached_releases[0].version = Some(online_version.into());
        release.cached_releases[0].tag_name = format!("v{online_version}");
        let config = config(vec![release]);
        let mut old = installed("release", "duplicate_lower_version");
        old.version = Some("18.3".into());
        old.load_warning = Some("存在更高版本，建议卸载".into());
        let mut loaded = installed("release", "latest");
        loaded.version = Some("19.0".into());
        for copies in [
            vec![old.clone(), loaded.clone()],
            vec![loaded.clone(), old.clone()],
        ] {
            let statuses = collect_update_status(&config, &copies);
            assert_eq!(statuses[0].local_version.as_deref(), Some("19.0"));
            assert_eq!(
                statuses[0].state,
                if online_version == "19.0" {
                    "latest"
                } else {
                    "update_available"
                }
            );
        }
    }
}

#[test]
fn lower_raw_copy_keeps_its_installation_source_and_never_reports_a_release_update() {
    let raw = repository("raw", RepositoryDownloadMode::RepositoryFile);
    let release = repository("release", RepositoryDownloadMode::Release);
    let mut lower_raw = installed("raw", "not_release_asset");
    lower_raw.version = Some("18.3".into());
    lower_raw.load_warning = Some("存在更高版本，建议卸载".into());
    lower_raw.repository_ids.push("release".into());
    let statuses = collect_update_status(&config(vec![raw, release]), &[lower_raw]);
    assert_eq!(statuses[0].local_version.as_deref(), Some("18.3"));
    assert_eq!(statuses[0].state, "not_release_asset");
    assert_eq!(statuses[1].local_version, None);
    assert_eq!(statuses[1].state, "unknown");
}
