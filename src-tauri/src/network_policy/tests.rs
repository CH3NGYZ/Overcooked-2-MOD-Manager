use super::*;

fn fixture() -> (String, BTreeMap<String, String>, String) {
    let index =
        "schema_version = 1\n[[proxy_sources]]\nid = 'api'\nurl = 'proxies/api.toml'".into();
    let document = r#"schema_version = 1
[[proxies]]
id = "test-api"
name = "Test"
provider = "github"
intent = "api"
access = "public"
template = "https://proxy.example/{url_path}"
token_transport = "matching_platform_token"
"#;
    let plans = r#"schema_version = 1
[[request_plans]]
id = "test"
provider = "github"
access = "public"
[request_plans.api]
route = [{kind = "official"}, {kind = "proxy", id = "test-api"}]
max_parallelism = 1
[request_plans.release]
route = [{kind = "direct"}]
max_parallelism = 1
[request_plans.raw]
route = [{kind = "direct"}]
max_parallelism = 1
"#
    .into();
    (
        index,
        BTreeMap::from([("proxies/api.toml".into(), document.into())]),
        plans,
    )
}

#[test]
fn bundled_typed_registry_and_plans_form_one_closed_snapshot() {
    let snapshot = NetworkPolicy::bundled().unwrap();
    assert_eq!(snapshot.proxies.len(), 45);
    assert_eq!(snapshot.plans.len(), 9);
    let groups = snapshot.bundled_proxy_groups().unwrap();
    assert_eq!(groups.release_proxies.len(), 30);
    assert_eq!(groups.raw_proxies.len(), 8);
    assert_eq!(groups.avatar_proxies[0].id, "cors-isteed-avatar");
    assert!(
        !groups
            .release_proxies
            .iter()
            .any(|proxy| proxy.id == "oc2gh-release")
    );
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        let name = match provider {
            Provider::Github => "github",
            Provider::Gitee => "gitee",
            Provider::Gitcode => "gitcode",
        };
        let plan = snapshot
            .plan(&format!("{name}-public-mod"), &provider)
            .unwrap();
        assert_eq!(plan.api.route[0], Step::Official);
        assert_eq!(plan.release.max_parallelism, 1);
        assert!(!plan.release.race);
    }
    assert!(
        snapshot
            .plan("github-public-mod", &Provider::Gitee)
            .is_err()
    );
    let mainland = snapshot
        .plan("github-mainland-mod", &Provider::Github)
        .unwrap();
    assert!(mainland.release.race);
    assert_eq!(mainland.release.max_parallelism, 3);
    assert!(!mainland.release.route.contains(&Step::Proxy {
        id: "oc2gh-release".into()
    }));
    let hidden = snapshot
        .plan("qingyuqingxue-map-proxy-only", &Provider::Github)
        .unwrap();
    assert_eq!(
        hidden.api.route,
        [Step::Proxy {
            id: "oc2ghapi".into()
        }]
    );
    assert_eq!(
        hidden.release.route,
        [Step::Proxy {
            id: "oc2gh-release".into()
        }]
    );
    assert_eq!(hidden.release.max_parallelism, 1);
    assert_eq!(
        url::Url::parse(&snapshot.proxies["oc2ghapi"].template)
            .unwrap()
            .origin()
            .ascii_serialization(),
        "https://oc2ghapi.ch3ng.top"
    );
    let maps = crate::repository_sources::SourceLocation::parse("maps.toml").unwrap();
    let index: toml::Value = toml::from_str(maps.bundled_text().unwrap()).unwrap();
    assert!(
        index["map_sources"]
            .as_array()
            .unwrap()
            .iter()
            .all(|source| {
                source.get("url").and_then(toml::Value::as_str)
                    != Some("maps/maps-qingyuqingxue.toml")
            })
    );
}

#[test]
fn unknown_fields_versions_and_broken_document_closures_fail() {
    let (index, documents, plans) = fixture();
    assert!(NetworkPolicy::parse(&index, &documents, &plans).is_ok());
    for bad in [
        "",
        "<html>bad</html>",
        "schema_version = 2",
        "schema_version = 1\nunknown = 0",
    ] {
        assert!(NetworkPolicy::parse(bad, &documents, &plans).is_err());
    }
    assert!(NetworkPolicy::parse(&index, &BTreeMap::new(), &plans).is_err());
    let mut extra = documents.clone();
    extra.insert("proxies/other.toml".into(), "schema_version = 1".into());
    assert!(NetworkPolicy::parse(&index, &extra, &plans).is_err());
    assert!(
        NetworkPolicy::parse(&index, &documents, &(plans.clone() + "\nunknown = true")).is_err()
    );
    assert!(
        NetworkPolicy::parse(
            &index,
            &documents,
            &plans.replace("schema_version = 1", "schema_version = 2")
        )
        .is_err()
    );
}

