use super::*;
use crate::map_sources::progress::{MapTreeProgress, SourceObserver};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

fn map(name: &str) -> String {
    format!(
        r#"[[maps]]
name = "{name}"
provider = "github"
username = "u"
repository = "r"
level_set_name = "{name}"
author = "A"
asset_rules = [{{ type = "glob", value = "*.zip" }}]
"#
    )
}

async fn root_document(
    origin: &SourceLocation,
    label: &SourceLabel,
    ctx: &MapSourceContext<'_>,
) -> Result<Vec<MapRepositoryConfig>, ConfigError> {
    let mut maps = Vec::new();
    load_map_document(
        origin.clone(),
        label,
        None,
        false,
        "global",
        0,
        &mut HashSet::new(),
        &mut maps,
        &mut Vec::new(),
        ctx,
    )
    .await?;
    Ok(maps)
}

#[tokio::test]
async fn release_tags_source_propagates_fetch_errors_and_preserves_other_branches() {
    let origin = SourceLocation::parse("maps.toml").unwrap();
    let document = parse_document(
        r#"
[[map_sources]]
id='limited-fixture'
mode='release-tags'
provider='gitcode'
username='fixture'
repository='rate-limited-maps'

[[map_sources]]
id='working-fixture'
mode='release-tags'
provider='gitcode'
username='fixture'
repository='working-maps'
"#,
        &origin,
        &Default::default(),
    )
    .unwrap();
    let observer = SourceObserver::new("cn", |_| {});
    let mut config = crate::config::default_config();
    config.region = "cn".into();
    let cache = crate::release_cache::RepoReleaseCache::default();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut targets = Vec::new();
    for mut declaration in document.map_sources {
        if declaration.id == "limited-fixture" {
            declaration.network_plan = toml::from_str("mainland = 'missing-plan'").unwrap();
        } else {
            let probe = super::super::release_tags::declaration_request(&declaration);
            let key = crate::releases::release_cache_key_for_test(&probe, &config.region, None);
            cache
                .list(key, None, false, || async { Ok(Vec::new()) })
                .await
                .unwrap();
        }
        let label = SourceLabel::default().child(&declaration);
        let node = SourceNode {
            id: label.id.clone(),
            parent_id: None,
            label: declaration.id.clone(),
            label_i18n: Default::default(),
            note_i18n: Default::default(),
            url: String::new(),
            status: "pending".into(),
            error: None,
        };
        observer.declare(node.clone());
        targets.push(SourceTarget {
            node,
            sequence: 0,
            origin: origin.clone(),
            label,
            builtin: false,
            depth: 1,
            declaration: Some(declaration),
        });
    }
    let ctx = MapSourceContext {
        observer: Some(&observer),
        cache: &cache,
        client: &client,
        config: &config,
        github_token: None,
        gitee_token: None,
        gitcode_token: None,
        pass_id: None,
        force: false,
        previous: &[],
    };
    let mut collected = Vec::new();
    load_deferred(targets, &mut collected, &ctx, "cn").await;
    let progress = observer.finish(Vec::new());
    let limited = progress
        .nodes
        .iter()
        .find(|node| node.id == "limited-fixture")
        .unwrap();
    let working = progress
        .nodes
        .iter()
        .find(|node| node.id == "working-fixture")
        .unwrap();
    assert_eq!(limited.status, "error");
    assert_eq!(
        limited.error.as_deref(),
        Some("Release operation failed: proxy.validationFailed")
    );
    assert_eq!(working.status, "ready");
    assert!(working.error.is_none());
}

