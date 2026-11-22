use crate::models::{AppConfig, MapRepositoryConfig, RepositoryRelation};
use crate::releases::compare_release_versions;
use std::cmp::Ordering;

#[derive(Default)]
pub(crate) struct LocalRelations {
    pub relations: Vec<RepositoryRelation>,
    pub source_labels: Vec<String>,
    pub applicable_sources: usize,
    pub conflict: bool,
    pub uncovered_sources: Vec<String>,
}

pub(crate) fn resolve<'a>(
    sources: impl IntoIterator<Item = &'a MapRepositoryConfig>,
    info_version: &str,
    bundled: &[RepositoryRelation],
) -> LocalRelations {
    let mut resolved = LocalRelations::default();
    let mut declarations = Vec::new();
    for source in sources {
        let label = source_label(source);
        resolved.source_labels.push(label.clone());
        let version = (!info_version.trim().is_empty()).then_some(info_version);
        let applicable = crate::mods::select_release_relations(&source.release_relations, version)
            .or_else(|| {
                (!source.release_derived || records_info_version(source, info_version))
                    .then_some(source.relations.as_slice())
            });
        let Some(applicable) = applicable else {
            if !source.relations.is_empty() || !source.release_relations.is_empty() {
                resolved.uncovered_sources.push(label);
            }
            continue;
        };
        declarations.push(applicable);
    }
    resolved.source_labels.sort();
    resolved.source_labels.dedup();
    resolved.uncovered_sources.sort();
    resolved.uncovered_sources.dedup();
    resolved.applicable_sources = declarations.len();
    (resolved.relations, resolved.conflict) =
        crate::mods::common_source_relations(declarations, bundled);
    resolved
}

pub(crate) fn matching_maps<'a>(
    config: &'a AppConfig,
    name: &'a str,
    name_zh: Option<&'a str>,
    author: &'a str,
) -> impl Iterator<Item = &'a MapRepositoryConfig> {
    config
        .map_repositories
        .iter()
        .filter(move |map| map.enabled && matches_map(name, name_zh, author, map))
}

pub(super) fn matches_map(
    name: &str,
    name_zh: Option<&str>,
    author: &str,
    map: &MapRepositoryConfig,
) -> bool {
    super::level_set_identity_matches(
        name,
        name_zh,
        author,
        &map.level_set_name,
        Some(&map.level_set_name_zh),
        &map.author,
    )
}

fn records_info_version(source: &MapRepositoryConfig, version: &str) -> bool {
    let matches = |candidate: &str| {
        !candidate.trim().is_empty()
            && !version.trim().is_empty()
            && (candidate.trim().eq_ignore_ascii_case(version.trim())
                || compare_release_versions(candidate, version) == Some(Ordering::Equal))
    };
    if source.asset_versions.is_empty() {
        source.latest_info_version.as_deref().is_some_and(matches)
    } else {
        source
            .asset_versions
            .iter()
            .any(|entry| matches(&entry.info_version))
    }
}

fn source_label(source: &MapRepositoryConfig) -> String {
    if !source.source_name.trim().is_empty() {
        return source.source_name.trim().to_owned();
    }
    source
        .source_id
        .clone()
        .filter(|id| !id.trim().is_empty())
        .unwrap_or_else(|| source.id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lagging_release_defaults_are_not_evidence_for_future_zip_versions() {
        let mut lagging: MapRepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "lagging",
            "release_derived": true,
            "latest_info_version": "2.24",
            "relations": [{"kind": "requires", "guid": "fixture.old-only", "version": ">=5.0.0"}],
            "release_relations": [{
                "version": ">=2.25",
                "relations": [{"kind": "requires", "guid": "OC2.OC2DIYLevelRuntimeWLoader", "version": ">=3.8.8"}],
            }],
        })).unwrap();
        let bundled = super::super::runtime_relation("3.10.2").unwrap();
        let with_explicit_range = resolve([&lagging], "2.25", std::slice::from_ref(&bundled));
        assert_eq!(with_explicit_range.applicable_sources, 1);
        assert_eq!(with_explicit_range.relations.len(), 2);
        assert!(with_explicit_range.relations.contains(&bundled));
        lagging.release_relations.clear();
        let without_version_evidence = resolve([&lagging], "2.25", std::slice::from_ref(&bundled));
        assert_eq!(without_version_evidence.applicable_sources, 0);
        assert_eq!(without_version_evidence.uncovered_sources, ["lagging"]);
        assert_eq!(without_version_evidence.relations, vec![bundled]);
    }

    #[test]
    fn common_requirements_use_release_version_order_without_rewriting_the_source() {
        let mut first: MapRepositoryConfig = serde_json::from_value(serde_json::json!({
            "id": "first",
            "relations": [{"kind": "requires", "guid": "OC2.OC2DIYLevelRuntimeWLoader", "version": ">=v3.8.8"}],
        }))
        .unwrap();
        let mut second = first.clone();
        second.id = "second".into();
        second.relations[0].guid.make_ascii_lowercase();
        second.relations[0].version = ">=3.8.8.0".into();

        let resolved = resolve([&first, &second], "2.25", &[]);
        assert!(!resolved.conflict);
        assert_eq!(resolved.relations, first.relations);
        let bundled = super::super::runtime_relation("3.8.8").unwrap();
        let resolved = resolve([&first, &second], "2.25", std::slice::from_ref(&bundled));
        assert!(!resolved.conflict);
        assert_eq!(resolved.relations, vec![bundled]);

        first.relations[0].version = ">=v3.8.8-beta.2,<v4.0".into();
        second.relations[0].version = "<4.0.0,>=3.8.8.2,>=v3.8.8-beta2".into();
        let resolved = resolve([&first, &second], "2.25", &[]);
        assert!(!resolved.conflict);
        assert_eq!(resolved.relations, first.relations);

        second.relations[0].prerelease = Some(true);
        let resolved = resolve([&first, &second], "2.25", &[]);
        assert!(resolved.conflict);
        assert!(resolved.relations.is_empty());

        second.relations[0].prerelease = None;
        second.relations[0].version = "<=4.0.0,>=3.8.8.2".into();
        let resolved = resolve([&first, &second], "2.25", &[]);
        assert!(resolved.conflict);
        assert!(resolved.relations.is_empty());
    }
}
