use super::*;
use crate::config::ConfigService;
use crate::models::ImportPreview;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

pub(super) fn test_state() -> AppState {
    AppState::with_config(ConfigService::for_test(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-source-refresh")
            .join(Uuid::new_v4().to_string()),
    ))
}

pub(super) async fn refresh(
    state: &AppState,
    source: &RepositorySource,
) -> Result<ImportPreview, String> {
    let result = import_repository_source_into_config(
        state,
        source.id.clone(),
        source.url.clone(),
        false,
        false,
    )
    .await?;
    if let Some(error) = result.access_error {
        return Err(error);
    }
    result
        .preview
        .ok_or_else(|| "missing import preview".into())
}

#[tokio::test]
async fn explicit_builtin_selection_is_immediate_and_legacy_forced_fallback_cannot_bypass_gate() {
    let state = test_state();
    // A legacy request cannot force bundled content without selecting Builtin;
    // only the official root gate owns automatic provider fallback.
    let mut online = state.config.load().unwrap();
    crate::config::synchronize_builtin_repository_sources(&mut online);
    state.config.save(&online).unwrap();
    let mut config = state.config.load().unwrap();
    let root = config.repository_sources[0].clone();
    let custom = RepositorySource {
        note_i18n: Default::default(),
        network_plan: Default::default(),
        id: "custom-source".into(),
        nickname: "Custom".into(),
        nickname_i18n: Default::default(),
        url: "https://example.com/custom.toml".into(),
        region: None,
        builtin: false,
        parent_id: None,
    };
    config.repository_sources.push(custom.clone());
    state.config.save(&config).unwrap();
    assert!(
        import_repository_source_into_config(
            &state,
            custom.id.clone(),
            custom.url.clone(),
            true,
            false
        )
        .await
        .is_err()
    );
    assert_eq!(
        state.config.load().unwrap().repository_sources,
        config.repository_sources
    );
    assert!(
        import_repository_source_into_config(
            &state,
            root.id.clone(),
            root.url.clone(),
            true,
            false
        )
        .await
        .is_err()
    );
    state
        .config
        .select_data_provider(crate::models::DataProvider::Builtin);
    let root = state.config.load().unwrap().repository_sources[0].clone();
    let result = import_repository_source_into_config(
        &state,
        root.id.clone(),
        root.url.clone(),
        false,
        false,
    )
    .await
    .unwrap();
    assert!(result.preview.unwrap().valid);
    assert!(result.access_error.is_none());
    assert!(!result.builtin_fallback_available);
    let loaded = state.config.load().unwrap();
    assert_eq!(loaded.repository_sources[0], root);
    assert!(loaded.repository_sources.contains(&custom));
    let children = loaded
        .repository_sources
        .iter()
        .filter(|s| s.parent_id.as_deref() == Some(root.id.as_str()))
        .collect::<Vec<_>>();
    /*
     * 数量从内置索引**实际声明**推导，而不是写死。
     *
     * 之前这里是 `assert_eq!(children.len(), 5)`：作者往索引里加一个子源
     * 就会让这条测试失败，而失败原因与回退逻辑毫无关系 —— 纯粹是常量过期。
     * 真正要断言的是「索引声明了几个，回退后就该有几个」。
     */
    let declared_children = {
        let location = SourceLocation::parse(repository_sources::BUILTIN_INDEX).unwrap();
        let document = repository_sources::parse_document(
            location.bundled_text().unwrap(),
            &location,
            &Default::default(),
        )
        .unwrap();
        document.repository_sources.len()
    };
    assert!(declared_children > 0, "内置索引应当至少声明一个子源");
    assert_eq!(children.len(), declared_children);
    for child in &children {
        assert!(matches!(
            SourceLocation::parse(&child.url).unwrap(),
            SourceLocation::Bundled(_)
        ));
        refresh(&state, child).await.unwrap();
    }
    let loaded = state.config.load().unwrap();
    assert_eq!(loaded.repository_sources[0].url, root.url);
    let expected_repository_count = children
        .iter()
        .map(|child| {
            SourceLocation::parse(&child.url)
                .unwrap()
                .bundled_document()
                .unwrap()
                .repositories
                .len()
        })
        .sum::<usize>();
    assert_eq!(loaded.repositories.len(), expected_repository_count);
    assert!(loaded.repository_sources.contains(&custom));
    fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
}

pub(super) fn serve_documents(
    responses: Vec<(&'static str, &'static str, String)>,
) -> (std::net::SocketAddr, std::thread::JoinHandle<()>) {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        for (path, status, body) in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline, "request did not arrive");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("{error}"),
                }
            };
            // Accepted sockets inherit nonblocking mode on Windows.
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            assert!(request.starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            assert!(
                request
                    .to_ascii_lowercase()
                    .contains("cache-control: no-cache, no-store, max-age=0\r\n")
            );
            assert!(!request.to_ascii_lowercase().contains("if-none-match:"));
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    (address, server)
}

