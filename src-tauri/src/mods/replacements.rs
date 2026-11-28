use crate::{
    archive::InstallManifest,
    models::{
        AppConfig, Channel, InstalledMod, ModDownloadMode, ModRepositoryBinding,
        ModRepositoryConfig, ReleaseInfo,
    },
    releases,
};
use std::{collections::HashMap, path::Path};

use super::binding::repository_matches_mod;

fn previous_source_removed(config: &AppConfig, binding: &ModRepositoryBinding) -> bool {
    binding.is_valid()
        && !config
            .repositories
            .iter()
            .any(|repo| binding.matches_repository(repo))
}

fn matching_replacement<'a>(
    config: &'a AppConfig,
    previous_id: &str,
    binding: &ModRepositoryBinding,
    matches: impl Fn(&ModRepositoryConfig) -> bool,
) -> Option<&'a ModRepositoryConfig> {
    uuid::Uuid::parse_str(previous_id).ok()?;
    if !previous_source_removed(config, binding)
        || crate::config::repository_id_is_configured(config, previous_id)
    {
        return None;
    }
    let mut repos = config.repositories.iter().filter(|repo| {
        repo.download_mode == ModDownloadMode::Release
            && !repo.plugin_guid.trim().is_empty()
            && crate::config::mod_repository_is_available(config, repo, &config.region)
            && matches(repo)
    });
    let replacement = repos.next()?;
    if repos.next().is_some() {
        return None;
    }
    crate::config::unique_mod_repository_by_id(config, &replacement.id)
}

pub(crate) fn orphan_replacement_repository<'a>(
    config: &'a AppConfig,
    manifest: &InstallManifest,
    target: &str,
    allow_declared: bool,
) -> Option<&'a ModRepositoryConfig> {
    let binding = manifest.repository_binding.as_ref()?;
    if manifest.is_manual()
        || binding.tag != manifest.release_tag
        || !manifest
            .files
            .iter()
            .any(|path| super::canonical_dll_name(path) == target)
    {
        return None;
    }
    let filename = Path::new(target).file_name()?.to_str()?;
    let single_dll = manifest
        .files
        .iter()
        .filter(|path| super::file_state::is_mod_file(Path::new(path)))
        .nth(1)
        .is_none();
    let version = manifest
        .installed_dll_versions
        .get(&super::manifest_dll_key(target))
        .map(String::as_str);
    matching_replacement(config, &manifest.repository_id, binding, |repo| {
        (single_dll && repository_matches_mod(repo, None, None, Some(filename), version))
            || (allow_declared
                && repo.channel == binding.channel
                && repo.replaces_revision.is_some()
                && repo.replaces.iter().any(|old| old.matches_binding(binding))
                && repository_matches_mod(repo, Some(&repo.plugin_guid), None, None, version))
    })
}

pub(super) fn replacement_targets<'a>(
    config: &'a AppConfig,
    mods: &[InstalledMod],
) -> Vec<Option<&'a ModRepositoryConfig>> {
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
            let package_id = item.package_id.as_deref()?;
            if !item.repository_replacement_eligible
                || item.is_dependency
                || item.manual_install
                || item.release_companion
                || item.bundled_with_dll.is_some()
                || item.duplicate_cleanup_available
                || counts.get(&guid.to_ascii_lowercase()) != Some(&1)
            {
                return None;
            }
            matching_replacement(config, package_id, binding, |repo| {
                repo.plugin_guid.trim().eq_ignore_ascii_case(guid)
                    && repository_matches_mod(
                        repo,
                        Some(guid),
                        item.product_name.as_deref(),
                        item.original_filename.as_deref(),
                        item.version.as_deref(),
                    )
            })
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
        let prerelease = releases::resolve_prerelease(
            release.api_prerelease,
            release.prerelease,
            Some(&release.tag_name),
        );
        !release.draft
            && repo.channel.allows_prerelease(prerelease)
            && (repo.channel != Channel::All || prerelease == installed.prerelease)
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
