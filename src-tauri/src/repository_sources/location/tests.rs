use super::super::document::parse_document;
use super::*;

#[test]
fn relative_child_uses_its_parent_file_directory() {
    let local = SourceLocation::parse("region/nested/source.toml").unwrap();
    assert_eq!(
        SourceLocation::resolve("../mods.toml", Some(&local))
            .unwrap()
            .address(),
        "region/mods.toml"
    );
    let remote = SourceLocation::parse("https://example.com/region/nested/source.toml").unwrap();
    assert_eq!(
        SourceLocation::resolve("../mods.toml", Some(&remote))
            .unwrap()
            .address(),
        "https://example.com/region/mods.toml"
    );
    assert_eq!(
        SourceLocation::resolve("mods.toml", Some(&remote))
            .unwrap()
            .address(),
        "https://example.com/region/nested/mods.toml"
    );
}

#[test]
fn bundled_and_raw_plain_paths_use_their_repository_root() {
    let local = SourceLocation::parse("config/nested/index.toml").unwrap();
    assert_eq!(
        SourceLocation::resolve("mods/child.toml", Some(&local))
            .unwrap()
            .address(),
        "mods/child.toml"
    );
    assert_eq!(
        SourceLocation::resolve("./child.toml", Some(&local))
            .unwrap()
            .address(),
        "config/nested/child.toml"
    );
    for prefix in [
        "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/",
        "https://raw.giteeusercontent.com/ch3ngyz/Overcooked-2-MOD-Manager-Releases/raw/main/",
        "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/",
        "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/master/",
        "https://raw.githubusercontent.com/kfyangc/Overcooked2-DIYLevelFastInit/refs/heads/master/",
        "https://raw.githubusercontent.com/kfyangc/Overcooked2-DIYLevelFastInit/master/",
        "https://github.com/owner/repo/raw/refs/tags/v1.0/",
        "https://github.com/owner/repo/raw/refs/heads/feature%2Fmods/",
    ] {
        let parent =
            SourceLocation::parse(&format!("{prefix}config/nested/index.toml?raw=true")).unwrap();
        assert_eq!(
            SourceLocation::resolve("mods/child.toml", Some(&parent))
                .unwrap()
                .address(),
            format!("{prefix}mods/child.toml")
        );
        assert_eq!(
            SourceLocation::resolve("./child.toml", Some(&parent))
                .unwrap()
                .address(),
            format!("{prefix}config/nested/child.toml")
        );
        assert_eq!(
            SourceLocation::resolve("../shared.toml", Some(&parent))
                .unwrap()
                .address(),
            format!("{prefix}config/shared.toml")
        );
        assert_eq!(
            SourceLocation::resolve("/mods/child.toml", Some(&parent))
                .unwrap()
                .address(),
            format!("{prefix}mods/child.toml")
        );
    }
}

#[test]
fn raw_file_paths_cannot_escape_the_repository_ref() {
    let parent =
        SourceLocation::parse("https://gitee.com/owner/repo/raw/main/config/index.toml").unwrap();
    for path in [
        "../../outside.toml",
        "%2e%2e/outside.toml",
        "\\\\other.example.com/outside.toml",
    ] {
        assert!(
            SourceLocation::resolve(path, Some(&parent)).is_err(),
            "{path}"
        );
    }
    assert!(SourceLocation::resolve("https://example.com/other.toml", Some(&parent)).is_ok());
}

#[test]
fn absolute_child_source_switches_the_repository_for_its_descendants() {
    let gitee = SourceLocation::parse("https://gitee.com/owner/repo/raw/main/index.toml").unwrap();
    let parent_document = parse_document(r#"
[[repository_sources]]
id = "github"
nickname = "GitHub"
url = "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/config/index.toml"
"#, &gitee, &Default::default()).unwrap();
    let github = SourceLocation::parse(&parent_document.repository_sources[0].url).unwrap();
    let child_document = parse_document(
        r#"
[[repository_sources]]
id = "mods"
nickname = "Mods"
url = "mods/child.toml"
"#,
        &github,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(
        child_document.repository_sources[0].url,
        "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/mods/child.toml"
    );
    let child = SourceLocation::parse(&child_document.repository_sources[0].url).unwrap();
    assert_eq!(
        SourceLocation::resolve("next.toml", Some(&child))
            .unwrap()
            .address(),
        "https://github.com/kfyangc/Overcooked2-DIYLevelFastInit/raw/refs/heads/master/next.toml"
    );
}

#[test]
fn raw_redirects_keep_repository_context_and_other_direct_links_use_the_destination() {
    let requested =
        Url::parse("https://github.com/owner/repo/raw/refs/heads/master/config/index.toml")
            .unwrap();
    for redirected in [
        "https://raw.githubusercontent.com/owner/repo/refs/heads/master/config/index.toml",
        "https://cdn.example.com/files/index.toml",
    ] {
        let location =
            remote_document_location(&requested, Url::parse(redirected).unwrap()).unwrap();
        assert_eq!(
            SourceLocation::resolve("mods/child.toml", Some(&location))
                .unwrap()
                .address(),
            "https://github.com/owner/repo/raw/refs/heads/master/mods/child.toml"
        );
    }
    let direct_link = Url::parse("https://example.com/download").unwrap();
    let location = remote_document_location(
        &direct_link,
        Url::parse("https://raw.githubusercontent.com/other/repo/main/config/index.toml").unwrap(),
    )
    .unwrap();
    assert_eq!(
        SourceLocation::resolve("mods/child.toml", Some(&location))
            .unwrap()
            .address(),
        "https://raw.githubusercontent.com/other/repo/main/mods/child.toml"
    );
}

#[test]
fn absolute_url_overrides_both_local_and_remote_base() {
    for base in ["index.toml", "https://example.com/index.toml"] {
        let base = SourceLocation::parse(base).unwrap();
        assert_eq!(
            SourceLocation::resolve("https://other.example.com/mods.toml", Some(&base))
                .unwrap()
                .address(),
            "https://other.example.com/mods.toml"
        );
    }
}

#[test]
fn unsafe_local_paths_and_non_http_urls_are_rejected() {
    for reference in [
        "../outside.toml",
        "/outside.toml",
        "C:/outside.toml",
        "file:///outside.toml",
        "ftp://example.com/source.toml",
    ] {
        assert!(SourceLocation::parse(reference).is_err(), "{reference}");
    }
    assert!(
        SourceLocation::parse("not-a-bundled-source.toml")
            .unwrap()
            .bundled_document()
            .is_err()
    );
}
