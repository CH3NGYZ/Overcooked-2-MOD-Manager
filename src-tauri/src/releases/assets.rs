use crate::models::AssetRule;
use crate::models::AssetRuleType;
use crate::models::ReleaseAsset;
use crate::models::ReleaseInfo;
use crate::releases::RepositoryRequest;
use globset::GlobBuilder;
use regex::Regex;

/// Downloads consume the version snapshot already shown by the card. A missing
/// requested or pinned version cannot silently turn into another version.
pub(crate) fn select_cached_release<'a>(
    releases: &'a [ReleaseInfo],
    tag: Option<&str>,
    pinned_version: Option<&str>,
) -> Option<&'a ReleaseInfo> {
    if let Some(tag) = tag {
        return releases.iter().find(|release| release.tag_name == tag);
    }
    if let Some(version) = pinned_version {
        return releases.iter().find(|release| {
            release.tag_name == version || release.version.as_deref() == Some(version)
        });
    }
    releases.first()
}

pub fn asset_matches(asset: &ReleaseAsset, rules: &[AssetRule]) -> bool {
    rules.iter().any(|rule| match rule.rule_type {
        AssetRuleType::Exact => asset.name.eq_ignore_ascii_case(&rule.value),
        AssetRuleType::Glob => GlobBuilder::new(&rule.value)
            .case_insensitive(true)
            .build()
            .ok()
            .map(|glob| glob.compile_matcher().is_match(&asset.name))
            .unwrap_or(false),
        AssetRuleType::Regex => Regex::new(&rule.value)
            .ok()
            .map(|regex| regex.is_match(&asset.name))
            .unwrap_or(false),
    })
}

pub fn select_install_asset<'a>(
    repo: &RepositoryRequest,
    rules: &[AssetRule],
    release: &'a ReleaseInfo,
    asset_name: Option<&str>,
) -> Option<&'a ReleaseAsset> {
    release.assets.iter().find(|asset| {
        !crate::community::is_source_archive(
            &repo.provider,
            repo.source_id.as_deref(),
            &release.tag_name,
            &asset.name,
        ) && asset_name
            .map(|name| asset.name == name)
            .unwrap_or_else(|| asset_matches(asset, rules))
    })
}

#[cfg(test)]
mod tests;
