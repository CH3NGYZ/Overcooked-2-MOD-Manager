use super::*;
use crate::archive::test_support::write_zip;
use uuid::Uuid;

#[test]
fn downloaded_bytes_require_nonempty_complete_data_and_the_expected_sha256() {
    let digest = format!("sha256:{:x}", Sha256::digest(b"hello"));
    assert!(validate_download_bytes(b"hello", Some(5), Some(&digest)).is_ok());
    assert!(validate_download_bytes(b"HELLO", Some(5), Some(&digest)).is_err());
    assert!(validate_download_bytes(b"hell", Some(5), Some(&digest)).is_err());
    assert!(validate_download_bytes(b"", None, None).is_err());
    assert!(validate_download_bytes(b"hello", Some(5), None).is_ok());
}

#[test]
fn sha256_is_stable() {
    let path = std::env::temp_dir().join("oc2-sha-test.bin");
    fs::write(&path, b"hello").unwrap();
    assert_eq!(
        sha256_file(&path).unwrap(),
        "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
}

#[test]
fn truncated_zip_is_rejected_before_installation() {
    let root = std::env::temp_dir().join(format!("oc2-truncated-zip-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let valid = root.join("valid.zip");
    let truncated = root.join("truncated.zip");
    write_zip(&valid, &[("Plugin.dll", b"plugin")]);
    let bytes = fs::read(&valid).unwrap();
    fs::write(&truncated, &bytes[..bytes.len() - 1]).unwrap();

    assert!(validate_zip(&valid).is_ok());
    assert!(validate_zip(&truncated).is_err());
    fs::remove_dir_all(root).unwrap();
}
