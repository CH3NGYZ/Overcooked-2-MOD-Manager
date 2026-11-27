use crate::{
    models::{
        AppConfig, Channel, InstalledMod, ModDownloadMode, ModRepositoryBinding,
        ModRepositoryConfig, ReleaseInfo,
    },
    releases,
};
use std::collections::HashMap;

pub(crate) fn replacement_repository<'a>(
    config: &'a AppConfig,
    binding: &ModRepositoryBinding,
    primary_guid: Option<&str>,
) -> Option<&'a ModRepositoryConfig> {
    let guid = primary_guid
        .map(str::trim)
        .filter(|guid| !guid.is_empty())?;
    if !binding.is_valid()
        || config
            .repositories
            .iter()
            .any(|repo| binding.matches_repository(repo))
    {
        return None;
    }
    let mut claims = config.repositories.iter().filter(|repo| {
        repo.replaces_revision.is_some()
            && repo.replaces.iter().any(|old| old.matches_binding(binding))
    });
    let repo = claims.next()?;
    (claims.next().is_none()
        && repo.download_mode == ModDownloadMode::Release
        && repo.channel == binding.channel
        && repo.plugin_guid.trim().eq_ignore_ascii_case(guid)
        && crate::config::mod_repository_is_available(config, repo, &config.region))
    .then_some(repo)
}

pub(super) fn replacement_targets<'a>(
    config: &'a AppConfig,
    mods: &[InstalledMod],
) -> Vec<Option<&'a ModRepositoryConfig>> {
    if !config
        .repositories
        .iter()
        .any(|repo| !repo.replaces.is_empty())
    {
        return vec![None; mods.len()];
    }
    let mut counts = HashMap::new();
    for guid in mods.iter().filter_map(|item| item.guid.as_deref()) {
        *counts
            .entry(guid.trim().to_ascii_lowercase())
            .or_insert(0usize) += 1;
    }
    mods.iter()
        .map(|item| {
            let guid = item.guid.as_deref()?.trim();
            let binding = item.repository_binding.as_ref()?;
            let package_id = uuid::Uuid::parse_str(item.package_id.as_deref()?).ok()?;
            if !item.repository_replacement_eligible
                || item.is_dependency
                || item.manual_install
                || item.release_companion
                || item.bundled_with_dll.is_some()
                || item.duplicate_cleanup_available
                || counts.get(&guid.to_ascii_lowercase()) != Some(&1)
                || (binding.channel == Channel::Stable && item.prerelease)
                || (binding.channel == Channel::Beta && !item.prerelease)
                || config
                    .repositories
                    .iter()
                    .any(|repo| uuid::Uuid::parse_str(&repo.id) == Ok(package_id))
                || config
                    .map_repositories
                    .iter()
                    .any(|repo| uuid::Uuid::parse_str(&repo.id) == Ok(package_id))
            {
                return None;
            }
            replacement_repository(config, binding, Some(guid))
        })
        .collect()
}

pub(super) fn replacement_release<'a>(
    repo: &'a ModRepositoryConfig,
    installed: &InstalledMod,
) -> Option<&'a ReleaseInfo> {
    if repo.last_error.is_some() {
        return None;
    }
    let on_line = |release: &&ReleaseInfo| {
        !release.draft
            && releases::resolve_prerelease(
                release.api_prerelease,
                release.prerelease,
                Some(&release.tag_name),
            ) == installed.prerelease
    };
    let release = match repo.pinned_version.as_deref() {
        Some(pin) => {
            releases::select_cached_release(&repo.cached_releases, None, Some(pin)).filter(on_line)
        }
        None => repo.cached_releases.iter().find(on_line),
    }?;
    let request = releases::RepositoryRequest::from(repo);
    let asset = releases::select_install_asset(&request, &repo.asset_rules, release, None)?;
    (!asset.download_url.trim().is_empty()
        && std::path::Path::new(&asset.name)
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("zip")))
    .then_some(release)
}
