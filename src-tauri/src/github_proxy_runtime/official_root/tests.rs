use super::*;
use std::collections::BTreeMap;
use std::future::{Future, poll_fn};
use std::io::{Cursor, Read, Write};
use std::sync::Arc;
use std::task::Poll;
use std::time::{Duration, Instant};

const ROOT: &str = "[[repository_sources]]\nid='child'\nnickname='Child'\nurl='./child.toml'\n";
const CHILD: &str = "[[repository_sources]]\nid='leaf'\nnickname='Leaf'\nurl='./leaf.toml'\n";

#[derive(Clone)]
struct Reply {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl Reply {
    fn zip(body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type: "application/zip",
            body,
        }
    }

    fn offline() -> Self {
        Self {
            status: 503,
            content_type: "text/plain",
            body: b"offline".to_vec(),
        }
    }
}

fn documents() -> BTreeMap<String, String> {
    let bundled = SourcePackage::bundled();
    let index = bundled.read("proxies.toml").unwrap();
    let mut documents = BTreeMap::from([
        ("mods.toml".into(), ROOT.into()),
        ("child.toml".into(), CHILD.into()),
        (
            "maps.toml".into(),
            bundled.read("maps.toml").unwrap().into(),
        ),
        ("proxies.toml".into(), index.into()),
        (
            "network-plans.toml".into(),
            bundled.read("network-plans.toml").unwrap().into(),
        ),
    ]);
    for path in crate::network_policy::proxy_documents(index).unwrap() {
        let body = bundled.read(&path).unwrap().to_owned();
        documents.insert(path, body);
    }
    documents
}

fn archive(documents: &BTreeMap<String, String>) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, body) in documents {
        writer
            .start_file(
                format!("fixture-main/{path}"),
                zip::write::SimpleFileOptions::default()
                    .compression_method(zip::CompressionMethod::Stored),
            )
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn route(provider: DataProvider) -> String {
    format!("/{provider:?}/main.zip")
}

fn routes(base: &str, provider: DataProvider) -> String {
    format!("{base}{}", route(provider))
}

fn serve(replies: Vec<(DataProvider, Reply)>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut paths = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(6);
        for (provider, reply) in replies {
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "missing request after {paths:?}");
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            // Accepted sockets must not inherit a platform-dependent nonblocking mode.
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
            let lowercase = request.to_ascii_lowercase();
            assert!(!lowercase.contains("authorization:"));
            assert!(!lowercase.contains("cookie:"));
            assert_eq!(request.split_whitespace().next(), Some("GET"));
            let path = request.split_whitespace().nth(1).unwrap().to_owned();
            assert_eq!(
                path,
                route(provider),
                "only archive requests may reach HTTP"
            );
            write!(stream, "HTTP/1.1 {} Fixture\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.status, reply.content_type, reply.body.len()).unwrap();
            stream.write_all(&reply.body).unwrap();
            paths.push(path);
        }
        paths
    });
    (base, server)
}

