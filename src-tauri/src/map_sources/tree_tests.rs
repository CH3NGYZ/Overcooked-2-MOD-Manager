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
                "/child.toml" => ("200 OK", map("Child")),
                "/fail.toml" if retry => ("200 OK", map("Recovered")),
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
    let observer = SourceObserver::new(move |event| capture.lock().unwrap().push(event));
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
    let ctx = MapSourceContext {
        previous: &collected,
        ..ctx
    };
    let recovered = load_map_branch("failed", &ctx, "global").await.unwrap();
    assert_eq!(recovered.len(), 3, "successful siblings survive retry");
    assert!(
        progress
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .nodes
            .iter()
            .any(|n| n.id == "failed" && n.status == "ready" && n.error.is_none())
    );
    assert_eq!(
        server.join().unwrap(),
        vec!["/root.toml", "/child.toml", "/fail.toml", "/fail.toml"]
    );
}
