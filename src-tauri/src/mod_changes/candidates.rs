//! Only cached, available, bound-channel assets become automatic candidates.
use super::model::*;
use crate::{models::*, releases};

pub(crate) fn for_repository(
    repo: &ModRepositoryConfig,
    current: Option<&Candidate>,
    explicit_root: bool,
    config: &AppConfig,
) -> Vec<Candidate> {
    collect(repo, current, explicit_root, config, None)
}

/// A chosen UI tag/asset is authoritative within the currently available bound repository.
/// It may cross its default pin/channel, but never availability, draft or asset existence.
pub(crate) fn for_root_asset(
    repo: &ModRepositoryConfig,
    current: Option<&Candidate>,
    config: &AppConfig,
    asset_name: Option<&str>,
) -> Vec<Candidate> {
    collect(repo, current, true, config, asset_name)
}

fn collect(
    repo: &ModRepositoryConfig,
    current: Option<&Candidate>,
    explicit_root: bool,
    config: &AppConfig,
    asset_name: Option<&str>,
) -> Vec<Candidate> {
    if !crate::config::mod_repository_is_available(config, repo, &config.region) {
        return vec![];
    }
    if repo.download_mode == ModDownloadMode::RepositoryFile {
        return vec![Candidate {
            id: format!("raw:{}", repo.id),
            source_id: Some(repo.id.clone()),
            tag: None,
            version: None,
            provides: (!repo.plugin_guid.is_empty())
                .then(|| ProvidedPlugin {
                    member_key: None,
                    guid: repo.plugin_guid.clone(),
                    version: None,
                    prerelease: None,
                    enabled: true,
                })
                .into_iter()
                .collect(),
            relations: super::inventory::owned(
                repo.relations.clone(),
                (!repo.plugin_guid.is_empty()).then(|| repo.plugin_guid.clone()),
            ),
            relations_known: true,
            installable: !repo.file_path.is_empty(),
            raw: true,
            verification_pending: true,
        }];
    }
    let pin = if !explicit_root && repo.pinned_version.is_some() {
        releases::select_cached_release(&repo.cached_releases, None, repo.pinned_version.as_deref())
            .map(|release| release.tag_name.as_str())
    } else {
        None
    };
    if !explicit_root && repo.pinned_version.is_some() && pin.is_none() {
        return vec![];
    }
    repo.cached_releases
        .iter()
        .filter_map(|release| {
            if release.draft {
                return None;
            }
            let prerelease = releases::resolve_prerelease(
                release.api_prerelease,
                release.prerelease,
                Some(&release.tag_name),
            );
            if !explicit_root
                && ((repo.channel == Channel::Stable && prerelease)
                    || (repo.channel == Channel::Beta && !prerelease))
            {
                return None;
            }
            if pin.is_some_and(|tag| tag != release.tag_name) {
                return None;
            }
            let asset = releases::select_install_asset(
                &releases::RepositoryRequest::from(repo),
                &repo.asset_rules,
                release,
                asset_name,
            )?;
            if asset.download_url.trim().is_empty() {
                return None;
            }
            // The declared primary provision is provisional. Downloaded DLL metadata
            // replaces it before a journal can be prepared, including package siblings.
            let guid = (!repo.plugin_guid.trim().is_empty())
                .then(|| repo.plugin_guid.clone())
                .or_else(|| {
                    current?
                        .provides
                        .iter()
                        .find(|p| !p.guid.is_empty())
                        .map(|p| p.guid.clone())
                });
            let version = Some(release.tag_name.clone());
            Some(Candidate {
                id: format!("release:{}:{}", repo.id, release.tag_name),
                source_id: Some(repo.id.clone()),
                tag: Some(release.tag_name.clone()),
                version: version.clone(),
                provides: guid
                    .iter()
                    .map(|guid| ProvidedPlugin {
                        member_key: None,
                        guid: guid.clone(),
                        version: version.clone(),
                        prerelease: Some(prerelease),
                        enabled: true,
                    })
                    .collect(),
                relations: super::inventory::owned(
                    crate::mods::effective_relations(repo, Some(&release.tag_name)),
                    guid,
                ),
                relations_known: true,
                installable: true,
                raw: false,
                verification_pending: true,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_uses_one_exact_existing_selector_and_explicit_roots_retain_tags() {
        let mut repo = crate::community::repository_from_coordinates(
            &Provider::Github,
            "Owner",
            "Mod",
            "",
            "",
        )
        .unwrap();
        repo.source_id = None;
        repo.plugin_guid = "test-mod".into();
        repo.channel = Channel::All;
        repo.pinned_version = Some("1.8.7".into());
        repo.cached_releases = releases::parse_release_json(&serde_json::json!([
            {"tag_name":"v1.8.7-beta.12","prerelease":true,"assets":[{"name":"mod.zip","browser_download_url":"https://example.invalid/12.zip"}]},
            {"tag_name":"v1.8.7-beta.2","prerelease":true,"assets":[{"name":"mod.zip","browser_download_url":"https://example.invalid/2.zip"}]}
        ]), &Provider::Github).unwrap();
        for release in &mut repo.cached_releases {
            release.version = Some("1.8.7".into());
        }
        let mut config = crate::config::default_config();
        config.repositories.push(repo.clone());
        let automatic = for_repository(&repo, None, false, &config);
        assert_eq!(automatic.len(), 1);
        assert_eq!(automatic[0].tag.as_deref(), Some("v1.8.7-beta.12"));
        assert!(automatic[0].verification_pending);
        assert_eq!(for_repository(&repo, None, true, &config).len(), 2);
        repo.pinned_version = Some("missing".into());
        assert!(for_repository(&repo, None, false, &config).is_empty());
        assert_eq!(for_repository(&repo, None, true, &config).len(), 2);
    }

    #[test]
    fn chosen_root_asset_and_cached_beta_bypass_default_pin_channel_and_rules_only() {
        let mut repo = crate::community::repository_from_coordinates(
            &Provider::Github,
            "Owner",
            "Mod",
            "",
            "",
        )
        .unwrap();
        repo.source_id = None;
        repo.plugin_guid = "test-mod".into();
        repo.channel = Channel::Stable;
        repo.pinned_version = Some("old".into());
        repo.cached_releases = releases::parse_release_json(&serde_json::json!([
            {"tag_name":"v2-beta.7","prerelease":true,"assets":[{"name":"chosen.dll","browser_download_url":"https://example.invalid/chosen.dll"}]},
            {"tag_name":"v3-beta","prerelease":true,"draft":true,"assets":[{"name":"chosen.dll","browser_download_url":"https://example.invalid/draft.dll"}]}
        ]), &Provider::Github).unwrap();
        repo.asset_rules = vec![AssetRule {
            rule_type: AssetRuleType::Exact,
            value: "no-such.zip".into(),
        }];
        let config = crate::config::default_config();
        assert!(for_repository(&repo, None, false, &config).is_empty());
        assert!(for_repository(&repo, None, true, &config).is_empty());
        let roots = for_root_asset(&repo, None, &config, Some("chosen.dll"));
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].tag.as_deref(), Some("v2-beta.7"));
        assert!(for_root_asset(&repo, None, &config, Some("missing.dll")).is_empty());
    }
}