#[tokio::test]
async fn pending_or_unreadable_transaction_state_does_not_prevent_source_refresh() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::with_config(ConfigService::for_test(temp.path().join("data")));
    let repository = |name: &str| {
        format!(
            "[[repositories]]\nname_i18n.zh-CN = \"{name}\"\ngroup_i18n.zh-CN = \"Tools\"\nnote_i18n.zh-CN = \"\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{type = \"glob\", value = \"*.dll\"}}]\n"
        )
    };
    let (address, server) = serve_documents(
        ["Original", "Pending", "Unclear", "Unclear", "Renamed"]
            .into_iter()
            .map(|name| ("/mods.toml", "200 OK", repository(name)))
            .collect(),
    );
    let source = RepositorySource {
        id: "live-manifest-sync".into(),
        nickname: "Source".into(),
        url: format!("http://{address}/mods.toml"),
        note_i18n: Default::default(),
        network_plan: Default::default(),
        nickname_i18n: Default::default(),
        region: None,
        builtin: false,
        parent_id: None,
    };
    let mut config = state.config.load().unwrap();
    config.repository_sources.push(source.clone());
    state.config.save(&config).unwrap();
    refresh(&state, &source).await.unwrap();
    let repo = state.config.load().unwrap().repositories.remove(0);
    let mut manifest = archive::new_manifest(
        &repo.id,
        "v1.0.0",
        archive::single_language("Original - GitHub"),
        Default::default(),
        vec!["BepInEx/plugins/Example.dll".into()],
        Vec::new(),
    );
    manifest.repository_binding = ModRepositoryBinding::from_release(&repo, &manifest.release_tag);
    manifest.group_i18n = repo.group_i18n;
    let manifests = state.config.manifests_dir();
    archive::write_manifest(&manifests, &manifest).unwrap();
    let manifest_path = manifests.join(format!("{}.toml", repo.id));
    let original = fs::read(&manifest_path).unwrap();
    let transaction_root = temp.path().join("data/mod-change-transactions");
    let journal = transaction_root.join("pending/journal.json");
    fs::create_dir_all(journal.parent().unwrap()).unwrap();
    fs::write(&journal, b"unparsed journal must remain byte-identical").unwrap();

    assert!(refresh(&state, &source).await.unwrap().valid);
    assert_eq!(state.config.load().unwrap().repositories[0].name, "Pending");
    assert_eq!(fs::read(&manifest_path).unwrap(), original);
    assert_eq!(
        fs::read(&journal).unwrap(),
        b"unparsed journal must remain byte-identical"
    );

    fs::remove_file(&journal).unwrap();
    fs::remove_dir(journal.parent().unwrap()).unwrap();
    fs::remove_dir(&transaction_root).unwrap();
    fs::write(&transaction_root, b"not a directory").unwrap();
    assert!(refresh(&state, &source).await.unwrap().valid);
    assert_eq!(state.config.load().unwrap().repositories[0].name, "Unclear");
    assert_eq!(fs::read(&manifest_path).unwrap(), original);
    assert_eq!(fs::read(&transaction_root).unwrap(), b"not a directory");

    fs::remove_file(&transaction_root).unwrap();
    manifest.package_name_i18n = archive::single_language("Unclear - GitHub");
    archive::write_manifest(&manifests, &manifest).unwrap();
    let mut bytes = fs::read(&manifest_path).unwrap();
    bytes.extend_from_slice(b"\n# harmless formatting survives an unchanged refresh\n");
    fs::write(&manifest_path, &bytes).unwrap();
    refresh(&state, &source).await.unwrap();
    assert_eq!(fs::read(&manifest_path).unwrap(), bytes);
    refresh(&state, &source).await.unwrap();
    assert_eq!(
        archive::read_manifest(&manifests, &repo.id)
            .unwrap()
            .package_name_i18n,
        archive::single_language("Renamed - GitHub")
    );
    server.join().unwrap();
}