#[tokio::test]
async fn root_failures_keep_branch_retry_targets_until_a_successful_root_replaces_them() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let origin = SourceLocation::parse(&format!(
        "http://{}/root.toml",
        listener.local_addr().unwrap()
    ))
    .unwrap();
    let server = std::thread::spawn(move || {
        let mut paths = Vec::new();
        let mut roots = 0;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while paths.len() < 8 {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "missing document request: {paths:?}"
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
                let mut buf = [0; 1024];
                let count = socket.read(&mut buf).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buf[..count]);
            }
            let request = String::from_utf8(bytes).unwrap();
            let path = request.split_whitespace().nth(1).unwrap().to_owned();
            let (status, body) = match path.as_str() {
                "/root.toml" => {
                    roots += 1;
                    match roots {
                        1 => (
                            "200 OK",
                            format!(
                                "{}\n[[map_sources]]\nid='old'\nurl='old.toml'\nnickname='Old'\n",
                                map("Old root")
                            ),
                        ),
                        2 => ("500 Failed", String::new()),
                        3 => ("200 OK", "maps = [ broken TOML".into()),
                        4 => (
                            "200 OK",
                            format!(
                                "{}\n[[map_sources]]\nid='new'\nurl='new.toml'\nnickname='New'\n",
                                map("New root")
                            ),
                        ),
                        _ => panic!("unexpected root request"),
                    }
                }
                "/old.toml" => ("200 OK", map("Old child")),
                "/new.toml" => ("200 OK", map("New child")),
                _ => panic!("unexpected request: {path}"),
            };
            paths.push(path);
            write!(
                socket,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
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
    let root_node = SourceNode {
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
        node: root_node.clone(),
        sequence: 0,
        origin: origin.clone(),
        label: label.clone(),
        builtin: false,
        depth: 0,
        declaration: None,
    });
    observer.declare(root_node);
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
    let old = root_document(&origin, &label, &ctx).await.unwrap();
    assert_eq!(old.len(), 2);
    let ctx = MapSourceContext {
        previous: &old,
        ..ctx
    };
    for failure in ["HTTP failure", "invalid TOML"] {
        assert!(
            root_document(&origin, &label, &ctx).await.is_err(),
            "{failure}"
        );
        {
            let events = events.lock().unwrap();
            let root = events
                .last()
                .unwrap()
                .nodes
                .iter()
                .find(|node| node.id == "root")
                .unwrap();
            assert_eq!(root.status, "error");
            assert!(root.error.is_some());
        }
        let retried = load_map_branch("old", &ctx, "global").await.unwrap();
        assert_eq!(
            retried.len(),
            2,
            "{failure} must retain old sibling maps and the child retry target"
        );
        assert!(
            retried
                .iter()
                .any(|map| map.source_id.as_deref() == Some("old"))
        );
    }
    let replacement = root_document(&origin, &label, &ctx).await.unwrap();
    assert_eq!(replacement.len(), 2);
    assert!(
        replacement
            .iter()
            .all(|map| map.source_id.as_deref() != Some("old"))
    );
    {
        let events = events.lock().unwrap();
        let first_new = events
            .iter()
            .find(|event| event.nodes.iter().any(|node| node.id == "new"))
            .unwrap();
        assert_eq!(
            first_new
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            ["root", "new"],
            "successful root removes stale local nodes before publishing new child titles"
        );
        assert_eq!(first_new.nodes[1].status, "pending");
        assert_eq!(first_new.nodes[0].status, "ready");
    }
    assert!(
        load_map_branch("old", &ctx, "global").await.is_err(),
        "successful root removes stale persistent retry targets"
    );
    assert_eq!(
        server.join().unwrap(),
        [
            "/root.toml",
            "/old.toml",
            "/root.toml",
            "/old.toml",
            "/root.toml",
            "/old.toml",
            "/root.toml",
            "/new.toml",
        ]
    );
}

