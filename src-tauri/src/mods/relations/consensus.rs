use crate::{
    models::RepositoryRelation, releases::compare_release_versions, version_range::VersionRange,
};
use std::cmp::Ordering;

pub(crate) fn common_source_relations<'a>(
    sources: impl IntoIterator<Item = &'a [RepositoryRelation]>,
    bundled: &'a [RepositoryRelation],
) -> (Vec<RepositoryRelation>, bool) {
    let mut common: Option<Vec<ComparableRelation<'_>>> = None;
    let mut conflict = false;
    for source in sources {
        let declared = unique_relations(source);
        if let Some(common) = &mut common {
            let previous_count = common.len();
            common.retain(|entry| entry.contained_in(&declared));
            conflict |= common.len() != previous_count || common.len() != declared.len();
        } else {
            common = Some(declared);
        }
    }
    let mut required = unique_relations(bundled);
    for entry in common.into_iter().flatten() {
        if !entry.contained_in(&required) {
            required.push(entry);
        }
    }
    (
        required
            .into_iter()
            .map(|entry| entry.relation.clone())
            .collect(),
        conflict,
    )
}

struct ComparableRelation<'a> {
    relation: &'a RepositoryRelation,
    range: Option<VersionRange>,
}

impl ComparableRelation<'_> {
    fn contained_in(&self, entries: &[ComparableRelation<'_>]) -> bool {
        entries.iter().any(|entry| self.equivalent(entry))
    }

    fn equivalent(&self, other: &ComparableRelation<'_>) -> bool {
        let left = self.relation;
        let right = other.relation;
        left.kind == right.kind
            && left.guid.trim().eq_ignore_ascii_case(right.guid.trim())
            && left.prerelease == right.prerelease
            && left.optional == right.optional
            && (left.version.trim() == right.version.trim()
                || match (&self.range, &other.range) {
                    (Some(left), Some(right)) => clauses_in(left, right) && clauses_in(right, left),
                    _ => false,
                })
    }
}

fn unique_relations(relations: &[RepositoryRelation]) -> Vec<ComparableRelation<'_>> {
    let mut entries = Vec::new();
    for relation in relations {
        let entry = ComparableRelation {
            relation,
            range: VersionRange::parse(&relation.version).ok(),
        };
        if !entry.contained_in(&entries) {
            entries.push(entry);
        }
    }
    entries
}

fn clauses_in(left: &VersionRange, right: &VersionRange) -> bool {
    left.clauses().iter().all(|left_clause| {
        right.clauses().iter().any(|right_clause| {
            left_clause.op == right_clause.op
                && (left_clause.version == right_clause.version
                    || compare_release_versions(&left_clause.version, &right_clause.version)
                        == Some(Ordering::Equal))
        })
    })
}
