use super::*;

#[test]
fn manifest_fallbacks_use_each_providers_download_route() {
    assert_eq!(
        installer_manifest_url(&Provider::Gitee),
        "https://gitee.com/ch3ngyz/Overcooked-2-MOD-Manager/releases/download/latest/latest.json"
    );
    assert_eq!(
        installer_manifest_url(&Provider::Github),
        "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases/latest/download/latest.json"
    );
    assert_eq!(
        installer_manifest_url(&Provider::Gitcode),
        "https://gitcode.com/CH3NGYZ/Overcooked-2-Manager/releases/download/latest/latest.json"
    );
    let configuration: serde_json::Value =
        serde_json::from_str(include_str!("../../../tauri.conf.json")).unwrap();
    let endpoints = configuration["plugins"]["updater"]["endpoints"]
        .as_array()
        .unwrap();
    for provider in [Provider::Gitee, Provider::Github, Provider::Gitcode] {
        assert!(endpoints.iter().any(|endpoint| {
            endpoint.as_str() == Some(installer_manifest_url(&provider).as_str())
        }));
    }
}
