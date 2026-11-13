use super::*;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const ROOT: &str = "[[repository_sources]]\nid='child'\nnickname='Child'\nurl='./child.toml'\n";
type Replies = BTreeMap<String, (u16, String)>;

fn serve(replies: Replies, count: usize) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut paths = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(6);
        while paths.len() < count {
            let (mut stream, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "missing request: {paths:?}");
                    std::thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = Vec::new();
            let mut buffer = [0; 1024];
            while !request.ends_with(b"\r\n\r\n") {
                let size = stream.read(&mut buffer).unwrap();
                assert!(size > 0);
                request.extend_from_slice(&buffer[..size]);
                assert!(request.len() < 16_384);
            }
            let request = String::from_utf8(request).unwrap();
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
            let path = request.split_whitespace().nth(1).unwrap().to_owned();
            let (status, body) = replies.get(&path).expect("unexpected request");
            write!(stream, "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            paths.push(path);
        }
        paths
    });
    (base, server)
}

fn routes(base: &str, provider: DataProvider) -> String {
    match provider {
        DataProvider::Builtin => "mods.toml".into(),
        _ => format!("{base}/{provider:?}/mods.toml"),
    }
}

#[tokio::test]
async fn root_http_and_parse_failure_matrix_only_visits_the_authorized_fallback_chain() {
    for initial in [
        DataProvider::Github,
        DataProvider::Gitee,
        DataProvider::Cloudflare,
    ] {
        for failure in [
            (503, "unavailable"),
            (200, "<html>bad</html>"),
            (200, "invalid ["),
            (200, ""),
            (200, "type='map'"),
        ] {
            for cloudflare_ok in [false, true] {
                let mut replies = Replies::from([(
                    format!("/{initial:?}/mods.toml"),
                    (failure.0, failure.1.into()),
                )]);
                let expected = if cloudflare_ok && initial != DataProvider::Cloudflare {
                    DataProvider::Cloudflare
                } else {
                    DataProvider::Builtin
                };
                let count = if initial == DataProvider::Cloudflare {
                    1
                } else {
                    replies.insert(
                        "/Cloudflare/mods.toml".into(),
                        if cloudflare_ok {
                            (200, ROOT.into())
                        } else {
                            (503, "offline".into())
                        },
                    );
                    2
                };
                let (base, server) = serve(replies, count);
                let runtime = ProxyRuntime::new(initial);
                let root = runtime
                    .official_root_from(true, |provider| routes(&base, provider))
                    .await
                    .unwrap();
                assert_eq!(
                    runtime.provider(),
                    expected,
                    "{initial:?} {failure:?} cf={cloudflare_ok}"
                );
                assert_eq!(root.selection, runtime.selection());
                assert!(!root.document.repository_sources.is_empty());
                assert_eq!(server.join().unwrap().len(), count);
                let cached = runtime
                    .official_root_from(false, |_| panic!("must use trusted root or builtin"))
                    .await;
                // Builtin never consults a remote address resolver either.
                assert!(cached.is_ok());
            }
        }
    }
}

#[tokio::test]
async fn success_is_shared_by_parallel_consumers_and_only_explicit_refresh_reloads_root() {
    for initial in [
        DataProvider::Github,
        DataProvider::Gitee,
        DataProvider::Cloudflare,
    ] {
        let (base, server) = serve(
            Replies::from([(format!("/{initial:?}/mods.toml"), (200, ROOT.into()))]),
            2,
        );
        let runtime = ProxyRuntime::new(initial);
        let address = |provider| routes(&base, provider);
        let (first, second) = tokio::join!(
            runtime.official_root_from(false, address),
            runtime.official_root_from(false, address)
        );
        assert_eq!(first.unwrap().selection, (initial, 0));
        assert_eq!(second.unwrap().selection, (initial, 0));
        assert_eq!(runtime.provider(), initial);
        runtime.official_root_from(true, address).await.unwrap();
        assert_eq!(server.join().unwrap().len(), 2);
    }
}

#[tokio::test]
async fn parallel_waiters_follow_automatic_fallback_without_cancelling_the_new_provider() {
    let (base, server) = serve(
        Replies::from([
            ("/Gitee/mods.toml".into(), (503, "offline".into())),
            ("/Cloudflare/mods.toml".into(), (200, ROOT.into())),
        ]),
        2,
    );
    let runtime = ProxyRuntime::new(DataProvider::Gitee);
    let address = |provider| routes(&base, provider);
    let (first, second) = tokio::join!(
        runtime.official_root_from(false, address),
        runtime.official_root_from(false, address)
    );
    assert_eq!(first.unwrap().selection, (DataProvider::Cloudflare, 1));
    assert_eq!(second.unwrap().selection, (DataProvider::Cloudflare, 1));
    assert_eq!(server.join().unwrap().len(), 2);
}

