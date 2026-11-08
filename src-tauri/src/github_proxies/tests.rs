use super::*;

#[test]
fn explicit_empty_manifests_are_valid_but_missing_or_misspelled_fields_are_not() {
    for text in [
        "schema_version = 1",
        "release_proxies = []",
        "avatar_proxies = []",
        "gitcode_api_proxies = []",
    ] {
        assert_eq!(parse_manifest(text).unwrap(), GitHubProxyConfig::default());
    }
    for text in [
        "",
        "# empty",
        "unrelated = true",
        "schema_version = 1\nrelease_proxy = []",
        "schema_version = 2",
        "<html>blocked</html>",
    ] {
        assert!(parse_manifest(text).is_err(), "{text}");
    }
    assert!(parse_manifest(&" ".repeat(1024 * 1024 + 1)).is_err());
    for template in [
        "http://proxy.example/{url}",
        "https://user@proxy.example/{url}",
        "https://proxy.example/{unknown}",
    ] {
        let text =
            format!("[[release_proxies]]\nid = 'test'\nname = 'Test'\ntemplate = '{template}'");
        assert!(parse_manifest(&text).is_err());
    }
}

#[test]
fn gitcode_api_proxy_keeps_api_path_and_pagination_in_its_own_group() {
    let bundled = bundled_defaults();
    let proxies = &bundled.gitcode_api_proxies;
    assert_eq!(proxies.len(), 1);
    assert_eq!(proxies[0].id, "oc2gcapi");
    let official =
        "https://api.atomgit.com/api/v5/repos/CH3NGY2/OC2-Map-Bundle/releases?per_page=50&page=2";
    assert_eq!(
        proxy_urls(proxies, official, false, None),
        [
            "https://oc2gcapi.ch3ng.top/api/v5/repos/CH3NGY2/OC2-Map-Bundle/releases?per_page=50&page=2"
        ]
    );
    let mut config = crate::config::default_config();
    config.github_proxies = bundled;
    assert_eq!(
        crate::providers::api_proxies(&config, &crate::models::Provider::Gitcode)[0].id,
        "oc2gcapi"
    );
    let invalid =
        "[[gitcode_api_proxies]]\nid='bad'\nname='Bad'\ntemplate='http://proxy.example/{url}'";
    assert!(parse_manifest(invalid).is_err());
}

#[test]
fn raw_host_replacement_preserves_encoding_query_and_qualified_refs() {
    let make = |template: &str| GitHubProxy {
        id: "test".into(),
        name: "Test".into(),
        template: template.into(),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    };
    let paths = [make("https://proxy.example/{url_path}")];
    for prefix in [
        "https://raw.githubusercontent.com/raw/tool/main/",
        "https://raw.githubusercontent.com/raw/tool/refs/heads/main/",
        "https://github.com/raw/tool/raw/main/",
        "https://github.com/raw/tool/raw/refs/heads/main/",
    ] {
        let target = format!("{prefix}bin/My%20MOD%25%E5%9C%B0.dll?raw=1&x=a%2Fb#fragment");
        assert_eq!(
            proxy_urls(&paths, &target, true, None),
            [
                "https://proxy.example/raw/tool/raw/refs/heads/main/bin/My%20MOD%25%E5%9C%B0.dll?raw=1&x=a%2Fb"
            ]
        );
        assert_eq!(
            proxy_urls(&[make("https://proxy.example/{url}")], &target, true, None),
            [
                "https://proxy.example/https://raw.githubusercontent.com/raw/tool/main/bin/My%20MOD%25%E5%9C%B0.dll?raw=1&x=a%2Fb"
            ]
        );
    }
    for reference in ["refs/tags/v1", "0123456789012345678901234567890123456789"] {
        for target in [
            format!("https://raw.githubusercontent.com/acme/tool/{reference}/mod.dll"),
            format!("https://github.com/acme/tool/raw/{reference}/mod.dll"),
        ] {
            assert_eq!(
                proxy_urls(&paths, &target, true, None),
                [format!(
                    "https://proxy.example/acme/tool/raw/{reference}/mod.dll"
                )]
            );
        }
    }
    assert_eq!(
        proxy_urls(
            &paths,
            "https://github.com/acme/tool/raw/main/old.dll",
            true,
            Some(RawProxyContext {
                branch: "release/1.x",
                file_path: "bin/My MOD%地.dll"
            })
        ),
        ["https://proxy.example/acme/tool/raw/refs/heads/release/1.x/bin/My%20MOD%25%E5%9C%B0.dll"]
    );
}

#[test]
fn manifest_deduplicates_templates_and_merges_regions() {
    let manifest = r#"
schema_version = 1
[[release_proxies]]
id = "proxy-us"
name = "Proxy"
template = "https://proxy.example/{url}"
regions = ["US"]
[[release_proxies]]
id = "proxy-hk"
name = "Proxy HK"
template = "https://proxy.example/{url}"
regions = ["Hong Kong"]
"#;
    let parsed = parse_manifest(manifest).unwrap();
    assert_eq!(parsed.release_proxies.len(), 1);
    assert_eq!(parsed.release_proxies[0].regions, ["US", "Hong Kong"]);
    assert!(parsed.release_proxies[0].builtin);
    assert!(!parsed.release_proxies[0].remote);
}

