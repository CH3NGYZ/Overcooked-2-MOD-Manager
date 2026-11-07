use super::*;

#[test]
fn zip_file_names_carry_the_version() {
    assert_eq!(version_from_file_name("gaoda"), "");
    assert_eq!(version_from_file_name("gaoda-1.2.0"), "1.2.0");
    assert_eq!(version_from_file_name("GAODA_1.0.3_beta"), "1.0.3");
    assert_eq!(version_from_file_name("地图-3.4"), "3.4");
    assert_eq!(version_from_file_name("v2"), "2");
    assert_eq!(version_from_file_name("gaoda..1.2."), "1.2");
    // 只有日期的名字本来就没有版本：取到 04，界面让作者改。
    assert_eq!(version_from_file_name("2026-11-04"), "04");
}

#[test]
fn a_staged_zip_is_copied_under_the_release_name() {
    let root = std::env::temp_dir().join(format!("oc2-publish-stage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("我的 地图-v1.zip");
    std::fs::write(&source, b"zip-bytes").unwrap();
    let staging = root.join("staging");
    let staged =
        stage_publish_zip_into(&staging, source.to_str().unwrap(), "gaoda_v1.2.0.zip").unwrap();
    assert_eq!(
        std::path::Path::new(&staged)
            .file_name()
            .and_then(|name| name.to_str()),
        Some("gaoda_v1.2.0.zip")
    );
    assert_eq!(std::fs::read(&staged).unwrap(), b"zip-bytes");
    // 用户手里的原文件原样留着
    assert!(source.is_file());
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn staging_rejects_anything_that_is_not_a_readable_zip() {
    let root = std::env::temp_dir().join(format!("oc2-publish-stage-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let text = root.join("notes.txt");
    std::fs::write(&text, b"nope").unwrap();
    // 附件名必须是 ZIP
    assert!(stage_publish_zip_into(&root, text.to_str().unwrap(), "gaoda_v1.0.0.txt").is_err());
    // 源文件本身也必须是 ZIP
    assert!(stage_publish_zip_into(&root, text.to_str().unwrap(), "gaoda_v1.0.0.zip").is_err());
    // 不存在的源文件
    assert!(stage_publish_zip_into(&root, "", "gaoda_v1.0.0.zip").is_err());
    std::fs::remove_dir_all(&root).unwrap();
}
