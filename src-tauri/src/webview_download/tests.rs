use super::*;

fn request() -> Request {
    Request {
        url: Url::parse("https://gitee.com/owner/repo/repository/archive/main.zip").unwrap(),
        user_agent: "Overcooked 2 MOD Manager/2.1.1".into(),
        max_bytes: 32 * 1024 * 1024,
        title: "Download".into(),
        timeout: Duration::from_secs(120),
        allowed_download_hosts: vec!["gitee.com".into()],
        allowed_download_paths: vec!["/owner/repo/repository/download".into()],
    }
}

#[test]
fn navigation_allows_initial_origin_and_explicit_trusted_https_hosts() {
    let policy = Policy::new(&request()).unwrap();
    assert!(policy.navigation(&Url::parse("https://gitee.com/verify?ticket=opaque").unwrap()));
    assert!(policy.navigation(&Url::parse("https://dl.gitee.com/verify?ticket=opaque").unwrap()));
    for candidate in [
        "http://gitee.com/",
        "https://gitee.com:444/",
        "https://user:secret@gitee.com/",
        "https://gitee.com.evil.test/",
        "file:///tmp/archive.zip",
        "about:blank",
    ] {
        assert!(
            !policy.navigation(&Url::parse(candidate).unwrap()),
            "{candidate}"
        );
    }
}

#[test]
fn navigation_follows_gitcode_and_obs_only_through_configured_https_hosts() {
    let mut value = request();
    value.url = Url::parse("https://raw.gitcode.com/owner/repo/archive/main.zip").unwrap();
    value.allowed_download_hosts = vec![
        "gitcode.com".into(),
        "huaweicloud.com".into(),
        "myhuaweicloud.com".into(),
    ];
    value.allowed_download_paths.clear();
    let policy = Policy::new(&value).unwrap();
    for candidate in [
        "https://raw.gitcode.com/verify?ticket=opaque",
        "https://gitcode.com/verify?ticket=opaque",
        "https://download.gitcode.com/archive.zip?signature=opaque",
        "https://bucket.obs.huaweicloud.com/archive.zip?signature=opaque",
        "https://bucket.obs.myhuaweicloud.com/archive.zip?signature=opaque",
    ] {
        assert!(
            policy.navigation(&Url::parse(candidate).unwrap()),
            "{candidate}"
        );
    }
    for candidate in [
        "https://evilgitcode.com/verify",
        "https://gitcode.com.evil.test/verify",
        "https://huaweicloud.com.evil.test/archive.zip",
        "https://evilmyhuaweicloud.com/archive.zip",
        "https://unlisted.example/archive.zip",
        "http://gitcode.com/verify",
        "http://bucket.obs.huaweicloud.com/archive.zip",
        "https://user@gitcode.com/verify",
        "https://user@bucket.obs.myhuaweicloud.com/archive.zip",
        "https://gitcode.com:444/verify",
        "https://bucket.obs.huaweicloud.com:8443/archive.zip",
    ] {
        assert!(
            !policy.navigation(&Url::parse(candidate).unwrap()),
            "{candidate}"
        );
    }
    assert!(policy.download(&value.url));
    assert!(!policy.download(&Url::parse("https://raw.gitcode.com/other/archive.zip").unwrap()));
    value.allowed_download_hosts.clear();
    let policy = Policy::new(&value).unwrap();
    assert!(!policy.navigation(&Url::parse("https://gitcode.com/verify").unwrap()));
}

#[test]
fn requested_downloads_require_trusted_https_and_reject_unrelated_origins() {
    let policy = Policy::new(&request()).unwrap();
    assert!(policy.download(&request().url));
    assert!(policy.download(
        &Url::parse("https://gitee.com/owner/repo/repository/download?ticket=opaque").unwrap()
    ));
    assert!(
        policy.download(
            &Url::parse("https://gitee.com/owner/repo/repository/archive/main.zip?ticket=opaque")
                .unwrap()
        )
    );
    assert!(policy.download(
        &Url::parse("https://download.gitee.com/signed/archive.zip?signature=opaque").unwrap()
    ));
    for candidate in [
        "https://untrusted.example/other.zip",
        "https://gitee.com/other/repo/repository/download?ticket=opaque",
        "https://gitee.com/owner/repo/repository/download/",
        "https://gitee.com/owner/repo/repository/other.zip",
        "http://download.gitee.com/archive.zip",
        "https://gitee.com.evil.test/archive.zip",
        "https://evilgitee.com/archive.zip",
        "https://user@download.gitee.com/archive.zip",
        "https://download.gitee.com:8443/archive.zip",
        "data:application/zip,bytes",
    ] {
        assert!(
            !policy.download(&Url::parse(candidate).unwrap()),
            "{candidate}"
        );
    }
}

#[test]
fn trusted_suffixes_respect_dns_labels_and_include_apex() {
    assert!(host_matches("a.b.gitee.com", "gitee.com"));
    assert!(host_matches("gitee.com", "gitee.com"));
    assert!(!host_matches("evilgitee.com", "gitee.com"));
    assert!(!host_matches("gitee.com.evil.test", "gitee.com"));
    assert_eq!(normalize_host("GITEE.COM").unwrap(), "gitee.com");
    for host in [
        "",
        "*gitee.com",
        "gitee.com/path",
        "gitee.com:443",
        "gitee.com.",
        "127.0.0.1",
    ] {
        assert!(normalize_host(host).is_err(), "{host}");
    }
}

