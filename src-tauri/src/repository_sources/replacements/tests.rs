use super::super::apply::import_repository_source_into_config;
use super::*;
use crate::app_state::AppState;
use crate::config::ConfigService;
use crate::models::{DataProvider, ModRepositoryBinding, Provider};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor, Write};

const TARGET: &str = r#"
[[repositories]]
name_i18n.zh-CN = "Skip Loading"
group_i18n.zh-CN = "Tools"
note_i18n.zh-CN = ""
provider = "github"
username = "NewPublisher"
repository = "SkipLoading"
channel = "stable"
plugin_guid = "com.example.skiploading"
enabled = true
install_target = "BepInEx/plugins"
asset_rules = [{type = "glob", value = "*.zip"}]
replaces = [{provider = "gitee", username = "OldPublisher", repository = "SkipLoading", channel = "stable"}]
"#;

const ROOT: &str = r#"
[[repository_sources]]
id = "child"
nickname = "Child"
url = "./child.toml"
"#;

fn old_binding() -> ModRepositoryBinding {
    ModRepositoryBinding {
        provider: Provider::Gitee,
        owner: "oldpublisher".into(),
        repository: "skiploading".into(),
        channel: Channel::Stable,
        tag: "v1.0.0".into(),
    }
}

fn fixture_zip(root: &str, extra: &[(&str, &str)]) -> Vec<u8> {
    let package = super::super::SourcePackage::bundled();
    let index = package.read("proxies.toml").unwrap();
    let mut documents = BTreeMap::from([("mods.toml".into(), root), ("child.toml".into(), TARGET)]);
    documents.extend(extra.iter().map(|(path, body)| (path.to_string(), *body)));
    for path in ["proxies.toml", "network-plans.toml", "maps.toml"]
        .map(str::to_owned)
        .into_iter()
        .chain(crate::network_policy::proxy_documents(index).unwrap())
    {
        let text = package.read(&path).unwrap();
        documents.insert(path, text);
    }
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (path, text) in documents {
        zip.start_file(path, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn serve(body: Vec<u8>, mime: &'static str) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut socket = loop {
            match listener.accept() {
                Ok((socket, _)) => break socket,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(std::time::Instant::now() < deadline);
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        socket.set_nonblocking(false).unwrap();
        socket
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let mut reader = BufReader::new(&mut socket);
        loop {
            let mut line = String::new();
            assert_ne!(reader.read_line(&mut line).unwrap(), 0);
            if line == "\r\n" {
                break;
            }
        }
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
        socket.write_all(&body).unwrap();
    });
    (format!("http://{address}/fixture"), worker)
}

async fn apply(state: &AppState, source: &RepositorySource) {
    let result = import_repository_source_into_config(
        state,
        source.id.clone(),
        source.url.clone(),
        false,
        false,
    )
    .await
    .unwrap();
    assert!(result.access_error.is_none(), "{:?}", result.access_error);
    assert!(result.preview.unwrap().valid);
}

#[tokio::test]
async fn only_applied_official_package_documents_can_authorize_replacements() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::with_config(ConfigService::for_test(temp.path().join("data")));
    let config = state.config.load().unwrap();
    let (address, worker) = serve(fixture_zip(ROOT, &[]), "application/zip");
    state.config.select_data_provider(DataProvider::Github);
    state
        .config
        .set_official_address_for_test(DataProvider::Github, address);
    let root = config
        .repository_sources
        .iter()
        .find(|source| source.id == DEFAULT_INDEX_ID)
        .unwrap()
        .clone();
    apply(&state, &root).await;
    worker.join().unwrap();

    let mut config = state.config.load().unwrap();
    let mut fake_child = root.clone();
    fake_child.id = "forged-child".into();
    fake_child.url = DataProvider::Github.document("child.toml");
    fake_child.region = Some(config.region.clone());
    fake_child.parent_id = Some(root.id.clone());
    config.repository_sources.push(fake_child.clone());
    state.config.save(&config).unwrap();

    apply(&state, &fake_child).await;
    assert!(
        state.config.load().unwrap().repositories[0]
            .replaces
            .is_empty()
    );
    let real_child = state
        .config
        .load()
        .unwrap()
        .repository_sources
        .into_iter()
        .find(|source| source.id == "child")
        .unwrap();
    apply(&state, &real_child).await;
    let config = state.config.load().unwrap();
    let target = &config.repositories[0];
    assert!(target.replaces[0].matches_binding(&old_binding()));
    assert!(!target.replaces[0].matches_binding(&ModRepositoryBinding {
        channel: Channel::Beta,
        ..old_binding()
    }));
    assert!(
        !std::fs::read_to_string(state.config.config_path())
            .unwrap()
            .contains("replaces")
    );
    assert!(
        !state
            .config
            .export_bundle(&config, None)
            .unwrap()
            .contains("replaces")
    );

    state.config.select_data_provider(DataProvider::Builtin);
    assert!(
        state.config.load().unwrap().repositories[0]
            .replaces
            .is_empty()
    );
}

#[tokio::test]
async fn external_source_cannot_authorize_replacements_even_under_official_root() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::with_config(ConfigService::for_test(temp.path().join("data")));
    let mut config = state.config.load().unwrap();
    let mut source = config
        .repository_sources
        .iter()
        .find(|source| source.id == DEFAULT_INDEX_ID)
        .unwrap()
        .clone();
    let (address, worker) = serve(TARGET.as_bytes().to_vec(), "text/plain");
    source.parent_id = Some(source.id.clone());
    source.id = "custom".into();
    source.url = address;
    config.repository_sources.push(source.clone());
    state.config.save(&config).unwrap();
    apply(&state, &source).await;
    worker.join().unwrap();
    assert!(
        state.config.load().unwrap().repositories[0]
            .replaces
            .is_empty()
    );
}

