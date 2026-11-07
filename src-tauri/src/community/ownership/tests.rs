use super::super::catalog::{SearchResults, repository_from_coordinates};
use super::super::merge::merge_results;
use super::*;
use crate::models::{Provider, RepositorySource};

#[test]
fn community_merge_checks_official_source_tree_before_addition() {
    for provider in [Provider::Github, Provider::Gitee] {
        let mut official =
            repository_from_coordinates(&provider, "Owner", "Listed", "main", "official").unwrap();
        official.id = "official-mod".into();
        official.source_id = Some("official-leaf".into());
        official.builtin = false;
        official.enabled = false;
        let found =
            repository_from_coordinates(&provider, "owner", "LISTED", "main", "community").unwrap();
        let mut config: AppConfig = serde_json::from_value(serde_json::json!({
                "schema_version":1,"ui_zoom":1.5,
                "game":{"path":""},
                "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
                "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
                "auth":{"github_token":"","gitee_token":"","official_api_only":true},
                "repository_sources":[
                    {"id":"official-leaf","parent_id":"official-child","nickname":"Leaf","url":"leaf.toml","builtin":false},
                    {"id":"official-child","parent_id":"official-repository-index","nickname":"Child","url":"child.toml","builtin":false},
                    {"id":"official-repository-index","nickname":"Official","url":"repositories.toml","builtin":true}
                ],
                "repositories":[official.clone()]
            })).unwrap();
        merge_results(
            &mut config,
            &provider,
            SearchResults {
                map_repositories: Vec::new(),
                discovery: CommunityDiscovery {
                    repository_ids: vec![found.id.clone()],
                    total_count: 1,
                    ..Default::default()
                },
                repositories: vec![found],
            },
            &[],
        );
        assert_eq!(config.repositories.len(), 1, "{provider:?}");
        assert_eq!(config.repositories[0].note, official.note);
        let discovery = match provider {
            Provider::Github => config.community_discovery.as_ref(),
            Provider::Gitee => config.gitee_community_discovery.as_ref(),
        };
        assert!(discovery.unwrap().repository_ids.is_empty());

        config.repository_sources.extend(
            serde_json::from_value::<Vec<RepositorySource>>(serde_json::json!([
                {"id":"custom","nickname":"Custom","url":"custom.toml"},
                {"id":"cycle-a","parent_id":"cycle-b","nickname":"A","url":"a.toml"},
                {"id":"cycle-b","parent_id":"cycle-a","nickname":"B","url":"b.toml"},
                {"id":"legacy-root","nickname":"Legacy","url":"legacy.toml","builtin":true}
            ]))
            .unwrap(),
        );
        for (source, expected) in [
            ("official-repository-index", true),
            ("official-leaf", true),
            ("legacy-root", true),
            ("custom", false),
            ("cycle-a", false),
            ("missing-parent", false),
            (source_id(&provider), false),
        ] {
            let mut configured = official.clone();
            configured.source_id = Some(source.into());
            config.repositories = vec![configured.clone()];
            let keys = official_repository_keys(&config);
            assert_eq!(
                keys.contains(&identity(&configured)),
                expected,
                "{provider:?}/{source}"
            );
            configured.provider = match provider {
                Provider::Github => Provider::Gitee,
                Provider::Gitee => Provider::Github,
            };
            assert!(!keys.contains(&identity(&configured)), "{source}");
        }
    }
}

/*
 * 地区过滤会把不适用的官方仓库从 config.repositories 里剔除。
 * 这些仓库必须仍然被社区搜索回避，否则它们会以社区结果的身份重新出现。
 * 内置快照是不做地区过滤的完整清单，这里验证它确实并入了回避集合。
 */
#[test]
fn community_search_avoids_bundled_repositories_missing_from_config() {
    let bundled = crate::repository_sources::bundled_repository_configs();
    assert!(!bundled.is_empty(), "内置快照应当包含仓库条目");
    let config: AppConfig = serde_json::from_value(serde_json::json!({
        "schema_version":1,"ui_zoom":1.5,
        "game":{"path":""},
        "network":{"timeout_seconds":5,"download_threads":1,"max_retry_count":3},
        "launch":{"ask_user_to_start_from_steam":false,"auto_restore_winhttp":false},
        "auth":{"github_token":"","gitee_token":"","official_api_only":true},
        "repository_sources":[],
        "repositories":[]
    }))
    .unwrap();
    assert!(config.repositories.is_empty());
    let keys = official_repository_keys(&config);
    for repo in &bundled {
        // 内置快照里的每个仓库都必须被回避，即便 config 里一个都没有。
        let key = (
            source_id(&repo.provider),
            repo.username.to_ascii_lowercase(),
            repo.repository.to_ascii_lowercase(),
        );
        assert!(keys.contains(&key), "{}/{}", repo.username, repo.repository);
    }
}
