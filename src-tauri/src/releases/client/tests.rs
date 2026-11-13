use super::*;

#[test]
fn user_agent_identifies_client_and_version() {
    assert!(APP_USER_AGENT.starts_with("Overcooked 2 MOD Manager/"));
    assert!(APP_USER_AGENT.contains("client=desktop"));
    assert!(APP_USER_AGENT.contains("arch=x64"));
}

#[test]
fn connection_timeout_allows_ten_seconds_and_preserves_longer_preferences() {
    for seconds in [0, 1, 5, 10] {
        assert_eq!(
            connection_timeout(seconds),
            std::time::Duration::from_secs(10)
        );
    }
    for seconds in [20, 30, 300] {
        assert_eq!(
            connection_timeout(seconds),
            std::time::Duration::from_secs(seconds)
        );
    }
    assert_eq!(crate::archive::FIRST_BYTE_TIMEOUT, REQUEST_START_TIMEOUT);
}
