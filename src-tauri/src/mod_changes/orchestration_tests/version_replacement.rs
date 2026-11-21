//! Exact, digest-verified version replacement with missing or incomplete manifests.
use super::super::{execution, mutations, progress::PreparationOutcome};
use super::*;
use crate::archive::release_download::tests::{Server, reuse_tests::served};
use std::{cell::Cell, io::Write};

#[path = "version_replacement/custody_fallback.rs"]
mod custody_fallback;
#[path = "version_replacement/provider_claims.rs"]
mod provider_claims;

const REPO: &str = "Replacement";
const NODE: &str = "mod:Replacement";
const GUID: &str = "com.example.version-replacement";
const COMPANION_GUID: &str = "com.example.replacement-companion";
const TAG: &str = "v1.0.0";
const OLD_TAG: &str = "v2.0.0";
const ASSET: &str = "Replacement.zip";
const CORE: &str = "BepInEx/plugins/Replacement/Replacement.dll";
const DISABLED_CORE: &str = "BepInEx/plugins/Replacement/Replacement.dll.disabled";
const COMPANION: &str = "BepInEx/plugins/Replacement/Companion.dll";
const DISABLED_COMPANION: &str = "BepInEx/plugins/Replacement/Companion.dll.disabled";
const RESOURCE: &str = "BepInEx/plugins/Replacement/commonW5";
const RESOURCE_TWIN: &str = "BepInEx/plugins/Replacement/commonW5.disabled";
const UNPACKAGED_COMMON: &str = "BepInEx/plugins/Replacement/commonW4";
const IDENTICAL: &str = "BepInEx/plugins/Replacement/same-data";
const UNRELATED: &str = "BepInEx/plugins/Replacement/personal.txt";
const PACKAGE_NOTES: &str = "BepInEx/plugins/Replacement/zz-package-notes";
const PE: &[u8] = include_bytes!("duplicate_release_target/fastinit_version.pe");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prohibition {
    IndependentSource,
    OtherManifest,
    MissingDigest,
    DifferentCorePath,
}

struct Replacement {
    fixture: Fixture,
    server: Server,
    new_core: Vec<u8>,
    new_companion: Vec<u8>,
}

fn replacement(
    installed_version: &str,
    partial_manifest: bool,
    disabled_package: bool,
    prohibition: Option<Prohibition>,
    extra_resources: &[(&str, &[u8], &[u8])],
) -> Replacement {
    let new_core = mods::plugin_blob(GUID, REPO, "1.0.0");
    let new_companion = mods::plugin_blob(COMPANION_GUID, "Companion", "1.0.0");
    let core_in_zip = if prohibition == Some(Prohibition::DifferentCorePath) {
        "subdirectory/Replacement.dll"
    } else {
        "Replacement.dll"
    };
    let companion = if disabled_package {
        DISABLED_COMPANION
    } else {
        COMPANION
    };
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let entries = [
        (core_in_zip, new_core.as_slice()),
        (
            Path::new(companion).file_name().unwrap().to_str().unwrap(),
            new_companion.as_slice(),
        ),
        ("commonW5", b"new asset"),
        ("same-data", b"identical contents"),
        ("zz-package-notes", b"new package note"),
    ];
    for (name, bytes) in entries.into_iter().chain(
        extra_resources
            .iter()
            .map(|(path, _, new)| (Path::new(path).file_name().unwrap().to_str().unwrap(), *new)),
    ) {
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(bytes).unwrap();
    }
    let bytes = zip.finish().unwrap().into_inner();
    let server = Server::new(vec![bytes.clone()], false);
    let mut repo = repository(REPO, GUID, &["1.0.0"], vec![]);
    repo.asset_rules[0].value = ASSET.into();
    let mut asset = served(&server, 0, ASSET, &bytes);
    if prohibition == Some(Prohibition::MissingDigest) {
        asset.digest = None;
    }
    repo.cached_releases[0].assets = vec![asset];
    let direct = PlanReference::Override(PlanOverride {
        release: Some(RouteOverride {
            route: Some(vec![PlanStep::Direct]),
            max_parallelism: Some(1),
            race: Some(false),
            download_threads: Some(1),
        }),
        ..Default::default()
    });
    repo.network_plan = NetworkPlanSelection {
        mainland: Some(direct.clone()),
        international: Some(direct),
    };
    let mut repositories = vec![repo];
    const OTHER_REPO: &str = "OtherOwner";
    const OTHER_GUID: &str = "com.example.other-owner";
    match prohibition {
        Some(Prohibition::IndependentSource) => {
            repositories.push(repository("CompanionSource", COMPANION_GUID, &[], vec![]));
        }
        Some(Prohibition::OtherManifest) => {
            repositories.push(repository(OTHER_REPO, OTHER_GUID, &[], vec![]));
        }
        _ => {}
    }
    let f = fixture(repositories);
    write(&f.game.join("BepInEx/core/BepInEx.dll"), PE);
    if partial_manifest {
        install(&f, REPO, GUID, installed_version, !disabled_package);
        write(&f.game.join(PACKAGE_NOTES), b"old package note");
        let dir = f.state.config.manifests_dir();
        let mut manifest = archive::read_manifest(&dir, REPO).unwrap();
        manifest.files.push(PACKAGE_NOTES.into());
        archive::write_manifest(&dir, &manifest).unwrap();
    } else {
        write(
            &f.game.join(if disabled_package {
                DISABLED_CORE
            } else {
                CORE
            }),
            &mods::plugin_blob(GUID, REPO, installed_version),
        );
    }
    write(
        &f.game.join(companion),
        &mods::plugin_blob(COMPANION_GUID, "Companion", installed_version),
    );
    write(&f.game.join(RESOURCE), b"old asset");
    for (path, old, _) in extra_resources {
        write(&f.game.join(path), old);
    }
    write(
        &f.game.join(UNPACKAGED_COMMON),
        b"common file of old version",
    );
    write(&f.game.join(IDENTICAL), b"identical contents");
    write(&f.game.join(UNRELATED), b"user's resource");
    if prohibition == Some(Prohibition::OtherManifest) {
        install(&f, OTHER_REPO, OTHER_GUID, "1.0.0", true);
        let dir = f.state.config.manifests_dir();
        let mut manifest = archive::read_manifest(&dir, OTHER_REPO).unwrap();
        manifest.files.push(RESOURCE.into());
        archive::write_manifest(&dir, &manifest).unwrap();
    }
    Replacement {
        fixture: f,
        server,
        new_core,
        new_companion,
    }
}