#[test]
fn index_cannot_escape_trusted_configuration_root_or_execute_scripts() {
    for path in [
        "../api.toml",
        "proxies/../api.toml",
        "proxies//api.toml",
        "proxies/./api.toml",
        "proxies/%2e%2e/api.toml",
        "proxies/a.toml?x=1",
        "https://bad.example/a.toml",
        "C:\\api.toml",
        "proxies/a.user.js",
    ] {
        let index = format!("schema_version = 1\n[[proxy_sources]]\nid = 'test'\nurl = '{path}'");
        assert!(proxy_documents(&index).is_err(), "{path}");
    }
    let (index, _, _) = fixture();
    assert!(
        proxy_documents(&(index + "\n[[proxy_sources]]\nid = 'api'\nurl = 'proxies/api.toml'"))
            .is_err()
    );
}

#[test]
fn token_capabilities_and_template_authorities_are_not_source_controlled() {
    let (index, documents, plans) = fixture();
    let original = &documents["proxies/api.toml"];
    for (from, to) in [
        ("https://proxy.example", "http://proxy.example"),
        ("https://proxy.example", "https://user:pass@proxy.example"),
        ("https://proxy.example", "https://{owner}.example"),
        ("{url_path}", "{unknown}"),
        ("{url_path}", "{url_path}?token=secret"),
        ("{url_path}", "{url_path}#fragment"),
        ("matching_platform_token", "none"),
        ("matching_platform_token", "arbitrary_cookie"),
        ("intent = \"api\"", "intent = \"release\""),
    ] {
        let mut modified = documents.clone();
        modified.insert("proxies/api.toml".into(), original.replace(from, to));
        assert!(
            NetworkPolicy::parse(&index, &modified, &plans).is_err(),
            "{from} -> {to}"
        );
    }
}

#[test]
fn plan_references_enforce_provider_intent_access_and_concurrency() {
    let (index, documents, plans) = fixture();
    for (from, to) in [
        ("id = \"test-api\"", "id = \"missing\""),
        ("provider = \"github\"", "provider = \"gitee\""),
        ("access = \"public\"", "access = \"private\""),
        ("kind = \"official\"", "kind = \"direct\""),
        ("kind = \"direct\"", "kind = \"official\""),
        ("kind = \"direct\"", "kind = \"proxy\", id = \"test-api\""),
        ("max_parallelism = 1", "max_parallelism = 0"),
        ("max_parallelism = 1", "max_parallelism = 4"),
        ("max_parallelism = 1", "max_parallelism = 2\nrace = true"),
        ("kind = \"official\"", "kind = \"unknown\""),
        ("route = [{kind = \"direct\"}]", "route = []"),
        (
            "route = [{kind = \"direct\"}]",
            "route = [{kind = \"direct\"}, {kind = \"direct\"}]",
        ),
    ] {
        assert!(
            NetworkPolicy::parse(&index, &documents, &plans.replace(from, to)).is_err(),
            "{from} -> {to}"
        );
    }
    let mut disabled = documents.clone();
    *disabled.get_mut("proxies/api.toml").unwrap() += "enabled = false\n";
    assert!(NetworkPolicy::parse(&index, &disabled, &plans).is_err());
}

#[test]
fn empty_registry_and_plans_are_an_explicit_valid_clear() {
    let clear =
        NetworkPolicy::parse("schema_version = 1", &BTreeMap::new(), "schema_version = 1").unwrap();
    assert_eq!(clear, NetworkPolicy::default());
}

#[test]
fn duplicate_proxy_and_plan_ids_cannot_shadow_previous_definitions() {
    let (index, mut documents, plans) = fixture();
    let proxy = documents["proxies/api.toml"]
        .split_once("[[proxies]]")
        .unwrap()
        .1
        .to_owned();
    documents
        .get_mut("proxies/api.toml")
        .unwrap()
        .push_str(&format!("\n[[proxies]]{proxy}"));
    assert!(NetworkPolicy::parse(&index, &documents, &plans).is_err());
    let (index, documents, plans) = fixture();
    let duplicate = format!(
        "{plans}\n[[request_plans]]{}",
        plans.split_once("[[request_plans]]").unwrap().1
    );
    assert!(NetworkPolicy::parse(&index, &documents, &duplicate).is_err());
    assert!(proxy_documents(&" ".repeat(1024 * 1024 + 1)).is_err());
}
