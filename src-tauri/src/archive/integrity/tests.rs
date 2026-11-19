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
fn release_digest_requires_sha256_hex_and_compares_case_insensitively() {
    let hex = format!("{:x}", Sha256::digest(b"hello"));
    let upper_hex = hex.to_ascii_uppercase();
    assert_eq!(normalize_sha256(&hex), Some(hex.as_str()));
    let api_digest = format!(" SHA256:{upper_hex} ");
    assert_eq!(normalize_sha256(&api_digest), Some(upper_hex.as_str()));
    assert!(validate_download_digest(&hex, &api_digest).is_ok());
    assert!(validate_download_bytes(b"hello", None, Some(&api_digest)).is_ok());

    let fake = format!("sha512:{hex}");
    let nonhex = format!("sha256:g{}", &hex[1..]);
    for malformed in ["", &hex[..63], &fake, &nonhex, "sha256:abc"] {
        assert_eq!(normalize_sha256(malformed), None);
        assert!(validate_download_digest(&hex, malformed).is_err());
        assert!(validate_download_bytes(b"hello", None, Some(malformed)).is_err());
    }
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

#[test]
fn zip_validation_can_cancel_inside_one_large_entry() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("single-entry.zip");
    let payload = vec![0x7b; 256 * 1024];
    write_zip(&path, &[("Large.dat", &payload)]);

    let checks = std::cell::Cell::new(0);
    let result = validate_zip_with_cancel(&path, || {
        checks.set(checks.get() + 1);
        // A single entry cannot reach the fifth check without chunked reads.
        checks.get() >= 5
    });
    assert_eq!(result.unwrap_err(), "download.cancelled");

    // The fixture is valid and ordinary archive validation still completes.
    assert!(validate_zip(&path).is_ok());
}
