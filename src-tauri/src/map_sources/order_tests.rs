use super::*;
use crate::map_sources::progress::{MapTreeProgress, SourceObserver};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

fn register_sources(observer: &SourceObserver, mode: &str) -> Vec<SourceTarget> {
    ["zeta", "alpha", "middle"]
        .into_iter()
        .map(|id| {
            let declaration = serde_json::from_value(serde_json::json!({
                "id": id, "nickname": id, "mode": mode,
                "username": "map-source-order-test", "repository": id,
                "map": {"level_set_name": id,
                    "asset_rules": [{"type": "glob", "value": "*.zip"}]}
            }))
            .unwrap();
            let label = SourceLabel::default().child(&declaration);
            let node = SourceNode {
                id: id.into(),
                parent_id: None,
                label: id.into(),
                label_i18n: Default::default(),
                note_i18n: Default::default(),
                url: String::new(),
                status: "pending".into(),
                error: None,
            };
            let target = SourceTarget {
                node: node.clone(),
                sequence: 0,
                origin: SourceLocation::parse("maps.toml").unwrap(),
                label,
                builtin: false,
                depth: 1,
                declaration: Some(declaration),
            };
            observer.register(target.clone());
            observer.declare(node);
            target
        })
        .collect()
}

fn map_for(target: &SourceTarget) -> MapRepositoryConfig {
    build_declaration_map(
        target.declaration.as_ref().unwrap(),
        &target.label,
        false,
        &target.origin,
    )
    .unwrap()
}

