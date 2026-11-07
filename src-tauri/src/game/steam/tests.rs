use super::*;

#[test]
fn native_steam_launch_uses_the_platform_handler() {
    assert_eq!(
        steam_launch_strategy("windows").unwrap(),
        SteamStrategy::ProtocolHandler
    );
    assert_eq!(
        steam_launch_strategy("macos").unwrap(),
        SteamStrategy::OpenBundle("com.valvesoftware.steam")
    );
    // 未列入的平台名一律拒绝：本项目只支持 Windows 与 macOS。
    assert!(steam_launch_strategy("linux").is_err());
}
