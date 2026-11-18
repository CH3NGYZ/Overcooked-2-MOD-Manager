//! Consume a successful native reset before the page's preference modules run.
use serde::Serialize;

const SCRIPT: &str = include_str!("webview_reset.js");
const CONSUMED_KEY: &str = "oc2-manager-data-reset-consumed";

#[derive(Serialize)]
struct ResetOptions {
    token: uuid::Uuid,
    marker: &'static str,
    protocol: String,
    host: String,
}

fn options(token: uuid::Uuid, config: &tauri::Config, development: bool) -> ResetOptions {
    let configured_url = if development {
        config.build.dev_url.as_ref()
    } else {
        match config.build.frontend_dist.as_ref() {
            Some(tauri::utils::config::FrontendDist::Url(url)) => Some(url),
            _ => None,
        }
    };
    let (protocol, host) = if let Some(url) = configured_url {
        (
            format!("{}:", url.scheme()),
            url[url::Position::BeforeHost..url::Position::AfterPort].to_string(),
        )
    } else if cfg!(windows) {
        let https = config
            .app
            .windows
            .iter()
            .find(|window| window.label == "main")
            .is_some_and(|window| window.use_https_scheme);
        (
            if https { "https:" } else { "http:" }.to_string(),
            "tauri.localhost".to_string(),
        )
    } else {
        ("tauri:".to_string(), "localhost".to_string())
    };
    ResetOptions {
        token,
        marker: CONSUMED_KEY,
        protocol,
        host,
    }
}

pub(crate) fn webview_reset_script(token: Option<uuid::Uuid>, config: &tauri::Config) -> String {
    let Some(token) = token else {
        return String::new();
    };
    // JSON encoding preserves URL/UUID boundaries in executable script text.
    let payload = serde_json::to_string(&options(token, config, tauri::is_dev()))
        .expect("reset options contain only serializable strings");
    // Tauri appends this verbatim after its bridge IIFE, without a separator.
    format!(";\n({SCRIPT})({payload});")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manager_data::storage::{ResetPhase, ResetRecord};

    #[test]
    fn only_completed_native_reset_exposes_a_token() {
        assert_eq!(crate::manager_data::completed_reset_token(None), None);
        let token = uuid::Uuid::new_v4();
        for phase in [
            ResetPhase::Pending,
            ResetPhase::Deleting,
            ResetPhase::Failed,
            ResetPhase::Completed,
        ] {
            let record = ResetRecord {
                token,
                owner_pid: 0,
                owner_started: 0,
                phase,
                error: None,
                reported: false,
                action: super::super::storage::DataAction::Clear,
                fingerprint: None,
                data_root: None,
            };
            assert_eq!(
                crate::manager_data::completed_reset_token(Some(&record)),
                (phase == ResetPhase::Completed).then_some(token)
            );
        }
    }

    #[test]
    fn reset_payload_uses_the_actual_app_origin_and_safe_json() {
        let mut config = tauri::Config::default();
        config.build.dev_url = Some("http://[::1]:5179/custom/path".parse().unwrap());
        let token = uuid::Uuid::new_v4();
        let dev = options(token, &config, true);
        assert_eq!(dev.protocol, "http:");
        assert_eq!(dev.host, "[::1]:5179");
        let production = options(token, &config, false);
        assert_eq!(
            (production.protocol.as_str(), production.host.as_str()),
            if cfg!(windows) {
                ("http:", "tauri.localhost")
            } else {
                ("tauri:", "localhost")
            }
        );
        assert!(webview_reset_script(None, &config).is_empty());
        let script = webview_reset_script(Some(token), &config);
        let payload: serde_json::Value = serde_json::from_str(
            script
                .strip_prefix(&format!(";\n({SCRIPT})("))
                .unwrap()
                .strip_suffix(");")
                .unwrap(),
        )
        .unwrap();
        assert_eq!(payload["token"], token.to_string());
        assert_eq!(payload["marker"], CONSUMED_KEY);
    }
}