#[tokio::test]
async fn invalid_archives_only_visit_the_authorized_regional_fallback_chain() {
    let valid = documents();
    let mut invalid_root = valid.clone();
    invalid_root.insert("mods.toml".into(), "invalid [".into());
    let mut invalid_plans = valid.clone();
    invalid_plans.insert("network-plans.toml".into(), "invalid [".into());
    let mut missing_proxy = valid.clone();
    let proxy = crate::network_policy::proxy_documents(&valid["proxies.toml"])
        .unwrap()
        .remove(0);
    missing_proxy.remove(&proxy);
    let mut failures = vec![
        ("HTTP failure", Reply::offline()),
        (
            "HTML content type",
            Reply {
                status: 200,
                content_type: "text/html",
                body: b"<html>verify</html>".to_vec(),
            },
        ),
        (
            "HTML disguised as ZIP",
            Reply::zip(b"<html>verify</html>".to_vec()),
        ),
        ("empty body", Reply::zip(Vec::new())),
        ("invalid ZIP", Reply::zip(b"PK\x03\x04truncated".to_vec())),
        ("invalid MOD root", Reply::zip(archive(&invalid_root))),
        ("invalid network plans", Reply::zip(archive(&invalid_plans))),
        ("missing network child", Reply::zip(archive(&missing_proxy))),
    ];
    for required in [
        "mods.toml",
        "maps.toml",
        "proxies.toml",
        "network-plans.toml",
    ] {
        let mut incomplete = valid.clone();
        incomplete.remove(required);
        failures.push((required, Reply::zip(archive(&incomplete))));
    }
    for (name, failure) in failures {
        for (initial, cloudflare_ok) in [
            (DataProvider::Github, true),
            (DataProvider::Github, false),
            (DataProvider::Cloudflare, false),
        ] {
            let mut replies = vec![(initial, failure.clone())];
            if initial != DataProvider::Cloudflare {
                replies.push((
                    DataProvider::Cloudflare,
                    if cloudflare_ok {
                        Reply::zip(archive(&valid))
                    } else {
                        Reply::offline()
                    },
                ));
            }
            let expected_paths = replies
                .iter()
                .map(|(provider, _)| route(*provider))
                .collect::<Vec<_>>();
            let (base, server) = serve(replies);
            let runtime = ProxyRuntime::new(initial);
            let root = runtime
                .official_root_from(true, |provider| routes(&base, provider))
                .await
                .unwrap();
            let expected = if cloudflare_ok {
                DataProvider::Cloudflare
            } else {
                DataProvider::Builtin
            };
            assert_eq!(
                runtime.provider(),
                expected,
                "{name}: {initial:?}, cloudflare={cloudflare_ok}"
            );
            assert_eq!(root.selection, runtime.selection());
            assert_eq!(
                root.selection.1,
                expected_paths.len() as u64 + u64::from(expected == DataProvider::Builtin)
            );
            assert!(!root.document.repository_sources.is_empty());
            {
                let snapshot = runtime.current.read().unwrap();
                assert_eq!(snapshot.manual_revision, 0);
                assert!(Arc::ptr_eq(
                    snapshot.package.as_ref().unwrap(),
                    &root.package
                ));
                assert!(snapshot.policy.is_some());
            }
            let cached = runtime
                .official_root_from(false, |_| panic!("cached root must not resolve an address"))
                .await
                .unwrap();
            assert_eq!(cached.selection, root.selection);
            assert!(Arc::ptr_eq(&cached.package, &root.package));
            assert_eq!(server.join().unwrap(), expected_paths);
        }
    }
}

#[tokio::test]
async fn startup_and_force_consumers_coalesce_but_a_later_force_downloads_again() {
    for initial in [
        DataProvider::Github,
        DataProvider::Gitcode,
        DataProvider::Cloudflare,
    ] {
        let reply = Reply::zip(archive(&documents()));
        let (base, server) = serve(vec![
            (initial, reply.clone()),
            (initial, reply.clone()),
            (initial, reply),
        ]);
        let runtime = ProxyRuntime::new(initial);
        let address = |provider| routes(&base, provider);
        let (first, second) = tokio::join!(
            runtime.official_root_from(false, address),
            runtime.official_root_from(true, address),
        );
        let (first, second) = (first.unwrap(), second.unwrap());
        assert_eq!(first.selection, (initial, 1));
        assert_eq!(first.selection, second.selection);
        assert!(Arc::ptr_eq(&first.package, &second.package));
        let cached = runtime.official_root(false).await.unwrap();
        assert!(Arc::ptr_eq(&cached.package, &first.package));
        runtime.refresh_policy().await.unwrap();
        assert_eq!(runtime.selection(), first.selection);
        let (first_force, second_force) = tokio::join!(
            runtime.official_root_from(true, address),
            runtime.official_root_from(true, address),
        );
        let (first_force, second_force) = (first_force.unwrap(), second_force.unwrap());
        assert_eq!(first_force.selection, (initial, 2));
        assert_eq!(first_force.selection, second_force.selection);
        assert!(Arc::ptr_eq(&first_force.package, &second_force.package));
        assert!(!Arc::ptr_eq(&first_force.package, &first.package));
        assert_eq!(
            runtime
                .official_root_from(true, address)
                .await
                .unwrap()
                .selection,
            (initial, 3)
        );
        assert_eq!(server.join().unwrap(), vec![route(initial); 3]);
    }
}

#[tokio::test]
async fn parallel_waiters_follow_automatic_fallback_without_cancelling_the_new_provider() {
    let (base, server) = serve(vec![
        (DataProvider::Github, Reply::offline()),
        (DataProvider::Cloudflare, Reply::zip(archive(&documents()))),
    ]);
    let runtime = ProxyRuntime::new(DataProvider::Github);
    let address = |provider| routes(&base, provider);
    let (first, second) = tokio::join!(
        runtime.official_root_from(false, address),
        runtime.official_root_from(true, address),
    );
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(first.selection, (DataProvider::Cloudflare, 2));
    assert_eq!(first.selection, second.selection);
    assert!(Arc::ptr_eq(&first.package, &second.package));
    assert_eq!(runtime.current.read().unwrap().manual_revision, 0);
    assert_eq!(
        server.join().unwrap(),
        [route(DataProvider::Github), route(DataProvider::Cloudflare)]
    );
}

