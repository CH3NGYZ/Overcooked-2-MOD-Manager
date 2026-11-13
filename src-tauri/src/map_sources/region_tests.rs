use super::*;
use crate::map_sources::progress::{MapTreeProgress, SourceObserver};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

fn sample_map() -> MapRepositoryConfig {
    serde_json::from_value(serde_json::json!({
        "username":"u", "repository":"r", "level_set_name":"Persisted",
        "asset_rules":[{"type":"exact","value":"map.zip"}],
        "note_i18n":{"en":"Map note"}
    }))
    .unwrap()
}

#[test]
fn source_metadata_and_ancestor_restrictions_survive_storage_and_projection() {
    let declaration: super::super::document::MapSourceDeclaration = serde_json::from_value(
        serde_json::json!({"id":"child", "nickname":"Child", "region":"chinese_mainland", "note_i18n":{"en":"Source note"}})
    ).unwrap();
    let parent = SourceLabel {
        regions: vec!["cn".into()],
        ..Default::default()
    };
    let label = parent.child(&declaration);
    let mut map = sample_map();
    label.attach(&mut map);
    let restored: MapRepositoryConfig = toml::from_str(&toml::to_string(&map).unwrap()).unwrap();
    assert_eq!(restored.source_regions, Some(vec!["cn".into()]));
    assert!(super::super::state::map_is_available(&restored, "cn"));
    assert!(!super::super::state::map_is_available(&restored, "global"));
    assert_eq!(
        super::super::view::map_to_info(&restored).source_note_i18n["en"],
        "Source note"
    );
    assert_eq!(restored.note_i18n["en"], "Map note");
    map.region = Some("global".into());
    for region in ["cn", "global"] {
        assert!(!super::super::state::map_is_available(&map, region));
    }
    let conflicting: super::super::document::MapSourceDeclaration =
        serde_json::from_value(serde_json::json!({"id":"grandchild", "region":"global"})).unwrap();
    let child = label.child(&conflicting);
    assert!(!child.matches_region("cn"));
    assert!(!child.matches_region("global"));
    assert!(
        child.note_i18n.is_empty(),
        "child notes never inherit parent notes"
    );
}

#[test]
fn old_map_metadata_is_backfilled_once_but_explicit_empty_source_note_stays_empty() {
    let (_, labels) = bundled_map_snapshot();
    let source = labels.get("maps-oc2-map-bundle").unwrap();
    let mut old = sample_map();
    old.source_id = Some(source.id.clone());
    old.source_name = "Previously loaded".into();
    assert!(fill_missing_source_names(std::slice::from_mut(&mut old)));
    assert_eq!(old.source_regions, Some(source.regions.clone()));
    assert_eq!(old.source_note_i18n, source.note_i18n);
    assert!(!fill_missing_source_names(std::slice::from_mut(&mut old)));
    old.source_regions = Some(Vec::new());
    old.source_note_i18n.clear();
    let mut restored: MapRepositoryConfig =
        toml::from_str(&toml::to_string(&old).unwrap()).unwrap();
    assert!(!fill_missing_source_names(std::slice::from_mut(
        &mut restored
    )));
    assert_eq!(restored.source_regions, Some(Vec::new()));
    assert!(restored.source_note_i18n.is_empty());
}

#[tokio::test]
async fn three_modes_skip_excluded_sources_before_requests_and_branch_retry_keeps_ancestor_filter()
{
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        let mut paths = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
        while paths.len() < 2 {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "missing map document request"
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
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
            }
            let request = String::from_utf8(bytes).unwrap();
            let path = request.split_whitespace().nth(1).unwrap().to_string();
            let body = match path.as_str() {
                "/root.toml" => {
                    r#"
[[map_sources]]
id='cn-document'
region='cn'
url='excluded.toml'
[[map_sources]]
id='cn-tags'
mode='release-tags'
region='cn'
username='u'
repository='r'
[[map_sources]]
id='cn-repository'
mode='repository'
region='cn'
username='u'
repository='r'
[map_sources.map]
level_set_name='Excluded'
asset_rules=[{type='exact',value='map.zip'}]
[[map_sources]]
id='shared'
mode='repository'
region=''
username='u'
repository='r'
[map_sources.note_i18n]
en='Source notice'
[map_sources.map]
level_set_name='Shared'
asset_rules=[{type='exact',value='map.zip'}]
[[map_sources]]
id='nested'
region='global'
url='nested.toml'
"#
                }
                "/nested.toml" => {
                    r#"
[[maps]]
username='u'
repository='r'
level_set_name='Nested'
asset_rules=[{type='exact',value='map.zip'}]
[[map_sources]]
id='conflicting-grandchild'
region='cn'
url='never.toml'
"#
                }
                _ => panic!("excluded source was fetched: {path}"),
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
    let observer = SourceObserver::new(move |event| capture.lock().unwrap().push(event));
    let cache = crate::release_cache::RepoReleaseCache::default();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let config = crate::config::default_config();
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
    let origin = SourceLocation::parse(&format!("http://{address}/root.toml")).unwrap();
    let root = SourceLabel {
        id: "root".into(),
        ..Default::default()
    };
    let mut collected = Vec::new();
    let mut deferred = Vec::new();
    let mut visited = HashSet::new();
    load_map_document(
        origin,
        &root,
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
    assert_eq!(collected.len(), 2);
    assert!(
        deferred.is_empty(),
        "excluded release-tags must not be queued"
    );
    assert_eq!(server.join().unwrap(), vec!["/root.toml", "/nested.toml"]);
    let nodes = events.lock().unwrap().last().unwrap().nodes.clone();
    assert_eq!(nodes.len(), 2);
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == "shared")
            .unwrap()
            .note_i18n["en"],
        "Source notice"
    );
    assert!(
        nodes
            .iter()
            .all(|node| !node.id.starts_with("cn-") && node.id != "conflicting-grandchild")
    );
    let ctx = MapSourceContext {
        previous: &collected,
        ..ctx
    };
    let refreshed = load_map_branch("nested", &ctx, "cn").await.unwrap();
    assert_eq!(
        refreshed.len(),
        1,
        "excluded branch content removed, sibling retained"
    );
    assert!(
        events
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .nodes
            .iter()
            .all(|node| node.id != "nested")
    );
}