#[tokio::test]
async fn live_index_changes_replace_children_and_failure_keeps_trusted_snapshots() {
    let declaration = |id, nickname: &str| {
        format!(
            "[[repository_sources]]\nid = \"{id}\"\nnickname = \"{nickname}\"\nurl = \"{id}.toml\"\n[repository_sources.nickname_i18n]\nzh-CN = \"{nickname}\"\n"
        )
    };
    let repository = |name| {
        format!(
            "[[repositories]]\nname = \"{name}\"\nname_i18n.zh-CN = \"{name}\"\ngroup_i18n.zh-CN = \"\"\nnote_i18n.zh-CN = \"\"\nprovider = \"github\"\nusername = \"example\"\nrepository = \"mod\"\ninstall_target = \"BepInEx/plugins\"\nenabled = true\nasset_rules = [{{type = \"glob\", value = \"*.dll\"}}]\n"
        )
    };
    let (address, server) = serve_documents(vec![
        ("/index.toml", "200 OK", declaration("a", "Old source")),
        ("/a.toml", "200 OK", repository("Old MOD")),
        (
            "/index.toml",
            "200 OK",
            format!(
                "{}{}",
                declaration("a", "Changed source"),
                declaration("b", "New source")
            ),
        ),
        ("/a.toml", "503 Service Unavailable", String::new()),
        ("/a.toml", "200 OK", repository("Changed MOD")),
        ("/index.toml", "200 OK", declaration("b", "New source")),
        ("/b.toml", "200 OK", repository("New MOD")),
        ("/index.toml", "503 Service Unavailable", String::new()),
        ("/index.toml", "200 OK", "invalid = [".into()),
        (
            "/index.toml",
            "200 OK",
            declaration("b", "Recovered source"),
        ),
        ("/b.toml", "200 OK", repository("Recovered MOD")),
    ]);
    let state = test_state();
    let mut config = state.config.load().unwrap();
    let root = RepositorySource {
        note_i18n: Default::default(),
        network_plan: Default::default(),
        id: "live-index-test".into(),
        nickname: "Live index".into(),
        nickname_i18n: Default::default(),
        url: format!("http://{address}/index.toml"),
        region: None,
        builtin: false,
        parent_id: None,
    };
    let mut unrelated = root.clone();
    unrelated.id = "unrelated-custom-source".into();
    unrelated.url = "https://example.com/custom.toml".into();
    config
        .repository_sources
        .extend([root.clone(), unrelated.clone()]);
    state.config.save(&config).unwrap();
    let mut installed_repository_ids = std::collections::HashMap::new();
    for (expected_source_name, expected_mod_name, child_id) in [
        ("Old source", "Old MOD", "a"),
        ("Changed source", "Changed MOD", "a"),
        ("New source", "New MOD", "b"),
    ] {
        refresh(&state, &root).await.unwrap();
        let index_config = state.config.load().unwrap();
        assert_eq!(
            index_config.repositories.len(),
            usize::from(installed_repository_ids.contains_key(child_id)),
            "still-declared children keep their trusted repositories until refreshed"
        );
        let child = index_config
            .repository_sources
            .iter()
            .find(|s| s.id == child_id)
            .unwrap();
        assert_eq!(child.nickname, expected_source_name);
        assert_eq!(child.nickname_i18n["zh-CN"], expected_source_name);
        assert_eq!(child.url, format!("http://{address}/{child_id}.toml"));
        assert_eq!(child.parent_id.as_deref(), Some(root.id.as_str()));
        if child_id == "a" && installed_repository_ids.contains_key(child_id) {
            assert!(refresh(&state, child).await.is_err());
            assert_eq!(
                serde_json::to_value(state.config.load().unwrap()).unwrap(),
                serde_json::to_value(&index_config).unwrap(),
                "parent success plus child failure must preserve active and staged snapshots"
            );
        }
        refresh(&state, child).await.unwrap();
        let mut loaded = state.config.load().unwrap();
        assert_eq!(loaded.repositories.len(), 1);
        assert_eq!(loaded.repositories[0].name, expected_mod_name);
        assert!(loaded.repository_sources.contains(&unrelated));
        if let Some(previous_id) = installed_repository_ids.get(child_id) {
            assert_eq!(
                &loaded.repositories[0].id, previous_id,
                "refresh must preserve the ID used by installed MOD manifests"
            );
        } else {
            installed_repository_ids.insert(child_id, loaded.repositories[0].id.clone());
        }
        if child_id == "b" {
            assert!(!loaded.repository_sources.iter().any(|s| s.id == "a"));
            let old_id = &installed_repository_ids["a"];
            assert_eq!(
                loaded.staged_release_cache[old_id]
                    .release_prerelease_metadata
                    .get("v1.0.0-beta"),
                Some(&false)
            );
        }
        loaded.repositories[0]
            .release_prerelease_metadata
            .insert("v1.0.0-beta".into(), false);
        loaded.repositories[0].releases_cached_at = Some(123);
        loaded.repositories[0].cached_releases_complete = true;
        state.config.save(&loaded).unwrap();
    }
    let trusted = state.config.load().unwrap();
    let inaccessible = import_repository_source_into_config(
        &state,
        root.id.clone(),
        root.url.clone(),
        false,
        false,
    )
    .await
    .unwrap();
    assert!(inaccessible.access_error.is_some());
    assert!(inaccessible.preview.is_none());
    assert!(
        !inaccessible.builtin_fallback_available,
        "custom sources cannot fallback"
    );
    assert_eq!(
        serde_json::to_value(state.config.load().unwrap()).unwrap(),
        serde_json::to_value(&trusted).unwrap(),
        "HTTP failure must not publish a new config"
    );
    assert!(
        refresh(&state, &root).await.is_err(),
        "invalid TOML remains a content error"
    );
    let failed = state.config.load().unwrap();
    assert_eq!(
        serde_json::to_value(&failed).unwrap(),
        serde_json::to_value(&trusted).unwrap(),
        "invalid TOML must keep the trusted tree too"
    );
    assert!(failed.repository_sources.contains(&root));
    assert!(failed.repository_sources.contains(&unrelated));
    // A new state owns no in-memory identity cache. Recovery must use the
    // persisted ID map, while still fetching the changed remote content.
    let restarted = AppState::with_config(ConfigService::for_test(
        state.config.config_path().parent().unwrap().to_path_buf(),
    ));
    let offline = restarted.config.load().unwrap();
    assert_eq!(
        offline.repositories[0]
            .release_prerelease_metadata
            .get("v1.0.0-beta"),
        Some(&false)
    );
    assert_eq!(
        offline.staged_release_cache[&installed_repository_ids["a"]]
            .release_prerelease_metadata
            .get("v1.0.0-beta"),
        Some(&false)
    );
    refresh(&restarted, &root).await.unwrap();
    let restored = restarted.config.load().unwrap();
    let child = restored
        .repository_sources
        .iter()
        .find(|source| source.id == "b")
        .unwrap();
    refresh(&restarted, child).await.unwrap();
    let restored = restarted.config.load().unwrap();
    assert_eq!(restored.repositories[0].id, installed_repository_ids["b"]);
    assert_eq!(restored.repositories[0].name, "Recovered MOD");
    assert!(restored.repositories[0].cached_releases.is_empty());
    assert_eq!(
        restored.repositories[0]
            .release_prerelease_metadata
            .get("v1.0.0-beta"),
        Some(&false)
    );
    assert_eq!(restored.repositories[0].releases_cached_at, Some(123));
    assert!(restored.repositories[0].cached_releases_complete);
    server.join().unwrap();
    fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
}

