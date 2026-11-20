//! Try a complete, provider-verified Release asset set before planning download routes.
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
    allow_dll: bool,
}

impl ReleaseCacheScope {
    pub(crate) fn for_map(
        map: &MapRepositoryConfig,
        version: &str,
        main: &ReleaseAsset,
    ) -> Option<Self> {
        let mut repository = crate::releases::repository_cache_key(
            map.provider.clone(),
            &map.username,
            &map.repository,
        );
        if let Some(tag) = crate::map_sources::map_tag(map) {
            let prefix = crate::map_sources::map_asset_version_prefix(map, version, &main.name)?;
            // A fixed tag can publish more than one filename family. Use the
            // actual prefix rather than the source entry or local display name.
            repository.push_str("\0map-tag\0");
            repository.push_str(tag);
            repository.push('\0');
            repository.push_str(prefix);
        } else {
            // A conventional map Release uses one real tag per version. Do
            // not split it into one cache group per version or share it with MODs.
            repository.push_str("\0map\0");
            repository.push_str(&crate::map_sources::map_identity(map));
        }
        Some(Self {
            repository,
            version: version.to_owned(),
            allow_dll: false,
        })
    }

    pub(crate) fn for_mod(
        provider: Provider,
        owner: &str,
        repository: &str,
        version: &str,
    ) -> Self {
        Self {
            repository: crate::releases::repository_cache_key(provider, owner, repository),
            version: version.to_owned(),
            allow_dll: true,
        }
    }
}

#[derive(Clone)]
pub(super) struct ReusableRelease {
    store: ReleaseDownloadCache,
    assets: Arc<[(ReleaseCacheKey, Option<u64>)]>,
    zip: bool,
}

impl ReusableRelease {
    pub(super) fn new(
        cache: &Path,
        scope: &ReleaseCacheScope,
        assets: &[ReleaseAsset],
    ) -> Option<Self> {
        // asset_set supplies the main file last. MOD Releases can also be a
        // standalone DLL; installation will still verify the plugin identity.
        let main = assets.last()?;
        let zip = is_zip(main);
        if !(zip
            || scope.allow_dll
                && assets.len() == 1
                && main.name.to_ascii_lowercase().ends_with(".dll"))
        {
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
            zip,
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
            if self.zip {
                validate_zip_with_cancel(file.path(), is_download_cancelled).map(|()| file)
            } else {
                Ok(file)
            }
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

    /// Called only after the download and any ZIP reassembly have validated.
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

    fn map(tag: Option<&str>) -> MapRepositoryConfig {
        serde_json::from_value(serde_json::json!({
            "id": "original-map-id",
            "source_id": "original-source",
            "provider": "github",
            "username": "Owner",
            "repository": "MapCollection",
            "tag": tag,
            "author": "Author",
            "level_set_name": "OC2Maps",
            "level_set_name_zh": "地图一",
            "asset_rules": [{"type": "regex", "value": "^厨房_v(?P<version>[0-9.]+)\\.zip$"}],
            "latest_mapset_version": "v2",
            "latest_info_version": "unrelated-info-version"
        }))
        .unwrap()
    }

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.into(),
            size: 0,
            digest: None,
            download_count: None,
            download_url: String::new(),
        }
    }

    #[test]
    fn fixed_map_tag_groups_the_same_prefix_even_for_declared_versions_and_renamed_sources() {
        let mut map = map(Some(" shared-tag "));
        let first = ReleaseCacheScope::for_map(&map, "1.0", &asset("厨房_v1.0.zip")).unwrap();
        map.level_set_name_zh = "新名字".into();
        map.id = "different-map-id".into();
        map.source_id = Some("another-group".into());
        map.release_derived = true;
        map.tag = Some("shared-tag".into());
        let fix = asset("厨房_v3.0-fix.zip");
        let current = ReleaseCacheScope::for_map(&map, "3.0-fix", &fix).unwrap();
        assert_eq!(current.repository, first.repository);
        assert_eq!(current.version, "3.0-fix");
        assert!(ReleaseCacheScope::for_map(&map, "3.1", &fix).is_none());
        map.tag = Some("other-map-tag".into());
        assert_ne!(
            current.repository,
            ReleaseCacheScope::for_map(&map, "3.0-fix", &fix)
                .unwrap()
                .repository
        );
    }

    #[test]
    fn ordinary_map_releases_keep_three_slots_per_map_across_release_tags() {
        let mut map = map(None);
        let main = asset("arbitrary.zip");
        let first = ReleaseCacheScope::for_map(&map, "v1", &main).unwrap();
        let current = ReleaseCacheScope::for_map(&map, "v3", &main).unwrap();
        let mod_scope =
            ReleaseCacheScope::for_mod(map.provider.clone(), &map.username, &map.repository, "v1");
        assert_ne!(first.repository, mod_scope.repository);
        assert_eq!(first.repository, current.repository);
        assert_eq!(current.version, "v3");

        map.level_set_name_zh = "另一张地图".into();
        assert_ne!(
            first.repository,
            ReleaseCacheScope::for_map(&map, "v3", &main)
                .unwrap()
                .repository
        );
        map.level_set_name_zh = " 地图一 ".into();
        map.name = "Localized label".into();
        assert_eq!(
            first.repository,
            ReleaseCacheScope::for_map(&map, "v1", &main)
                .unwrap()
                .repository
        );
        assert_eq!(first.version, "v1");
    }
}
