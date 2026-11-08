use super::*;

/*
 * 本地版本落在哪个渠道：按「是否预发布」判断，不再数版本段数。
 *
 * 段数是个间接指标，遇到 `1.8.7.beta4` 这类写法就会错判（按 `.`
 * 数出 4 段，其实是三段 + 标识）。语义判定统一交给
 * `releases::is_prerelease_version`，它认得本项目全部 beta 写法。
 */
#[test]
fn local_release_channel_follows_prerelease_semantics() {
    use crate::releases::is_prerelease_version;
    // 三段正式版
    assert!(!is_prerelease_version("1.8.5"));
    assert!(!is_prerelease_version("v1.8.5"));
    // 四段 = beta 序号
    assert!(is_prerelease_version("1.8.5.2"));
    assert!(is_prerelease_version("v1.8.5.2-beta"));
    // 其它 beta 写法：无分隔符、点号分隔、pre 前缀
    assert!(is_prerelease_version("1.8.7-beta1"));
    assert!(is_prerelease_version("1.8.7.beta4"));
    assert!(is_prerelease_version("1.9.7-beta.4"));
    assert!(is_prerelease_version("1.8.5-pre.01"));
}

#[test]
fn repository_binding_matches_any_configured_identity() {
    let repo = binding_repository();
    assert!(super::repository_matches_mod(
        &repo,
        Some("different"),
        Some("example product"),
        None,
        None
    ));
    assert!(super::repository_matches_mod(
        &repo,
        None,
        None,
        Some("EXAMPLE.DLL"),
        None
    ));
    assert!(super::repository_matches_mod(
        &repo,
        Some("COM.EXAMPLE.GUID"),
        None,
        None,
        None
    ));
    let mut versioned = repo.clone();
    versioned.version_regex = r"^1\.2\.\d+$".into();
    assert!(super::repository_matches_mod(
        &versioned,
        Some("com.example.guid"),
        None,
        None,
        Some("1.2.3")
    ));
    assert!(!super::repository_matches_mod(
        &versioned,
        Some("com.example.guid"),
        None,
        None,
        Some("1.3.0")
    ));
}

#[test]
fn failed_release_source_does_not_rebind_to_another_cached_repository() {
    let mut source = binding_repository();
    source.id = "source".into();
    source.channel = crate::models::Channel::Stable;
    source.cached_releases = vec![high_cached_release()];
    source.last_error = Some("Release request failed".into());
    let mut mirror = source.clone();
    mirror.id = "mirror".into();
    mirror.last_error = None;
    let selected =
        super::select_repository(&[&mirror, &source], Some("1.2.3"), Some(&source)).unwrap();
    assert_eq!(selected.id, "source");
    assert_eq!(
        super::repository_update_state(Some(selected), Some("1.2.3")),
        (false, "repository_unavailable")
    );
}

#[test]
fn raw_binding_uses_identity_and_version_regex_without_release_comparison() {
    let mut raw = binding_repository();
    raw.download_mode = crate::models::ModDownloadMode::RepositoryFile;
    raw.cached_releases = vec![high_cached_release()];
    for (guid, product, filename) in [
        (Some("COM.EXAMPLE.GUID"), None, None),
        (None, Some("example product"), None),
        (None, None, Some("EXAMPLE.DLL")),
    ] {
        assert!(super::repository_matches_mod(
            &raw,
            guid,
            product,
            filename,
            Some("1.2.3.4")
        ));
    }
    assert_eq!(super::repository_remote_version(&raw), None);
    assert_eq!(
        super::repository_update_state(Some(&raw), Some("1.2.3.4")),
        (false, "not_release_asset")
    );
    raw.version_regex = r"^1\.2\.\d+\.\d+$".into();
    assert!(super::repository_matches_mod(
        &raw,
        Some("com.example.guid"),
        None,
        None,
        Some("1.2.3.4")
    ));
    assert!(!super::repository_matches_mod(
        &raw,
        Some("com.example.guid"),
        None,
        None,
        Some("1.3.0.0")
    ));
    assert!(!super::repository_matches_mod(
        &raw,
        Some("com.example.guid"),
        None,
        None,
        None
    ));
    assert!(!super::repository_matches_mod(
        &raw,
        Some("unknown.guid"),
        None,
        None,
        Some("1.2.3.4")
    ));
}

