use super::*;

#[test]
fn old_network_settings_use_new_defaults_without_reusing_legacy_timeout() {
    let settings: NetworkConfig =
        toml::from_str("timeout_seconds = 50\ndownload_threads = 4\nmax_retry_count = 3\n")
            .unwrap();
    assert_eq!(settings.timeout_seconds, 50);
    assert_eq!(settings.race_min_speed_kbps, 256);
    assert_eq!(settings.race_minimum_speed(), 256 * 1024);
    assert_eq!(settings.race_window().as_secs(), 10);
    assert_eq!(settings.api_timeout().as_secs(), 20);
    assert_eq!(settings.first_byte_timeout().as_secs(), 20);
}

#[test]
fn all_speed_choices_convert_to_bytes_and_invalid_values_use_default() {
    for speed in RACE_SPEED_OPTIONS_KBPS {
        let mut settings = NetworkConfig {
            race_min_speed_kbps: speed,
            ..Default::default()
        };
        assert_eq!(settings.race_minimum_speed(), speed * 1024);
        assert!(!settings.normalize());
    }
    for speed in [0, 31, 65, 2049, u64::MAX] {
        let mut settings = NetworkConfig {
            race_min_speed_kbps: speed,
            ..Default::default()
        };
        assert_eq!(settings.race_minimum_speed(), 256 * 1024);
        assert!(settings.normalize());
        assert_eq!(settings.race_min_speed_kbps, 256);
        assert!(!settings.normalize());
    }
}

#[test]
fn second_settings_share_the_same_bounds_and_normalize_idempotently() {
    for (seconds, expected) in [
        (0, 5),
        (4, 5),
        (5, 5),
        (10, 10),
        (20, 20),
        (21, 20),
        (u64::MAX, 20),
    ] {
        let mut settings = NetworkConfig {
            race_window_seconds: seconds,
            api_timeout_seconds: seconds,
            first_byte_timeout_seconds: seconds,
            ..Default::default()
        };
        assert_eq!(settings.race_window().as_secs(), expected);
        assert_eq!(settings.api_timeout().as_secs(), expected);
        assert_eq!(settings.first_byte_timeout().as_secs(), expected);
        settings.normalize();
        assert_eq!(settings.race_window_seconds, expected);
        assert_eq!(settings.api_timeout_seconds, expected);
        assert_eq!(settings.first_byte_timeout_seconds, expected);
        assert!(!settings.normalize());
    }
}

#[test]
fn configured_network_settings_round_trip_with_all_four_values() {
    let settings = NetworkConfig {
        race_min_speed_kbps: 2048,
        race_window_seconds: 5,
        api_timeout_seconds: 15,
        first_byte_timeout_seconds: 20,
        ..Default::default()
    };
    let encoded = toml::to_string(&settings).unwrap();
    assert_eq!(toml::from_str::<NetworkConfig>(&encoded).unwrap(), settings);
}
