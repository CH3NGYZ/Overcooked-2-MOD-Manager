//! The manager shell serves local UI. Dedicated verification/download windows own their own rules.
use tauri::{App, Runtime, WebviewWindowBuilder, utils::config::FrontendDist};
use url::Url;

const LABEL: &str = "main";

pub(crate) fn app_url(config: &tauri::Config, development: bool) -> Url {
    let configured = if development {
        config.build.dev_url.as_ref()
    } else if let Some(FrontendDist::Url(url)) = config.build.frontend_dist.as_ref() {
        Some(url)
    } else {
        None
    };
    configured.cloned().unwrap_or_else(|| {
        let https = config
            .app
            .windows
            .iter()
            .any(|window| window.label == LABEL && window.use_https_scheme);
        let origin = if cfg!(windows) {
            if https {
                "https://tauri.localhost"
            } else {
                "http://tauri.localhost"
            }
        } else {
            "tauri://localhost"
        };
        Url::parse(origin).expect("valid Tauri app origin")
    })
}

fn allows_navigation(app: &Url, target: &Url) -> bool {
    // Comparing Url::origin() would treat every custom-scheme origin as opaque on macOS.
    app.scheme() == target.scheme()
        && app.host() == target.host()
        && app.port() == target.port()
        && target.username().is_empty()
        && target.password().is_none()
}

pub(crate) fn create<R: Runtime>(app: &App<R>) -> tauri::Result<()> {
    let config = app.config();
    let window = config
        .app
        .windows
        .iter()
        .find(|window| window.label == LABEL)
        .ok_or(tauri::Error::WindowNotFound)?;
    let local = app_url(config, tauri::is_dev());
    WebviewWindowBuilder::from_config(app, window)?
        .on_navigation(move |target| allows_navigation(&local, target))
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .build()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn main_navigation_stays_on_the_configured_app_authority() {
        let mut config = tauri::Config::default();
        config.build.dev_url = Some("http://127.0.0.1:5173/".parse().unwrap());
        for (app, good, bad) in [
            (
                app_url(&config, true),
                "http://127.0.0.1:5173/index.html#/settings",
                "http://127.0.0.1:5174/",
            ),
            (
                "http://tauri.localhost/".parse().unwrap(),
                "http://tauri.localhost/#/tools",
                "https://tauri.localhost/",
            ),
            (
                "tauri://localhost/".parse().unwrap(),
                "tauri://localhost/#/tools",
                "tauri://other/",
            ),
        ] {
            assert!(allows_navigation(&app, &good.parse().unwrap()));
            assert!(!allows_navigation(&app, &bad.parse().unwrap()));
            for blocked in [
                "https://example.com/",
                "file:///tmp/index.html",
                "about:blank",
            ] {
                assert!(!allows_navigation(&app, &blocked.parse().unwrap()));
            }
        }
    }
}
