use super::super::binding::{installed_prerelease, rebind_installed_mods};
use super::*;

#[test]
fn exact_api_false_promotes_numeric_beta_after_filter_and_reopen() {
    let mut repo = binding_repository();
    repo.channel = crate::models::Channel::Beta;
    let tag = "v1.9.9.2";
    let binding = crate::models::ModRepositoryBinding::from_release(&repo, tag).unwrap();
    let releases = crate::releases::parse_release_json(
        &serde_json::json!([
            {"tag_name": tag, "prerelease": false},
            {"tag_name": "v1.9.9-beta.3", "prerelease": true}
        ]),
        &repo.provider,
    )
    .unwrap();
    let snapshot = crate::releases::project_mod_releases(releases, &repo.channel);
    assert_eq!(snapshot.cached_releases.len(), 1);
    repo.cached_releases = snapshot.cached_releases;
    repo.release_prerelease_metadata = snapshot.release_prerelease_metadata;
    repo.releases_cached_at = Some(100);
    let mut config = crate::config::default_config();
    config.repositories = vec![repo];
    let saved = toml::to_string(&config).unwrap();
    let config: crate::models::AppConfig = toml::from_str(&saved).unwrap();
    assert!(!installed_prerelease(
        &config,
        Some(&binding),
        true,
        Some(tag)
    ));
    let mut local = vec![plain_mod(
        "missing.dll",
        serde_json::json!({
            "version": "1.9.9.2", "update_version": tag,
            "prerelease": true, "repository_binding": binding
        }),
    )];
    rebind_installed_mods(&config, &mut local);
    assert!(
        !local[0].prerelease,
        "rebind uses parsed facts without reading missing.dll"
    );
    let mut failed = config.clone();
    failed.repositories[0].last_error = Some("offline".into());
    assert!(!installed_prerelease(
        &failed,
        local[0].repository_binding.as_ref(),
        true,
        Some(tag)
    ));
    let repo = failed.repositories.remove(0);
    failed.staged_release_cache.insert(
        repo.id.clone(),
        crate::models::StagedReleaseCache {
            cached_releases_complete: repo.cached_releases_complete,
            repository_key: Some(crate::releases::repository_cache_key(
                repo.provider,
                &repo.username,
                &repo.repository,
            )),
            release_prerelease_metadata: repo.release_prerelease_metadata,
            releases: repo.cached_releases,
            cached_at: repo.releases_cached_at,
            last_error: repo.last_error,
        },
    );
    assert!(
        !installed_prerelease(
            &failed,
            local[0].repository_binding.as_ref(),
            true,
            Some(tag)
        ),
        "source rebuild staging preserves exact authority even without an installable tag"
    );
}

#[test]
fn unrelated_tags_and_repository_coordinates_cannot_clear_history() {
    let mut repo = binding_repository();
    let tag = "v1.0.0.1";
    let binding = crate::models::ModRepositoryBinding::from_release(&repo, tag).unwrap();
    repo.release_prerelease_metadata
        .insert("v2.0.0".into(), false);
    let mut config = crate::config::default_config();
    config.repositories = vec![repo.clone()];
    assert!(installed_prerelease(
        &config,
        Some(&binding),
        true,
        Some(tag)
    ));
    for provider in [
        crate::models::Provider::Github,
        crate::models::Provider::Gitcode,
    ] {
        let mut other = repo.clone();
        other.release_prerelease_metadata.insert(tag.into(), false);
        other.provider = provider;
        config.repositories = vec![other];
        assert!(
            installed_prerelease(&config, Some(&binding), true, Some(tag)),
            "provider"
        );
    }
    for field in ["owner", "repository"] {
        let mut other = repo.clone();
        other.release_prerelease_metadata.insert(tag.into(), false);
        match field {
            "owner" => other.username = "someone-else".into(),
            _ => other.repository = "another-repo".into(),
        }
        config.repositories = vec![other];
        assert!(
            installed_prerelease(&config, Some(&binding), true, Some(tag)),
            "{field}"
        );
    }
    repo.release_prerelease_metadata.insert(tag.into(), false);
    config.repositories = vec![repo];
    assert!(
        installed_prerelease(&config, None, true, Some(tag)),
        "GUID or latest alone cannot provide authority"
    );
}

#[test]
fn newest_exact_verdict_wins_across_channels() {
    let mut old = binding_repository();
    let tag = "v1.0.0.1";
    let binding = crate::models::ModRepositoryBinding::from_release(&old, tag).unwrap();
    old.release_prerelease_metadata.insert(tag.into(), true);
    old.releases_cached_at = Some(10);
    let mut fresh = old.clone();
    fresh.channel = crate::models::Channel::Beta;
    fresh.release_prerelease_metadata.insert(tag.into(), false);
    fresh.releases_cached_at = Some(20);
    let mut config = crate::config::default_config();
    config.repositories = vec![old, fresh];
    assert!(!installed_prerelease(
        &config,
        Some(&binding),
        true,
        Some(tag)
    ));
}

#[test]
fn explicit_beta_binding_stays_prerelease_after_platform_false_and_reopen() {
    for tag in ["v2.0.0-beta.3", "2.0-beta.3"] {
        let mut repo = binding_repository();
        repo.channel = crate::models::Channel::Beta;
        let binding = crate::models::ModRepositoryBinding::from_release(&repo, tag).unwrap();
        let snapshot = crate::releases::project_mod_releases(
            crate::releases::parse_release_json(
                &serde_json::json!({"tag_name": tag, "prerelease": false}),
                &repo.provider,
            )
            .unwrap(),
            &repo.channel,
        );
        repo.cached_releases = snapshot.cached_releases;
        repo.release_prerelease_metadata = snapshot.release_prerelease_metadata;
        let mut config = crate::config::default_config();
        config.repositories = vec![repo];
        let config: crate::models::AppConfig =
            toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
        assert_eq!(
            config.repositories[0].release_prerelease_metadata.get(tag),
            Some(&false)
        );
        for dll_version in ["2.0", "2.0.0", "2.0.0.3", "2.0-beta.3"] {
            assert!(
                installed_prerelease(&config, Some(&binding), false, Some(dll_version)),
                "{tag}, {dll_version}"
            );
            let mut local = vec![plain_mod(
                "missing.dll",
                serde_json::json!({
                    "guid": "com.example.guid", "version": dll_version, "update_version": tag,
                    "prerelease": false, "repository_binding": binding
                }),
            )];
            rebind_installed_mods(&config, &mut local);
            assert!(local[0].prerelease, "{tag}, {dll_version}");
            assert_eq!(local[0].status, "latest");
        }
    }
}

#[test]
fn invalid_binding_cannot_supply_an_explicit_beta_suffix() {
    let repo = binding_repository();
    let mut binding =
        crate::models::ModRepositoryBinding::from_release(&repo, "v2.0.0-beta.3").unwrap();
    binding.owner.clear();
    let config = crate::config::default_config();
    assert!(!installed_prerelease(
        &config,
        Some(&binding),
        false,
        Some("2.0")
    ));
}
