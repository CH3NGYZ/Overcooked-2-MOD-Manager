//! A trusted MOD bundle is a switch unit. Maps retain their own DLL semantics.
use super::*;

pub(super) fn replacement_candidate(
    trusted: Option<&TrustedInventory>,
    node: &ChangeNode,
    mut candidate: Candidate,
) -> Candidate {
    let (Some(trusted), Some(previous)) = (trusted, node.current.as_ref()) else {
        return candidate;
    };
    if previous.id == candidate.id {
        return candidate;
    }
    for old in trusted.declared_bundle_members(&node.id) {
        // An omitted loose DLL stays on disk. Include its provision and own
        // constraints before the solver filters or compares Release candidates.
        // Payload verification rejects any ambiguous overlap with that member.
        if candidate
            .provides
            .iter()
            .any(|member| member.member_key == old.member_key)
        {
            continue;
        }
        candidate.provides.push(old.clone());
        candidate.relations.extend(
            previous
                .relations
                .iter()
                .filter(|relation| {
                    relation.owner_member_key.is_some()
                        && relation.owner_member_key == old.member_key
                })
                .cloned(),
        );
        candidate.relations_known &= previous.relations_known;
    }
    candidate
}

impl Search<'_, '_> {
    pub(super) fn whole_mod_package(&self, node: &ChangeNode) -> bool {
        self.trusted
            .is_some_and(|trusted| trusted.atomic_mod_package(&node.id))
    }

    pub(super) fn enable_required_members(
        &self,
        index: usize,
        node: &ChangeNode,
        candidate: &mut Candidate,
        relation: &RepositoryRelation,
    ) -> bool {
        let eligible = |member: &ProvidedPlugin| {
            !member.enabled
                && same_guid(&member.guid, &relation.guid)
                && matches(relation, member) == Some(true)
                && member
                    .member_key
                    .as_ref()
                    .is_none_or(|key| !self.fixed_members.contains(&(index, key.clone())))
        };
        if !candidate.provides.iter().any(&eligible) {
            return false;
        }
        let whole = self.whole_mod_package(node);
        for member in &mut candidate.provides {
            if whole || eligible(member) {
                member.enabled = true;
            }
        }
        true
    }
}
