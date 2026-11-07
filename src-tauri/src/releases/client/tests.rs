use super::*;

#[test]
fn user_agent_identifies_client_and_version() {
    assert!(APP_USER_AGENT.starts_with("Overcooked 2 MOD Manager/"));
    assert!(APP_USER_AGENT.contains("client=desktop"));
    assert!(APP_USER_AGENT.contains("arch=x64"));
}
