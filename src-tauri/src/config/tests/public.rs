use crate::config::ConfigService;
use crate::config::defaults::default_config;
use crate::config::identity::repository_from_export;
use crate::models::*;
use std::path::Path;

use super::REGION_REPOSITORY;

#[test]
fn public_discovery_hides_late_official_descendants_without_erasing_saved_bindings() {
    let service = ConfigService::for_test(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/verification/community-official-dedup-20261014/config-fixture"),
    );
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        let community = crate::community::repository_from_coordinates(
            &provider,
            "Owner",
            "Listed",
            "main",
            "community",
        )
        .unwrap();
        let binding = crate::models::RepositoryBinding::from_release(&community, "v1.0.0").unwrap();
        let mut config = default_config();
        let mut child = config.repository_sources[0].clone();
        child.id = "official-child".into();
        child.parent_id = Some(config.repository_sources[0].id.clone());
        child.builtin = false;
        let mut leaf = child.clone();
        leaf.id = "official-leaf".into();
        leaf.parent_id = Some(child.id.clone());
        config.repository_sources.extend([leaf, child]);
        config.repositories.push(community.clone());
        let discovery = crate::models::CommunityDiscovery {
            repository_ids: vec![community.id.clone()],
            total_count: 1,
            ..Default::default()
        };
        match provider {
            Provider::Github => config.community_discovery = Some(discovery),
            Provider::Gitee => config.gitee_community_discovery = Some(discovery),
            Provider::Gitcode => config.gitcode_community_discovery = Some(discovery),
        }
        let ids = |public: &PublicConfig| {
            match provider {
                Provider::Github => public.community_discovery.as_ref(),
                Provider::Gitee => public.gitee_community_discovery.as_ref(),
                Provider::Gitcode => public.gitcode_community_discovery.as_ref(),
            }
            .unwrap()
            .repository_ids
            .clone()
        };
        assert_eq!(ids(&service.public_config(&config)), [community.id.clone()]);
        let mut official = community.clone();
        official.id = "official-mod".into();
        official.source_id = Some("official-leaf".into());
        official.username = "OWNER".into();
        official.repository = "LISTED".into();
        official.builtin = false;
        official.enabled = false;
        official.channel = Channel::Stable;
        config.repositories.push(official);
        let before = serde_json::to_value(&config).unwrap();
        let public = service.public_config(&config);
        assert!(ids(&public).is_empty(), "{provider:?}");
        assert_eq!(serde_json::to_value(&config).unwrap(), before);
        assert!(
            public
                .repositories
                .iter()
                .any(|repo| repo.id == community.id
                    && repo.channel == Channel::All
                    && binding.matches_repository(repo))
        );
        config.repositories.retain(|repo| repo.id != "official-mod");
        assert_eq!(ids(&service.public_config(&config)), [community.id]);
    }
}

#[test]
fn structured_repository_fields_survive_storage_and_public_config_without_urls() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let export = crate::repository_sources::parse_document(REGION_REPOSITORY, &location)
        .unwrap()
        .repositories
        .remove(0);
    let mut config = default_config();
    config.repositories = vec![repository_from_export(&export, "structured".into(), false)];
    let stored = toml::to_string_pretty(&config).unwrap();
    let restored: AppConfig = toml::from_str(&stored).unwrap();
    let repo = &restored.repositories[0];
    assert_eq!(repo.username, "example");
    assert_eq!(repo.repository, "mod");
    assert_eq!(repo.channel, Channel::Stable);
    let public = serde_json::to_value(service.public_config(&restored)).unwrap();
    let public_repo = &public["repositories"][0];
    assert_eq!(public_repo["username"], "example");
    assert_eq!(public_repo["repository"], "mod");
    assert!(public_repo.get("url").is_none());
    assert!(public_repo.get("owner").is_none());
    let serialized_repo = serde_json::to_value(repo).unwrap();
    assert!(serialized_repo.get("url").is_none());
    let old_format = REGION_REPOSITORY.replace(
        "username = \"example\"\nrepository = \"mod\"",
        "url = \"https://github.com/example/mod\"",
    );
    assert!(crate::repository_sources::parse_document(&old_format, &location).is_err());
}

#[test]
fn gitcode_token_roundtrips_without_entering_public_config() {
    let service = ConfigService::for_test(std::env::temp_dir().join("oc2-gitcode-token-fixture"));
    let mut config = default_config();
    service
        .set_tokens(
            &mut config,
            "github-fixture",
            "gitee-fixture",
            "gitcode-fixture",
        )
        .unwrap();
    assert_eq!(
        service.api_token(&config, &Provider::Gitcode).as_deref(),
        Some("gitcode-fixture")
    );
    assert_ne!(config.auth.gitcode_token, "gitcode-fixture");
    let encrypted = config.auth.gitcode_token.clone();
    service.set_tokens(&mut config, "", "", "").unwrap();
    assert_eq!(config.auth.gitcode_token, encrypted);
    let restored: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        service.api_token(&restored, &Provider::Gitcode).as_deref(),
        Some("gitcode-fixture")
    );
    let public = serde_json::to_string(&service.public_config(&restored)).unwrap();
    assert!(public.contains("\"gitcode_token_set\":true"));
    assert!(!public.contains("gitcode-fixture"));
    assert!(!public.contains(&encrypted));
}