#[tokio::test]
async fn every_official_document_alias_reads_the_captured_package_without_http() {
    let (base, server) = serve(vec![(
        DataProvider::Github,
        Reply::zip(archive(&documents())),
    )]);
    let runtime = ProxyRuntime::new(DataProvider::Github);
    for provider in [DataProvider::Github, DataProvider::Cloudflare] {
        runtime.set_official_address_for_test(provider, routes(&base, provider));
    }
    let root = runtime.official_root(false).await.unwrap();
    let child = SourceLocation::parse(&root.document.repository_sources[0].url).unwrap();
    let loaded = runtime
        .in_selection(root.selection, async {
            let active = SourcePackage::active().unwrap();
            assert!(Arc::ptr_eq(&active, &root.package));
            let child = load_document(&child, &Default::default())
                .await
                .map_err(|error| error.to_string())?;
            assert_eq!(child.repository_sources[0].id, "leaf");
            for provider in [
                DataProvider::Github,
                DataProvider::Gitee,
                DataProvider::Gitcode,
                DataProvider::Cloudflare,
                DataProvider::Builtin,
            ] {
                let location = SourceLocation::parse(&provider.document("child.toml")).unwrap();
                let child = load_document(&location, &Default::default())
                    .await
                    .map_err(|error| error.to_string())?;
                assert_eq!(child.repository_sources[0].id, "leaf");
            }
            let maps = SourceLocation::parse(&DataProvider::Github.document("maps.toml")).unwrap();
            crate::repository_sources::load_text_document(&maps, |text, origin| {
                crate::map_sources::parse_document(text, origin, &Default::default())
            })
            .await
            .map_err(|error| error.to_string())?;
            Ok(())
        })
        .await;
    loaded.unwrap();
    assert_eq!(runtime.selection(), root.selection);
    assert_eq!(server.join().unwrap(), [route(DataProvider::Github)]);
}