#[tokio::test]
#[ignore = "requires access to the maintained official remote source"]
async fn official_remote_index_loads_each_declared_child() {
    let state = test_state();
    // 线上索引用例：默认配置只读内置快照，这里显式切回在线入口。
    // `save` 会把配置放进读取缓存，所以根地址一并换掉。
    let mut online = state.config.load().unwrap();
    crate::config::synchronize_builtin_repository_sources(&mut online);
    state.config.save(&online).unwrap();
    let config = state.config.load().unwrap();
    let root = config.repository_sources[0].clone();
    assert_eq!(root.url, config.data_provider.document("mods.toml"));
    // Simulate an explicit bundled selection in this process, then return to
    // the regional provider using the same runtime selector as Settings.
    state
        .config
        .select_data_provider(crate::models::DataProvider::Builtin);
    let bundled_root = state.config.load().unwrap().repository_sources[0].clone();
    import_repository_source_into_config(&state, bundled_root.id, bundled_root.url, false, false)
        .await
        .unwrap();
    state.config.select_data_provider(config.data_provider);
    refresh(&state, &root).await.unwrap();
    let mut pending = state
        .config
        .load()
        .unwrap()
        .repository_sources
        .into_iter()
        .filter(|s| s.parent_id.as_deref() == Some(root.id.as_str()))
        .collect::<Vec<_>>();
    assert!(!pending.is_empty());
    let mut visited = HashSet::from([root.id]);
    while let Some(source) = pending.pop() {
        assert!(visited.insert(source.id.clone()), "source cycle");
        assert!(matches!(
            SourceLocation::parse(&source.url).unwrap(),
            SourceLocation::Remote(_)
        ));
        refresh(&state, &source).await.unwrap();
        let loaded = state.config.load().unwrap();
        eprintln!(
            "source={} repositories={}",
            source.id,
            loaded
                .repositories
                .iter()
                .filter(|r| r.source_id.as_deref() == Some(source.id.as_str()))
                .count()
        );
        pending.extend(
            loaded
                .repository_sources
                .into_iter()
                .filter(|s| s.parent_id.as_deref() == Some(source.id.as_str())),
        );
    }
    let loaded = state.config.load().unwrap();
    assert!(!loaded.repositories.is_empty());
    assert!(loaded.repository_sources.iter().all(|s| s.builtin));
    eprintln!(
        "live index: sources={} repositories={}",
        loaded.repository_sources.len(),
        loaded.repositories.len()
    );
    fs::remove_dir_all(state.config.config_path().parent().unwrap()).unwrap();
}
