//! Reject Release contents that cannot justify retiring proven duplicates.
use super::*;

fn assert_replacement_payload_rejected(version: &str, extra_guid: Option<&str>) {
    use crate::mod_preview::staging::{ORIGIN_FILE, PreviewOriginRecord, preview_root};
    use std::io::Write;

    const ASSET: &str = "FastInit.zip";
    let retired_guid = extra_guid.unwrap_or(GUID);
    let f = fastinit_release_fixture(ASSET, retired_guid, retired_guid, retired_guid);
    let before = game_and_manifest_bytes(&f);
    let plan = planning::create(&f.state, request(FASTINIT, Some(TARGET), true)).unwrap();
    let session = take(&f, &plan);
    let candidate = session
        .solved
        .as_ref()
        .unwrap()
        .nodes
        .iter()
        .find(|node| node.id == NODE)
        .and_then(|node| node.current.as_ref())
        .unwrap();
    let preview_id = uuid::Uuid::new_v4().to_string();
    let path = preview_root(&f.state).join(&preview_id);
    fs::create_dir_all(&path).unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(path.join(ASSET)).unwrap());
    let members = std::iter::once(("FastInit.dll", GUID, version))
        .chain(extra_guid.map(|guid| ("LegacyFastInit.dll", guid, "1.3")));
    for (file, guid, installed_version) in members {
        zip.start_file(file, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&mods::plugin_blob(guid, FASTINIT, installed_version))
            .unwrap();
    }
    zip.finish().unwrap();
    write(
        &path.join(ORIGIN_FILE),
        &serde_json::to_vec(&PreviewOriginRecord {
            repository_id: FASTINIT.into(),
            release_tag: TARGET.into(),
            prerelease: false,
        })
        .unwrap(),
    );
    let result = crate::mod_changes::payload::from_preview(
        &f.state,
        &session.trusted,
        &preview_id,
        NODE,
        candidate,
        None,
        None,
        &session.staging.join("bad-payload"),
    );
    assert_eq!(
        result.err().as_deref(),
        Some("modChanges.duplicateReleasePayload"),
        "asset must not authorize retirement: version={version}, extra_guid={extra_guid:?}"
    );
    assert_eq!(game_and_manifest_bytes(&f), before);
}

#[test]
fn newer_release_tag_does_not_authorize_retirement_when_actual_dll_is_older() {
    assert_replacement_payload_rejected("1.9.4", None);
}

#[test]
fn release_payload_requires_a_unique_core_dll() {
    assert_replacement_payload_rejected("1.9.6", Some(GUID));
}

#[test]
#[cfg(windows)]
fn release_payload_cannot_reintroduce_a_retired_guid_as_a_second_dll() {
    assert_replacement_payload_rejected("1.9.6", Some(OLD_GUID));
}