#[test]
fn invalid_limits_credentials_and_user_agents_fail_before_window_creation() {
    let mut value = request();
    value.max_bytes = 0;
    assert!(Policy::new(&value).is_err());
    value = request();
    value.timeout = Duration::ZERO;
    assert!(Policy::new(&value).is_err());
    value = request();
    value.user_agent = "agent\r\nInjected: value".into();
    assert!(Policy::new(&value).is_err());
    value = request();
    value.url = Url::parse("https://user:secret@gitee.com/archive.zip").unwrap();
    assert!(Policy::new(&value).is_err());
    value = request();
    value.allowed_download_hosts.clear();
    let policy = Policy::new(&value).unwrap();
    assert!(policy.download(&value.url));
    assert!(!policy.download(&Url::parse("https://download.gitee.com/archive.zip").unwrap()));
}

#[test]
fn callback_accepts_only_first_matching_request_and_its_completion() {
    let state = DownloadState::new();
    let policy = Policy::new(&request()).unwrap();
    let url = request().url;
    let unrelated = Url::parse("https://untrusted.example/unrelated.zip").unwrap();
    assert!(!state.requested(&unrelated, &policy));
    state.finished(&url, true);
    assert!(matches!(*state.phase.borrow(), Phase::Waiting));
    assert!(state.requested(&url, &policy));
    assert!(!state.requested(&url, &policy));
    state.finished(&unrelated, true);
    assert!(matches!(*state.phase.borrow(), Phase::Downloading(_)));
    state.finished(&url, true);
    assert!(matches!(*state.phase.borrow(), Phase::Finished(true)));
    state.finished(&url, false);
    assert!(matches!(*state.phase.borrow(), Phase::Finished(true)));
    assert!(!state.requested(&url, &policy));
}

#[test]
fn signature_mismatch_and_failed_completion_never_publish_success() {
    let state = DownloadState::new();
    let policy = Policy::new(&request()).unwrap();
    let url = Url::parse("https://download.gitee.com/archive.zip?signature=first").unwrap();
    assert!(state.requested(&url, &policy));
    state.finished(
        &Url::parse("https://download.gitee.com/archive.zip?signature=other").unwrap(),
        true,
    );
    assert!(matches!(*state.phase.borrow(), Phase::Downloading(_)));
    state.finished(&url, false);
    assert!(matches!(*state.phase.borrow(), Phase::Finished(false)));
    state.finished(&url, true);
    assert!(matches!(*state.phase.borrow(), Phase::Finished(false)));
}

#[test]
fn cancellation_wins_before_request_during_download_and_after_finish() {
    let policy = Policy::new(&request()).unwrap();
    let url = request().url;
    for stage in 0..3 {
        let state = DownloadState::new();
        if stage > 0 {
            assert!(state.requested(&url, &policy));
        }
        if stage > 1 {
            state.finished(&url, true);
        }
        state.cancel();
        state.cancel();
        assert!(!state.requested(&url, &policy));
        state.finished(&url, true);
        assert!(matches!(*state.phase.borrow(), Phase::Cancelled));
    }
}

#[test]
fn incremental_limit_accepts_exact_boundary_and_rejects_without_mutation() {
    let mut bytes = vec![0, 1];
    extend_bounded(&mut bytes, &[2, 255], 4).unwrap();
    assert_eq!(bytes, vec![0, 1, 2, 255]);
    assert_eq!(extend_bounded(&mut bytes, &[3], 4).unwrap_err(), TOO_LARGE);
    assert_eq!(bytes, vec![0, 1, 2, 255]);
    extend_bounded(&mut bytes, &[], 4).unwrap();
}

#[test]
fn download_paths_are_exact_normalized_and_optional() {
    let mut value = request();
    value.allowed_download_paths.clear();
    let policy = Policy::new(&value).unwrap();
    assert!(policy.download(&value.url));
    assert!(!policy.download(
        &Url::parse("https://gitee.com/owner/repo/repository/download?ticket=opaque").unwrap()
    ));
    value.allowed_download_paths = vec!["/owner/repo/repository/./download".into()];
    let policy = Policy::new(&value).unwrap();
    assert_eq!(policy.paths, vec!["/owner/repo/repository/download"]);
    assert!(policy.download(
        &Url::parse("https://gitee.com/owner/repo/repository/download?ticket=opaque").unwrap()
    ));
    for path in [
        "",
        "repository/download",
        "//other.example/download",
        "/repository/download?ticket=opaque",
        "/repository/download#fragment",
        "/repository/download endpoint",
        "https://other.example/download",
    ] {
        value.allowed_download_paths = vec![path.into()];
        assert!(Policy::new(&value).is_err(), "{path}");
    }
    for character in [char::from(92), char::from(10), char::from(0)] {
        value.allowed_download_paths = vec![format!("/owner/{character}repo/download")];
        assert!(Policy::new(&value).is_err());
    }
}
