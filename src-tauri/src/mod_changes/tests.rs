use super::{
    evaluate::evaluate,
    inventory_maps,
    model::*,
    solve::{solve, solve_with_inventory},
};
use crate::models::{RelationKind, RepositoryRelation};

fn relation(kind: RelationKind, guid: &str, version: &str) -> OwnedRelation {
    OwnedRelation {
        owner_member_key: None,
        owner_guid: None,
        relation: RepositoryRelation {
            kind,
            guid: guid.into(),
            version: version.into(),
            prerelease: None,
            optional: false,
        },
    }
}

fn candidate(guid: &str, version: &str, relations: Vec<OwnedRelation>) -> Candidate {
    Candidate {
        id: format!("{guid}@{version}"),
        source_id: Some(guid.into()),
        tag: Some(format!("v{version}")),
        version: Some(version.into()),
        provides: vec![ProvidedPlugin {
            member_key: Some(format!("{guid}.dll")),
            guid: guid.into(),
            version: Some(version.into()),
            prerelease: None,
            enabled: true,
        }],
        relations,
        relations_known: true,
        installable: true,
        raw: false,
        verification_pending: false,
    }
}

fn node(guid: &str, current: Option<Candidate>, candidates: Vec<Candidate>) -> ChangeNode {
    ChangeNode {
        id: guid.into(),
        kind: NodeKind::Mod,
        enabled: current.is_some(),
        current,
        mutable: true,
        candidates,
        candidates_complete: true,
    }
}

fn intent(node_id: &str, candidate_id: Option<String>, enabled: bool) -> ChangeRequest {
    ChangeRequest {
        roots: vec![RootIntent {
            node_id: node_id.into(),
            candidate_id,
            enabled,
            member_key: None,
        }],
    }
}

fn action_for<'a>(result: &'a SolveResult, id: &str, kind: ActionKind) -> &'a ChangeAction {
    result
        .actions
        .iter()
        .find(|action| action.node_id == id && action.kind == kind)
        .unwrap_or_else(|| panic!("missing {id} {kind:?}: {result:?}"))
}

mod constraints;
mod diagnostics;
mod evaluation;
mod maps;
mod members;
mod solver;