#[test]
fn real_raw_source_wins_while_release_sources_keep_their_channel_rules() {
    let mut release = binding_repository();
    release.id = "release".into();
    release.channel = crate::models::Channel::Stable;
    let mut raw = binding_repository();
    raw.id = "raw".into();
    raw.download_mode = crate::models::ModDownloadMode::RepositoryFile;
    // Raw selection ignores its unused channel field, including four-part versions.
    raw.channel = crate::models::Channel::Stable;
    assert_eq!(
        super::select_repository(&[&release, &raw], Some("1.2.3"), Some(&raw))
            .unwrap()
            .id,
        "raw"
    );
    assert_eq!(
        super::select_repository(&[&release, &raw], Some("1.2.3"), Some(&release))
            .unwrap()
            .id,
        "release"
    );
    assert!(super::select_repository(&[&release], Some("1.2.3"), Some(&raw)).is_none());
    assert_eq!(
        super::select_repository(&[&release, &raw], Some("1.2.3.4"), None)
            .unwrap()
            .id,
        "raw"
    );
    release.channel = crate::models::Channel::Beta;
    assert_eq!(
        super::select_repository(&[&release, &raw], Some("1.2.3.4"), None)
            .unwrap()
            .id,
        "release"
    );
    assert!(super::select_repository(&[], Some("1.2.3.4"), None).is_none());
    assert_eq!(
        super::repository_update_state(None, Some("1.2.3")),
        (false, "unbound")
    );
    raw.channel = crate::models::Channel::All;
    assert_eq!(
        super::select_repository(&[&raw], Some("1.2.3.4"), None)
            .unwrap()
            .id,
        "raw"
    );
}

#[test]
fn dependency_inherits_repository_from_sibling_in_same_directory() {
    /*
     * 用户手动把 ZIP 解到 plugins/SomeMod/：主插件能读 GUID 并绑到仓库，
     * 依赖读不出元数据。依赖必须继承同目录的仓库归属，否则前端会把它
     * 分到「本地安装」分组、永远折不成同一个包，仓库更新时也不会一起回收。
     */
    let mut mods = vec![
        plain_mod(
            "BepInEx/plugins/SomeMod/Main.dll",
            serde_json::json!({
                "repository_id": "repo-1", "repository_ids": ["repo-1"], "guid": "com.example.main"
            }),
        ),
        plain_mod(
            "BepInEx/plugins/SomeMod/Dep.dll",
            serde_json::json!({ "is_dependency": true, "labels": ["dependency"] }),
        ),
    ];
    super::inherit_repository_from_sibling(&mut mods);
    assert_eq!(mods[1].repository_ids, vec!["repo-1".to_string()]);
    assert_eq!(mods[1].repository_id.as_deref(), Some("repo-1"));
    // 主插件自身不变
    assert_eq!(mods[0].repository_ids, vec!["repo-1".to_string()]);
}

#[test]
fn repository_identity_never_leaks_across_directories() {
    /*
     * plugins 根目录下平铺的两个插件互不相干，不能因为「同目录」就把
     * 一个的仓库身份传给另一个。
     */
    let mut mods = vec![
        plain_mod(
            "BepInEx/plugins/Main.dll",
            serde_json::json!({ "repository_id": "repo-1", "repository_ids": ["repo-1"], "guid": "com.example.main" }),
        ),
        plain_mod(
            "BepInEx/plugins/Other.dll",
            serde_json::json!({ "is_dependency": true, "labels": ["dependency"] }),
        ),
    ];
    super::inherit_repository_from_sibling(&mut mods);
    assert!(mods[1].repository_ids.is_empty(), "跨目录不传播仓库身份");
    assert!(mods[1].repository_id.is_none());
}

#[test]
fn dependency_keeps_its_own_repository_identity() {
    // 依赖自己已经有归属时不能被同目录其它插件覆盖
    let mut mods = vec![
        plain_mod(
            "BepInEx/plugins/Pack/Main.dll",
            serde_json::json!({ "repository_id": "repo-1", "repository_ids": ["repo-1"], "guid": "com.example.main" }),
        ),
        plain_mod(
            "BepInEx/plugins/Pack/Dep.dll",
            serde_json::json!({ "repository_id": "repo-2", "repository_ids": ["repo-2"], "is_dependency": true }),
        ),
    ];
    super::inherit_repository_from_sibling(&mut mods);
    assert_eq!(mods[1].repository_ids, vec!["repo-2".to_string()]);
}
