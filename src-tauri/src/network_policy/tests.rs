use super::*;

#[test]
fn avatars_validate_endpoint_intent_and_sequential_execution() {
    let mut policy = NetworkPolicy::bundled().unwrap();
    let plan = policy
        .plan("github-public", &Provider::Github)
        .unwrap()
        .clone();
    assert!(validation::plan(&policy, &plan).is_ok());
    for invalid in [
        "endpoint",
        "race",
        "parallel",
        "threads",
        "raw",
        "official",
        "proxy-first",
    ] {
        let mut bad = plan.clone();
        let route = bad.avatars.as_mut().unwrap();
        match invalid {
            "endpoint" => route.url_template = Some("https://example.com/{owner}.png".into()),
            "race" => route.race = true,
            "parallel" => route.max_parallelism = 2,
            "threads" => route.download_threads = 2,
            "raw" => route.route.push(Step::Proxy {
                id: "oc2gh-raw".into(),
            }),
            "official" => route.route.push(Step::Official),
            "proxy-first" => {
                route.route.remove(0);
            }
            _ => unreachable!(),
        }
        assert!(validation::plan(&policy, &bad).is_err(), "{invalid}");
    }
    let mut bad = plan.clone();
    bad.release.url_template = plan.avatars.as_ref().unwrap().url_template.clone();
    assert!(validation::plan(&policy, &bad).is_err());
    policy
        .proxies
        .get_mut("cors-isteed-avatar")
        .unwrap()
        .provider = Provider::Gitee;
    let mut bad = policy
        .plan("gitee-public", &Provider::Gitee)
        .unwrap()
        .clone();
    bad.avatars.as_mut().unwrap().route.push(Step::Proxy {
        id: "cors-isteed-avatar".into(),
    });
    assert!(
        validation::plan(&policy, &bad).is_err(),
        "Gitee must remain direct-only"
    );
}

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
    assert!(snapshot.proxies.len() >= 45);
    assert_eq!(snapshot.plans.len(), 5);
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
    assert!(snapshot.plan("github-public", &Provider::Gitee).is_err());
    let mainland = snapshot.plan("github-mainland", &Provider::Github).unwrap();
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

#[test]
fn defaults_are_selected_by_provider_kind_and_region() {
    let policy = NetworkPolicy::bundled().unwrap();
    for provider in [Provider::Github, Provider::Gitee, Provider::Gitcode] {
        for kind in [RepositoryKind::Mod, RepositoryKind::Map] {
            for region in ["chinese_mainland", "international"] {
                let plan = policy
                    .resolve(&provider, kind, region, &NetworkPlanSelection::default())
                    .unwrap();
                assert_eq!(plan.provider, provider);
                assert!(plan.kinds.contains(&kind));
                assert_eq!(plan.release.download_threads, 1);
                assert_eq!(plan.raw.download_threads, 1);
                assert_eq!(
                    plan.release.race,
                    provider == Provider::Github && region == "chinese_mainland"
                );
            }
        }
    }
}

#[test]
fn sparse_overrides_inherit_and_explicit_ids_reset_parent_overrides() {
    let parent: NetworkPlanSelection = toml::from_str(
        r#"
mainland = { id = "github-public", raw = { download_threads = 8 } }
international = "github-public"
"#,
    )
    .unwrap();
    let child: NetworkPlanSelection = toml::from_str(
        r#"
mainland = { release = { download_threads = 4 } }
"#,
    )
    .unwrap();
    let merged = child.inherit(&parent);
    let policy = NetworkPolicy::bundled().unwrap();
    let plan = policy
        .resolve(
            &Provider::Github,
            RepositoryKind::Mod,
            "chinese_mainland",
            &merged,
        )
        .unwrap();
    assert_eq!(plan.raw.download_threads, 8);
    assert_eq!(plan.release.download_threads, 4);
    assert!(merged.international.is_some());
    let fresh: NetworkPlanSelection = toml::from_str("mainland = 'github-mainland'").unwrap();
    let plan = policy
        .resolve(
            &Provider::Github,
            RepositoryKind::Mod,
            "chinese_mainland",
            &fresh.inherit(&merged),
        )
        .unwrap();
    assert!(plan.release.race);
    assert_eq!(plan.raw.download_threads, 1);
    // An inherited route is replaced as a whole, never appended.
    let route: NetworkPlanSelection = toml::from_str(
        r#"
mainland = { raw = { route = [{kind = "direct"}] } }
"#,
    )
    .unwrap();
    let plan = policy
        .resolve(
            &Provider::Github,
            RepositoryKind::Mod,
            "chinese_mainland",
            &route.inherit(&parent),
        )
        .unwrap();
    assert_eq!(plan.raw.route, [Step::Direct]);
    assert_eq!(plan.raw.download_threads, 8);
}

