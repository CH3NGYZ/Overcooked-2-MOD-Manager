use super::super::catalog::{
    SOURCE_ID, is_source, repository_from_coordinates, repository_from_search,
};
use super::super::test_support::item;
use super::*;
use crate::models::{Channel, CommunityDiscovery, CommunityMapRepository, RepositoryBinding};

#[test]
fn merge_replaces_only_the_discovered_platform_map_repositories() {
    let mut config = crate::config::default_config();
    config.community_map_repositories = vec![
        CommunityMapRepository {
            provider: Provider::Github,
            username: "Old".into(),
            repository: "Map".into(),
        },
        CommunityMapRepository {
            provider: Provider::Gitee,
            username: "Keep".into(),
            repository: "Map".into(),
        },
    ];
    merge_results(
        &mut config,
        &Provider::Github,
        SearchResults {
            repositories: Vec::new(),
            map_repositories: vec![CommunityMapRepository {
                provider: Provider::Github,
                username: "New".into(),
                repository: "Map".into(),
            }],
            discovery: CommunityDiscovery::default(),
        },
        &[],
    );
    assert_eq!(config.community_map_repositories.len(), 2);
    assert!(
        config
            .community_map_repositories
            .iter()
            .any(|repository| repository.username == "New")
    );
    assert!(
        config
            .community_map_repositories
            .iter()
            .any(|repository| repository.provider == Provider::Gitee)
    );
}

#[test]
fn community_merge_preserves_official_entries_and_installed_repositories_missing_from_search() {
    let mut official = repository_from_search(&item("Owner", "Official")).unwrap();
    official.id = "official-mod".into();
    official.source_id = None;
    official.builtin = true;
    official.note = "official description".into();
    let mut retained = repository_from_search(&item("Owner", "Installed")).unwrap();
    retained.enabled = false;
    let removed = repository_from_search(&item("Owner", "Removed")).unwrap();
    let mut config: AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1,"ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repositories":[official, retained, removed]
    }))
    .unwrap();
    let mut manifest = crate::archive::new_manifest(
        "old-id",
        "nightly",
        Default::default(),
        Default::default(),
        vec![],
        Vec::new(),
    );
    manifest.repository_binding = RepositoryBinding::from_release(&retained, "nightly");
    let found = repository_from_search(&item("Owner", "Official")).unwrap();
    merge_results(
        &mut config,
        &Provider::Github,
        SearchResults {
            map_repositories: Vec::new(),
            discovery: CommunityDiscovery {
                repository_ids: vec![found.id.clone()],
                total_count: 1,
                ..Default::default()
            },
            repositories: vec![found],
        },
        &[manifest],
    );
    assert_eq!(config.repositories.len(), 2);
    assert_eq!(config.repositories[0].id, "official-mod");
    assert_eq!(config.repositories[0].note, "official description");
    assert!(
        config
            .repositories
            .iter()
            .any(|repo| repo.id == retained.id && !repo.enabled)
    );
    assert!(!config.repositories.iter().any(|repo| repo.id == removed.id));
    assert!(
        config
            .community_discovery
            .as_ref()
            .unwrap()
            .repository_ids
            .is_empty()
    );
    assert_eq!(config.repository_sources.last().unwrap().id, SOURCE_ID);
    assert_eq!(manifest_binding_channel(&retained), Channel::All);
}

#[test]
fn official_dedup_preserves_installed_channels_and_does_not_hide_custom_or_gitee_coordinates() {
    let mut official = repository_from_search(&item("Owner", "Listed")).unwrap();
    official.id = "official-stable".into();
    official.source_id = Some("official-child".into());
    official.builtin = true;
    official.enabled = false;
    official.channel = Channel::Stable;
    let installed = repository_from_search(&item("owner", "listed")).unwrap();
    let mut manifest = crate::archive::new_manifest(
        &installed.id,
        "v1.0.0",
        Default::default(),
        Default::default(),
        vec![],
        Vec::new(),
    );
    manifest.repository_binding = RepositoryBinding::from_release(&installed, "v1.0.0");
    let binding = manifest.repository_binding.clone();

    let mut custom = repository_from_search(&item("Owner", "Custom")).unwrap();
    custom.id = "custom".into();
    custom.source_id = None;
    let mut gitee = repository_from_search(&item("Owner", "GiteeOnly")).unwrap();
    gitee.id = "gitee".into();
    gitee.source_id = Some("official-child".into());
    gitee.provider = Provider::Gitee;
    gitee.builtin = true;
    let found = vec![
        repository_from_search(&item("OWNER", "LISTED")).unwrap(),
        repository_from_search(&item("Owner", "Custom")).unwrap(),
        repository_from_search(&item("Owner", "GiteeOnly")).unwrap(),
    ];
    let visible_ids = found[1..]
        .iter()
        .map(|repo| repo.id.clone())
        .collect::<Vec<_>>();
    let mut config: AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1,"ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repositories":[official, custom, gitee, installed.clone()]
    }))
    .unwrap();
    merge_results(
        &mut config,
        &Provider::Github,
        SearchResults {
            map_repositories: Vec::new(),
            discovery: CommunityDiscovery {
                repository_ids: found.iter().map(|repo| repo.id.clone()).collect(),
                total_count: 3,
                ..Default::default()
            },
            repositories: found,
        },
        std::slice::from_ref(&manifest),
    );
    assert_eq!(
        config.community_discovery.as_ref().unwrap().repository_ids,
        visible_ids
    );
    let hidden = config
        .repositories
        .iter()
        .find(|repo| repo.id == installed.id)
        .unwrap();
    assert_eq!(hidden.channel, Channel::All);
    /*
     * 被官方仓库源覆盖的社区副本视为隐藏：不在安装页显示、也不发起刷新
     * （官方那份已经在列表里）。但它自身仍保留有效的 binding 与渠道，
     * 已安装清单的更新检测不受影响。
     */
    assert!(!hidden.enabled);
    assert_eq!(manifest.repository_binding, binding);
    assert!(binding.as_ref().unwrap().matches_repository(hidden));
    assert!(
        !binding
            .as_ref()
            .unwrap()
            .matches_repository(&config.repositories[0])
    );

    // A later empty search must not erase an installed duplicate's update entry.
    merge_results(
        &mut config,
        &Provider::Github,
        SearchResults {
            map_repositories: Vec::new(),
            repositories: vec![],
            discovery: CommunityDiscovery::default(),
        },
        &[manifest],
    );
    assert!(
        config
            .repositories
            .iter()
            .any(|repo| repo.id == installed.id)
    );
}