#[tokio::test]
async fn branch_refresh_preserves_non_alphabetic_declaration_order_after_restart() {
    let events = Arc::new(Mutex::new(Vec::<MapTreeProgress>::new()));
    let capture = events.clone();
    let observer = SourceObserver::new("global", move |event| capture.lock().unwrap().push(event));
    let targets = register_sources(&observer, "repository");
    let mut previous: Vec<_> = targets.iter().map(map_for).collect();
    let mut community = previous[1].clone();
    community.id = "community".into();
    community.source_id = Some("community".into());
    let mut unknown = previous[2].clone();
    unknown.id = "unknown".into();
    unknown.source_id = None;
    previous.extend([community, unknown]);
    let untouched: Vec<_> = previous[1..]
        .iter()
        .map(|map| serde_json::to_value(map).unwrap())
        .collect();
    let cache = crate::release_cache::RepoReleaseCache::default();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let config = crate::config::default_config();
    let ctx = MapSourceContext {
        observer: Some(&observer),
        cache: &cache,
        client: &client,
        config: &config,
        github_token: None,
        gitcode_token: None,
        gitee_token: None,
        pass_id: None,
        force: false,
        previous: &previous,
    };
    let refreshed = load_map_branch("zeta", &ctx, "global").await.unwrap();
    assert_eq!(
        refreshed
            .iter()
            .map(|map| map.source_id.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("zeta"),
            Some("alpha"),
            Some("middle"),
            Some("community"),
            None
        ],
        "refreshing an earlier source must not append it after its siblings"
    );
    assert_eq!(
        refreshed[1..]
            .iter()
            .map(|map| serde_json::to_value(map).unwrap())
            .collect::<Vec<_>>(),
        untouched,
        "unrelated branches and unknown sources retain their relative order and fields"
    );
    let events = events.lock().unwrap();
    let last = events.last().unwrap();
    assert_eq!(
        last.nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>(),
        ["zeta", "alpha", "middle"]
    );
    assert_eq!(
        last.maps
            .iter()
            .take(3)
            .map(|map| map.source_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        last.nodes
            .iter()
            .map(|node| node.id.as_str())
            .collect::<Vec<_>>()
    );
    let mut persisted = crate::config::default_config();
    persisted.map_repositories = refreshed;
    let restored: crate::models::AppConfig =
        toml::from_str(&toml::to_string(&persisted).unwrap()).unwrap();
    let mut first_seen = HashSet::new();
    let recovered_titles: Vec<_> = restored
        .map_repositories
        .iter()
        .filter_map(|map| map.source_id.as_deref())
        .filter(|id| first_seen.insert(*id))
        .collect();
    assert_eq!(recovered_titles, ["zeta", "alpha", "middle", "community"]);
}

#[tokio::test]
async fn installed_priority_changes_execution_but_not_map_or_progress_order() {
    let events = Arc::new(Mutex::new(Vec::<MapTreeProgress>::new()));
    let capture = events.clone();
    let observer = SourceObserver::new("global", move |event| capture.lock().unwrap().push(event));
    let targets = register_sources(&observer, "release-tags");
    let previous: Vec<_> = targets.iter().map(map_for).collect();
    let cache = crate::release_cache::RepoReleaseCache::default();
    let config = crate::config::default_config();
    for (target, map) in targets.iter().zip(&previous) {
        let id = &target.label.id;
        let json = serde_json::json!([{
            "tag_name": "z-map",
            "body": format!("<!-- oc2-map\nlevel_set_name = '{id}-Z'\nauthor = 'A'\n-->"),
            "assets": [{"name": "z-map_v1.0.zip", "size": 1,
                "browser_download_url": format!("https://github.com/map-source-order-test/{id}/releases/download/z-map/z-map_v1.0.zip")}]
        }, {
            "tag_name": "a-map",
            "body": format!("<!-- oc2-map\nlevel_set_name = '{id}-A'\nauthor = 'A'\n-->"),
            "assets": [{"name": "a-map_v1.0.zip", "size": 1,
                "browser_download_url": format!("https://github.com/map-source-order-test/{id}/releases/download/a-map/a-map_v1.0.zip")}]
        }]);
        cache
            .list(
                crate::releases::release_cache_key_for_test(
                    &crate::releases::RepositoryRequest::from(map),
                    &config.region,
                    None,
                ),
                None,
                false,
                || async {
                    crate::releases::parse_release_json(&json, &crate::models::Provider::Github)
                        .map_err(|error| error.to_string())
                },
            )
            .await
            .unwrap();
    }
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let ctx = MapSourceContext {
        observer: Some(&observer),
        cache: &cache,
        client: &client,
        config: &config,
        github_token: None,
        gitcode_token: None,
        gitee_token: None,
        pass_id: None,
        force: false,
        previous: &previous,
    };
    crate::api_scheduler::set_installed(
        "map-source-order-tests",
        [crate::api_scheduler::repository_key(
            &crate::models::Provider::Github,
            "map-source-order-test",
            "middle",
        )],
    );
    let mut collected = Vec::new();
    load_deferred(targets.clone(), &mut collected, &ctx, "global").await;
    crate::api_scheduler::set_installed("map-source-order-tests", []);
    let expected = ["zeta", "zeta", "alpha", "alpha", "middle", "middle"];
    assert_eq!(
        collected
            .iter()
            .map(|map| map.source_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        expected,
        "persisted map order follows declarations even when an installed source runs first"
    );
    assert_eq!(
        collected
            .iter()
            .map(|map| map.level_set_name.as_str())
            .collect::<Vec<_>>(),
        [
            "zeta-Z", "zeta-A", "alpha-Z", "alpha-A", "middle-Z", "middle-A"
        ],
        "sorting sources preserves each source's own Release order"
    );
    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter_map(|event| event
                .nodes
                .iter()
                .find(|node| node.status == "loading")
                .map(|node| node.id.as_str()))
            .collect::<Vec<_>>(),
        ["middle", "zeta", "alpha"]
    );
    for event in events.iter() {
        let expected: Vec<_> = event
            .nodes
            .iter()
            .flat_map(|node| {
                event
                    .maps
                    .iter()
                    .filter(move |map| map.source_id.as_deref() == Some(&node.id))
            })
            .map(|map| map.id.as_str())
            .collect();
        assert_eq!(
            event
                .maps
                .iter()
                .map(|map| map.id.as_str())
                .collect::<Vec<_>>(),
            expected,
            "every progress snapshot follows the same source order as its nodes"
        );
    }
}

#[tokio::test]
async fn refreshing_a_document_keeps_redeclared_and_new_children_before_the_next_branch() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = SourceLocation::parse(&format!(
        "http://{}/root.toml",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let server = std::thread::spawn(move || {
        let mut paths = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while paths.len() < 4 {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "missing request: {paths:?}"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            while !bytes.windows(4).any(|part| part == b"\r\n\r\n") {
                let mut buffer = [0; 1024];
                let count = socket.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let request = String::from_utf8(bytes).unwrap();
            let path = request.split_whitespace().nth(1).unwrap().to_owned();
            let map = |id: &str| {
                format!(
                    "[[maps]]\nlevel_set_name='{id}'\nusername='u'\nrepository='r'\nasset_rules=[{{type='glob',value='*.zip'}}]\n"
                )
            };
            let child = |id: &str| {
                format!(
                    "[[map_sources]]\nid='{id}'\nmode='repository'\nusername='u'\nrepository='r'\n[map_sources.map]\nlevel_set_name='{id}'\nasset_rules=[{{type='glob',value='*.zip'}}]\n"
                )
            };
            let body = match path.as_str() {
                "/root.toml" => "[[map_sources]]\nid='zeta'\nurl='zeta.toml'\n[[map_sources]]\nid='alpha'\nurl='alpha.toml'\n".into(),
                "/zeta.toml" => {
                    let mut body = map("zeta") + &child("zeta-child");
                    if paths.iter().any(|path| path == "/zeta.toml") {
                        body += &child("zeta-new");
                    }
                    body
                }
                "/alpha.toml" => map("alpha") + &child("alpha-child"),
                _ => panic!("unexpected request: {path}"),
            };
            paths.push(path);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        paths
    });
    let events = Arc::new(Mutex::new(Vec::<MapTreeProgress>::new()));
    let capture = events.clone();
    let observer = SourceObserver::new("global", move |event| capture.lock().unwrap().push(event));
    let label = SourceLabel {
        id: "root".into(),
        ..Default::default()
    };
    let node = SourceNode {
        id: label.id.clone(),
        parent_id: None,
        label: "Root".into(),
        label_i18n: Default::default(),
        note_i18n: Default::default(),
        url: origin.address(),
        status: "pending".into(),
        error: None,
    };
    observer.register(SourceTarget {
        node: node.clone(),
        sequence: 0,
        origin: origin.clone(),
        label: label.clone(),
        builtin: false,
        depth: 0,
        declaration: None,
    });
    observer.declare(node);
    let cache = crate::release_cache::RepoReleaseCache::default();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let config = crate::config::default_config();
    let ctx = MapSourceContext {
        observer: Some(&observer),
        cache: &cache,
        client: &client,
        config: &config,
        github_token: None,
        gitcode_token: None,
        gitee_token: None,
        pass_id: None,
        force: false,
        previous: &[],
    };
    let mut previous = Vec::new();
    load_map_document(
        origin,
        &label,
        None,
        false,
        "global",
        0,
        &mut HashSet::new(),
        &mut previous,
        &mut Vec::new(),
        &ctx,
    )
    .await
    .unwrap();
    let ctx = MapSourceContext {
        previous: &previous,
        ..ctx
    };
    let refreshed = load_map_branch("zeta", &ctx, "global").await.unwrap();
    let expected = ["zeta", "zeta-child", "zeta-new", "alpha", "alpha-child"];
    assert_eq!(
        refreshed
            .iter()
            .map(|map| map.source_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        expected
    );
    assert_eq!(
        serde_json::to_value(&refreshed[3..]).unwrap(),
        serde_json::to_value(&previous[2..]).unwrap(),
        "the unrelated branch stays unchanged"
    );
    for event in events.lock().unwrap().iter() {
        let observed: Vec<_> = event
            .maps
            .iter()
            .map(|map| map.source_id.as_deref().unwrap())
            .collect();
        assert_eq!(
            observed,
            expected
                .iter()
                .copied()
                .filter(|id| observed.contains(id))
                .collect::<Vec<_>>(),
            "incremental maps follow parent/child tree reading order"
        );
    }
    assert_eq!(
        server.join().unwrap(),
        ["/root.toml", "/zeta.toml", "/alpha.toml", "/zeta.toml"]
    );
}

#[test]
fn orphaned_cyclic_and_unknown_sources_keep_a_stable_tail() {
    let observer = SourceObserver::new("global", |_| {});
    let targets = register_sources(&observer, "repository");
    let mut orphan = targets[0].clone();
    orphan.node.id = "orphan".into();
    orphan.node.parent_id = Some("missing".into());
    observer.register(orphan);
    let mut cyclic = targets[0].clone();
    cyclic.node.id = "cyclic".into();
    cyclic.node.parent_id = Some("cyclic".into());
    observer.register(cyclic);
    let mut ids = [
        Some("orphan"),
        Some("middle"),
        None,
        Some("cyclic"),
        Some("alpha"),
        Some("community"),
        Some("zeta"),
    ];
    observer.sort_by_source(&mut ids, |id| *id);
    assert_eq!(
        ids,
        [
            Some("zeta"),
            Some("alpha"),
            Some("middle"),
            Some("orphan"),
            None,
            Some("cyclic"),
            Some("community")
        ]
    );
}