impl Replacement {
    async fn prepare(&self, kind: ActionKind) -> PreparationOutcome {
        self.prepare_request(exact_request(), kind).await
    }

    async fn prepare_request(&self, request: PlanRequest, kind: ActionKind) -> PreparationOutcome {
        let f = &self.fixture;
        let plan = planning::create(&f.state, request).unwrap();
        assert_exact_action(&plan, &kind);
        assert!(!plan.prepared && plan.untracked_overwrites.is_empty());
        let before = fingerprint(f);
        let outcome = preparation::prepare(f.state.clone(), plan.plan_id, |_| {}).await;
        assert_eq!(self.server.request_count(), 1, "{:?}", outcome.error);
        assert_eq!(
            fingerprint(f),
            before,
            "preparation must not write live files"
        );
        outcome
    }

    async fn ready(&self, kind: ActionKind) -> ChangePlan {
        let outcome = self.prepare(kind.clone()).await;
        assert_eq!(outcome.error, None);
        let plan = outcome.plan.unwrap();
        assert_exact_action(&plan, &kind);
        assert!(plan.prepared);
        plan
    }
}

fn exact_request() -> PlanRequest {
    let mut request = request(REPO, Some(TAG), true);
    request.roots[0].asset_name = Some(ASSET.into());
    request
}

fn assert_exact_action(plan: &ChangePlan, kind: &ActionKind) {
    assert_eq!(plan.result.status, SolveStatus::Ready, "{:?}", plan.result);
    assert!(plan.result.actions.iter().any(|action| {
        action.node_id == NODE && &action.kind == kind && action.to_tag.as_deref() == Some(TAG)
    }));
}

fn assert_overwrites(plan: &ChangePlan, mut expected: Vec<&str>) {
    let wire = serde_json::to_value(plan).unwrap();
    let mut disclosed = wire["untracked_overwrites"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| {
            assert_eq!(item["node_id"], NODE);
            item["path"].as_str().unwrap()
        })
        .collect::<Vec<_>>();
    disclosed.sort_unstable();
    expected.sort_unstable();
    assert_eq!(disclosed, expected);
}

