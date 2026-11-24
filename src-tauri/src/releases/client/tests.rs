use super::*;

#[test]
fn clients_accept_configured_operation_timeouts() {
    for seconds in [5, 10, 20] {
        let network = crate::models::NetworkConfig {
            api_timeout_seconds: seconds,
            first_byte_timeout_seconds: seconds,
            ..Default::default()
        };
        assert!(build_client(network.api_timeout()).is_ok());
        assert!(build_download_client(network.first_byte_timeout()).is_ok());
    }
}