#[tokio::test]
async fn documents_publish_incrementally_before_release_apis_and_failed_branch_retries_alone() {
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
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "missing document request"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            socket.set_nonblocking(false).unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            socket
                .set_write_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buf = [0; 1024];
            while !bytes.ends_with(b"\r\n\r\n") {
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let request = String::from_utf8(bytes).unwrap();
            let path = request.split_whitespace().nth(1).unwrap().to_owned();
            let retry = paths.iter().any(|p| p == "/fail.toml");
            let (status, body) = match path.as_str() {
                "/root.toml" => (
                    "200 OK",
                    format!(
                        "{}{}",
                        map("Root"),
                        r#"
[[map_sources]]
id = "tags"
mode = "release-tags"
username = "u"
repository = "bundle"
[[map_sources]]
id = "child"
url = "child.toml"
nickname = "Child"
[[map_sources]]
id = "failed"
url = "fail.toml"
nickname = "Failed"
"#
                    ),
                ),
                "/child.toml" => (
                    "200 OK",
                    format!("{}{}enabled = false\n", map("Child"), map("Hidden sibling")),
                ),
                "/fail.toml" if retry => (
                    "200 OK",
                    format!(
                        "{}{}enabled = false\n",
                        map("Recovered"),
                        map("Hidden recovered")
                    ),
                ),
                "/fail.toml" => ("500 Failed", String::new()),
                _ => panic!("unexpected API or document: {path}"),
            };
            paths.push(path);
            write!(
                socket,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        paths
    });
    let progress = Arc::new(Mutex::new(Vec::<MapTreeProgress>::new()));
    let capture = progress.clone();
    let observer = SourceObserver::new("global", move |event| capture.lock().unwrap().push(event));
    let label = SourceLabel {
        id: "root".into(),
        ..Default::default()
    };
    observer.declare(SourceNode {
        note_i18n: Default::default(),
        id: "root".into(),
        parent_id: None,
        label: "Root".into(),
        label_i18n: Default::default(),
        url: origin.address(),
        status: "pending".into(),
        error: None,
    });
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
    let mut collected = Vec::new();
    let mut deferred = Vec::new();
    let mut visited = HashSet::new();
    load_map_document(
        origin,
        &label,
        None,
        false,
        "global",
        0,
        &mut visited,
        &mut collected,
        &mut deferred,
        &ctx,
    )
    .await
    .unwrap();
    assert_eq!(
        deferred.len(),
        1,
        "release-tags stays deferred until all documents finish"
    );
    assert_eq!(collected.len(), 2);
    {
        let events = progress.lock().unwrap();
        let first = events.iter().find(|e| e.maps.len() == 1).unwrap();
        assert_eq!(
            first.nodes.len(),
            4,
            "parent publishes every child title at once"
        );
        assert!(
            first
                .nodes
                .iter()
                .filter(|n| n.id != "root")
                .all(|n| n.status == "pending")
        );
        let last = events.last().unwrap();
        assert!(
            last.nodes
                .iter()
                .any(|n| n.id == "failed" && n.status == "error" && n.error.is_some())
        );
        assert!(
            !last
                .maps
                .iter()
                .any(|m| m.source_id.as_deref() == Some("failed"))
        );
    }
    {
        let settled = observer.finish(
            collected
                .iter()
                .map(super::super::view::map_to_info)
                .collect(),
        );
        assert!(settled.nodes.iter().any(|node| node.status == "error"));
        assert_eq!(settled.excluded_map_identities.len(), 1);
        assert_eq!(settled.excluded_map_identities[0].source_id, "child");
        assert_eq!(
            settled.excluded_map_identities[0].level_set_name,
            "Hidden sibling"
        );
    }
    let ctx = MapSourceContext {
        previous: &collected,
        ..ctx
    };
    let recovered = load_map_branch("failed", &ctx, "global").await.unwrap();
    assert_eq!(recovered.len(), 3, "successful siblings survive retry");
    let settled = observer.finish(
        recovered
            .iter()
            .map(super::super::view::map_to_info)
            .collect(),
    );
    assert!(
        settled
            .nodes
            .iter()
            .any(|n| n.id == "failed" && n.status == "ready" && n.error.is_none())
    );
    assert_eq!(settled.excluded_map_identities.len(), 1);
    assert_eq!(settled.excluded_map_identities[0].source_id, "failed");
    assert_eq!(
        settled.excluded_map_identities[0].level_set_name,
        "Hidden recovered"
    );
    assert_eq!(
        server.join().unwrap(),
        vec!["/root.toml", "/child.toml", "/fail.toml", "/fail.toml"]
    );
}