#[tokio::test]
async fn map_child_and_network_closure_failures_keep_the_successful_provider_and_trusted_policy() {
    for failed_path in [
        "maps.toml",
        "child.toml",
        "proxies.toml",
        "network-plans.toml",
    ] {
        let provider = DataProvider::Cloudflare;
        let mut replies = Replies::from([
            ("/Cloudflare/mods.toml".into(), (200, ROOT.into())),
            ("/Cloudflare/child.toml".into(), (503, "offline".into())),
        ]);
        let index = SourceLocation::parse("proxies.toml")
            .unwrap()
            .bundled_text()
            .unwrap();
        for path in crate::network_policy::proxy_documents(index).unwrap() {
            replies.insert(
                format!("/Cloudflare/{path}"),
                (
                    200,
                    SourceLocation::parse(&path)
                        .unwrap()
                        .bundled_text()
                        .unwrap()
                        .into(),
                ),
            );
        }
        replies.insert("/Cloudflare/proxies.toml".into(), (200, index.into()));
        replies.insert(
            "/Cloudflare/network-plans.toml".into(),
            (
                200,
                SourceLocation::parse("network-plans.toml")
                    .unwrap()
                    .bundled_text()
                    .unwrap()
                    .into(),
            ),
        );
        replies.insert(
            format!("/Cloudflare/{failed_path}"),
            (200, "<html>bad</html>".into()),
        );
        let count = if failed_path == "network-plans.toml" {
            10
        } else {
            2
        };
        let (base, server) = serve(replies, count);
        let runtime = ProxyRuntime::new(provider);
        runtime.set_official_address_for_test(provider, routes(&base, provider));
        let root = runtime.official_root(false).await.unwrap();
        let mut trusted = crate::network_policy::NetworkPolicy::bundled().unwrap();
        for proxy in trusted.proxies.values_mut() {
            proxy.name = format!("TRUSTED {}", proxy.name);
        }
        runtime.publish_policy(trusted, provider, 0).unwrap();
        let before = runtime.configuration_snapshot().unwrap();
        if failed_path == "maps.toml" {
            let maps = SourceLocation::parse(&format!("{base}/Cloudflare/maps.toml")).unwrap();
            assert!(
                crate::repository_sources::load_text_document(&maps, |text, origin| {
                    crate::map_sources::parse_document(text, origin, &Default::default())
                })
                .await
                .is_err()
            );
        } else if failed_path == "child.toml" {
            let child = SourceLocation::parse(&root.document.repository_sources[0].url).unwrap();
            assert!(load_document(&child, &Default::default()).await.is_err());
        } else {
            assert!(runtime.refresh_policy().await.is_err());
        }
        assert_eq!(runtime.selection(), (provider, 0));
        let after = runtime.configuration_snapshot().unwrap();
        assert_eq!(before.1, after.1);
        assert_eq!(*before.2, *after.2);
        assert_eq!(server.join().unwrap().len(), count);
    }
}

#[tokio::test]
async fn user_switch_cancels_root_http_and_waiters_and_rejects_a_b_a() {
    let runtime = Arc::new(ProxyRuntime::new(DataProvider::Github));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}/mods.toml", listener.local_addr().unwrap());
    let pending = runtime.clone();
    let task =
        tokio::spawn(async move { pending.official_root_from(false, |_| address.clone()).await });
    let (_socket, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let old = runtime.selection();
    runtime.select_provider(DataProvider::Cloudflare);
    runtime.select_provider(DataProvider::Github);
    let result = tokio::time::timeout(Duration::from_millis(200), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.err().as_deref(), Some("source.addressMismatch"));
    assert!(runtime.current.read().unwrap().official_root.is_none());
    assert_ne!(runtime.selection(), old);
    assert!(runtime.in_selection(old, async { Ok(()) }).await.is_err());
    let _guard = runtime.root_refresh.lock().await;
    runtime.select_provider(DataProvider::Builtin);
    let builtin = tokio::time::timeout(Duration::from_millis(200), runtime.official_root(false))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(builtin.selection.0, DataProvider::Builtin);
}

#[test]
fn explicit_selection_and_automatic_fallback_have_distinct_revision_ownership() {
    let runtime = ProxyRuntime::new(DataProvider::Github);
    let changes = Arc::new(Mutex::new(runtime.selection_changes()));
    runtime.select_provider(DataProvider::Cloudflare);
    assert!(changes.lock().unwrap().has_changed().unwrap());
    assert_eq!(runtime.current.read().unwrap().manual_revision, 1);
}
