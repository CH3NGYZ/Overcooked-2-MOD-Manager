use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub(super) fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let file = File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for (name, contents) in entries {
        if name.ends_with('/') {
            writer
                .add_directory(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            continue;
        }
        writer
            .start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        writer.write_all(contents).unwrap();
    }
    writer.finish().unwrap();
}

pub(super) fn temp_manifest_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("oc2-{tag}-{}", Uuid::new_v4()))
}

pub(super) fn requires(guid: &str, min: &str) -> crate::models::RepositoryRelation {
    crate::models::RepositoryRelation {
        kind: crate::models::RelationKind::Requires,
        guid: guid.into(),
        version: if min.trim().is_empty() {
            String::new()
        } else {
            format!(">={}", min.trim())
        },
        prerelease: None,
        optional: false,
    }
}
