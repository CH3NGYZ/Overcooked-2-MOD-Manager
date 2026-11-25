//! Replay source changes with archived public ZIPs and the bundled catalogue.
use super::*;
use crate::repository_sources::{DEFAULT_INDEX_ID, SourceLocation};
use sha2::{Digest, Sha256};
use std::io::Read;

const HOST_GUID: &str = "com.ch3ngyz.plugin.HostUtilities";
const HOST_SOURCE: &str = "repositories-azhe-hostutilities";
const SETTINGS_SOURCE: &str = "repositories-azhe-settings-manager";
const STABLE_REPOSITORY: &str = "Overcooked-2-HostUtilities-Stable-Releases";
const BETA_REPOSITORY: &str = "Overcooked-2-HostUtilities-Beta-Releases";
const SETTINGS_REPOSITORY: &str = "Overcooked-2-ModSettings";
const ASSET_NAME: &str = "HostUtilities.zip";
const STABLE_TAG: &str = "v2.0.3";
const STABLE_VERSION: &str = "2.0.3";
const STABLE_SHA256: &str = "2dfc62d7d8e5e16fc7749ce70f8a8855adec8c964742c7418c7a96136c622c43";

struct PreviousRelease {
    provider: Provider,
    repository: &'static str,
    region: &'static str,
    tag: &'static str,
    version: &'static str,
    prerelease: bool,
    zip_env: &'static str,
    sha256: &'static str,
}

#[tokio::test]
#[ignore = "set OC2_HOSTUTILITIES_GITEE_STABLE_ZIP, OC2_HOSTUTILITIES_GITHUB_BETA_ZIP and OC2_HOSTUTILITIES_GITHUB_STABLE_ZIP to the archived public ZIPs"]
async fn hostutilities_real_packages_switch_existing_sources() {
    let stable_zip = verified_zip("OC2_HOSTUTILITIES_GITHUB_STABLE_ZIP", STABLE_SHA256);
    let stable_dll = read_dll(&stable_zip, STABLE_VERSION);
    for previous in [
        PreviousRelease {
            provider: Provider::Gitee,
            repository: STABLE_REPOSITORY,
            region: "cn",
            tag: "v2.0.2",
            version: "2.0.2",
            prerelease: false,
            zip_env: "OC2_HOSTUTILITIES_GITEE_STABLE_ZIP",
            sha256: "fda2b94f6308a52f2cb174b7de23af36d78cc702806a0613bcf768de6fad5219",
        },
        PreviousRelease {
            provider: Provider::Github,
            repository: BETA_REPOSITORY,
            region: "global",
            tag: "v2.0.3-beta.1",
            version: "2.0.3.1",
            prerelease: true,
            zip_env: "OC2_HOSTUTILITIES_GITHUB_BETA_ZIP",
            sha256: "747fbc1f8caed51a13c7e66b6261f738b5e40707a427173c529f783a8c008a94",
        },
    ] {
        let old_zip = verified_zip(previous.zip_env, previous.sha256);
        replay(
            &previous,
            &read_dll(&old_zip, previous.version),
            &stable_zip,
            &stable_dll,
        )
        .await;
    }
}

fn verified_zip(env: &str, sha256: &str) -> Vec<u8> {
    let path = std::env::var_os(env).unwrap_or_else(|| panic!("set {env} to its public ZIP"));
    let bytes = fs::read(&path).unwrap_or_else(|error| panic!("read {env}={path:?}: {error}"));
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), sha256, "{env}");
    bytes
}

fn dll_path(version: &str) -> String {
    format!("BepInEx/plugins/OC2HostUtilities/v{version}/HostUtilities.dll")
}

fn read_dll(bytes: &[u8], version: &str) -> Vec<u8> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
    assert_eq!(zip.len(), 1);
    let path = dll_path(version);
    let mut dll = Vec::new();
    zip.by_name(path.strip_prefix("BepInEx/plugins/").unwrap())
        .unwrap()
        .read_to_end(&mut dll)
        .unwrap();
    dll
}

fn install_catalogue(fixture: &Fixture, config: &mut AppConfig) {
    let mut exports = Vec::new();
    for (source_id, url, names, expected) in [
        (
            HOST_SOURCE,
            "mods/mods-azhe-hostutilities.toml",
            [STABLE_REPOSITORY, BETA_REPOSITORY].as_slice(),
            4,
        ),
        (
            SETTINGS_SOURCE,
            "mods/mods-azhe-settings-manager.toml",
            [SETTINGS_REPOSITORY].as_slice(),
            1,
        ),
    ] {
        let mut repositories = SourceLocation::parse(url)
            .unwrap()
            .bundled_document()
            .unwrap()
            .repositories;
        repositories.retain(|repo| names.contains(&repo.repository.as_str()));
        assert_eq!(repositories.len(), expected);
        for repo in &mut repositories {
            repo.source_id = Some(source_id.into());
        }
        exports.extend(repositories);
        config.repository_sources.push(RepositorySource {
            id: source_id.into(),
            nickname: source_id.into(),
            nickname_i18n: BTreeMap::new(),
            note_i18n: BTreeMap::new(),
            url: url.into(),
            region: None,
            builtin: true,
            parent_id: Some(DEFAULT_INDEX_ID.into()),
            network_plan: Default::default(),
        });
    }
    let result = fixture.state.config.import_bundle(
        config,
        &ModRepositoryBundle {
            format: "oc2-mod-repository-bundle".into(),
            schema_version: 1,
            exported_at: String::new(),
            repositories: exports,
            repository_sources: vec![],
        },
        "overwrite",
    );
    assert!(result.valid);
    assert_eq!(config.repositories.len(), 5);
}

