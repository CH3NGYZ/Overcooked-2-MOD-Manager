use super::{MapSourceContext, document::parse_document, release_tags::load_release_tag_maps};
use super::{releases::fetch_map_release_data, state::SourceLabel};
use crate::models::Provider;
use crate::repository_sources::SourceLocation;
use std::io::{Read, Write};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

fn response(version: u32) -> String {
    serde_json::to_string(&(0..2).map(|i| serde_json::json!({
        "tag_name": format!("map{i}"),
        "body": format!("Updated {version}\n<!-- oc2-map\nlevel_set_name = 'Map{i}'\nauthor = 'A'\nasset = 'map{i}_v${{version}}.zip'\n[[asset_versions]]\nmapset_version = '{version}.0'\ninfo_version = '{version}.1'\n-->"),
        "assets": [{"name": format!("map{i}_v{version}.0.zip"), "type":"attach", "size":1,
            "browser_download_url":format!("https://gitcode.com/u/b/releases/download/map{i}/map{i}_v{version}.0.zip")}]
    })).collect::<Vec<_>>()).unwrap()
}

#[tokio::test]
async fn source_cards_and_checks_share_bundle_http_snapshot_and_reparse_current_note() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let stopped = Arc::new(AtomicBool::new(false));
    let seen = requests.clone();
    let stop = stopped.clone();
    let server = std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
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
                let n = socket.read(&mut buf).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buf[..n]);
            }
            let version = {
                let mut seen = seen.lock().unwrap();
                seen.push(String::from_utf8(bytes).unwrap());
                seen.len() as u32
            };
            let body = response(version);
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let outcome = async {
        let origin = SourceLocation::parse("maps.toml").unwrap();
        let document = parse_document(
            r#"
[[map_sources]]
id='bundle'
mode='release-tags'
provider='gitcode'
username='u'
repository='b'
"#,
            &origin,
            &Default::default(),
        )
        .unwrap();
        let declaration = &document.map_sources[0];
        let mut label = SourceLabel::default().child(declaration);
        label.note_i18n.insert("en".into(), "Source only".into());
        label.regions.push("cn".into());
        let cache = crate::release_cache::RepoReleaseCache::default();
        let client = reqwest::Client::builder().no_proxy().build().unwrap();
        let config = crate::config::default_config();
        let ctx = MapSourceContext {
            observer: None,
            cache: &cache,
            client: &client,
            config: &config,
            github_token: None,
            gitee_token: None,
            gitcode_token: None,
            pass_id: Some("source"),
            force: false,
            previous: &[],
        };
        acquire(&ctx, address).await;
        let maps = load_release_tag_maps(declaration, &label, false, &origin, &ctx)
            .await
            .unwrap();
        assert_eq!(maps.len(), 2);
        assert!(maps.iter().all(|map| map.release_derived));
        let started = std::time::Instant::now();
        for (map, pass) in maps.iter().zip(["card-a", "card-b"]) {
            let ctx = MapSourceContext {
                pass_id: Some(pass),
                ..ctx
            };
            let fresh = fetch_map_release_data(&ctx, map).await.unwrap();
            assert_eq!(fresh.cached_releases[0].tag_name, "1.0");
            assert_eq!(fresh.source_note_i18n, map.source_note_i18n);
            assert_eq!(fresh.source_regions, map.source_regions);
        }
        assert_eq!(
            requests.lock().unwrap().len(),
            1,
            "source then cards must not request HTTP again"
        );
        println!(
            "bundle cache: two cards elapsed_ms={}",
            started.elapsed().as_millis()
        );
        let ctx = MapSourceContext {
            pass_id: Some("full-force"),
            force: true,
            ..ctx
        };
        acquire(&ctx, address).await;
        let fresh = load_release_tag_maps(declaration, &label, false, &origin, &ctx)
            .await
            .unwrap();
        let same_pass = fetch_map_release_data(&ctx, &maps[0]).await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 2, "force once per pass");
        assert_eq!(same_pass.latest_mapset_version.as_deref(), Some("2.0"));
        assert_eq!(same_pass.latest_info_version.as_deref(), Some("2.1"));
        assert_eq!(same_pass.cached_releases[0].tag_name, "2.0");
        assert_eq!(fresh[0].cached_releases[0].tag_name, "2.0");
        // A new card action must use the newer raw snapshot, even if its own
        // old projected cache is still fresh. Repeating must not extend raw TTL.
        let ctx = MapSourceContext {
            pass_id: Some("later-card"),
            force: false,
            ..ctx
        };
        assert_eq!(
            fetch_map_release_data(&ctx, &maps[1])
                .await
                .unwrap()
                .latest_mapset_version
                .as_deref(),
            Some("2.0")
        );
        assert_eq!(requests.lock().unwrap().len(), 2);
        cache.expire_for_test(&crate::releases::repository_cache_key(
            Provider::Gitcode,
            "u",
            "b",
        ));
        acquire(&ctx, address).await;
        let expired = fetch_map_release_data(&ctx, &maps[0]).await.unwrap();
        assert_eq!(requests.lock().unwrap().len(), 3);
        assert_eq!(expired.latest_mapset_version.as_deref(), Some("3.0"));
        let cached = load_release_tag_maps(declaration, &label, false, &origin, &ctx)
            .await
            .unwrap();
        assert_eq!(cached[0].latest_mapset_version.as_deref(), Some("3.0"));
        assert_eq!(
            requests.lock().unwrap().len(),
            3,
            "source refresh shares card-acquired snapshot"
        );
    }
    .await;
    stopped.store(true, Ordering::Relaxed);
    server.join().unwrap();
    outcome
}

// Replace only the external API boundary with loopback HTTP. Source/card
// consumers below still execute the real cache, note parser and projections.
async fn acquire(ctx: &MapSourceContext<'_>, address: std::net::SocketAddr) {
    ctx.cache
        .list(
            crate::releases::repository_cache_key(Provider::Gitcode, "u", "b"),
            ctx.pass_id,
            ctx.force,
            || async {
                let value = ctx
                    .client
                    .get(format!("http://{address}/releases"))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                crate::releases::parse_release_json(&value, &Provider::Gitcode)
                    .map_err(|error| error.to_string())
            },
        )
        .await
        .unwrap();
}