#[test]
fn effective_overrides_reject_wrong_provider_kind_threads_and_proxy_intent() {
    let policy = NetworkPolicy::bundled().unwrap();
    for text in [
        "mainland = 'does-not-exist'",
        "mainland = 'gitcode-public'",
        "mainland = 'qingyuqingxue-map-proxy-only'",
        "mainland = { release = { download_threads = 2 } }", // default is racing
        "mainland = { release = { download_threads = 0 } }",
        "mainland = { release = { download_threads = 33 } }",
        "mainland = { api = { download_threads = 2 } }",
        "mainland = { release = { route = [] } }",
        "mainland = { release = { route = [{kind = 'proxy', id = 'oc2ghapi'}] } }",
        "mainland = { release = { race = false } }", // parallelism must also be reset
    ] {
        let selection: NetworkPlanSelection = toml::from_str(text).unwrap();
        assert!(
            policy
                .resolve(
                    &Provider::Github,
                    RepositoryKind::Mod,
                    "chinese_mainland",
                    &selection
                )
                .is_err(),
            "{text}"
        );
    }
    for text in [
        "public = 'github-public'",
        "mainland = { release = { threads = 8 } }",
    ] {
        assert!(toml::from_str::<NetworkPlanSelection>(text).is_err());
    }
}

#[test]
fn default_table_rejects_duplicate_keys_missing_ids_and_mismatched_kind() {
    let (index, documents, plans) = fixture();
    let valid = "\n[[default_plans]]\ntype = 'mod'\nprovider = 'github'\nmainland = 'test'\ninternational = 'test'\n";
    assert!(NetworkPolicy::parse(&index, &documents, &(plans.clone() + valid)).is_ok());
    for text in [
        plans.clone() + valid + valid,
        plans.clone() + &valid.replace("mainland = 'test'", "mainland = 'unknown'"),
        plans + &valid.replace("type = 'mod'", "type = 'map'"),
    ] {
        assert!(NetworkPolicy::parse(&index, &documents, &text).is_err());
    }
}

#[test]
fn shared_type_lists_cover_both_roles_and_legacy_scalar_types_still_work() {
    let (index, documents, plans) = fixture();
    let shared = plans.replace(
        "provider = \"github\"",
        "type = [\"mod\", \"map\"]\nprovider = \"github\"",
    ) + "\n[[default_plans]]\ntype = ['mod', 'map']\nprovider = 'github'\nmainland = 'test'\ninternational = 'test'\n";
    let policy = NetworkPolicy::parse(&index, &documents, &shared).unwrap();
    for kind in [RepositoryKind::Mod, RepositoryKind::Map] {
        let resolved = policy
            .resolve(
                &Provider::Github,
                kind,
                "international",
                &NetworkPlanSelection::default(),
            )
            .unwrap();
        assert_eq!(resolved.id, "test");
        assert_eq!(resolved.kinds, [RepositoryKind::Mod, RepositoryKind::Map]);
    }
    let scalar = shared
        .replace("[\"mod\", \"map\"]", "\"mod\"")
        .replace("['mod', 'map']", "'mod'");
    let policy = NetworkPolicy::parse(&index, &documents, &scalar).unwrap();
    assert_eq!(policy.plans["test"].kinds, [RepositoryKind::Mod]);
    assert!(
        policy
            .resolve(
                &Provider::Github,
                RepositoryKind::Map,
                "international",
                &NetworkPlanSelection::default()
            )
            .is_err()
    );
}

