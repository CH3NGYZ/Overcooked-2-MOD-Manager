//! Metadata-only invalidation. Never reads resource contents to detect changes.
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const MAX_MANIFEST_BYTES: usize = 2 * 1024 * 1024;

pub(crate) fn directory(path: &Path) -> Result<String, String> {
    let root = path
        .canonicalize()
        .map_err(|_| "diychef.previewUnavailable")?;
    let mut rows = vec![];
    let mut pending = vec![root.clone()];
    let mut path_bytes = 0usize;
    let mut directories = 1usize;
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(dir).map_err(|_| "diychef.previewUnavailable")? {
            let entry = entry.map_err(|_| "diychef.previewUnavailable")?;
            let path = entry.path();
            let info = entry.metadata().map_err(|_| "diychef.previewUnavailable")?;
            // Reparse points and links can otherwise escape the chef's resource boundary.
            if is_link(&info) {
                return Err("diychef.previewUnsafeResource".into());
            }
            if info.is_dir() {
                directories += 1;
                if directories > 16_384 {
                    return Err("diychef.previewLimit".into());
                }
                pending.push(path);
                continue;
            }
            if !info.is_file() {
                continue;
            }
            if rows.len() >= 16_384 {
                return Err("diychef.previewLimit".into());
            }
            let name = path
                .strip_prefix(&root)
                .map_err(|_| "diychef.previewUnsafeResource")?;
            let name = name.to_string_lossy().replace('\\', "/");
            path_bytes = path_bytes.saturating_add(name.len());
            if path_bytes > 2 * 1024 * 1024 {
                return Err("diychef.previewLimit".into());
            }
            rows.push((name, stamp(&info)?));
        }
    }
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    manifest(format_args!("{}|{:?}", root.display(), rows))
}

#[cfg(test)]
pub(crate) fn files(paths: &[PathBuf]) -> Result<String, String> {
    let mut rows = vec![];
    for path in paths {
        let info = path.metadata().map_err(|_| "diychef.previewUnavailable")?;
        rows.push((path.clone(), info));
    }
    metadata_files(&rows)
}

pub(super) fn metadata_files(rows: &[(PathBuf, std::fs::Metadata)]) -> Result<String, String> {
    let mut ordered = rows
        .iter()
        .map(|(path, info)| Ok((path.to_string_lossy(), stamp(info)?)))
        .collect::<Result<Vec<_>, String>>()?;
    ordered.sort_by(|a, b| a.0.cmp(&b.0));
    manifest(format_args!("{ordered:?}"))
}

// Keep the original byte representation: existing PNGs use the digest of this
// manifest. Reject oversized manifests while writing, before allocating them.
fn manifest(arguments: std::fmt::Arguments<'_>) -> Result<String, String> {
    struct Bounded(String);
    impl Write for Bounded {
        fn write_str(&mut self, value: &str) -> std::fmt::Result {
            let length = self.0.len().saturating_add(value.len());
            if length > MAX_MANIFEST_BYTES {
                return Err(std::fmt::Error);
            }
            if length > self.0.capacity() {
                let capacity = self
                    .0
                    .capacity()
                    .saturating_mul(2)
                    .max(length)
                    .min(MAX_MANIFEST_BYTES);
                self.0
                    .try_reserve_exact(capacity - self.0.len())
                    .map_err(|_| std::fmt::Error)?;
            }
            self.0.push_str(value);
            Ok(())
        }
    }
    let mut output = Bounded(String::new());
    output
        .write_fmt(arguments)
        .map_err(|_| "diychef.previewLimit")?;
    Ok(output.0)
}

