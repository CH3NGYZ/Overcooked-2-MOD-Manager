use super::*;

#[test]
fn old_network_settings_use_new_defaults_without_reusing_legacy_timeout() {
    let settings: NetworkConfig =
        toml::from_str("timeout_seconds = 50\ndownload_threads = 4\nmax_retry_count = 3\n")
            .unwrap();
    assert_eq!(settings.timeout_seconds, 50);
    assert_eq!(settings.api_timeout().as_secs(), 20);
    assert_eq!(settings.first_byte_timeout().as_secs(), 20);
}

#[test]
fn removed_race_settings_are_not_used_or_serialized() {
    let base = toml::to_string(&NetworkConfig::default()).unwrap();
    let old = format!("{base}race_min_speed_kbps = 2048\nrace_window_seconds = 5\n");
    let settings = toml::from_str::<NetworkConfig>(&old).unwrap();
    assert_eq!(settings, NetworkConfig::default());
    assert_eq!(toml::to_string(&settings).unwrap(), base);
}

#[test]
fn second_settings_share_the_same_bounds_and_normalize_idempotently() {
    for (seconds, expected) in [
        (0, 5),
        (4, 5),
        (5, 5),
        (10, 10),
        (20, 20),
        (21, 21),
        (30, 30),
        (31, 30),
        (u64::MAX, 30),
    ] {
        let mut settings = NetworkConfig {
            api_timeout_seconds: seconds,
            first_byte_timeout_seconds: seconds,
            ..Default::default()
        };
        assert_eq!(settings.api_timeout().as_secs(), expected);
        assert_eq!(settings.first_byte_timeout().as_secs(), expected);
        settings.normalize();
        assert_eq!(settings.api_timeout_seconds, expected);
        assert_eq!(settings.first_byte_timeout_seconds, expected);
        assert!(!settings.normalize());
    }
}

#[test]
fn configured_timeouts_round_trip() {
    let settings = NetworkConfig {
        api_timeout_seconds: 30,
        first_byte_timeout_seconds: 5,
        ..Default::default()
    };
    let encoded = toml::to_string(&settings).unwrap();
    assert_eq!(toml::from_str::<NetworkConfig>(&encoded).unwrap(), settings);
}
