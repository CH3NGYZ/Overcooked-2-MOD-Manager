use super::*;

#[test]
fn slugs_reject_path_tricks() {
    assert!(valid_slug("CH3NGYZ"));
    assert!(valid_slug("oc2-map.v2"));
    assert!(!valid_slug(""));
    assert!(!valid_slug("../etc"));
    assert!(!valid_slug("owner/repo"));
    assert!(!valid_slug(".hidden"));
    assert!(!valid_slug("名字"));
}

#[test]
fn tags_are_map_ids_that_only_need_to_be_git_refs() {
    // tag 是地图身份：chan、中文名都行，同名 tag 下换附件就是发新版。
    assert_eq!(validate_tag(" chan ").unwrap(), "chan");
    assert_eq!(validate_tag("大厨-合集").unwrap(), "大厨-合集");
    assert_eq!(validate_tag("1.2.0-beta.1").unwrap(), "1.2.0-beta.1");
    // 以前被当成「版本号必须无 v 前缀」拦掉的值，现在都是合法的地图 ID。
    assert_eq!(validate_tag("v1.2.0").unwrap(), "v1.2.0");
    assert!(validate_tag("").is_err());
    assert!(validate_tag("a b").is_err());
    assert!(validate_tag("-v1").is_err());
    assert!(validate_tag("1.2.0/..").is_err());
    assert!(validate_tag("maps/chan").is_err());
    assert!(validate_tag("a..b").is_err());
    assert!(validate_tag("a:b").is_err());
    assert!(validate_tag("x.lock").is_err());
}

#[test]
fn asset_names_keep_unicode_but_reject_path_and_reserved_characters() {
    assert_eq!(
        validate_asset_name("gaoda-1.2.0.zip").unwrap(),
        "gaoda-1.2.0.zip"
    );
    // 作者的中文文件名原样上传；进 URL 时由 encode_component 按 UTF-8 字节编码。
    assert_eq!(validate_asset_name(" 地图.zip ").unwrap(), "地图.zip");
    assert_eq!(encode_component("地图.zip"), "%E5%9C%B0%E5%9B%BE.zip");
    for bad in [
        "",
        ".hidden",
        "a/b.zip",
        "a\\b.zip",
        "a:b.zip",
        "a?b.zip",
        "a|b.zip",
        "a\u{7}.zip",
        "map.",
    ] {
        assert!(validate_asset_name(bad).is_err(), "{bad:?}");
    }
    assert!(validate_asset_name(&"a".repeat(201)).is_err());
}

#[test]
fn asset_name_encoding_escapes_reserved_characters() {
    assert_eq!(encode_component("a b.zip"), "a%20b.zip");
    assert_eq!(encode_component("a&b"), "a%26b");
}