fn manifest_binding_channel(repo: &RepositoryConfig) -> Channel {
    RepositoryBinding::from_release(repo, "tag")
        .unwrap()
        .channel
}

#[test]
fn gitee_merge_preserves_github_catalogue_and_uses_provider_scoped_official_dedup() {
    let github = repository_from_search(&item("Owner", "Listed")).unwrap();
    let found =
        repository_from_coordinates(&Provider::Gitee, "owner", "listed", "", "Gitee description")
            .unwrap();
    assert_ne!(github.id, found.id);
    let mut official = found.clone();
    official.id = "official-gitee".into();
    official.builtin = true;
    official.source_id = Some("official-child".into());
    official.channel = Channel::Stable;
    let unique =
        repository_from_coordinates(&Provider::Gitee, "Owner", "New", "", "New description")
            .unwrap();
    let mut manifest = crate::archive::new_manifest(
        &found.id,
        "v1.0.0",
        Default::default(),
        Default::default(),
        vec![],
        Vec::new(),
    );
    manifest.repository_binding = RepositoryBinding::from_release(&found, "v1.0.0");
    let mut config: AppConfig = serde_json::from_value(serde_json::json!({
            "schema_version":1,"ui_zoom":1.5,
            "game":{"path":""},
            "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
            "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
            "auth":{"github_token":"","gitee_token":"","official_api_only":true},
            "repositories":[github, official, found],
            "community_discovery":{"repository_ids":[github.id],"total_count":1,"incomplete_results":false,"truncated":false}
        })).unwrap();
    let github_before = serde_json::to_value(&config.repositories[0]).unwrap();
    let discovery_before = serde_json::to_value(&config.community_discovery).unwrap();
    merge_results(
        &mut config,
        &Provider::Gitee,
        SearchResults {
            map_repositories: Vec::new(),
            discovery: CommunityDiscovery {
                repository_ids: vec![found.id.clone(), unique.id.clone()],
                total_count: 2,
                ..Default::default()
            },
            repositories: vec![found.clone(), unique.clone()],
        },
        std::slice::from_ref(&manifest),
    );
    assert_eq!(
        serde_json::to_value(&config.repositories[0]).unwrap(),
        github_before
    );
    assert_eq!(
        serde_json::to_value(&config.community_discovery).unwrap(),
        discovery_before
    );
    assert_eq!(
        config
            .gitee_community_discovery
            .as_ref()
            .unwrap()
            .repository_ids,
        vec![unique.id]
    );
    let hidden = config
        .repositories
        .iter()
        .find(|repo| repo.id == found.id)
        .unwrap();
    assert_eq!(hidden.channel, Channel::All);
    assert!(
        manifest
            .repository_binding
            .as_ref()
            .unwrap()
            .matches_repository(hidden)
    );
    assert!(is_source(
        config.repository_sources.last().unwrap().id.as_str()
    ));
    let round_trip: AppConfig = toml::from_str(&toml::to_string(&config).unwrap()).unwrap();
    assert_eq!(
        round_trip
            .gitee_community_discovery
            .unwrap()
            .repository_ids
            .len(),
        1
    );
    assert_eq!(
        serde_json::to_value(&round_trip.community_discovery).unwrap(),
        discovery_before
    );

    merge_results(
        &mut config,
        &Provider::Gitee,
        SearchResults {
            map_repositories: Vec::new(),
            repositories: vec![],
            discovery: CommunityDiscovery::default(),
        },
        &[manifest],
    );
    assert!(config.repositories.iter().any(|repo| repo.id == found.id));
    assert_eq!(
        serde_json::to_value(&config.repositories[0]).unwrap(),
        github_before
    );
}