fn stable_cache(repository: &mut ModRepositoryConfig, server: &Server, zip: &[u8]) {
    repository.network_plan.international = Some(PlanReference::Override(PlanOverride {
        release: Some(RouteOverride {
            route: Some(vec![PlanStep::Direct]),
            race: Some(false),
            max_parallelism: Some(1),
            download_threads: Some(1),
        }),
        ..Default::default()
    }));
    let mut asset = served(server, 0, ASSET_NAME, zip);
    asset.download_url.push_str("?replay=1");
    repository.cached_releases = vec![ReleaseInfo {
        release_id: None,
        tag_name: STABLE_TAG.into(),
        version: Some(STABLE_VERSION.into()),
        name: STABLE_TAG.into(),
        body: String::new(),
        published_at: None,
        api_prerelease: Some(false),
        prerelease: false,
        draft: false,
        assets: vec![asset],
    }];
    repository.cached_releases_complete = true;
    repository.releases_cached_at = Some(1);
}

async fn prepare(fixture: &Fixture, selection: PlanRequest) -> PreparationOutcome {
    let plan = planning::create(&fixture.state, selection).unwrap();
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(!plan.prepared);
    preparation::prepare(fixture.state.clone(), plan.plan_id, |_| {}).await
}

async fn replay(previous: &PreviousRelease, old_dll: &[u8], stable_zip: &[u8], stable_dll: &[u8]) {
    let fixture = fixture(vec![]);
    write(&fixture.game.join("BepInEx/core/BepInEx.dll"), PE);
    let service = &fixture.state.config;
    let manifests = service.manifests_dir();
    let mut config = service.load().unwrap();
    install_catalogue(&fixture, &mut config);
    let server = Server::new(vec![stable_zip.to_vec()], false);
    let target = config
        .repositories
        .iter_mut()
        .find(|repo| repo.provider == Provider::Github && repo.repository == STABLE_REPOSITORY)
        .unwrap();
    assert_eq!(target.region.as_deref(), Some("global"));
    stable_cache(target, &server, stable_zip);
    let target_id = target.id.clone();
    let next_binding = ModRepositoryBinding::from_release(target, STABLE_TAG).unwrap();
    let old = config
        .repositories
        .iter()
        .find(|repo| repo.provider == previous.provider && repo.repository == previous.repository)
        .unwrap();
    assert_eq!(old.region.as_deref(), Some(previous.region));
    let old_id = old.id.clone();
    let previous_binding = ModRepositoryBinding::from_release(old, previous.tag).unwrap();
    let old_path = dll_path(previous.version);
    let new_path = dll_path(STABLE_VERSION);
    assert_ne!(old_path, new_path);
    let mut record = archive::new_manifest(
        &old_id,
        previous.tag,
        archive::repository_package_names(old),
        BTreeMap::new(),
        vec![old_path.clone()],
        mods::effective_relations(old, Some(previous.tag)),
    );
    record.repository_binding = Some(previous_binding.clone());
    record.prerelease = previous.prerelease;
    record
        .installed_dll_versions
        .insert(mods::manifest_dll_key(&old_path), previous.version.into());
    let settings = config
        .repositories
        .iter()
        .find(|repo| repo.repository == SETTINGS_REPOSITORY)
        .unwrap();
    let (settings_id, settings_guid) = (settings.id.clone(), settings.plugin_guid.clone());
    let settings_path = format!("BepInEx/plugins/{settings_id}/{settings_id}.dll");
    service
        .save_with_selection(&mut config, Some(previous.region), None)
        .unwrap();
    assert!(crate::config::mod_repository_is_available(
        &config,
        config
            .repositories
            .iter()
            .find(|repo| repo.id == old_id)
            .unwrap(),
        previous.region,
    ));
    assert_eq!(
        install(&fixture, &settings_id, &settings_guid, "1.2.1", true),
        settings_path
    );
    write(&fixture.game.join(&old_path), old_dll);
    archive::write_manifest(&manifests, &record).unwrap();
    assert_installed(
        &fixture,
        &old_id,
        &old_path,
        previous.version,
        &previous_binding,
    );
    service
        .save_with_selection(&mut config, Some("global"), None)
        .unwrap();
    let before = || {
        (
            Fingerprint::capture(&fixture.game).unwrap(),
            Fingerprint::capture(&manifests).unwrap(),
        )
    };
    let original = before();
    let settings_bytes = fs::read(fixture.game.join(&settings_path)).unwrap();
    let settings_record =
        Fingerprint::capture(&manifests.join(format!("{settings_id}.toml"))).unwrap();
    let mut selection = request(&target_id, Some(STABLE_TAG), true);
    if previous.prerelease {
        selection.roots[0].asset_name = Some(ASSET_NAME.into());
    }
    let blocked = prepare(&fixture, selection.clone()).await;
    assert_eq!(blocked.progress.stage, PreparationStage::Failed);
    assert!(blocked.plan.is_none());
    let offer = blocked.manifest_overwrite.unwrap_or_else(|| {
        panic!(
            "{}: expected a switch offer, got {:?}",
            previous.tag, blocked.error
        )
    });
    assert_eq!(offer.repository_id, target_id);
    assert!(!offer.fingerprint.is_empty());
    assert!(
        offer
            .manifest_path
            .replace('\\', "/")
            .ends_with(&format!("manifests/{old_id}.toml"))
    );
    assert_eq!(offer.recorded_paths, [old_path.clone()]);
    let switch = offer.source_switch.unwrap();
    assert_eq!(switch.previous_source, previous_binding);
    assert_eq!(switch.next_source, next_binding);
    assert_eq!(switch.installed_path.as_deref(), Some(old_path.as_str()));
    assert_eq!(switch.target_path, new_path);
    assert_eq!(before(), original);
    assert_eq!(server.request_count(), 1);

    selection.manifest_overwrite = Some(ManifestOverwriteIntent {
        repository_id: target_id.clone(),
        fingerprint: offer.fingerprint,
    });
    let ready = prepared_plan(prepare(&fixture, selection.clone()).await, STABLE_TAG);
    assert!(ready.untracked_overwrites.is_empty());
    assert_eq!(before(), original);
    execution::cancel(&fixture.state, &ready.plan_id).unwrap();
    assert_eq!(before(), original);

    let ready = prepared_plan(prepare(&fixture, selection).await, STABLE_TAG);
    assert_eq!(before(), original);
    execution::commit(&fixture.state, &mut take(&fixture, &ready), |_| {}).unwrap();
    assert_eq!(fs::read(fixture.game.join(&new_path)).unwrap(), stable_dll);
    assert!(!fixture.game.join(&old_path).exists());
    assert!(
        !fixture
            .game
            .join(mods::disabled_dll_file(&old_path))
            .exists()
    );
    assert!(
        !fixture
            .game
            .join(mods::disabled_dll_file(&new_path))
            .exists()
    );
    assert!(!manifests.join(format!("{old_id}.toml")).exists());
    let records = archive::read_manifests(&manifests);
    assert_eq!(records.len(), 2);
    let replacement = records
        .iter()
        .find(|record| record.repository_id == target_id)
        .unwrap();
    assert_eq!(replacement.files, [new_path.clone()]);
    assert_eq!(replacement.repository_binding.as_ref(), Some(&next_binding));
    assert_eq!(
        fs::read(fixture.game.join(settings_path)).unwrap(),
        settings_bytes
    );
    assert_eq!(
        Fingerprint::capture(&manifests.join(format!("{settings_id}.toml"))).unwrap(),
        settings_record
    );
    assert!(!has_pending(&service.mod_change_transactions_dir()).unwrap());
    assert_installed(
        &fixture,
        &target_id,
        &new_path,
        STABLE_VERSION,
        &next_binding,
    );
    println!(
        "{} {:?} -> {STABLE_TAG} GitHub: verified switch {old_path} -> {new_path}",
        previous.tag, previous.provider
    );
}

fn assert_installed(
    fixture: &Fixture,
    repository: &str,
    path: &str,
    version: &str,
    binding: &ModRepositoryBinding,
) {
    let inventory = inventory::build(&fixture.state).unwrap();
    let actual = inventory
        .installed_mods
        .iter()
        .filter(|item| item.guid.as_deref() == Some(HOST_GUID))
        .collect::<Vec<_>>();
    let [installed] = actual.as_slice() else {
        panic!("expected exactly one HostUtilities GUID, found {actual:?}");
    };
    assert!(installed.enabled);
    assert_eq!(installed.dll_file, path);
    assert_eq!(installed.version.as_deref(), Some(version));
    assert_eq!(installed.package_id.as_deref(), Some(repository));
    assert_eq!(installed.repository_id.as_deref(), Some(repository));
    assert_eq!(installed.repository_binding.as_ref(), Some(binding));
}