#[test]
fn shared_type_lists_reject_empty_duplicate_unknown_and_overlapping_defaults() {
    let (index, documents, plans) = fixture();
    let typed = plans.replace(
        "provider = \"github\"",
        "type = [\"mod\", \"map\"]\nprovider = \"github\"",
    );
    let default = "\n[[default_plans]]\ntype = ['mod', 'map']\nprovider = 'github'\nmainland = 'test'\ninternational = 'test'\n";
    for bad in ["[]", "['mod', 'mod']", "['mod', 'unknown']", "true"] {
        let bad_plan = typed.replace("[\"mod\", \"map\"]", bad);
        assert!(
            NetworkPolicy::parse(&index, &documents, &bad_plan).is_err(),
            "plan: {bad}"
        );
        let bad_default = default.replace("['mod', 'map']", bad);
        assert!(
            NetworkPolicy::parse(&index, &documents, &(typed.clone() + &bad_default)).is_err(),
            "default: {bad}"
        );
    }
    let overlapping = default.replace("['mod', 'map']", "'map'");
    assert!(
        NetworkPolicy::parse(
            &index,
            &documents,
            &(typed.clone() + default + &overlapping)
        )
        .is_err()
    );
    // A shared default must be supported by both referenced request plans.
    assert!(NetworkPolicy::parse(&index, &documents, &(plans + default)).is_err());
    // Two deliberately separate defaults with disjoint roles remain valid.
    let disjoint = default.replace("['mod', 'map']", "'mod'") + &overlapping;
    assert!(NetworkPolicy::parse(&index, &documents, &(typed + &disjoint)).is_ok());
}

#[test]
fn merged_builtin_plans_keep_legacy_references_without_duplicate_plans() {
    let policy = NetworkPolicy::bundled().unwrap();
    assert_eq!(policy.plans.len(), 5);
    for (provider, name) in [
        (Provider::Github, "github-public"),
        (Provider::Github, "github-mainland"),
        (Provider::Gitee, "gitee-public"),
        (Provider::Gitcode, "gitcode-public"),
    ] {
        for kind in ["mod", "map"] {
            let old = policy.plan(&format!("{name}-{kind}"), &provider).unwrap();
            assert_eq!(old.id, name);
            assert_eq!(old, policy.plan(name, &provider).unwrap());
            assert_eq!(old.kinds, [RepositoryKind::Mod, RepositoryKind::Map]);
        }
    }
    let special = policy
        .plan("qingyuqingxue-map-proxy-only", &Provider::Github)
        .unwrap();
    assert_eq!(special.kinds, [RepositoryKind::Map]);
    let old_reference: NetworkPlanSelection = toml::from_str(
        "international = { id = 'github-public-map', release = { download_threads = 8 } }",
    )
    .unwrap();
    let resolved = policy
        .resolve(
            &Provider::Github,
            RepositoryKind::Map,
            "international",
            &old_reference,
        )
        .unwrap();
    assert_eq!(resolved.id, "github-public");
    assert_eq!(resolved.release.download_threads, 8);
}

#[test]
fn aliases_cannot_collide_with_plan_ids_or_each_other() {
    let (index, documents, plans) = fixture();
    for aliases in ["['test']", "['legacy', 'legacy']", "['bad/id']"] {
        let bad = plans.replace(
            "id = \"test\"",
            &format!("id = \"test\"\naliases = {aliases}"),
        );
        assert!(NetworkPolicy::parse(&index, &documents, &bad).is_err());
    }
    let second = plans
        .split_once("[[request_plans]]")
        .unwrap()
        .1
        .replace("id = \"test\"", "id = \"second\"\naliases = ['test']");
    assert!(
        NetworkPolicy::parse(
            &index,
            &documents,
            &format!("{plans}\n[[request_plans]]{second}")
        )
        .is_err()
    );
}
