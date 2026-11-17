use super::repository_source_refresh_tests::{refresh, serve_documents, test_state};
use super::*;

fn declaration(id: &str, url: &str, region: &str) -> String {
    format!(
        "[[repository_sources]]\nid = {id:?}\nnickname = {id:?}\nurl = {url:?}\nregion = {region:?}\n"
    )
}

fn document(ids: &[&str]) -> String {
    ids.iter()
        .map(|id| declaration(id, &format!("{id}.toml"), ""))
        .collect()
}

fn source(id: &str, url: String, parent: Option<&str>) -> RepositorySource {
    RepositorySource {
        id: id.into(),
        nickname: id.into(),
        nickname_i18n: Default::default(),
        note_i18n: Default::default(),
        url,
        region: None,
        builtin: false,
        parent_id: parent.map(str::to_string),
        network_plan: Default::default(),
    }
}

fn child_ids(config: &AppConfig, parent: &str) -> Vec<String> {
    config
        .repository_sources
        .iter()
        .filter(|source| source.parent_id.as_deref() == Some(parent))
        .map(|source| source.id.clone())
        .collect()
}

#[tokio::test]
async fn successful_refresh_orders_direct_children_and_preserves_other_sources_and_caches() {
    for nested in [false, true] {
        let (address, server) = serve_documents(vec![
            ("/parent.toml", "200 OK", document(&["a", "b"])),
            ("/parent.toml", "200 OK", document(&["new", "b", "a"])),
        ]);
        let state = test_state();
        let root = source("root", format!("http://{address}/root.toml"), None);
        let parent = source(
            "parent",
            format!("http://{address}/parent.toml"),
            nested.then_some("root"),
        );
        let unrelated = source(
            "unrelated",
            "https://example.com/unrelated.toml".into(),
            None,
        );
        let mut config = state.config.load().unwrap();
        config.repository_sources = vec![root, parent.clone(), unrelated.clone()];
        state.config.save(&config).unwrap();
        refresh(&state, &parent).await.unwrap();
        let mut before = state.config.load().unwrap();
        assert_eq!(child_ids(&before, "parent"), ["a", "b"]);
        // Interleave unrelated sources and a descendant so reordering cannot
        // accidentally replace the whole source tree or rebuild descendants.
        let descendant = source(
            "a-child",
            format!("http://{address}/a-child.toml"),
            Some("a"),
        );
        before.repository_sources.insert(4, descendant);
        before.repository_sources.push(source(
            "unrelated-child",
            "https://example.com/unrelated-child.toml".into(),
            Some("unrelated"),
        ));
        let mut repository = crate::mods::test_support::repository(
            "retained-repository",
            crate::models::ModDownloadMode::Release,
        );
        repository.source_id = Some("a-child".into());
        repository.releases_cached_at = Some(123);
        let mut direct_repository = repository.clone();
        direct_repository.id = "direct-child-repository".into();
        direct_repository.source_id = Some("a".into());
        before.repositories.push(repository);
        before.repositories.push(direct_repository);
        before.staged_release_cache.insert(
            "unrelated-staged".into(),
            crate::models::StagedReleaseCache {
                cached_at: Some(456),
                ..Default::default()
            },
        );
        before.source_repository_ids.insert(
            "unrelated-source-identity".into(),
            "unrelated-stable-id".into(),
        );
        state.config.save(&before).unwrap();
        refresh(&state, &parent).await.unwrap();
        let after = state.config.load().unwrap();
        assert_eq!(child_ids(&after, "parent"), ["new", "b", "a"]);
        let unaffected = |config: &AppConfig| {
            config
                .repository_sources
                .iter()
                .filter(|source| source.parent_id.as_deref() != Some("parent"))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(unaffected(&after), unaffected(&before));
        assert_eq!(
            serde_json::to_value(&after.repositories).unwrap(),
            serde_json::to_value(&before.repositories).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&after.staged_release_cache).unwrap(),
            serde_json::to_value(&before.staged_release_cache).unwrap()
        );
        assert_eq!(after.source_repository_ids, before.source_repository_ids);
        server.join().unwrap();
        std::fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
    }
}