#[tokio::test]
async fn replacement_requires_complete_package_tree_even_when_other_documents_are_not_applied() {
    let root_document = format!(
        "{ROOT}\n[[repository_sources]]\nid = 'other'\nnickname = 'Other'\nurl = './other.toml'\nregion = 'cn'\n"
    );
    let old = original_release_document();
    for other in [Some(old.as_str()), None] {
        let temp = tempfile::tempdir().unwrap();
        let state = AppState::with_config(ConfigService::for_test(temp.path().join("data")));
        let docs = other
            .into_iter()
            .map(|body| ("other.toml", body))
            .collect::<Vec<_>>();
        let (address, worker) = serve(fixture_zip(&root_document, &docs), "application/zip");
        state
            .config
            .set_official_address_for_test(DataProvider::Github, address);
        let root = state
            .config
            .load()
            .unwrap()
            .repository_sources
            .into_iter()
            .find(|source| source.id == DEFAULT_INDEX_ID)
            .unwrap();
        apply(&state, &root).await;
        worker.join().unwrap();
        let child = state
            .config
            .load()
            .unwrap()
            .repository_sources
            .into_iter()
            .find(|source| source.id == "child")
            .unwrap();
        apply(&state, &child).await;
        let config = state.config.load().unwrap();
        assert_eq!(config.repositories.len(), 1);
        assert!(config.repositories[0].replaces.is_empty());
    }
}

#[tokio::test]
async fn same_name_catalogue_import_cannot_hide_another_configured_original() {
    let temp = tempfile::tempdir().unwrap();
    let state = AppState::with_config(ConfigService::for_test(temp.path().join("data")));
    let (address, worker) = serve(fixture_zip(ROOT, &[]), "application/zip");
    state
        .config
        .set_official_address_for_test(DataProvider::Github, address);
    let root = state
        .config
        .load()
        .unwrap()
        .repository_sources
        .into_iter()
        .find(|source| source.id == DEFAULT_INDEX_ID)
        .unwrap();
    apply(&state, &root).await;
    worker.join().unwrap();

    let original = original_release_document().replace("channel = \"all\"", "channel = \"stable\"");
    let bundle = state
        .config
        .validate_bundle(&format!(
            "format = 'oc2-mod-repository-bundle'\nschema_version = 1\nexported_at = ''\n{original}"
        ))
        .unwrap();
    let mut config = state.config.load().unwrap();
    state
        .config
        .import_bundle(&mut config, &bundle, "overwrite");
    assert_eq!(config.repositories[0].provider, Provider::Gitee);
    state.config.save(&config).unwrap();

    let child = config
        .repository_sources
        .into_iter()
        .find(|source| source.id == "child")
        .unwrap();
    apply(&state, &child).await;
    let config = state.config.load().unwrap();
    assert_eq!(config.repositories.len(), 1);
    assert_eq!(config.repositories[0].provider, Provider::Github);
    assert!(config.repositories[0].replaces.is_empty());
}