#[test]
fn formats_raw_and_release_templates_without_expression_evaluation() {
    let target = "https://github.com/acme/tool/raw/refs/heads/main/bin/tool.dll";
    let proxies = vec![GitHubProxy {
        id: "test".into(),
        name: "Test".into(),
        template: "https://proxy.example/{owner}/{repo}/{branch}/{path}?source={url}".into(),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];
    assert_eq!(
        proxy_urls(&proxies, target, true, None),
        [
            "https://proxy.example/acme/tool/main/bin/tool.dll?source=https://raw.githubusercontent.com/acme/tool/main/bin/tool.dll"
        ]
    );
    let bad = GitHubProxy {
        template: "https://proxy.example/{expression}".into(),
        ..proxies[0].clone()
    };
    assert!(normalize_proxy(&mut bad.clone()).is_err());
}

#[test]
fn api_proxy_can_replace_gitee_host_with_url_path() {
    let proxies = vec![GitHubProxy {
        id: "gitee-api".into(),
        name: "Gitee API".into(),
        template: "https://oc2gtapi.example{url_path}".into(),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];
    assert_eq!(
        proxy_urls(
            &proxies,
            "https://gitee.com/api/v5/repos/acme/tool/releases?per_page=10",
            false,
            None,
        ),
        ["https://oc2gtapi.example/api/v5/repos/acme/tool/releases?per_page=10"]
    );
}

#[test]
fn api_proxy_can_replace_github_host_with_url_path() {
    let proxies = vec![GitHubProxy {
        id: "github-api".into(),
        name: "GitHub API".into(),
        template: "https://oc2ghapi.example{url_path}".into(),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];
    assert_eq!(
        proxy_urls(
            &proxies,
            "https://api.github.com/repos/sinai-dev/UnityExplorer/releases?per_page=10&direction=desc&sort=created",
            false,
            None,
        ),
        [
            "https://oc2ghapi.example/repos/sinai-dev/UnityExplorer/releases?per_page=10&direction=desc&sort=created"
        ]
    );
}

#[test]
fn raw_proxy_uses_configured_branch_and_path_including_branch_slashes() {
    let target = "https://github.com/acme/tool/raw/refs/heads/release/1.x/old.dll";
    let proxies = vec![GitHubProxy {
        id: "test".into(),
        name: "Test".into(),
        template: "https://cdn.example/gh/{owner}/{repo}@{branch}/{path}?source={url}".into(),
        regions: vec![],
        description: String::new(),
        enabled: true,
        builtin: false,
        remote: false,
    }];
    let urls = proxy_urls(
        &proxies,
        target,
        true,
        Some(RawProxyContext {
            branch: "release/1.x",
            file_path: "bin/Release/My MOD.dll",
        }),
    );
    assert_eq!(
        urls,
        [
            "https://cdn.example/gh/acme/tool@release/1.x/bin/Release/My%20MOD.dll?source=https://raw.githubusercontent.com/acme/tool/release/1.x/bin/Release/My%20MOD.dll"
        ]
    );
}

#[test]
fn raw_url_parts_are_derived_from_both_raw_entry_shapes() {
    // raw.githubusercontent 的路径里没有 `raw` 段，分支紧跟在 owner/repo 之后
    assert_eq!(
        raw_branch_and_path("https://raw.githubusercontent.com/acme/tool/main/bin/mod.dll"),
        ("main".to_string(), "bin/mod.dll".to_string())
    );
    // github / gitee 的仓库 raw 入口，含 refs/heads 写法
    assert_eq!(
        raw_branch_and_path("https://github.com/acme/tool/raw/dev/maps/plus.zip"),
        ("dev".to_string(), "maps/plus.zip".to_string())
    );
    assert_eq!(
        raw_branch_and_path("https://gitee.com/acme/tool/raw/refs/heads/main/a.zip"),
        ("main".to_string(), "a.zip".to_string())
    );
    // 不是 raw 入口、也不是合法 URL 时给空值，由调用方决定怎么处理
    assert_eq!(
        raw_branch_and_path("https://github.com/acme/tool"),
        (String::new(), String::new())
    );
    assert_eq!(
        raw_branch_and_path("not a url"),
        (String::new(), String::new())
    );
}

#[test]
fn bundled_url_path_proxies_render_one_slash_and_request_each_target_once() {
    let bundled = bundled_defaults();
    for (proxies, target, is_raw, expected) in [
        (
            &bundled.release_proxies,
            "https://github.com/acme/tool/releases/download/v1/tool.zip",
            false,
            "https://oc2gh.ch3ng.top/acme/tool/releases/download/v1/tool.zip",
        ),
        (
            &bundled.raw_proxies,
            "https://github.com/acme/tool/raw/refs/heads/main/bin/tool.dll",
            true,
            "https://oc2gh.ch3ng.top/acme/tool/raw/refs/heads/main/bin/tool.dll",
        ),
    ] {
        let source = proxies
            .iter()
            .find(|proxy| proxy.name == "oc2gh.ch3ng.top")
            .unwrap()
            .clone();
        assert_eq!(source.template, "https://oc2gh.ch3ng.top/{url_path}");
        let duplicate = GitHubProxy {
            id: "other-id".into(),
            template: source.template.replace("/{url_path}", "{url_path}"),
            ..source.clone()
        };
        assert_eq!(
            proxy_urls(&[source, duplicate], target, is_raw, None),
            [expected]
        );
    }
}
