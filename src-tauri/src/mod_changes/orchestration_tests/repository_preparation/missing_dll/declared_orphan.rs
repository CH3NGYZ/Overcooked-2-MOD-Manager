use super::*;
use sha2::{Digest, Sha256};

struct DependencyRoot {
    id: &'static str,
    provider: Provider,
    owner: &'static str,
    repository: &'static str,
    version: &'static str,
    prerelease: bool,
    dll: &'static str,
    guid: &'static str,
    asset: &'static str,
    requirement: &'static str,
}

const DIYLEVEL: DependencyRoot = DependencyRoot {
    id: "f7705000-0000-4000-8000-000000000005",
    provider: Provider::Github,
    owner: "gua248",
    repository: "Overcooked2-LevelEditor",
    version: "0.11.1",
    prerelease: true,
    dll: "OC2DIYLevel/OC2DIYLevel.dll",
    guid: "dev.gua.overcooked.diylevel",
    asset: "OC2DIYLevel-v0.11.1.zip",
    requirement: ">=1.15.1",
};

const HOST_UTILITIES: DependencyRoot = DependencyRoot {
    id: "f7705000-0000-4000-8000-000000000006",
    provider: Provider::Gitee,
    owner: "ch3ngyz",
    repository: "Overcooked-2-HostUtilities-Stable-Releases",
    version: "2.0.4",
    prerelease: false,
    dll: "OC2HostUtilities/v2.0.4/HostUtilities.dll",
    guid: "com.ch3ngyz.plugin.HostUtilities",
    asset: "HostUtilities.zip",
    requirement: ">=1.3.0",
};

struct CustomerCase {
    old_id: &'static str,
    new_id: &'static str,
    old_owner: &'static str,
    old_repo: &'static str,
    new_repo: &'static str,
    old_tag: &'static str,
    old_dll_version: Option<&'static str>,
    new_version: &'static str,
    guid: &'static str,
    source_document: &'static str,
    dll: &'static str,
    asset: &'static str,
    expected_replacements: usize,
}

const SETTINGS: CustomerCase = CustomerCase {
    old_id: "f7705000-0000-4000-8000-000000000001",
    new_id: "f7705000-0000-4000-8000-000000000002",
    old_owner: "CH3NGYZ",
    old_repo: "Overcooked-2-ModSettings",
    new_repo: "Overcooked-2-ModSettings",
    old_tag: "v1.1.1",
    old_dll_version: None,
    new_version: "1.3.1",
    guid: "com.ch3ngyz.plugin.overcooked2.modsettings",
    source_document: "mods/mods-azhe-settings-manager.toml",
    dll: "OC2ModSettings.dll",
    asset: "OC2ModSettings-v1.3.1.zip",
    expected_replacements: 1,
};

const EXPERIENCE: CustomerCase = CustomerCase {
    old_id: "f7705000-0000-4000-8000-000000000003",
    new_id: "f7705000-0000-4000-8000-000000000004",
    old_owner: "CH3NGY2",
    old_repo: "Overcooked-2-DIYLevel-Sorting",
    new_repo: "Overcooked-2-DIYLevel-ExperienceOptimization",
    old_tag: "v1.17.0",
    old_dll_version: Some("1.17.0"),
    new_version: "1.17.3",
    guid: "com.ch3ngyz.plugin.diylevel.sorting",
    source_document: "mods/mods-azhe-others.toml",
    dll: "OC2DIYLevel-ExperienceOptimization-体验优化.dll",
    asset: "OC2DIYLevel-ExperienceOptimization-v1.17.3.zip",
    expected_replacements: 2,
};

fn plugin_zip(dll: &str, guid: &str, product: &str, version: &str) -> Vec<u8> {
    let mut bytes = PE.to_vec();
    bytes.extend(mods::plugin_blob(guid, product, version));
    zip(&[(dll, &bytes)])
}

fn official_replacements(case: &CustomerCase) -> Vec<ReplacedModRepository> {
    let provider = crate::models::DataProvider::Github;
    let repositories = crate::repository_sources::bundled_repository_configs();
    let target = repositories
        .iter()
        .find(|repo| {
            repo.provider == Provider::Github
                && repo.username == "CH3NGY2"
                && repo.repository == case.new_repo
        })
        .unwrap_or_else(|| {
            panic!(
                "official replacement repository is missing: {}",
                case.new_repo
            )
        });
    let declarations = crate::repository_sources::replacements::verified_declarations(
        &crate::repository_sources::SourcePackage::bundled(),
        provider,
        &provider.document(case.source_document),
        &repositories,
    );
    declarations
        .get(&crate::config::export_key(target))
        .cloned()
        .filter(|aliases| aliases.len() == case.expected_replacements)
        .unwrap_or_else(|| {
            panic!(
                "official replacement aliases are incomplete: {}",
                case.new_repo
            )
        })
}

