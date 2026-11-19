//! Try a complete, provider-verified ZIP set before planning download routes.
use super::cache::{ReleaseCacheKey, ReleaseDownloadCache};
use super::{TemporaryFile, expected_size, is_zip, merge_volumes};
use crate::archive::{integrity::validate_zip_with_cancel, is_download_cancelled};
use crate::models::{MapRepositoryConfig, Provider, ReleaseAsset};
use std::path::Path;
use std::sync::Arc;

pub(super) const RELEASE_ASSETS_CACHE: &str = "release-assets";

pub(crate) struct ReleaseCacheScope {
    repository: String,
    version: String,
}

impl ReleaseCacheScope {
    pub(crate) fn for_map(map: &MapRepositoryConfig, version: &str) -> Self {
        let tag = crate::map_sources::map_tag(map);
        let mut scope = Self::new(
            map.provider.clone(),
            &map.username,
            &map.repository,
            tag,
            version,
        );
        // A derived Release describes one map, but explicit entries may
        // select different maps from a shared fixed tag. Their source IDs
        // can change on regrouping, so use the existing content identity.
        if tag.is_some() && !map.release_derived {
            scope.repository.push('\0');
            scope
                .repository
                .push_str(&crate::map_sources::map_identity(map));
        }
        scope
    }

    pub(crate) fn new(
        provider: Provider,
        owner: &str,
        repository: &str,
        map_tag: Option<&str>,
        version: &str,
    ) -> Self {
        let mut scope = crate::releases::repository_cache_key(provider, owner, repository);
        // Several maps can share a repository: a fixed tag identifies the
        // map, while the projected Release tag is its installable version.
        if let Some(tag) = map_tag {
            scope.push('\0');
            scope.push_str(tag);
        }
        Self {
            repository: scope,
            version: version.to_owned(),
        }
    }
}

#[derive(Clone)]
pub(super) struct ReusableRelease {
    store: ReleaseDownloadCache,
    assets: Arc<[(ReleaseCacheKey, Option<u64>)]>,
}

impl ReusableRelease {
    pub(super) fn new(
        cache: &Path,
        scope: &ReleaseCacheScope,
        assets: &[ReleaseAsset],
    ) -> Option<Self> {
        // asset_set supplies the main file last. DLLs, unversioned files and
        // a set with any missing/unsupported digest bypass the cache entirely.
        if !is_zip(assets.last()?) {
            return None;
        }
        let keys: Vec<_> = assets
            .iter()
            .map(|asset| {
                let key = ReleaseCacheKey::new(
                    &scope.repository,
                    &scope.version,
                    &asset.name,
                    asset.digest.as_deref(),
                )?;
                Some((key, expected_size(asset)))
            })
            .collect::<Option<_>>()?;
        Some(Self {
            store: ReleaseDownloadCache::new(cache.join(RELEASE_ASSETS_CACHE)),
            assets: keys.into(),
        })
    }

    pub(super) fn restore(self, cache: &Path) -> Result<Option<(TemporaryFile, Vec<u64>)>, String> {
        let mut sizes = Vec::with_capacity(self.assets.len());
        let mut files = Vec::with_capacity(self.assets.len());
        for (key, size) in self.assets.iter() {
            let Some(restored) = self.store.restore(key, cache, *size, is_download_cancelled)
            else {
                return Ok(None);
            };
            let Ok(metadata) = std::fs::metadata(&restored) else {
                return Ok(None);
            };
            let Ok(path) = restored.keep() else {
                return Ok(None);
            };
            sizes.push(metadata.len());
            files.push(TemporaryFile(Some(path)));
        }
        let validated = if files.len() == 1 {
            let file = files.pop().unwrap();
            validate_zip_with_cancel(file.path(), is_download_cancelled).map(|()| file)
        } else {
            merge_volumes(&files, cache)
        };
        let output = match validated {
            Ok(file) => file,
            // Bad cache bytes fall back to network; user cancellation does not.
            Err(error) if error == "download.cancelled" => return Err(error),
            Err(_) => return Ok(None),
        };
        if is_download_cancelled() {
            return Err("download.cancelled".into());
        }
        self.store.touch(&self.assets[0].0);
        Ok(Some((output, sizes)))
    }

    /// Called only after every volume and the resulting ZIP have validated.
    pub(super) fn remember(&self, files: &[TemporaryFile]) {
        let verified = self
            .assets
            .iter()
            .zip(files)
            .map(|((key, size), file)| (key, file.path(), *size))
            .collect::<Vec<_>>();
        self.store.remember_set(&verified, is_download_cancelled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(tag: Option<&str>, release_derived: bool) -> MapRepositoryConfig {
        serde_json::from_value(serde_json::json!({
            "id": "original-map-id",
            "source_id": "original-source",
            "provider": "github",
            "username": "Owner",
            "repository": "MapCollection",
            "tag": tag,
            "release_derived": release_derived,
            "author": "Author",
            "level_set_name": "OC2Maps",
            "level_set_name_zh": "地图一",
            "latest_mapset_version": "v2",
            "latest_info_version": "unrelated-info-version"
        }))
        .unwrap()
    }

    #[test]
    fn explicit_maps_at_one_fixed_tag_keep_independent_version_slots() {
        let map_a = map(Some(" shared-tag "), false);
        let mut map_b = map_a.clone();
        map_b.level_set_name_zh = "地图二".into();
        let first = ReleaseCacheScope::for_map(&map_a, "v1");
        let other = ReleaseCacheScope::for_map(&map_b, "v1");
        assert_ne!(first.repository, other.repository);

        // A duplicate declaration can be renamed or regrouped; its canonical
        // map identity and previously cached version still point to those bytes.
        map_b.level_set_name_zh = " 地图一 ".into();
        map_b.level_set_name = "oc2maps".into();
        map_b.id = "different-map-id".into();
        map_b.source_id = Some("another-group".into());
        map_b.name = "Localized label".into();
        assert_eq!(
            first.repository,
            ReleaseCacheScope::for_map(&map_b, "v1").repository
        );
        assert_eq!(first.version, "v1");
    }

    #[test]
    fn release_tags_keep_the_real_tag_scope_through_source_and_version_changes() {
        let mut map = map(Some("map-tag"), true);
        let original = ReleaseCacheScope::for_map(&map, "v1");
        map.id = "different-map-id".into();
        map.source_id = Some("another-group".into());
        map.name = "Updated display name".into();
        map.latest_mapset_version = Some("v3".into());
        map.latest_info_version = Some("99".into());
        let historical = ReleaseCacheScope::for_map(&map, "v1");
        let current = ReleaseCacheScope::for_map(&map, "v3");
        assert_eq!(original.repository, historical.repository);
        assert_eq!(original.repository, current.repository);
        assert_eq!(original.version, historical.version);
        assert_eq!(current.version, "v3");

        map.tag = Some("other-map-tag".into());
        assert_ne!(
            original.repository,
            ReleaseCacheScope::for_map(&map, "v1").repository
        );
    }
}
