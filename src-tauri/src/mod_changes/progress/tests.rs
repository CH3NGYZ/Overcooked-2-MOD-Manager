use super::*;
use crate::mod_changes::{inventory::TrustedInventory, model::*, session::PlanRequest};
use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    time::Instant,
};

fn reporter() -> (
    PreparationReporter,
    Arc<Mutex<Vec<ChangePreparationProgress>>>,
) {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = seen.clone();
    let reporter = PreparationReporter::new("input-plan".into(), move |p| {
        captured.lock().unwrap().push(p)
    });
    reporter.snapshot.lock().unwrap().items = vec![PreparationItem {
        node_id: "framework".into(),
        candidate_id: "v0.10".into(),
        label: "DIYLevel".into(),
        tag: Some("v0.10".into()),
        stage: ItemStage::Queued,
        download: None,
    }];
    (reporter, seen)
}

#[test]
fn transfer_completion_keeps_bytes_until_actual_payload_verification_finishes() {
    let (reporter, seen) = reporter();
    let mut download = DownloadProgress::connecting("同名仓库");
    download.downloaded = 500;
    download.total = Some(500);
    download.finished = true;
    reporter.download("framework", "v0.10", download);
    let last = seen.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.stage, PreparationStage::Verifying);
    assert_eq!(last.items[0].stage, ItemStage::Verifying);
    assert_eq!(last.items[0].download.as_ref().unwrap().downloaded, 500);
    reporter.verified("framework", "v0.10");
    let last = seen.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.items[0].stage, ItemStage::Ready);
    assert_eq!(last.items[0].download.as_ref().unwrap().downloaded, 500);
    assert_eq!(last.plan_id, "input-plan");
}

#[test]
fn failed_item_retains_the_last_real_snapshot_and_candidate_identity_ignores_old_progress() {
    let (reporter, seen) = reporter();
    let mut download = DownloadProgress::connecting("同名仓库");
    download.downloaded = 250;
    reporter.download("framework", "v0.10", download.clone());
    download.downloaded = 499;
    reporter.download("framework", "old-candidate", download);
    reporter.failed("download.cancelled");
    let last = seen.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.items[0].download.as_ref().unwrap().downloaded, 250);
    assert_eq!(last.items[0].stage, ItemStage::Failed);
    assert_eq!(last.error.as_deref(), Some("download.cancelled"));
}

fn session(candidate_id: &str) -> PlanSession {
    let candidate = Candidate {
        id: candidate_id.into(),
        source_id: None,
        tag: Some(candidate_id.into()),
        version: None,
        provides: vec![],
        relations: vec![],
        relations_known: true,
        installable: true,
        raw: false,
        verification_pending: true,
    };
    let inventory = Inventory {
        nodes: vec![ChangeNode {
            id: "framework".into(),
            kind: NodeKind::Mod,
            current: Some(candidate),
            enabled: true,
            mutable: true,
            candidates: vec![],
            candidates_complete: true,
        }],
    };
    PlanSession {
        trusted: TrustedInventory {
            inventory: inventory.clone(),
            files: BTreeMap::new(),
            revision: "test".into(),
            game_root: PathBuf::new(),
            level_roots: crate::level_sets::LevelSetRoots::new(PathBuf::new(), None),
            config: crate::config::default_config(),
            managed_mods: Default::default(),
            toggle_unbound_nodes: Default::default(),
            installed_mods: Default::default(),
            duplicate_releases: Default::default(),
            duplicate_release_safe_nodes: Default::default(),
            declared_bundle_switch_members: Default::default(),
        },
        request: ChangeRequest::default(),
        wire_request: PlanRequest::default(),
        plan: ChangePlan {
            plan_id: "revised-plan".into(),
            roots: vec![],
            nodes: vec![],
            relation_labels: Default::default(),
            previous_enabled_members: Default::default(),
            retired_mod_copies: Default::default(),
            install_notes: vec![],
            prepared: false,
            result: SolveResult {
                status: SolveStatus::Ready,
                evaluations: vec![],
                blockers: vec![],
                impacts: vec![],
                actions: vec![ChangeAction {
                    node_id: "framework".into(),
                    member_key: None,
                    kind: ActionKind::Downgrade,
                    from_candidate_id: Some("v0.11".into()),
                    to_candidate_id: Some(candidate_id.into()),
                    from_tag: Some("v0.11".into()),
                    to_tag: Some(candidate_id.into()),
                    reason: "version".into(),
                    caused_by: vec![],
                    reason_details: vec![],
                }],
            },
        },
        solved: Some(inventory),
        payloads: HashMap::new(),
        verified_payloads: HashMap::new(),
        map_payload: None,
        staging: PathBuf::new(),
        payload_fingerprints: HashMap::new(),
        mutations: None,
        residue_adoptions: Default::default(),
        created: Instant::now(),
    }
}

#[test]
fn resolving_a_new_candidate_replaces_obsolete_downloads_and_actions() {
    let (reporter, seen) = reporter();
    let mut download = DownloadProgress::connecting("DIYLevel");
    download.downloaded = 250;
    reporter.download("framework", "v0.10", download);
    let mut session = session("v0.9");
    reporter.synchronize(&session);
    let last = seen.lock().unwrap().last().unwrap().clone();
    assert_eq!(last.items.len(), 1);
    assert_eq!(last.items[0].candidate_id, "v0.9");
    assert!(last.items[0].download.is_none());
    assert_eq!(last.items[0].stage, ItemStage::Queued);
    assert_eq!(last.actions[0].to_tag.as_deref(), Some("v0.9"));
    session.plan.result.actions.clear();
    reporter.synchronize(&session);
    let last = seen.lock().unwrap().last().unwrap().clone();
    assert!(last.items.is_empty());
    assert!(last.actions.is_empty());
}

#[test]
fn final_outcome_carries_the_authoritative_ready_or_failed_snapshot() {
    let (reporter, _) = reporter();
    let mut plan = session("v0.10").plan;
    plan.prepared = true;
    let outcome = reporter.finish(Ok(plan));
    assert!(outcome.plan.unwrap().prepared);
    assert!(outcome.error.is_none());
    assert_eq!(outcome.progress.plan_id, "input-plan");
    assert_eq!(outcome.progress.stage, PreparationStage::Ready);

    let (reporter, _) = self::reporter();
    let mut download = DownloadProgress::connecting("DIYLevel");
    download.downloaded = 250;
    reporter.download("framework", "v0.10", download);
    let outcome = reporter.finish(Err("download.cancelled".into()));
    assert!(outcome.plan.is_none());
    assert_eq!(outcome.error.as_deref(), Some("download.cancelled"));
    assert_eq!(outcome.progress.stage, PreparationStage::Failed);
    assert_eq!(outcome.progress.items[0].stage, ItemStage::Failed);
    assert_eq!(
        outcome.progress.items[0]
            .download
            .as_ref()
            .unwrap()
            .downloaded,
        250
    );
}

#[test]
fn revised_incomplete_plan_returns_without_inventing_readiness() {
    let (reporter, _) = reporter();
    let mut session = session("v0.10");
    session.plan.result.status = SolveStatus::Incomplete;
    reporter.synchronize(&session);
    let outcome = reporter.finish(Ok(session.plan));
    assert!(!outcome.plan.unwrap().prepared);
    assert!(outcome.error.is_none());
    assert_eq!(outcome.progress.stage, PreparationStage::Resolving);
    assert_eq!(outcome.progress.items[0].stage, ItemStage::Queued);
}
