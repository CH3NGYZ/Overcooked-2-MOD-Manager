//! Order live writes after the complete verified graph has become physical records.
use super::*;

pub(super) fn for_apply(
    session: &PlanSession,
    mutations: Vec<OwnedMutation>,
) -> Vec<OwnedMutation> {
    let mut requested = BTreeMap::<&str, BTreeSet<Option<&str>>>::new();
    for root in &session.request.roots {
        if session
            .map_payload
            .as_ref()
            .is_some_and(|batch| batch.has_external_plugin(&root.node_id))
        {
            // The import pins its external DLLs during solving, but they're
            // parts of the payload to install before publishing the maps.
            continue;
        }
        requested
            .entry(&root.node_id)
            .or_default()
            .insert(if root.candidate_id.is_some() {
                None
            } else {
                root.member_key.as_deref()
            });
    }
    let mut prerequisites = Vec::new();
    let mut metadata = Vec::new();
    let mut targets = Vec::new();
    for owned in mutations {
        let retires_copy = matches!(&owned.mutation, Mutation::File(change) if change.source.is_none())
            && owned.items.iter().any(|item| {
                session
                    .plan
                    .retired_mod_copies
                    .get(&item.node_id)
                    .is_some_and(|copies| {
                        copies.iter().any(|copy| {
                            Some(copy.member_key.as_str()) == item.member_key.as_deref()
                        })
                    })
            });
        if retires_copy {
            // This is a separately confirmed obsolete DLL/manifest, not the
            // requested package. Same-path replacement still belongs to that package.
            prerequisites.push(owned);
            continue;
        }
        if owned.items.iter().any(|item| {
            requested.get(item.node_id.as_str()).is_some_and(|keys| {
                keys.contains(&None)
                    || keys.contains(&item.member_key.as_deref())
                    // A tree edit can move the map gate and its internal DLLs
                    // together. Individual sibling DLL renames remain independent.
                    || matches!(owned.mutation, Mutation::Tree(_))
                    || (item.member_key.is_none()
                        && !owned.items.iter().any(|sibling| {
                            sibling.node_id == item.node_id && sibling.member_key.is_some()
                        }))
            })
        }) {
            targets.push(owned);
        } else if owned.items.is_empty()
            && matches!(&owned.mutation, Mutation::File(change) if change.target.root == RootKind::Data)
        {
            // Register the final map paths/relations after prerequisites and
            // before publishing a target map. This also keeps bookkeeping
            // failures from reaching the target's live paths.
            metadata.push(owned);
        } else {
            prerequisites.push(owned);
        }
    }
    // Stable within each group: preserve conflict/retirement order, preserve
    // composed old/new tree order, and never split an atomic replacement.
    prerequisites.extend(metadata);
    prerequisites.extend(targets);
    prerequisites
}
