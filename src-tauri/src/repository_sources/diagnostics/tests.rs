use super::*;

#[test]
fn log_addresses_preserve_source_identity_without_credentials_or_queries() {
    assert_eq!(
        safe_log_address(
            "https://user:password@example.com/scripts/proxies.user.js?token=secret#private"
        ),
        "https://example.com/scripts/proxies.user.js"
    );
    assert_eq!(
        safe_log_address("repositories-proxies.toml"),
        "repositories-proxies.toml"
    );
}
