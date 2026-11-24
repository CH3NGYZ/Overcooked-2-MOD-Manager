use super::*;
use std::path::Path;

#[test]
fn manifest_sync_uses_exact_platform_verdict_and_preserves_unknown_history() {
    use crate::models::{ModDownloadMode, Provider};
    use crate::mods::test_support::{config, repository};
    for (case, api, legacy, tag, expected) in [
        ("metadata-false", Some(false), true, "v1.0.0.1", false),
        ("cached-false", Some(false), true, "v1.0.0.1", false),
        (
            "metadata-beta-false",
            Some(false),
            false,
            "v2.0.0-beta.3",
            true,
        ),
        (
            "cached-beta-false",
            Some(false),
            false,
            "v2.0.0-beta.3",
            true,
        ),
        ("short-beta-false", Some(false), false, "2.0-beta.3", true),
        ("custom-fix-false", Some(false), true, "3.4.1-fix", false),
        ("explicit-true", Some(true), false, "v1.0.0", true),
        ("legacy-true", None, true, "v1.0.0", true),
        ("legacy-suffix", None, false, "v1.0.0-beta", true),
        ("legacy-stable", None, false, "v1.0.0", false),
        ("other-tag", Some(false), true, "v1.0.0.1", true),
        ("other-provider", Some(false), true, "v1.0.0.1", true),
        ("other-owner", Some(false), true, "v1.0.0.1", true),
        ("other-repository", Some(false), true, "v1.0.0.1", true),
        ("missing-binding", Some(false), true, "v1.0.0.1", false),
    ] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-source-refresh")
            .join(Uuid::new_v4().to_string());
        let state = AppState::with_config(crate::config::ConfigService::for_test(root.clone()));
        let mut repo = repository(case, ModDownloadMode::Release);
        let mut binding = ModRepositoryBinding::from_release(&repo, tag).unwrap();
        match case {
            "other-provider" => binding.provider = Provider::Gitee,
            "other-owner" => binding.owner = "someone-else".into(),
            "other-repository" => binding.repository = "another-repo".into(),
            _ => {}
        }
        repo.cached_releases.clear();
        if let Some(api) = api {
            if case.starts_with("cached-") {
                repo.cached_releases.push(
                    serde_json::from_value(serde_json::json!({
                        "tag_name": tag, "name": "", "body": "", "api_prerelease": api,
                        "prerelease": true, "draft": false, "assets": []
                    }))
                    .unwrap(),
                );
            } else {
                repo.release_prerelease_metadata
                    .insert(if case == "other-tag" { "v2.0.0" } else { tag }.into(), api);
            }
        }
        let manifest = archive::InstallManifest {
            installed_dll_versions: Default::default(),
            repository_id: repo.id.clone(),
            release_tag: tag.into(),
            repository_binding: (case != "missing-binding").then_some(binding),
            manual_install: None,
            package_name_i18n: Default::default(),
            group_i18n: Default::default(),
            note_i18n: Default::default(),
            prerelease: legacy,
            relations: Vec::new(),
            installed_at: "2026-11-13T00:00:00Z".into(),
            files: Vec::new(),
        };
        archive::write_manifest(&state.config.manifests_dir(), &manifest).unwrap();
        sync_manifest_groups(&state, &config(vec![repo.clone()]));
        let restored = archive::read_manifest(&state.config.manifests_dir(), &repo.id).unwrap();
        assert_eq!(restored.prerelease, expected, "{case}");
        if case == "missing-binding" {
            assert!(restored.repository_binding.is_some());
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn manifest_sync_preserves_local_provenance_when_a_repository_id_is_reused() {
    use crate::models::ModDownloadMode;
    use crate::mods::test_support::{config, repository};

    for (id, manual_install, expect_manual) in [
        ("known-package", Some(true), true),
        ("manual-legacy", None, true),
        ("manual-explicit-release", Some(false), false),
    ] {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-source-refresh")
            .join(Uuid::new_v4().to_string());
        let state = AppState::with_config(crate::config::ConfigService::for_test(root.clone()));
        let mut repo = repository(id, ModDownloadMode::Release);
        repo.name_i18n = archive::single_language("Online package");
        repo.group_i18n = archive::single_language("Online group");
        let tag = "v2.0.0";
        repo.release_prerelease_metadata.insert(tag.into(), true);
        let local_name = archive::single_language("Local package");
        let local_group = archive::single_language("Local group");
        let mut manifest = archive::new_manifest(
            id,
            tag,
            local_name.clone(),
            Default::default(),
            vec!["BepInEx/plugins/Local.dll".into()],
            Vec::new(),
        );
        manifest.manual_install = manual_install;
        manifest.group_i18n = local_group.clone();
        let manifests = state.config.manifests_dir();
        archive::write_manifest(&manifests, &manifest).unwrap();
        sync_manifest_groups(&state, &config(vec![repo.clone()]));

        let restored = archive::read_manifest(&manifests, id).unwrap();
        assert_eq!(restored.manual_install, manual_install, "{id}");
        assert_eq!(restored.is_manual(), expect_manual, "{id}");
        if expect_manual {
            assert!(restored.repository_binding.is_none(), "{id}");
            assert!(!restored.prerelease, "{id}");
            assert_eq!(restored.package_name_i18n, local_name, "{id}");
            assert_eq!(restored.group_i18n, local_group, "{id}");
        } else {
            assert_eq!(
                restored.repository_binding,
                ModRepositoryBinding::from_release(&repo, tag),
                "{id}"
            );
            assert!(restored.prerelease, "{id}");
            assert_eq!(
                restored.package_name_i18n,
                archive::single_language("Online package - GitHub"),
                "{id}"
            );
            assert_eq!(restored.group_i18n, repo.group_i18n, "{id}");
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
