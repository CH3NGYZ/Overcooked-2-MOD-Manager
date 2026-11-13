use super::*;
use std::path::Path;

#[test]
fn manifest_sync_uses_exact_platform_verdict_and_preserves_unknown_history() {
    use crate::models::{ModDownloadMode, Provider};
    use crate::mods::test_support::{config, repository};
    for (case, api, legacy, tag, expected) in [
        ("metadata-false", Some(false), true, "v1.0.0-beta", false),
        ("cached-false", Some(false), true, "v1.0.0-beta", false),
        ("explicit-true", Some(true), false, "v1.0.0", true),
        ("legacy-true", None, true, "v1.0.0", true),
        ("legacy-suffix", None, false, "v1.0.0-beta", true),
        ("legacy-stable", None, false, "v1.0.0", false),
        ("other-tag", Some(false), true, "v1.0.0-beta", true),
        ("other-provider", Some(false), true, "v1.0.0-beta", true),
        ("other-owner", Some(false), true, "v1.0.0-beta", true),
        ("other-repository", Some(false), true, "v1.0.0-beta", true),
        ("missing-binding", Some(false), true, "v1.0.0-beta", false),
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
            if case == "cached-false" {
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
            repository_id: repo.id.clone(),
            release_tag: tag.into(),
            repository_binding: (case != "missing-binding").then_some(binding),
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
