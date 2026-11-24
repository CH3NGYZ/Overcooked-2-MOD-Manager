use super::*;

#[test]
fn old_config_gets_download_defaults_and_preserves_legacy_fields() {
    let root = tempfile::tempdir().unwrap();
    let mut value = toml::Value::try_from(default_config()).unwrap();
    let network = value.get_mut("network").unwrap().as_table_mut().unwrap();
    for field in ["api_timeout_seconds", "first_byte_timeout_seconds"] {
        network.remove(field);
    }
    network.insert("timeout_seconds".into(), 50.into());
    fs::write(
        root.path().join(CONFIG_FILE),
        toml::to_string(&value).unwrap(),
    )
    .unwrap();
    let service = ConfigService::open(root.path().into()).unwrap();
    let config = service.load().unwrap();
    assert_eq!(config.network.timeout_seconds, 50);
    assert_eq!(config.network.api_timeout_seconds, 20);
    assert_eq!(config.network.first_byte_timeout_seconds, 20);
}

#[test]
fn download_settings_survive_save_cache_public_projection_and_restart() {
    let root = tempfile::tempdir().unwrap();
    let service = ConfigService::open(root.path().into()).unwrap();
    let mut config = service.load().unwrap();
    let running_operation = config.network.clone();
    config.network.api_timeout_seconds = 5;
    config.network.first_byte_timeout_seconds = 30;
    service.save(&config).unwrap();
    assert_eq!(service.load().unwrap().network, config.network);
    assert_eq!(service.network_snapshot(), config.network);
    assert_eq!(service.public_config(&config).network, config.network);
    let disk: AppConfig =
        toml::from_str(&fs::read_to_string(service.config_path()).unwrap()).unwrap();
    assert_eq!(disk.network, config.network);
    let restarted = ConfigService::open(root.path().into()).unwrap();
    assert_eq!(restarted.load().unwrap().network, config.network);
    assert_eq!(running_operation.api_timeout().as_secs(), 20);
    assert_eq!(running_operation.first_byte_timeout().as_secs(), 20);
}

#[test]
fn invalid_download_settings_are_normalized_and_written_back_once() {
    let root = tempfile::tempdir().unwrap();
    let mut config = default_config();
    config.network.api_timeout_seconds = 31;
    config.network.first_byte_timeout_seconds = 0;
    fs::write(
        root.path().join(CONFIG_FILE),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let service = ConfigService::open(root.path().into()).unwrap();
    let network = service.load().unwrap().network;
    assert_eq!(network.api_timeout_seconds, 30);
    assert_eq!(network.first_byte_timeout_seconds, 5);
    let saved = fs::read(service.config_path()).unwrap();
    let restarted = ConfigService::open(root.path().into()).unwrap();
    assert_eq!(restarted.load().unwrap().network, network);
    assert_eq!(fs::read(service.config_path()).unwrap(), saved);
}