#[tokio::test]
async fn accepted_declaration_order_keeps_allocated_ids_regions_and_reserved_sources() {
    let first = format!(
        "{}{}{}{}{}{}",
        declaration("github-community", "reserved.toml", ""),
        declaration("self", "parent.toml", ""),
        declaration("taken", "allocated.toml", ""),
        declaration("duplicate-owner", "shared.toml", "cn"),
        declaration("regional", "shared.toml", "global"),
        declaration("duplicate-document", "shared.toml", "global")
    );
    let second = format!(
        "{}{}",
        declaration("regional", "shared.toml", "global"),
        declaration("taken", "allocated.toml", "")
    );
    let (address, server) = serve_documents(vec![
        ("/parent.toml", "200 OK", first),
        ("/parent.toml", "200 OK", second),
    ]);
    let state = test_state();
    let parent = source("parent", format!("http://{address}/parent.toml"), None);
    let mut config = state.config.load().unwrap();
    let occupied = source("taken", "https://example.com/external.toml".into(), None);
    let mut owner = source("owner", format!("http://{address}/shared.toml"), None);
    owner.region = Some("cn".into());
    let reserved = source(
        "github-community",
        "https://example.com/community".into(),
        None,
    );
    config.repository_sources = vec![
        parent.clone(),
        occupied.clone(),
        owner.clone(),
        reserved.clone(),
    ];
    state.config.save(&config).unwrap();
    refresh(&state, &parent).await.unwrap();
    let mut before = state.config.load().unwrap();
    let initial = child_ids(&before, "parent");
    assert_eq!(initial.len(), 2);
    let allocated = initial[0].clone();
    assert_ne!(allocated, "taken");
    assert_eq!(initial[1], "regional");
    before.repository_sources.push(source(
        "retained-descendant",
        format!("http://{address}/descendant.toml"),
        Some(&allocated),
    ));
    let mut repository =
        crate::mods::test_support::repository("cached", crate::models::ModDownloadMode::Release);
    repository.source_id = Some("retained-descendant".into());
    before.repositories.push(repository);
    state.config.save(&before).unwrap();
    refresh(&state, &parent).await.unwrap();
    let after = state.config.load().unwrap();
    assert_eq!(
        child_ids(&after, "parent"),
        ["regional".to_string(), allocated]
    );
    for unchanged in before
        .repository_sources
        .iter()
        .filter(|source| source.parent_id.as_deref() != Some("parent"))
    {
        assert!(
            after.repository_sources.contains(unchanged),
            "lost source {}",
            unchanged.id
        );
    }
    assert_eq!(
        serde_json::to_value(&after.repositories).unwrap(),
        serde_json::to_value(&before.repositories).unwrap()
    );
    server.join().unwrap();
    std::fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
}