fn original_release_document() -> String {
    TARGET
        .split_once("replaces =")
        .unwrap()
        .0
        .replace("github", "gitee")
        .replace("NewPublisher", "OLDPUBLISHER")
        .replace("stable", "all")
        .replace("enabled = true", "enabled = false")
}

#[test]
fn bundled_official_catalogue_verifies_every_current_repository_replacement() {
    let provider = DataProvider::Github;
    let package = SourcePackage::bundled();
    let repositories = crate::repository_sources::bundled_repository_configs();
    let expected = [
        (
            "Overcooked-2-ChefRotation",
            Provider::Gitee,
            "ch3ngyz",
            "Overcooked-2-ChefRotation-Releases",
        ),
        (
            "Overcooked-2-SkipLoading",
            Provider::Gitee,
            "ch3ngyz",
            "Overcooked-2-SkipLoading",
        ),
        (
            "Overcooked-2-ModSettings",
            Provider::Github,
            "CH3NGYZ",
            "Overcooked-2-ModSettings",
        ),
        (
            "Overcooked-2-DIYLevel-ExperienceOptimization",
            Provider::Github,
            "CH3NGY2",
            "Overcooked-2-DIYLevel-Sorting",
        ),
        (
            "Overcooked-2-DIYLevel-ExperienceOptimization",
            Provider::Github,
            "CH3NGYZ",
            "Overcooked-2-DIYLevelSorting",
        ),
    ];
    let verified = verified_declarations(
        &package,
        provider,
        &provider.document("mods/mods-azhe-others.toml"),
        &repositories,
    );

    for (target_name, old_provider, old_owner, old_repository) in expected {
        let target = repositories
            .iter()
            .find(|repo| repo.repository == target_name && repo.username == "CH3NGY2")
            .unwrap_or_else(|| panic!("official replacement repository is missing: {target_name}"));
        let old = ReplacedModRepository {
            provider: old_provider,
            username: old_owner.into(),
            repository: old_repository.into(),
            channel: Channel::All,
        };

        assert_eq!(target.channel, Channel::All, "{target_name}");
        assert!(target.replaces.contains(&old), "{target_name}: {old:?}");
        assert!(
            repositories.iter().all(|repository| !old.matches_identity(
                &repository.provider,
                &repository.username,
                &repository.repository,
            )),
            "the replaced repository is still in the official catalogue: {old:?}"
        );
        assert!(
            verified
                .get(&crate::config::export_key(target))
                .is_some_and(|aliases| aliases.contains(&old)),
            "the official package did not authorize {target_name}: {old:?}"
        );
    }

    let experience = repositories
        .iter()
        .find(|repo| repo.repository == "Overcooked-2-DIYLevel-ExperienceOptimization")
        .unwrap();
    assert_eq!(experience.replaces.len(), 2);
}

#[test]
fn old_release_still_listed_and_duplicate_claims_do_not_authorize_continuation() {
    let source = SourceLocation::parse("mods.toml").unwrap();
    let parse = |text: &str| {
        super::super::parse_document(text, &source, &Default::default())
            .unwrap()
            .repositories
    };
    let duplicate_target = TARGET.split_once("replaces =").unwrap().0.to_owned();
    for extra in [
        original_release_document(),
        TARGET.replace("NewPublisher", "OtherPublisher"),
        duplicate_target,
    ] {
        let repositories = parse(&format!("{TARGET}\n{extra}"));
        assert!(
            declarations(&repositories, &repositories)
                .values()
                .all(Vec::is_empty)
        );
    }
}

#[test]
fn malformed_replacement_declarations_are_rejected() {
    let source = SourceLocation::parse("mods.toml").unwrap();
    for text in [
        TARGET.replace(", channel = \"stable\"}", "}"),
        TARGET.replace(", channel = \"stable\"}", ", channel = \"beta\"}"),
        TARGET.replace("OldPublisher", "../../OldPublisher"),
        TARGET
            .replace("\"gitee\"", "\"github\"")
            .replace("OldPublisher", "newpublisher"),
        TARGET.replace("com.example.skiploading", ""),
    ] {
        assert!(
            super::super::parse_document(&text, &source, &Default::default()).is_err(),
            "{text}"
        );
    }
}
