use crate::config::ConfigService;
use crate::config::defaults::default_config;
use crate::config::identity::repository_from_export;
use crate::models::*;
use std::fs;
use std::path::Path;
use uuid::Uuid;

use super::REGION_REPOSITORY;

#[test]
fn imported_repositories_follow_bundle_order() {
    let service = ConfigService {
        root: std::env::temp_dir(),
        cache: std::sync::Arc::new(std::sync::Mutex::new(None)),
        proxies: Default::default(),
    };
    let text = r#"
format = "oc2-mod-repository-bundle"
schema_version = 1
exported_at = ""

[[repositories]]
name = "Order-C"
name_i18n.zh-CN = "Order-C"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "example"
repository = "order-c"
channel = "all"
install_target = ""
plugin_guid = ""
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Order-C.zip"

[[repositories]]
name = "Order-A"
name_i18n.zh-CN = "Order-A"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "example"
repository = "order-a"
channel = "all"
install_target = ""
plugin_guid = ""
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Order-A.zip"

[[repositories]]
name = "Order-B"
name_i18n.zh-CN = "Order-B"
group_i18n.zh-CN = ""
note_i18n.zh-CN = ""
provider = "github"
username = "example"
repository = "order-b"
channel = "all"
install_target = ""
plugin_guid = ""
enabled = true

[[repositories.asset_rules]]
type = "exact"
value = "Order-B.zip"
"#;
    let bundle = service.validate_bundle(text).unwrap();
    let mut config = default_config();
    service.import_bundle(&mut config, &bundle, "overwrite");
    let names = config
        .repositories
        .iter()
        .filter(|repository| repository.name.starts_with("Order-"))
        .map(|repository| repository.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["Order-C", "Order-A", "Order-B"]);
}

#[test]
fn community_catalogue_survives_config_reads_and_cannot_be_overwritten_by_import() {
    let verification =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("target/verification/community-20261013");
    let service =
        ConfigService::for_test(verification.join(format!("catalogue-config-{}", Uuid::new_v4())));
    let location = crate::repository_sources::SourceLocation::parse("index.toml").unwrap();
    let template = crate::repository_sources::parse_document(REGION_REPOSITORY, &location)
        .unwrap()
        .repositories
        .remove(0);
    let mut repository = repository_from_export(&template, "community-fixture".into(), false);
    repository.source_id = Some(crate::community::SOURCE_ID.into());
    let mut config = default_config();
    config.repositories.push(repository.clone());
    config
        .repository_sources
        .push(crate::community::source(&Provider::Github));
    config.community_discovery = Some(crate::models::CommunityDiscovery {
        repository_ids: vec![repository.id.clone()],
        total_count: 1,
        ..Default::default()
    });
    service.save(&config).unwrap();

    let mut loaded = service.load().unwrap();
    assert_eq!(loaded.repository_sources, config.repository_sources);
    assert_eq!(loaded.repositories[0].id, repository.id);
    assert_eq!(
        loaded.community_discovery.as_ref().unwrap().repository_ids,
        [repository.id.clone()]
    );
    let text = service.export_bundle(&loaded, None).unwrap();
    let mut bundle = service.validate_bundle(&text).unwrap();
    bundle.repositories[0]
        .note_i18n
        .insert("zh-CN".into(), "Custom imported note".into());
    bundle.repositories[0].note = "Custom imported note".into();
    let preview = service.import_bundle(&mut loaded, &bundle, "overwrite");
    assert!(preview.valid);
    assert_eq!(loaded.repositories.len(), 2);
    let retained = loaded
        .repositories
        .iter()
        .find(|repo| crate::community::is_repository(repo))
        .unwrap();
    assert_eq!(retained.id, repository.id);
    assert_eq!(retained.note, repository.note);
    let imported = loaded
        .repositories
        .iter()
        .find(|repo| !crate::community::is_repository(repo))
        .unwrap();
    assert_ne!(imported.id, repository.id);
    assert!(imported.source_id.is_none());
    assert_eq!(imported.note, "Custom imported note");
    service.save(&loaded).unwrap();
    let reloaded = service.load().unwrap();
    assert_eq!(reloaded.repositories.len(), 2);
    assert_eq!(
        reloaded
            .repository_sources
            .iter()
            .filter(|source| source.id == crate::community::SOURCE_ID)
            .count(),
        1
    );
    let target = fs::canonicalize(&service.root).unwrap();
    assert!(target.starts_with(fs::canonicalize(&verification).unwrap()));
    fs::remove_dir_all(target).unwrap();
}
