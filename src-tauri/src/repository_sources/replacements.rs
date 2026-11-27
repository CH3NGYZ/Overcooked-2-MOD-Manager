use super::{DEFAULT_INDEX_ID, SourceLocation, SourcePackage, package_document_path};
use crate::config::ConfigError;
use crate::models::{
    AppConfig, Channel, DataProvider, ModDownloadMode, ModRepositoryExport, Provider,
    ReplacedModRepository, RepositorySource,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplacementSource {
    revision: u64,
    url: String,
    parent_id: Option<String>,
    region: Option<String>,
}

impl ReplacementSource {
    fn new(source: &RepositorySource, revision: u64) -> Self {
        Self {
            revision,
            url: source.url.clone(),
            parent_id: source.parent_id.clone(),
            region: source.region.clone(),
        }
    }

    fn matches(&self, source: &RepositorySource, revision: u64) -> bool {
        source.builtin
            && self.revision == revision
            && self.url == source.url
            && self.parent_id == source.parent_id
            && self.region == source.region
    }
}

pub(super) fn is_official_root(config: &AppConfig, source: &RepositorySource) -> bool {
    source.id == DEFAULT_INDEX_ID
        && source.builtin
        && source.parent_id.is_none()
        && source.url == config.data_provider.document(super::BUILTIN_INDEX)
}

pub(super) fn is_authorized<'a>(
    config: &'a AppConfig,
    mut source: &'a RepositorySource,
    revision: u64,
) -> bool {
    for _ in 0..=config.repository_sources.len() {
        if is_official_root(config, source) {
            return true;
        }
        if !config
            .replacement_sources
            .get(&source.id)
            .is_some_and(|stamp| stamp.matches(source, revision))
        {
            break;
        }
        let Some(parent) = source.parent_id.as_ref().and_then(|id| {
            config
                .repository_sources
                .iter()
                .find(|entry| entry.id == *id)
        }) else {
            break;
        };
        source = parent;
    }
    false
}

pub(super) fn authorize_child_sources(
    config: &mut AppConfig,
    parent_id: &str,
    accepted_child_ids: &[String],
    revision: u64,
) {
    config
        .replacement_sources
        .retain(|_, stamp| stamp.parent_id.as_deref() != Some(parent_id));
    for source in &config.repository_sources {
        if source.builtin
            && source.parent_id.as_deref() == Some(parent_id)
            && accepted_child_ids.contains(&source.id)
            && SourceLocation::parse(&source.url)
                .ok()
                .and_then(|location| package_document_path(&location))
                .is_some()
        {
            config
                .replacement_sources
                .insert(source.id.clone(), ReplacementSource::new(source, revision));
        }
    }
}

pub(crate) fn retain_current(config: &mut AppConfig, revision: Option<u64>) {
    let Some(revision) = revision else {
        config.replacement_sources.clear();
        for repository in &mut config.repositories {
            repository.clear_replacements();
        }
        return;
    };
    config.replacement_sources.retain(|id, stamp| {
        config
            .repository_sources
            .iter()
            .any(|source| source.id == *id && stamp.matches(source, revision))
    });
    if !config
        .repositories
        .iter()
        .any(|repo| !repo.replaces.is_empty())
    {
        return;
    }
    let allowed = config
        .repository_sources
        .iter()
        .filter(|source| is_authorized(config, source, revision))
        .map(|source| source.id.as_str())
        .collect::<HashSet<_>>();
    for repo in &mut config.repositories {
        if repo.replaces_revision != Some(revision)
            || repo
                .source_id
                .as_deref()
                .is_none_or(|source| !allowed.contains(source))
        {
            repo.clear_replacements();
        }
    }
}

pub(super) type Declarations = HashMap<String, Vec<ReplacedModRepository>>;

pub(crate) fn verified_declarations(
    package: &SourcePackage,
    provider: DataProvider,
    source_url: &str,
    repositories: &[ModRepositoryExport],
) -> Declarations {
    let verified = || {
        let location = SourceLocation::parse(source_url)?;
        let Some(declaring_path) = package_document_path(&location) else {
            return Ok(Declarations::new());
        };
        let declaring_member = package.read(&declaring_path)?;
        let root = SourceLocation::parse(&provider.document(super::BUILTIN_INDEX))?;
        let mut pending = vec![(root, Default::default())];
        let mut visited = HashSet::new();
        let mut members = HashSet::new();
        let mut declaring_member_seen = false;
        let mut catalogue = Vec::new();
        while let Some((location, inherited)) = pending.pop() {
            let Some(path) = package_document_path(&location) else {
                continue;
            };
            if !visited.insert(location.address()) {
                continue;
            }
            let text = package.read(&path)?;
            declaring_member_seen |= std::ptr::eq(text, declaring_member);
            let document = super::document::parse_document(text, &location, &inherited)?;
            if members.insert(text.as_ptr()) {
                catalogue.extend(document.repositories);
            }
            for child in document.repository_sources {
                pending.push((SourceLocation::parse(&child.url)?, child.network_plan));
            }
        }
        Ok::<_, ConfigError>(if declaring_member_seen {
            declarations(repositories, &catalogue)
        } else {
            Declarations::new()
        })
    };
    verified()
        .inspect_err(|error| {
            log::warn!(
                "未授权仓库接续：官方源码包目录读取不完整，原因={}",
                error.diagnostic_code()
            );
        })
        .unwrap_or_default()
}

pub(super) fn suppress_configured_originals(
    declarations: &mut Declarations,
    config: &AppConfig,
    declaring_source: &str,
) {
    for aliases in declarations.values_mut() {
        aliases.retain(|old| {
            !config.repositories.iter().any(|current| {
                current.source_id.as_deref() != Some(declaring_source)
                    && blocks_replacement(
                        old,
                        current.download_mode,
                        current.channel,
                        &current.provider,
                        &current.username,
                        &current.repository,
                    )
            })
        });
    }
}

fn declarations(
    repositories: &[ModRepositoryExport],
    catalogue: &[ModRepositoryExport],
) -> Declarations {
    let mut seen = HashSet::new();
    let mut ambiguous = HashSet::new();
    for old in catalogue.iter().flat_map(|repo| &repo.replaces) {
        let key = old_key(old);
        if !seen.insert(key.clone()) {
            ambiguous.insert(key);
        }
    }
    let mut by_target = Declarations::new();
    for repo in repositories {
        let unshadowed = repo
            .replaces
            .iter()
            .filter(|old| {
                !ambiguous.contains(&old_key(old))
                    && !catalogue.iter().any(|current| {
                        blocks_replacement(
                            old,
                            current.download_mode,
                            current.channel,
                            &current.provider,
                            &current.username,
                            &current.repository,
                        )
                    })
            })
            .cloned()
            .collect();
        by_target
            .entry(crate::config::export_key(repo))
            .and_modify(Vec::clear)
            .or_insert(unshadowed);
    }
    by_target
}

fn blocks_replacement(
    old: &ReplacedModRepository,
    mode: ModDownloadMode,
    channel: Channel,
    provider: &Provider,
    username: &str,
    repository: &str,
) -> bool {
    mode == ModDownloadMode::Release
        && (channel == old.channel || channel == Channel::All)
        && old.matches_identity(provider, username.trim(), repository.trim())
}

fn old_key(old: &ReplacedModRepository) -> (&'static str, String, String, String, Channel) {
    let (provider, username, repository, region) =
        crate::config::repository_address_key(&old.provider, &old.username, &old.repository, None);
    (provider, username, repository, region, old.channel)
}

#[cfg(test)]
mod tests;