#[tokio::test]
async fn allocated_alias_does_not_claim_an_explicit_sibling_id() {
    for alias_first in [false, true] {
        let original = format!(
            "{}{}",
            declaration("c", "old.toml", ""),
            declaration("d", "d.toml", "")
        );
        let explicit = declaration("c", "new.toml", "");
        let alias = declaration("taken", "old.toml", "");
        let changed = if alias_first {
            alias.clone() + &explicit
        } else {
            explicit + &alias
        } + &declaration("d", "d.toml", "");
        let (address, server) = serve_documents(vec![
            ("/parent.toml", "200 OK", original),
            ("/parent.toml", "200 OK", changed.clone()),
            ("/parent.toml", "200 OK", changed),
        ]);
        let state = test_state();
        let parent = source("parent", format!("http://{address}/parent.toml"), None);
        let occupied = source("taken", "https://example.com/external.toml".into(), None);
        let mut config = state.config.load().unwrap();
        config.repository_sources = vec![parent.clone(), occupied.clone()];
        state.config.save(&config).unwrap();
        refresh(&state, &parent).await.unwrap();
        let mut before = state.config.load().unwrap();
        for id in ["c", "d"] {
            before.repository_sources.push(source(
                &format!("{id}-child"),
                format!("http://{address}/{id}-child.toml"),
                Some(id),
            ));
            let mut repository = crate::mods::test_support::repository(
                &format!("{id}-cached"),
                crate::models::ModDownloadMode::Release,
            );
            repository.source_id = Some(format!("{id}-child"));
            repository.releases_cached_at = Some(123);
            before.repositories.push(repository);
        }
        state.config.save(&before).unwrap();
        refresh(&state, &parent).await.unwrap();
        let after = state.config.load().unwrap();
        let children = child_ids(&after, "parent");
        assert_eq!(
            children.len(),
            3,
            "all three accepted declarations must survive"
        );
        assert_eq!(
            children.iter().collect::<HashSet<_>>().len(),
            3,
            "effective IDs must be unique"
        );
        let allocated = children
            .iter()
            .find(|id| *id != "c" && *id != "d")
            .unwrap()
            .clone();
        let expected = if alias_first {
            vec![allocated.clone(), "c".into(), "d".into()]
        } else {
            vec!["c".into(), allocated.clone(), "d".into()]
        };
        assert_eq!(children, expected);
        assert_ne!(allocated, "taken");
        for (id, file) in [
            ("c", "new.toml"),
            (allocated.as_str(), "old.toml"),
            ("d", "d.toml"),
        ] {
            assert_eq!(
                after
                    .repository_sources
                    .iter()
                    .find(|source| source.id == id)
                    .unwrap()
                    .url,
                format!("http://{address}/{file}")
            );
        }
        let unaffected = |config: &AppConfig| {
            config
                .repository_sources
                .iter()
                .filter(|source| source.parent_id.as_deref() != Some("parent"))
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(unaffected(&after), unaffected(&before));
        assert_eq!(
            serde_json::to_value(&after.repositories).unwrap(),
            serde_json::to_value(&before.repositories).unwrap()
        );
        // The alias must recover its actual UUID on the next identical refresh.
        refresh(&state, &parent).await.unwrap();
        let repeated = state.config.load().unwrap();
        assert_eq!(child_ids(&repeated, "parent"), expected);
        assert_eq!(unaffected(&repeated), unaffected(&before));
        assert_eq!(
            serde_json::to_value(&repeated.repositories).unwrap(),
            serde_json::to_value(&before.repositories).unwrap()
        );
        server.join().unwrap();
        std::fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
    }
}

#[tokio::test]
async fn failed_refresh_keeps_child_order_and_all_persisted_state() {
    let (address, server) = serve_documents(vec![
        ("/parent.toml", "200 OK", document(&["b", "a"])),
        ("/parent.toml", "503 Service Unavailable", String::new()),
        ("/parent.toml", "200 OK", "invalid = [".into()),
    ]);
    let state = test_state();
    let parent = source("parent", format!("http://{address}/parent.toml"), None);
    let mut config = state.config.load().unwrap();
    config.repository_sources = vec![parent.clone()];
    state.config.save(&config).unwrap();
    refresh(&state, &parent).await.unwrap();
    let before = serde_json::to_value(state.config.load().unwrap()).unwrap();
    let disk = std::fs::read(state.config.config_path()).unwrap();
    for _ in 0..2 {
        assert!(refresh(&state, &parent).await.is_err());
        assert_eq!(
            serde_json::to_value(state.config.load().unwrap()).unwrap(),
            before
        );
        assert_eq!(std::fs::read(state.config.config_path()).unwrap(), disk);
    }
    server.join().unwrap();
    std::fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
}
