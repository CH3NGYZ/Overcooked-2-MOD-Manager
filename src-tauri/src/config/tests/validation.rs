use crate::config::validation::validate_mod_repository;
use crate::models::*;

#[test]
fn repository_source_does_not_require_bundle_metadata() {
    let text = r#"
[[repositories]]
name = "Example"
name_i18n.zh-CN = "Example"
group_i18n.zh-CN = ""
note_i18n.zh-CN = "**Example** repository"
provider = "gitee"
username = "example"
repository = "release"
channel = "all"
install_target = "BepInEx/plugins"
note = """
**Example** repository
"""
plugin_guid = "com.example.plugin"
download_threads = 1
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Example.dll"
"#;
    let location = crate::repository_sources::SourceLocation::parse("source.toml").unwrap();
    let document =
        crate::repository_sources::parse_document(text, &location, &Default::default()).unwrap();
    assert_eq!(document.repositories.len(), 1);
    assert_eq!(document.repositories[0].name, "Example");
}

#[test]
fn repository_source_can_declare_child_sources() {
    let text = r#"
[[repository_sources]]
id = "azhe-Others"
nickname = "Azhe的其他MOD仓库源"
url = "https://example.com/others.oc2repo.toml"
builtin = true
"#;
    let location = crate::repository_sources::SourceLocation::parse("source.toml").unwrap();
    let document =
        crate::repository_sources::parse_document(text, &location, &Default::default()).unwrap();
    assert!(document.repositories.is_empty());
    assert_eq!(document.repository_sources.len(), 1);
    assert_eq!(document.repository_sources[0].id, "azhe-Others");
    assert_eq!(document.repository_sources[0].builtin, Some(true));
}

#[test]
fn unsafe_install_target_is_rejected() {
    let result = validate_mod_repository(&ModRepositoryExport {
        network_plan: Default::default(),
        kind: Default::default(),
        name: "bad".into(),
        name_i18n: std::collections::BTreeMap::new(),
        group: String::new(),
        group_i18n: std::collections::BTreeMap::new(),
        provider: Provider::Github,
        username: "a".into(),
        repository: "b".into(),
        download_mode: ModDownloadMode::Release,
        branch: String::new(),
        file_path: String::new(),
        region: None,
        platform: None,
        channel: Channel::Stable,
        asset_rules: vec![AssetRule {
            rule_type: AssetRuleType::Exact,
            value: "a.zip".into(),
        }],
        install_target: "../plugins".into(),
        note: String::new(),
        note_i18n: std::collections::BTreeMap::new(),
        plugin_guid: String::new(),
        local_product_name: String::new(),
        local_original_filename: String::new(),
        relations: Vec::new(),
        release_relations: Vec::new(),
        version_regex: String::new(),
        max_release_count: 10,
        download_threads: 1,
        enabled: true,
        source_id: None,
    });
    assert!(result.is_err());
}

#[test]
fn repository_file_mode_does_not_require_release_asset_rules() {
    let valid = r#"
name = "Many Recipes"
provider = "github"
username = "gua248"
repository = "Overcooked2-ManyRecipes"
download_mode = "repository_file"
branch = "master"
file_path = "bin/Release/OC2ManyRecipes.dll"
install_target = "BepInEx/plugins"
enabled = true
"#;
    let repository: ModRepositoryExport = toml::from_str(valid).unwrap();
    assert!(validate_mod_repository(&repository).is_ok());
    assert_eq!(repository.channel, Channel::All);

    let missing_branch = valid.replace("branch = \"master\"\n", "");
    let repository: ModRepositoryExport = toml::from_str(&missing_branch).unwrap();
    assert!(validate_mod_repository(&repository).is_err());

    let repository: ModRepositoryExport = toml::from_str(valid).unwrap();
    for path in ["bin/*.dll", "bin/MOD?.dll", "../MOD.dll", "bin/./MOD.dll"] {
        let mut invalid = repository.clone();
        invalid.file_path = path.into();
        assert!(
            validate_mod_repository(&invalid).is_err(),
            "accepted {path}"
        );
    }
    let mut invalid = repository;
    invalid.asset_rules.push(AssetRule {
        rule_type: AssetRuleType::Regex,
        value: ".*\\.dll".into(),
    });
    assert!(validate_mod_repository(&invalid).is_err());
}
