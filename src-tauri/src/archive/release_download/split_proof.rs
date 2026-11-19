use crate::archive::integrity::{normalize_sha256, validate_download_digest};
use crate::models::ReleaseAsset;
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, path::Path};

/// A backend-only receipt for verified volumes and the exact resulting bytes.
/// The merged ZIP has no Release digest of its own; its digest is captured
/// after verified reassembly and compared again when consuming the preview.
#[derive(Debug)]
pub(crate) struct SplitZipProof {
    assets: Vec<ReleaseAsset>,
    merged_size: u64,
    merged_sha256: String,
}

impl SplitZipProof {
    pub(super) fn capture(path: &Path, assets: Vec<ReleaseAsset>) -> Result<Self, String> {
        let mut merged = File::open(path).map_err(|error| error.to_string())?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut merged_size = 0u64;
        loop {
            if crate::archive::is_download_cancelled() {
                return Err("download.cancelled".into());
            }
            let size = merged
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if size == 0 {
                break;
            }
            merged_size += size as u64;
            hasher.update(&buffer[..size]);
        }
        Ok(Self {
            assets,
            merged_size,
            merged_sha256: format!("{:x}", hasher.finalize()),
        })
    }

    /// Return whether every volume was authenticated by the Release API, for
    /// callers that need provenance in addition to a successfully verified ZIP.
    pub(crate) fn validate(
        &self,
        assets: &[ReleaseAsset],
        merged_size: u64,
        merged_sha256: &str,
    ) -> Result<bool, String> {
        if assets.len() != self.assets.len()
            || self.assets.iter().zip(assets).any(|(actual, expected)| {
                actual.name != expected.name
                    || actual.size != expected.size
                    || actual.download_url != expected.download_url
                    || actual.digest != expected.digest
            })
        {
            return Err("modChanges.targetChanged".into());
        }
        if merged_size != self.merged_size {
            return Err("modChanges.targetChanged".into());
        }
        validate_download_digest(merged_sha256, &self.merged_sha256)?;
        Ok(assets
            .iter()
            .all(|asset| asset.digest.as_deref().and_then(normalize_sha256).is_some()))
    }
}