#[tokio::test]
async fn missing_business_documents_fail_in_the_node_without_builtin_or_http_repair() {
    // Choose an actual bundled business child as a fallback trap. All four
    // entry documents remain present in the remote archive.
    let builtin = ProxyRuntime::new(DataProvider::Builtin);
    let bundled_root = builtin.official_root(false).await.unwrap();
    let bundled_child = bundled_root
        .document
        .repository_sources
        .iter()
        .find_map(|source| {
            let location = SourceLocation::parse(&source.url).unwrap();
            match location {
                SourceLocation::Bundled(path) => Some(path),
                SourceLocation::Remote(url) => DataProvider::official_document_path(url.as_str()),
            }
        })
        .expect("bundled root must declare a package child");
    assert!(bundled_root.package.read(&bundled_child).is_ok());
    let mut documents = documents();
    documents.remove("child.toml");
    assert!(!documents.contains_key(&bundled_child));
    let (base, server) = serve(vec![(
        DataProvider::Cloudflare,
        Reply::zip(archive(&documents)),
    )]);
    let runtime = ProxyRuntime::new(DataProvider::Cloudflare);
    let root = runtime
        .official_root_from(false, |provider| routes(&base, provider))
        .await
        .unwrap();
    assert_eq!(root.selection, (DataProvider::Cloudflare, 1));
    let before = runtime.configuration_snapshot().unwrap();
    runtime
        .in_selection(root.selection, async {
            let child = SourceLocation::parse(&root.document.repository_sources[0].url).unwrap();
            let error = load_document(&child, &Default::default())
                .await
                .err()
                .unwrap()
                .to_string();
            assert!(error.contains("source.packageFileMissing"), "{error}");
            // Both relative and official remote identities must read the active
            // archive even though the shipped package contains this exact child.
            for location in [
                SourceLocation::parse(&bundled_child).unwrap(),
                SourceLocation::parse(&DataProvider::Cloudflare.document(&bundled_child)).unwrap(),
            ] {
                let error = crate::repository_sources::load_text_document(&location, |text, _| {
                    Ok(text.to_owned())
                })
                .await
                .err()
                .unwrap()
                .to_string();
                assert!(error.contains("source.packageFileMissing"), "{error}");
            }
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(runtime.selection(), root.selection);
    let after = runtime.configuration_snapshot().unwrap();
    assert_eq!(before.0, after.0);
    assert_eq!(before.1, after.1);
    assert!(Arc::ptr_eq(&before.2, &after.2));
    assert_eq!(server.join().unwrap(), [route(DataProvider::Cloudflare)]);
}

#[tokio::test]
async fn malformed_maps_root_is_a_page_error_without_replacing_the_trusted_package() {
    let mut documents = documents();
    documents.insert("maps.toml".into(), "invalid [".into());
    let (base, server) = serve(vec![(
        DataProvider::Cloudflare,
        Reply::zip(archive(&documents)),
    )]);
    let runtime = ProxyRuntime::new(DataProvider::Cloudflare);
    let root = runtime
        .official_root_from(false, |provider| routes(&base, provider))
        .await
        .unwrap();
    assert_eq!(root.selection, (DataProvider::Cloudflare, 1));
    let before = runtime.configuration_snapshot().unwrap();
    runtime
        .in_selection(root.selection, async {
            let maps =
                SourceLocation::parse(&DataProvider::Cloudflare.document("maps.toml")).unwrap();
            assert!(
                crate::repository_sources::load_text_document(&maps, |text, origin| {
                    crate::map_sources::parse_document(text, origin, &Default::default())
                })
                .await
                .is_err()
            );
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(runtime.selection(), root.selection);
    let after = runtime.configuration_snapshot().unwrap();
    assert_eq!(before.0, after.0);
    assert_eq!(before.1, after.1);
    assert!(Arc::ptr_eq(&before.2, &after.2));
    assert_eq!(server.join().unwrap(), [route(DataProvider::Cloudflare)]);
}

#[tokio::test]
async fn same_provider_package_replacement_cancels_old_scopes_and_keeps_old_arcs_immutable() {
    let first_documents = documents();
    let mut second_documents = first_documents.clone();
    second_documents.insert(
        "child.toml".into(),
        CHILD.replace("Leaf", "Second generation"),
    );
    let (base, server) = serve(vec![
        (DataProvider::Github, Reply::zip(archive(&first_documents))),
        (DataProvider::Github, Reply::zip(archive(&second_documents))),
    ]);
    let runtime = ProxyRuntime::new(DataProvider::Github);
    let address = |provider| routes(&base, provider);
    let first = runtime.official_root_from(false, address).await.unwrap();
    let (started, ready) = tokio::sync::oneshot::channel();
    let old_scope = runtime.in_selection(first.selection, async {
        assert!(Arc::ptr_eq(
            &SourcePackage::active().unwrap(),
            &first.package
        ));
        started.send(()).unwrap();
        std::future::pending::<()>().await;
        Ok(())
    });
    let replace = async {
        ready.await.unwrap();
        runtime.official_root_from(true, address).await.unwrap()
    };
    let (cancelled, second) = tokio::time::timeout(Duration::from_secs(2), async {
        tokio::join!(old_scope, replace)
    })
    .await
    .expect("same-provider refresh must release old scopes");
    assert_eq!(cancelled.err().as_deref(), Some("source.addressMismatch"));
    assert_eq!(first.selection, (DataProvider::Github, 1));
    assert_eq!(second.selection, (DataProvider::Github, 2));
    assert_eq!(first.package.read("child.toml").unwrap(), CHILD);
    assert_eq!(
        second.package.read("child.toml").unwrap(),
        second_documents["child.toml"]
    );
    assert!(!Arc::ptr_eq(&first.package, &second.package));
    let snapshot = runtime.current.read().unwrap();
    assert_eq!(snapshot.manual_revision, 0);
    assert!(Arc::ptr_eq(
        snapshot.package.as_ref().unwrap(),
        &second.package
    ));
    assert!(snapshot.policy.is_some());
    assert!(
        snapshot
            .groups
            .raw_proxies
            .iter()
            .all(|proxy| proxy.remote && !proxy.builtin)
    );
    drop(snapshot);
    assert_eq!(server.join().unwrap(), vec![route(DataProvider::Github); 2]);
}

#[tokio::test]
async fn user_switch_cancels_http_and_lock_waiters_and_rejects_a_b_a() {
    let runtime = Arc::new(ProxyRuntime::new(DataProvider::Github));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}/main.zip", listener.local_addr().unwrap());
    let pending = runtime.clone();
    let first_address = address.clone();
    let task = tokio::spawn(async move {
        pending
            .official_root_from(false, |_| first_address.clone())
            .await
    });
    let (_socket, _) = tokio::time::timeout(Duration::from_secs(2), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut waiter = Box::pin(runtime.official_root_from(true, |_| address.clone()));
    // Poll the lock waiter before changing provider, rather than guessing that
    // a spawned waiter has started after a delay or a scheduler yield.
    poll_fn(|context| {
        assert!(waiter.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
    let old = runtime.selection();
    runtime.select_provider(DataProvider::Cloudflare, None);
    runtime.select_provider(DataProvider::Github, None);
    let result = tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result.err().as_deref(), Some("source.addressMismatch"));
    let result = tokio::time::timeout(Duration::from_secs(2), waiter)
        .await
        .unwrap();
    assert_eq!(result.err().as_deref(), Some("source.addressMismatch"));
    assert!(runtime.current.read().unwrap().official_root.is_none());
    assert!(runtime.current.read().unwrap().package.is_none());
    assert_ne!(runtime.selection(), old);
    assert!(runtime.in_selection(old, async { Ok(()) }).await.is_err());
    let _guard = runtime.root_refresh.lock().await;
    runtime.select_provider(DataProvider::Builtin, None);
    let builtin = tokio::time::timeout(Duration::from_secs(2), runtime.official_root(false))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(builtin.selection.0, DataProvider::Builtin);
    assert!(Arc::ptr_eq(&builtin.package, &SourcePackage::bundled()));
}

#[tokio::test]
async fn missing_native_browser_client_skips_gitee_http_and_follows_the_domestic_chain() {
    let rejected = prepare_package(
        DataProvider::Gitee,
        "not a network address".into(),
        "cn",
        None,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(rejected, "source.webDownloadFailed");
    for (initial, gitcode_reply) in [
        (DataProvider::Gitee, None),
        (DataProvider::Gitcode, Some(Reply::offline())),
        (
            DataProvider::Gitcode,
            Some(Reply {
                status: 200,
                content_type: "text/html",
                body: b"<html>verify</html>".to_vec(),
            }),
        ),
        (
            DataProvider::Gitcode,
            Some(Reply::zip(b"<html>verify</html>".to_vec())),
        ),
    ] {
        for cloudflare_ok in [true, false] {
            let mut replies = Vec::new();
            if let Some(reply) = gitcode_reply.clone() {
                replies.push((DataProvider::Gitcode, reply));
            }
            replies.push((
                DataProvider::Cloudflare,
                if cloudflare_ok {
                    Reply::zip(archive(&documents()))
                } else {
                    Reply::offline()
                },
            ));
            let expected_paths = replies
                .iter()
                .map(|(provider, _)| route(*provider))
                .collect::<Vec<_>>();
            let (base, server) = serve(replies);
            // No AppHandle is installed. Gitee must report failure without
            // opening a browser or silently substituting an HTTP download.
            let runtime = ProxyRuntime::new(initial);
            let resolved = std::sync::Mutex::new(Vec::new());
            let root = runtime
                .official_root_from(false, |provider| {
                    resolved.lock().unwrap().push(provider);
                    routes(&base, provider)
                })
                .await
                .unwrap();
            let mut expected_chain = if initial == DataProvider::Gitcode {
                vec![
                    DataProvider::Gitcode,
                    DataProvider::Gitee,
                    DataProvider::Cloudflare,
                ]
            } else {
                vec![DataProvider::Gitee, DataProvider::Cloudflare]
            };
            if !cloudflare_ok {
                expected_chain.push(DataProvider::Builtin);
            }
            let expected = *expected_chain.last().unwrap();
            assert_eq!(root.selection, (expected, expected_chain.len() as u64));
            assert_eq!(*resolved.lock().unwrap(), expected_chain);
            assert_eq!(runtime.current.read().unwrap().manual_revision, 0);
            assert_eq!(server.join().unwrap(), expected_paths);
        }
    }
}

#[test]
fn explicit_selection_and_automatic_fallback_have_distinct_revision_ownership() {
    let runtime = ProxyRuntime::new(DataProvider::Github);
    let changes = runtime.selection_changes();
    runtime.select_provider(DataProvider::Cloudflare, None);
    assert!(changes.has_changed().unwrap());
    assert_eq!(runtime.current.read().unwrap().manual_revision, 1);
}
