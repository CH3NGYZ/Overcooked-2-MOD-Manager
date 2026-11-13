use crate::config::ConfigService;
use crate::config::availability::REGION_CHINESE_MAINLAND;
use crate::config::availability::REGION_INTERNATIONAL;
use crate::config::bundle::BUNDLE_FORMAT;
use crate::config::defaults::default_config;
use crate::config::identity::repo_key;
use crate::config::identity::repository_address_key;
use crate::models::*;

use super::REGION_REPOSITORY;

#[test]
fn regional_repository_variants_coexist_and_export_all_channels() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let document = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap();
    let repositories = [REGION_CHINESE_MAINLAND, REGION_INTERNATIONAL].map(|region| {
        let mut repository = document.repositories[0].clone();
        repository.region = Some(region.into());
        repository.download_threads = 16;
        repository
    });
    let bundle = ModRepositoryBundle {
        format: BUNDLE_FORMAT.into(),
        schema_version: 1,
        exported_at: String::new(),
        repositories: repositories.to_vec(),
        repository_sources: Vec::new(),
    };
    let mut config = default_config();
    service.import_bundle(&mut config, &bundle, "overwrite");
    assert_eq!(config.repositories.len(), 2);
    assert!(
        config
            .repositories
            .iter()
            .all(|repo| repo.name == "Regional MOD")
    );
    assert_eq!(
        config
            .repositories
            .iter()
            .map(|repo| repo.region.as_deref())
            .collect::<Vec<_>>(),
        vec![Some(REGION_CHINESE_MAINLAND), Some(REGION_INTERNATIONAL)]
    );
    let original_ids = config
        .repositories
        .iter()
        .map(|repo| repo.id.clone())
        .collect::<Vec<_>>();
    service.import_bundle(&mut config, &bundle, "overwrite");
    assert_eq!(
        config
            .repositories
            .iter()
            .map(|repo| repo.id.clone())
            .collect::<Vec<_>>(),
        original_ids
    );
    let exported = service.export_bundle(&config, None).unwrap();
    let restored = service.validate_bundle(&exported).unwrap();
    assert_eq!(
        serde_json::to_value(&restored.repositories).unwrap(),
        serde_json::to_value(&repositories).unwrap()
    );
    let mut imported = default_config();
    service.import_bundle(&mut imported, &restored, "overwrite");
    assert_eq!(
        imported
            .repositories
            .iter()
            .map(|repo| repo.region.as_deref())
            .collect::<Vec<_>>(),
        vec![Some(REGION_CHINESE_MAINLAND), Some(REGION_INTERNATIONAL)]
    );
}

#[test]
fn channel_variants_of_one_repository_coexist_with_distinct_stable_ids() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let document = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap();
    let repositories = [Channel::Stable, Channel::Beta].map(|channel| {
        let mut repository = document.repositories[0].clone();
        repository.channel = channel;
        repository.download_threads = 16;
        repository.name_i18n.insert(
            "zh-CN".into(),
            match channel {
                Channel::Beta => "Regional MOD - Beta".into(),
                _ => "Regional MOD - Stable".into(),
            },
        );
        repository
    });
    let bundle = ModRepositoryBundle {
        format: BUNDLE_FORMAT.into(),
        schema_version: 1,
        exported_at: String::new(),
        repositories: repositories.to_vec(),
        repository_sources: Vec::new(),
    };
    let mut config = default_config();
    let preview = service.import_bundle(&mut config, &bundle, "overwrite");
    assert_eq!(config.repositories.len(), 2);
    assert_eq!(preview.additions.len(), 2);
    assert!(preview.duplicates.is_empty());
    assert_eq!(
        config
            .repositories
            .iter()
            .map(|repo| repo.channel)
            .collect::<Vec<_>>(),
        vec![Channel::Stable, Channel::Beta]
    );
    assert_ne!(
        repo_key(&config.repositories[0]),
        repo_key(&config.repositories[1])
    );
    let original_ids = config
        .repositories
        .iter()
        .map(|repo| repo.id.clone())
        .collect::<Vec<_>>();
    service.import_bundle(&mut config, &bundle, "overwrite");
    assert_eq!(
        config
            .repositories
            .iter()
            .map(|repo| repo.id.clone())
            .collect::<Vec<_>>(),
        original_ids
    );
    let beta = &config.repositories[1];
    let binding = crate::models::ModRepositoryBinding::from_release(beta, "v1.0.0-beta.1").unwrap();
    assert_eq!(binding.channel, Channel::Beta);
    assert!(binding.matches_repository(beta));
    assert!(!binding.matches_repository(&config.repositories[0]));

    let exported = service.export_bundle(&config, None).unwrap();
    let restored = service.validate_bundle(&exported).unwrap();
    let mut imported = default_config();
    service.import_bundle(&mut imported, &restored, "overwrite");
    assert_eq!(
        imported
            .repositories
            .iter()
            .map(|repo| repo.channel)
            .collect::<Vec<_>>(),
        vec![Channel::Stable, Channel::Beta]
    );
}

#[test]
fn repository_identity_uses_provider_username_repository_and_region() {
    let github = repository_address_key(&Provider::Github, " Example ", "MOD", Some("global"));
    assert_eq!(
        github,
        repository_address_key(&Provider::Github, "example", "mod", Some("global"))
    );
    assert_ne!(
        github,
        repository_address_key(&Provider::Gitee, "example", "mod", Some("global"))
    );
    assert_ne!(
        github,
        repository_address_key(&Provider::Github, "example", "mod", Some("cn"))
    );
    assert_ne!(
        github,
        repository_address_key(&Provider::Github, "other", "mod", Some("global"))
    );
    assert_ne!(
        github,
        repository_address_key(&Provider::Github, "example", "another", Some("global"))
    );
}

#[test]
fn network_plan_changes_do_not_change_repository_identity() {
    let location = crate::repository_sources::SourceLocation::parse("mods.toml").unwrap();
    let mut document = crate::repository_sources::parse_document(
        REGION_REPOSITORY,
        &location,
        &Default::default(),
    )
    .unwrap();
    let export = &mut document.repositories[0];
    let before = crate::config::export_key(export);
    export.network_plan = toml::from_str(
        "international = { id = 'github-public', release = { download_threads = 8 } }",
    )
    .unwrap();
    assert_eq!(before, crate::config::export_key(export));
    let repo = crate::config::identity::mod_repository_from_export(export, "same-id".into(), false);
    assert_eq!(before, repo_key(&repo));
}