async fn replay(
    case: CustomerCase,
    asset_zip: Vec<u8>,
    dependency_root: Option<(DependencyRoot, Option<Vec<u8>>)>,
) {
    let target = format!("BepInEx/plugins/{}", case.dll);
    let tag = format!("v{}", case.new_version);
    let root = dependency_root.map(|(root, bytes)| {
        let bytes =
            bytes.unwrap_or_else(|| plugin_zip(root.dll, root.guid, root.repository, root.version));
        (root, bytes)
    });
    let server = Server::new(
        std::iter::once(asset_zip.clone())
            .chain(root.as_ref().map(|(_, bytes)| bytes.clone()))
            .collect(),
        false,
    );
    let mut next = repository(case.new_repo, case.guid, &[case.new_version], vec![]);
    next.id = case.new_id.into();
    next.username = "CH3NGY2".into();
    next.channel = Channel::All;
    next.install_target = "BepInEx/plugins".into();
    next.asset_rules[0].value = case.asset.into();
    next.cached_releases[0].assets = vec![served(&server, 0, case.asset, &asset_zip)];
    next.source_id = Some(crate::repository_sources::DEFAULT_INDEX_ID.into());
    let binding = ModRepositoryBinding::from_release(&next, &tag).unwrap();
    let fixture = versioned_fixture(next);
    let mut config = fixture.state.config.load().unwrap();
    config.repositories[0].replaces = official_replacements(&case);
    config.repositories[0].replaces_revision = Some(fixture.state.config.data_selection().1);
    let (root_id, root_tag) = if let Some((root_fixture, root_zip)) = &root {
        let mut root = repository(
            root_fixture.repository,
            root_fixture.guid,
            &[root_fixture.version],
            vec![requires(case.guid, root_fixture.requirement)],
        );
        root.id = root_fixture.id.into();
        root.provider = root_fixture.provider.clone();
        root.username = root_fixture.owner.into();
        root.install_target = "BepInEx/plugins".into();
        root.asset_rules[0].value = root_fixture.asset.into();
        root.cached_releases[0].assets = vec![served(&server, 1, root_fixture.asset, root_zip)];
        root.cached_releases[0].prerelease = root_fixture.prerelease;
        root.cached_releases[0].api_prerelease = Some(root_fixture.prerelease);
        config.repositories.push(root);
        (root_fixture.id, format!("v{}", root_fixture.version))
    } else {
        (case.new_id, tag.clone())
    };
    fixture.state.config.save(&config).unwrap();
    assert_eq!(
        fixture.state.config.load().unwrap().repositories[0]
            .replaces
            .len(),
        case.expected_replacements
    );

    let manifest_dir = fixture.state.config.manifests_dir();
    let old_manifest_path = manifest_dir.join(format!("{}.toml", case.old_id));
    let new_manifest_path = manifest_dir.join(format!("{}.toml", case.new_id));
    let mut old = archive::new_manifest(
        case.old_id,
        case.old_tag,
        BTreeMap::new(),
        BTreeMap::new(),
        vec![target.clone()],
        vec![],
    );
    old.repository_binding = Some(ModRepositoryBinding {
        provider: Provider::Github,
        owner: case.old_owner.into(),
        repository: case.old_repo.into(),
        channel: Channel::All,
        tag: case.old_tag.into(),
    });
    if let Some(version) = case.old_dll_version {
        old.installed_dll_versions
            .insert(target.clone(), version.into());
    }
    archive::write_manifest(&manifest_dir, &old).unwrap();
    let old_toml = fs::read(&old_manifest_path).unwrap();
    let snapshots = || {
        (
            Fingerprint::capture(&fixture.game).unwrap(),
            Fingerprint::capture(&manifest_dir).unwrap(),
        )
    };
    let before = snapshots();
    let enabled = fixture.game.join(&target);
    let disabled = fixture.game.join(mods::disabled_dll_file(&target));
    let assert_unchanged = || {
        assert!(!enabled.exists() && !disabled.exists());
        assert!(!new_manifest_path.exists());
        assert_eq!(fs::read(&old_manifest_path).unwrap(), old_toml);
        assert_eq!(snapshots(), before);
    };
    assert_unchanged();

    for commit in [false, true] {
        let initial =
            planning::create(&fixture.state, request(root_id, Some(&root_tag), true)).unwrap();
        assert!(!initial.prepared);
        let outcome = preparation::prepare(fixture.state.clone(), initial.plan_id, |_| {}).await;
        assert_unchanged();
        assert_eq!(outcome.error, None);
        assert_eq!(outcome.progress.stage, PreparationStage::Ready);
        let plan = outcome.plan.unwrap();
        assert!(plan.prepared);
        assert_eq!(plan.result.status, SolveStatus::Ready);
        assert!(plan.untracked_overwrites.is_empty());
        let action = plan
            .result
            .actions
            .iter()
            .find(|action| action.node_id == format!("mod:{}", case.new_id))
            .unwrap();
        assert_eq!(action.kind, ActionKind::Install);
        assert_eq!(action.to_tag.as_deref(), Some(tag.as_str()));
        if let Some((root_fixture, _)) = &root {
            assert!(
                action
                    .reason_details
                    .iter()
                    .any(|reason| reason.kind == ReasonKind::RequiredBy
                        && reason.requirement.as_deref() == Some(root_fixture.requirement)
                        && reason.related_node_ids == [format!("mod:{}", root_fixture.id)])
            );
        }
        if commit {
            let mut session = take(&fixture, &plan);
            assert_unchanged();
            execution::commit(&fixture.state, &mut session, |_| {}).unwrap();
        } else {
            execution::cancel(&fixture.state, &plan.plan_id).unwrap();
            assert_unchanged();
        }
    }
    assert!(!old_manifest_path.exists());
    assert!(new_manifest_path.is_file() && enabled.is_file() && !disabled.exists());
    let installed = archive::read_manifests(&manifest_dir);
    assert_eq!(installed.len(), if root.is_some() { 2 } else { 1 });
    let next = installed
        .iter()
        .find(|record| record.repository_id == case.new_id)
        .unwrap();
    assert_eq!(next.repository_binding.as_ref(), Some(&binding));
    assert_eq!(next.files, [target]);
    if let Some((root_fixture, _)) = &root {
        assert!(
            fixture
                .game
                .join("BepInEx/plugins")
                .join(root_fixture.dll)
                .is_file()
        );
    }
}

