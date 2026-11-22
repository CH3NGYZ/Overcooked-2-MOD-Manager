use super::*;
use std::cmp::Ordering;

#[derive(Default)]
pub(crate) struct ManualDeclarations {
    pub relations: Vec<OwnedRelation>,
    pub notices: Vec<String>,
}

pub(crate) fn for_members(config: &AppConfig, members: &[ProvidedPlugin]) -> ManualDeclarations {
    let mut result = ManualDeclarations::default();
    for member in members
        .iter()
        .filter(|member| !member.guid.trim().is_empty())
    {
        let mut sources = config
            .repositories
            .iter()
            .filter(|repo| {
                crate::config::mod_repository_is_available(config, repo, &config.region)
                    && repo
                        .plugin_guid
                        .trim()
                        .eq_ignore_ascii_case(member.guid.trim())
                    && mods::repository_matches_mod(
                        repo,
                        Some(&member.guid),
                        None,
                        None,
                        member.version.as_deref(),
                    )
            })
            .filter_map(|repo| {
                let version = member.version.as_deref();
                let release = version.and_then(|version| {
                    repo.cached_releases
                        .iter()
                        .find(|release| !release.draft && same_release_version(release, version))
                });
                if !channel_applies(repo, release, version) {
                    return None;
                }
                let explicit = mods::select_release_relations(&repo.release_relations, version);
                let version_matched = release.is_some() || explicit.is_some();
                Some((version_matched, repo, explicit.unwrap_or(&repo.relations)))
            })
            .collect::<Vec<_>>();
        if sources
            .iter()
            .any(|(version_matched, _, _)| *version_matched)
        {
            sources.retain(|(version_matched, _, _)| *version_matched);
        }
        sources.sort_by(|(_, left, _), (_, right, _)| left.id.cmp(&right.id));
        let (relations, conflict) =
            mods::common_source_relations(sources.iter().map(|(_, _, relations)| *relations), &[]);
        if conflict {
            let labels = sources
                .iter()
                .map(|(_, repo, _)| {
                    format!(
                        "{} ({:?} {}/{})",
                        repo.name, repo.provider, repo.username, repo.repository
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            result.notices.push(crate::locale_tag::with_params(
                "modChanges.localModSourceConflict",
                &[
                    ("guid", &member.guid),
                    ("path", member.member_key.as_deref().unwrap_or(&member.guid)),
                    ("sources", &labels),
                ],
            ));
        }
        result
            .relations
            .extend(relations.into_iter().map(|relation| OwnedRelation {
                owner_member_key: member.member_key.clone(),
                owner_guid: Some(member.guid.clone()),
                relation,
            }));
    }
    result
}

fn channel_applies(
    repo: &ModRepositoryConfig,
    release: Option<&ReleaseInfo>,
    version: Option<&str>,
) -> bool {
    if repo.download_mode == ModDownloadMode::RepositoryFile || repo.channel == Channel::All {
        return true;
    }
    let verdict = release
        .map(|release| {
            releases::resolve_prerelease(
                release.api_prerelease,
                release.prerelease,
                Some(&release.tag_name),
            )
        })
        .or_else(|| {
            let version = version?;
            let mut recorded = repo
                .release_prerelease_metadata
                .iter()
                .filter_map(|(tag, beta)| {
                    (releases::compare_release_versions(tag, version) == Some(Ordering::Equal))
                        .then_some(*beta)
                });
            let first = recorded.next()?;
            recorded.all(|beta| beta == first).then_some(first)
        })
        .or_else(|| {
            version
                .filter(|version| releases::compare_release_versions(version, version).is_some())
                .map(releases::is_prerelease_version)
        });
    verdict.is_none_or(|beta| beta == (repo.channel == Channel::Beta))
}

fn same_release_version(release: &ReleaseInfo, version: &str) -> bool {
    let matches = |tag: &str| {
        tag.trim().eq_ignore_ascii_case(version.trim())
            || releases::compare_release_versions(tag, version) == Some(Ordering::Equal)
    };
    matches(&release.tag_name)
        || (releases::compare_release_versions(&release.tag_name, &release.tag_name).is_none()
            && release.version.as_deref().is_some_and(matches))
}