fn assert_common_resources(f: &Fixture) {
    assert_eq!(fs::read(f.game.join(RESOURCE)).unwrap(), b"new asset");
    assert_eq!(
        fs::read(f.game.join(UNPACKAGED_COMMON)).unwrap(),
        b"common file of old version"
    );
    assert_eq!(
        fs::read(f.game.join(IDENTICAL)).unwrap(),
        b"identical contents"
    );
    assert_eq!(
        fs::read(f.game.join(UNRELATED)).unwrap(),
        b"user's resource"
    );
}

fn fingerprint(f: &Fixture) -> (Fingerprint, Fingerprint) {
    (
        Fingerprint::capture(&f.game).unwrap(),
        Fingerprint::capture(&f.state.config.manifests_dir()).unwrap(),
    )
}

fn no_pending(f: &Fixture) {
    assert!(
        !has_pending(
            &f.state
                .config
                .config_path()
                .with_file_name("mod-change-transactions")
        )
        .unwrap()
    );
}

#[tokio::test]
async fn exact_downgrade_discloses_only_changed_untracked_paths_and_replaces_the_bytes() {
    for partial_manifest in [false, true] {
        // With no manifest the disabled core is itself an untracked overwrite.
        // Exact version changes preserve the installed DLL's switch state.
        let test = replacement("2.0.0", partial_manifest, !partial_manifest, None, &[]);
        let f = &test.fixture;
        let plan = test.ready(ActionKind::Downgrade).await;
        let mut expected = vec![RESOURCE];
        let (core, companion, absent_core, absent_companion) = if partial_manifest {
            (CORE, COMPANION, DISABLED_CORE, DISABLED_COMPANION)
        } else {
            (DISABLED_CORE, DISABLED_COMPANION, CORE, COMPANION)
        };
        expected.push(companion);
        if !partial_manifest {
            expected.push(DISABLED_CORE);
        }
        assert_overwrites(&plan, expected);

        execution::commit(&f.state, &mut take(f, &plan), |_| {}).unwrap();
        assert_eq!(fs::read(f.game.join(core)).unwrap(), test.new_core);
        assert_eq!(
            fs::read(f.game.join(companion)).unwrap(),
            test.new_companion
        );
        assert_eq!(
            fs::read(f.game.join(PACKAGE_NOTES)).unwrap(),
            b"new package note"
        );
        assert_common_resources(f);
        assert!(!f.game.join(absent_core).exists());
        assert!(!f.game.join(absent_companion).exists());
        let manifests = archive::read_manifests(&f.state.config.manifests_dir());
        let [manifest] = manifests.as_slice() else {
            panic!("expected one committed manifest: {manifests:?}");
        };
        assert_eq!(manifest.release_tag, TAG);
        assert_eq!(manifest.repository_binding.as_ref().unwrap().tag, TAG);
        let mut files = manifest
            .files
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        files.sort_unstable();
        assert_eq!(files, [companion, core, RESOURCE, IDENTICAL, PACKAGE_NOTES]);
        no_pending(f);
    }
}

#[tokio::test]
async fn exact_reinstall_without_manifest_treats_non_dll_suffix_twins_as_distinct_resources() {
    let test = replacement(
        "1.0.0",
        false,
        false,
        None,
        &[(RESOURCE_TWIN, b"old suffixed data", b"new suffixed data")],
    );
    let f = &test.fixture;
    let plan = test.ready(ActionKind::Replace).await;
    assert_overwrites(&plan, vec![RESOURCE, RESOURCE_TWIN]);
    execution::commit(&f.state, &mut take(f, &plan), |_| {}).unwrap();
    assert_common_resources(f);
    assert_eq!(
        fs::read(f.game.join(RESOURCE_TWIN)).unwrap(),
        b"new suffixed data"
    );
    assert_eq!(fs::read(f.game.join(CORE)).unwrap(), test.new_core);
    let manifest = archive::read_manifest(&f.state.config.manifests_dir(), REPO).unwrap();
    assert_eq!(manifest.release_tag, TAG);
    for path in [RESOURCE, RESOURCE_TWIN] {
        assert_eq!(
            manifest.files.iter().filter(|file| *file == path).count(),
            1
        );
    }
    assert!(
        manifest
            .files
            .iter()
            .all(|file| file != UNPACKAGED_COMMON && file != UNRELATED)
    );
    no_pending(f);
}

