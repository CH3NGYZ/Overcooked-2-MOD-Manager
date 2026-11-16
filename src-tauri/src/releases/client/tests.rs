use super::*;

#[test]
fn user_agent_identifies_client_and_version() {
    assert!(APP_USER_AGENT.starts_with("Overcooked 2 MOD Manager/"));
    assert!(APP_USER_AGENT.contains("client=desktop"));
    assert!(APP_USER_AGENT.contains("arch=x64"));
}

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
