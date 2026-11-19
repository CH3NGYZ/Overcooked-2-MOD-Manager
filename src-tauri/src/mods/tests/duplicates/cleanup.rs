//! Metadata combinations copied from the five physically separate FastInit DLLs.
use super::*;
use crate::models::{AppConfig, Channel, InstalledMod, Provider};

fn fastinit_source() -> crate::models::ModRepositoryConfig {
    let mut repo = crate::mods::test_support::binding_repository();
    repo.provider = Provider::Github;
    repo.username = "kfyangc".into();
    repo.repository = "Overcooked2-DIYLevelFastInit".into();
    repo.plugin_guid = "oc2.diylevel.fastinit".into();
    repo.local_original_filename = "DIYLevelFastInit.dll".into();
    repo.local_product_name = "DIYLevelFastInit".into();
    repo
}

fn copies(config: &AppConfig) -> [InstalledMod; 5] {
    ["1.3.0", "1.4.0", "1.5.0", "1.6.4", "1.9.5"].map(|version| {
        let legacy = version == "1.3.0";
        let current = version == "1.9.5";
        let name = if current {
            "DIYLevel 快速载入 / DIYLevel 启动速度优化 / DIYLevel FastInit"
        } else {
            "DIYLevel FastInit"
        };
        let filename = if version == "1.5.0" {
            "DIYLevelFastInit 1.5.0（优化存档加载速度）.dll"
        } else {
            "DIYLevelFastInit.dll"
        };
        plain_mod(
            &format!("BepInEx/plugins/{version}/{filename}"),
            serde_json::json!({
                "name": name,
                "guid": if legacy { "yang.oc2.diylevel.fastinit" } else { "oc2.diylevel.fastinit" },
                "version": version,
                "product_name": "DIYLevelFastInit",
                "original_filename": "DIYLevelFastInit.dll",
                "repository_id": config.repositories[0].id,
                "repository_binding": current.then(|| serde_json::json!({
                    "provider": "github", "owner": "kfyangc", "repository": "Overcooked2-DIYLevelFastInit",
                    "tag": "v1.9.5", "channel": "all",
                })),
            }),
        )
    })
}

#[test]
fn actual_fastinit_identity_offers_cleanup_for_the_four_lower_copies() {
    let config = crate::mods::test_support::config(vec![fastinit_source()]);
    let mut installed = copies(&config);
    crate::mods::rebind_installed_mods(&config, &mut installed);
    assert!(installed.iter().all(|item| item.duplicate_guid));
    assert_eq!(
        installed
            .each_ref()
            .map(|item| item.duplicate_cleanup_available),
        [true, true, true, true, false]
    );
    for old in &installed[..4] {
        assert!(crate::mods::is_newer_duplicate(old, &installed[4], &config));
        assert!(crate::mods::same_release_plugin_identity(
            &crate::mods::PluginIdentity::from_installed(old, &config),
            &crate::mods::PluginIdentity::from_installed(&installed[4], &config),
        ));
    }
    installed[4].enabled = false;
    mark_duplicate_guids(&mut installed, &config);
    assert!(!installed[4].duplicate_cleanup_available);
    // The renamed-GUID 1.3 still has a higher enabled equivalent via the
    // declared GUID (1.6.4), until the last newer core goes away as well.
    assert!(installed[0].duplicate_cleanup_available);
    for remainder in &mut installed[1..4] {
        remainder.enabled = false;
    }
    crate::mods::rebind_installed_mods(&config, &mut installed);
    assert!(
        !installed
            .iter()
            .any(|item| item.duplicate_cleanup_available)
    );
}

#[test]
fn changed_guid_cleanup_rejects_ambiguous_sources_and_weak_or_conflicting_dll_evidence() {
    for failure in [
        "competing mirror",
        "other manifest upstream",
        "unknown tag",
        "other release line",
        "only product",
        "different original filename",
        "different product",
        "declared package member",
    ] {
        let mut config = crate::mods::test_support::config(vec![fastinit_source()]);
        let [mut old, _, _, _, mut current] = copies(&config);
        match failure {
            "competing mirror" => {
                let mut mirror = config.repositories[0].clone();
                mirror.provider = Provider::Gitcode;
                mirror.id = "matching-but-unproven-mirror".into();
                config.repositories.push(mirror);
            }
            "other manifest upstream" => {
                old.repository_binding = current.repository_binding.clone();
                old.repository_binding.as_mut().unwrap().provider = Provider::Gitcode;
            }
            "unknown tag" => current.repository_binding.as_mut().unwrap().tag = "latest".into(),
            "other release line" => {
                let binding = current.repository_binding.as_mut().unwrap();
                binding.tag = "v1.9.5-beta.1".into();
                binding.channel = Channel::Beta;
            }
            "only product" => old.original_filename = None,
            "different original filename" => old.original_filename = Some("Sibling.dll".into()),
            "different product" => old.product_name = Some("Unrelated product".into()),
            "declared package member" => {
                config.repositories[0]
                    .bundled_plugin_guids
                    .push(old.guid.clone().unwrap());
                // Even an independently recorded Release binding for the
                // package cannot turn the declared helper into its core.
                old.repository_binding = current.repository_binding.clone();
            }
            _ => unreachable!(),
        }
        let mut installed = [old, current];
        mark_duplicate_guids(&mut installed, &config);
        assert!(!installed[0].duplicate_cleanup_available, "{failure}");
        assert!(
            !crate::mods::is_newer_duplicate(&installed[0], &installed[1], &config),
            "{failure}"
        );
    }
}

#[test]
fn release_planner_accepts_same_guid_with_no_pe_but_rejects_conflicting_pe() {
    let config = crate::mods::test_support::config(vec![fastinit_source()]);
    let [_, mut old, _, _, mut current] = copies(&config);
    for item in [&mut old, &mut current] {
        item.product_name = None;
        item.original_filename = None;
    }
    let same = |old: &InstalledMod, current: &InstalledMod| {
        crate::mods::same_release_plugin_identity(
            &crate::mods::PluginIdentity::from_installed(old, &config),
            &crate::mods::PluginIdentity::from_installed(current, &config),
        )
    };
    assert!(same(&old, &current));
    old.original_filename = Some("Different.dll".into());
    current.original_filename = Some("DIYLevelFastInit.dll".into());
    assert!(!same(&old, &current));
}

#[test]
fn cleanup_capability_does_not_hide_an_independent_warning() {
    let config = crate::mods::test_support::config(vec![fastinit_source()]);
    let [mut old, _, _, _, current] = copies(&config);
    old.load_warning = Some("other load warning".into());
    old.status = "repository_unavailable".into();
    let mut installed = [old, current];
    mark_duplicate_guids(&mut installed, &config);
    super::refresh_status_labels(&mut installed);
    assert!(installed[0].duplicate_cleanup_available);
    assert_eq!(
        installed[0].load_warning.as_deref(),
        Some("other load warning")
    );
    assert!(
        installed[0]
            .labels
            .contains(&crate::models::InstalledModStatusLabel::RepositoryUnavailable)
    );
    assert!(
        installed[0]
            .labels
            .contains(&crate::models::InstalledModStatusLabel::LoadWarning)
    );
}

#[test]
fn two_source_entries_for_one_upstream_are_not_mirror_ambiguity() {
    let mut config = crate::mods::test_support::config(vec![fastinit_source()]);
    let mut same = config.repositories[0].clone();
    same.id = "other-entry-same-upstream".into();
    same.username = "KFYANGC".into();
    config.repositories.push(same);
    let [old, _, _, _, current] = copies(&config);
    assert!(crate::mods::is_newer_duplicate(&old, &current, &config));
}
