use super::*;

fn persisted(service: &ConfigService) -> AppConfig {
    toml::from_str(&fs::read_to_string(service.config_path()).unwrap()).unwrap()
}

#[test]
fn first_start_persists_system_region_before_selecting_the_source() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::open(root.path().join("first-start")).unwrap();
    let config = service.load().unwrap();
    assert_eq!(
        config.region,
        super::super::availability::detect_system_region()
    );
    assert_eq!(persisted(&service).region, config.region);
    assert_eq!(
        config.data_provider,
        DataProvider::for_region(&config.region)
    );
    assert_eq!(
        config.repository_sources[0].url,
        config.data_provider.document("mods.toml")
    );
}

#[test]
fn old_missing_empty_or_invalid_regions_are_initialized_and_written_once() {
    for value in [None, Some(""), Some("not-a-region")] {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join(CONFIG_FILE);
        let mut config = toml::Value::try_from(default_config()).unwrap();
        let table = config.as_table_mut().unwrap();
        match value {
            Some(value) => {
                table.insert("region".into(), value.into());
            }
            None => {
                table.remove("region");
            }
        }
        fs::write(&path, toml::to_string(&config).unwrap()).unwrap();
        let service = ConfigService::open(root.path().into()).unwrap();
        let loaded = service.load().unwrap();
        assert_eq!(
            loaded.region,
            super::super::availability::detect_system_region()
        );
        assert_eq!(persisted(&service).region, loaded.region);
        let once = fs::read(&path).unwrap();
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        let restarted = ConfigService::open(root.path().into()).unwrap();
        assert_eq!(restarted.load().unwrap().region, loaded.region);
        assert_eq!(fs::read(&path).unwrap(), once);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), modified);
    }
}

#[test]
fn saved_region_survives_restart_and_manual_provider_only_survives_the_session() {
    for region in ["cn", "global"] {
        let root = tempfile::tempdir().unwrap();
        let mut initial = default_config();
        initial.region = region.into();
        fs::write(
            root.path().join(CONFIG_FILE),
            toml::to_string(&initial).unwrap(),
        )
        .unwrap();
        let service = ConfigService::open(root.path().into()).unwrap();
        let mut config = service.load().unwrap();
        assert_eq!(config.region, region);
        assert_eq!(config.data_provider, DataProvider::for_region(region));
        service
            .save_with_selection(&mut config, None, Some(DataProvider::Cloudflare))
            .unwrap();
        for _ in 0..3 {
            let loaded = service.clone().load().unwrap();
            assert_eq!(loaded.region, region);
            assert_eq!(loaded.data_provider, DataProvider::Cloudflare);
        }
        let restarted = ConfigService::open(root.path().into()).unwrap();
        let config = restarted.load().unwrap();
        assert_eq!(config.region, region);
        assert_eq!(config.data_provider, DataProvider::for_region(region));
        assert_eq!(restarted.public_config(&config).region, region);
        assert!(
            !fs::read_to_string(restarted.config_path())
                .unwrap()
                .contains("data_provider")
        );
    }
}

#[test]
fn startup_recovers_corrupt_config_but_propagates_io_errors() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join(CONFIG_FILE), "invalid = [").unwrap();
    let service = ConfigService::open(root.path().into()).unwrap();
    let loaded = service.load().unwrap();
    assert_eq!(
        loaded.region,
        super::super::availability::detect_system_region()
    );
    assert_eq!(
        loaded.data_provider,
        DataProvider::for_region(&loaded.region)
    );
    assert_eq!(persisted(&service).region, loaded.region);
    let failed = tempfile::tempdir().unwrap();
    fs::create_dir(failed.path().join(CONFIG_FILE)).unwrap();
    assert!(matches!(
        ConfigService::open(failed.path().into()),
        Err(ConfigError::Read(_))
    ));
}

#[test]
fn external_region_edits_take_effect_on_restart_without_changing_this_session() {
    let root = tempfile::tempdir().unwrap();
    let mut initial = default_config();
    initial.region = "global".into();
    let path = root.path().join(CONFIG_FILE);
    fs::write(&path, toml::to_string(&initial).unwrap()).unwrap();
    let service = ConfigService::open(root.path().into()).unwrap();
    let selection = service.data_selection();
    initial.region = "cn".into();
    initial.ui_zoom = 1.75;
    fs::write(&path, toml::to_string(&initial).unwrap()).unwrap();
    let active = service.load().unwrap();
    assert_eq!(active.ui_zoom, 1.75);
    assert_eq!(active.region, "global");
    assert_eq!(active.data_provider, DataProvider::Github);
    assert_eq!(service.data_selection(), selection);
    let restarted = ConfigService::open(root.path().into()).unwrap();
    let loaded = restarted.load().unwrap();
    assert_eq!(loaded.region, "cn");
    assert_eq!(loaded.data_provider, DataProvider::Gitcode);
}

#[cfg(windows)]
#[test]
fn startup_propagates_failed_migration_save_instead_of_hiding_it_as_recovery() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join(CONFIG_FILE);
    let mut legacy = toml::Value::try_from(default_config()).unwrap();
    legacy.as_table_mut().unwrap().remove("region");
    fs::write(&path, toml::to_string(&legacy).unwrap()).unwrap();
    let original = fs::read(&path).unwrap();
    let _locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    assert!(matches!(
        ConfigService::open(root.path().into()),
        Err(ConfigError::Write(_))
    ));
    assert_eq!(fs::read(&path).unwrap(), original);
}