#[tokio::test]
async fn approved_downgrade_rejects_an_equal_size_equal_mtime_edit() {
    let test = replacement("2.0.0", true, false, None, &[]);
    let f = &test.fixture;
    let plan = test.ready(ActionKind::Downgrade).await;
    let mut session = take(f, &plan);
    let path = f.game.join(RESOURCE);
    let before = path.metadata().unwrap();
    let external = b"user edit";
    assert_eq!(external.len() as u64, before.len());
    let previous = fingerprint(f);
    fs::write(&path, external).unwrap();
    fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(before.modified().unwrap())
        .unwrap();
    assert_eq!(
        path.metadata().unwrap().modified().unwrap(),
        before.modified().unwrap()
    );
    let now = fingerprint(f);
    assert_ne!(previous.0, now.0);
    assert_eq!(
        inventory::revision(&f.state, &f.game, &f.state.config.load_or_default()).unwrap(),
        session.trusted.revision,
        "exercise content revalidation instead of the cheap revision stamp"
    );

    assert_eq!(
        execution::commit(&f.state, &mut session, |_| {}).unwrap_err(),
        blocked("modChanges.installTargetVerificationChanged", RESOURCE)
    );
    assert_eq!(fingerprint(f), now);
    assert_eq!(fs::read(path).unwrap(), external);
    no_pending(f);
}

#[tokio::test]
async fn interrupted_downgrade_restores_changed_files_and_the_old_partial_manifest() {
    let test = replacement("2.0.0", true, true, None, &[]);
    let f = &test.fixture;
    let plan = test.ready(ActionKind::Downgrade).await;
    let session = take(f, &plan);
    let before = fingerprint(f);
    let roots = mutations::transaction_roots(&f.state, &session).unwrap();
    let journal = super::super::transaction::prepare(
        &roots.data_root.join("mod-change-transactions"),
        &roots,
        session
            .mutations
            .unwrap()
            .into_iter()
            .map(|owned| owned.mutation)
            .collect(),
    )
    .unwrap();
    let interrupted = Cell::new(false);
    let result = journal.apply_with_cancel(
        || interrupted.get(),
        |event| {
            if matches!(event, RecordProgress::Processed(_))
                && !f.game.join(CORE).exists()
                && fs::read(f.game.join(DISABLED_CORE)).is_ok_and(|bytes| bytes == test.new_core)
                && archive::read_manifest(&f.state.config.manifests_dir(), REPO)
                    .is_some_and(|manifest| manifest.release_tag == TAG)
            {
                // Leave a real content-changing record pending after the core
                // and manifest, even if identical file writes are elided.
                assert_eq!(
                    fs::read(f.game.join(PACKAGE_NOTES)).unwrap(),
                    b"old package note"
                );
                interrupted.set(true);
            }
        },
    );
    assert!(
        interrupted.get(),
        "the disabled replacement and new manifest were live"
    );
    assert_eq!(result.unwrap_err(), "download.cancelled");
    assert_eq!(fingerprint(f), before);
    assert_eq!(
        archive::read_manifest(&f.state.config.manifests_dir(), REPO)
            .unwrap()
            .release_tag,
        OLD_TAG
    );
    assert!(!f.game.join(CORE).exists());
    assert!(f.game.join(DISABLED_CORE).is_file());
    no_pending(f);
}

fn blocked(tag: &str, path: &str) -> String {
    crate::locale_tag::with_params(tag, &[("path", path)])
}

#[tokio::test]
async fn precise_downgrade_cannot_claim_separate_owners_or_bypass_core_and_digest_proof() {
    for (case, tag, paths) in [
        (
            Prohibition::IndependentSource,
            "modChanges.installTargetOwned",
            &[COMPANION][..],
        ),
        (
            Prohibition::OtherManifest,
            "modChanges.installTargetClaimed",
            &[RESOURCE][..],
        ),
        (
            Prohibition::MissingDigest,
            "modChanges.installTargetNeedsVerifiedRelease",
            &[COMPANION, RESOURCE][..],
        ),
        (
            Prohibition::DifferentCorePath,
            "modChanges.fileOwnershipConflict",
            &[COMPANION, RESOURCE][..],
        ),
    ] {
        let test = replacement("2.0.0", true, false, Some(case), &[]);
        let outcome = test.prepare(ActionKind::Downgrade).await;
        assert!(outcome.plan.is_none(), "{case:?}");
        let error = outcome
            .error
            .unwrap_or_else(|| panic!("{case:?} was accepted"));
        assert!(
            paths.iter().any(|path| blocked(tag, path) == error),
            "{case:?}: expected {tag} at {paths:?}, got {error}"
        );
        no_pending(&test.fixture);
    }
}