fn stamp(info: &std::fs::Metadata) -> Result<(u64, u128), String> {
    let modified = info
        .modified()
        .map_err(|_| "diychef.previewUnavailable")?
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "diychef.previewUnavailable")?;
    Ok((info.len(), modified.as_nanos()))
}
fn is_link(info: &std::fs::Metadata) -> bool {
    if info.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if info.file_attributes() & 0x400 != 0 {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    // Reference representation from before the metadata enumeration optimization.
    // Keep this independent from metadata_files/directory's bounded formatter.
    fn legacy_files(paths: &[PathBuf]) -> String {
        let mut rows = vec![];
        for path in paths {
            let info = path.metadata().unwrap();
            rows.push((path.to_string_lossy().into_owned(), stamp(&info).unwrap()));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        format!("{rows:?}")
    }

    fn legacy_directory(path: &Path) -> String {
        let root = path.canonicalize().unwrap();
        let mut rows = vec![];
        let mut pending = vec![root.clone()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                let info = std::fs::symlink_metadata(&path).unwrap();
                assert!(!is_link(&info));
                if info.is_dir() {
                    pending.push(path);
                } else if info.is_file() {
                    rows.push((
                        path.strip_prefix(&root)
                            .unwrap()
                            .to_string_lossy()
                            .replace('\\', "/"),
                        stamp(&info).unwrap(),
                    ));
                }
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        format!("{}|{:?}", root.display(), rows)
    }

    fn legacy_context(context: &crate::diychef::session::PreviewContext) -> String {
        let paths = super::super::metadata::source_metadata(&context.game_root)
            .unwrap()
            .into_iter()
            .map(|(path, _)| path)
            .collect::<Vec<_>>();
        let mut manifest = format!(
            "chef-scene-v6-complete-openhands|{}|{}|{}",
            context.chef_name,
            context.hat_value,
            legacy_files(&paths)
        );
        for dir in [&context.chef_directory, &context.hat_directory]
            .into_iter()
            .flatten()
        {
            manifest.push_str(&legacy_directory(dir));
        }
        format!("{:x}", Sha256::digest(manifest.as_bytes()))
    }

    #[test]
    fn original_manifests_and_final_scene_fingerprints_remain_byte_compatible() {
        let _guard = super::super::TEST_CACHE_ACCESS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = std::env::temp_dir().join(format!("chef-legacy-key-{}", uuid::Uuid::new_v4()));
        let chef = root.join("Resources/猫咪 chef 😀");
        let hat = root.join("Resources/HATS/帽子");
        let streaming = root.join("Overcooked2_Data/StreamingAssets/DLC");
        for dir in [&chef, &hat, &streaming] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for path in [
            chef.join("Head.obj"),
            chef.join("排序, bracket[1].bin"),
            hat.join("Hat.png"),
            streaming.join("bundle0"),
            streaming.join("帽子 bundle1"),
            root.join("Overcooked2_Data/resources.assets"),
            root.join("Overcooked2_Data/resources.assets.resS"),
        ] {
            std::fs::write(path, b"metadata-only invalid resource contents").unwrap();
        }
        let rows = super::super::metadata::source_metadata(&root).unwrap();
        let paths: Vec<_> = rows.iter().rev().map(|(path, _)| path.clone()).collect();
        let expected = legacy_files(&paths);
        assert_eq!(files(&paths).unwrap(), expected);
        assert_eq!(metadata_files(&rows).unwrap(), expected);
        assert_eq!(directory(&chef).unwrap(), legacy_directory(&chef));
        assert_eq!(directory(&hat).unwrap(), legacy_directory(&hat));
        let context = crate::diychef::session::PreviewContext {
            session_id: "legacy-key-fixture".into(),
            game_root: root.clone(),
            chef_name: "猫咪 chef 😀".into(),
            chef_directory: Some(chef),
            hat_value: "帽子".into(),
            hat_directory: Some(hat),
        };
        assert_eq!(
            super::super::context_fingerprint(&context).unwrap(),
            legacy_context(&context)
        );
        let canonical = root.canonicalize().unwrap();
        let temp = std::env::temp_dir().canonicalize().unwrap();
        assert!(canonical.starts_with(&temp) && canonical != temp);
        std::fs::remove_dir_all(canonical).unwrap();
    }

    #[test]
    fn debug_escaping_and_manifest_budget_preserve_the_original_representation() {
        let info = std::fs::metadata(std::env::temp_dir()).unwrap();
        let rows = [
            (PathBuf::from("quote\"and\nnewline"), info.clone()),
            (PathBuf::from("unicode_帽子_😀"), info),
        ];
        let mut expected: Vec<_> = rows
            .iter()
            .map(|(path, info)| (path.to_string_lossy().into_owned(), stamp(info).unwrap()))
            .collect();
        expected.sort_by(|a, b| a.0.cmp(&b.0));
        assert_eq!(metadata_files(&rows).unwrap(), format!("{expected:?}"));
        let limit = "x".repeat(MAX_MANIFEST_BYTES);
        assert_eq!(manifest(format_args!("{limit}")).unwrap(), limit);
        assert!(manifest(format_args!("{limit}x")).is_err());
    }

    #[test]
    #[ignore = "requires installed game/custom resources; only reads names, size and mtime"]
    fn real_game_and_custom_fingerprints_match_the_original_algorithm() {
        let _guard = super::super::TEST_CACHE_ACCESS
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let game = Path::new("E:/SteamLibrary/steamapps/common/Overcooked! 2");
        let rows = super::super::metadata::source_metadata(game).unwrap();
        let paths: Vec<_> = rows.iter().map(|(path, _)| path.clone()).collect();
        let original = legacy_files(&paths);
        assert_eq!(metadata_files(&rows).unwrap(), original);
        eprintln!(
            "legacy native metadata: files={} manifest_bytes={} identical=true",
            paths.len(),
            original.len()
        );
        let resources = game.join("BepInEx/plugins/OC2DIYChef-FastInit/Resources");
        let chef = resources.join("00-Fox");
        let hat = resources.join("HATS/Wizardhat");
        for dir in [&chef, &hat] {
            let manifest = directory(dir).unwrap();
            assert_eq!(manifest, legacy_directory(dir));
            eprintln!(
                "legacy directory {}: manifest_bytes={} identical=true",
                dir.display(),
                manifest.len()
            );
        }
        for (name, chef_directory, hat_value, hat_directory) in [
            ("Chef_Black_cat", None, "None", None),
            ("00-Fox", Some(chef.clone()), "None", None),
            ("00-Fox", Some(chef), "Wizardhat", Some(hat)),
        ] {
            let context = crate::diychef::session::PreviewContext {
                session_id: "legacy-real-key-fixture".into(),
                game_root: game.to_owned(),
                chef_name: name.into(),
                chef_directory,
                hat_value: hat_value.into(),
                hat_directory,
            };
            let current = super::super::context_fingerprint(&context).unwrap();
            assert_eq!(current, legacy_context(&context));
            eprintln!("legacy final fingerprint {name}/{hat_value}: {current} identical=true");
        }
    }

    #[test]
    fn includes_added_renamed_and_nested_files_without_reading_contents() {
        let dir = std::env::temp_dir().join(format!("chef-stamp-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("nested")).unwrap();
        std::fs::write(dir.join("Head.obj"), "abc").unwrap();
        let first = directory(&dir).unwrap();
        assert_eq!(first, directory(&dir).unwrap());
        std::fs::write(dir.join("nested/extra.bin"), "x").unwrap();
        let extra = directory(&dir).unwrap();
        assert_ne!(first, extra);
        std::fs::rename(dir.join("Head.obj"), dir.join("Head1.obj")).unwrap();
        assert_ne!(extra, directory(&dir).unwrap());
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn size_mtime_and_deletion_invalidate_but_metadata_restoration_keeps_identity() {
        let dir =
            std::env::temp_dir().join(format!("chef-stamp-boundaries-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("Head.obj");
        std::fs::write(&path, b"abc").unwrap();
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        let modified = file.metadata().unwrap().modified().unwrap();
        let before = directory(&dir).unwrap();
        file.set_len(4).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        let resized = directory(&dir).unwrap();
        assert_ne!(before, resized, "size alone invalidates");
        file.set_len(3).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(
            before,
            directory(&dir).unwrap(),
            "same metadata identity is restored"
        );
        file.set_times(
            std::fs::FileTimes::new().set_modified(modified + std::time::Duration::from_secs(60)),
        )
        .unwrap();
        let retimed = directory(&dir).unwrap();
        assert_ne!(before, retimed, "mtime alone invalidates");
        drop(file);
        std::fs::remove_file(&path).unwrap();
        assert_ne!(retimed, directory(&dir).unwrap(), "deletion invalidates");
        std::fs::remove_dir_all(dir).unwrap();
    }
}