#[tokio::test]
async fn customer_log_mod_settings_missing_dll_reclaims_orphan_only_on_commit() {
    let bytes = plugin_zip(
        SETTINGS.dll,
        SETTINGS.guid,
        "MOD Settings",
        SETTINGS.new_version,
    );
    replay(SETTINGS, bytes.clone(), Some((HOST_UTILITIES, None))).await;
    replay(SETTINGS, bytes, None).await;
}

#[tokio::test]
async fn customer_log_diylevel_installs_missing_experience_dependency_and_reclaims_orphan() {
    let bytes = plugin_zip(
        EXPERIENCE.dll,
        EXPERIENCE.guid,
        "Experience Optimization",
        EXPERIENCE.new_version,
    );
    replay(EXPERIENCE, bytes, Some((DIYLEVEL, None))).await;
}

#[tokio::test]
#[ignore = "requires locally downloaded OC2_MOD_SETTINGS_ZIP, OC2_EXPERIENCE_OPTIMIZATION_ZIP, OC2_HOST_UTILITIES_ZIP and OC2_DIYLEVEL_ZIP"]
async fn customer_log_real_release_zip_replays() {
    for (case, variable, sha256, root, root_variable, root_sha256) in [
        (
            SETTINGS,
            "OC2_MOD_SETTINGS_ZIP",
            "8adec7b3295ebbda3757209a7766b59a96caeaf665f195b2d89fc144c92181f6",
            HOST_UTILITIES,
            "OC2_HOST_UTILITIES_ZIP",
            "06b682346c89fc4e0ecad67c49dee912769ccc474006c574cdde8716e9861f29",
        ),
        (
            EXPERIENCE,
            "OC2_EXPERIENCE_OPTIMIZATION_ZIP",
            "b66324215c6263fd5097c79c8c71d0e9aae337e7ba1aadb0093d51dfadb774d2",
            DIYLEVEL,
            "OC2_DIYLEVEL_ZIP",
            "d76e0f4ce497bfaed93d256219c0867563c199969958729c819a25ac03268681",
        ),
    ] {
        let bytes = verified_zip(variable, sha256);
        let root_bytes = verified_zip(root_variable, root_sha256);
        replay(case, bytes, Some((root, Some(root_bytes)))).await;
    }
}

fn verified_zip(variable: &str, sha256: &str) -> Vec<u8> {
    let path = std::env::var_os(variable).unwrap_or_else(|| panic!("set {variable}"));
    let bytes = fs::read(path).unwrap_or_else(|error| panic!("read {variable}: {error}"));
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        sha256,
        "{variable}"
    );
    bytes
}
