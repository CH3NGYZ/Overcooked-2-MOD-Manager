use super::UpdateTarget;
use super::channels::{
    INSTALLER_GITEE_OWNER, INSTALLER_GITEE_REPO, INSTALLER_GITHUB_OWNER, INSTALLER_GITHUB_REPO,
    InstallerRegion,
};
use crate::http_diagnostics::HttpRequestExt;
use crate::models::{AppUpdateInfo, Provider};
use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, HeaderMap};
use semver::Version;
use std::time::Duration;

const RECOVERY_PROBE_TIMEOUT: Duration = Duration::from_millis(1200);

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Destination {
    Website,
    Alternate,
}

pub(crate) fn release_list_url(region: InstallerRegion) -> String {
    match region {
        InstallerRegion::MainlandChina => {
            format!("https://gitee.com/{INSTALLER_GITEE_OWNER}/{INSTALLER_GITEE_REPO}/releases")
        }
        InstallerRegion::International => {
            format!("https://github.com/{INSTALLER_GITHUB_OWNER}/{INSTALLER_GITHUB_REPO}/releases")
        }
    }
}

pub(crate) fn website_recovery_url(locale: &str) -> String {
    let locale = match locale {
        "zh-CN" | "zh-TW" | "en" | "ja" | "ko" => locale,
        _ => "en",
    };
    format!("https://oc2modmanager.ch3ng.top/?lang={locale}#download")
}

pub(crate) async fn alternate_recovery_url(
    region: InstallerRegion,
    target: UpdateTarget,
    update: &AppUpdateInfo,
) -> String {
    let github = release_list_url(InstallerRegion::International);
    let Ok(version) = Version::parse(&update.version) else {
        return github;
    };
    if !version.pre.is_empty() || !version.build.is_empty() {
        return github;
    }
    if region == InstallerRegion::MainlandChina
        && let Some(package) = gitee_recovery_package(target, update, &version)
        && let Ok(client) = crate::releases::build_download_client(RECOVERY_PROBE_TIMEOUT)
        && let Ok(response) = client
            .head(package)
            .timeout(RECOVERY_PROBE_TIMEOUT)
            .send_logged_metadata()
            .await
        && response.status() == reqwest::StatusCode::OK
        && response.url().scheme() == "https"
        && has_installer_headers(response.headers(), target.kind)
    {
        return format!("{}/tag/v{version}", release_list_url(region));
    }
    format!("{github}/tag/v{version}")
}

fn gitee_recovery_package(
    target: UpdateTarget,
    update: &AppUpdateInfo,
    version: &Version,
) -> Option<String> {
    if !matches!(target.kind, "exe" | "msi")
        || update.installer_type != target.kind
        || update.provider != Some(Provider::Gitee)
        || !target.matches_name(&update.installer_name, version)
    {
        return None;
    }
    let package = format!(
        "{}/download/v{version}/{}",
        release_list_url(InstallerRegion::MainlandChina),
        update.installer_name
    );
    (update.installer_url.as_deref() == Some(package.as_str())).then_some(package)
}

fn has_installer_headers(headers: &HeaderMap, kind: &str) -> bool {
    let length = headers
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok());
    let media_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .map(str::trim)
        .unwrap_or_default()
        .to_ascii_lowercase();
    length.is_some_and(|bytes| bytes > 0)
        && (media_type == "application/octet-stream"
            || matches!(
                (kind, media_type.as_str()),
                (
                    "exe",
                    "application/x-msdownload"
                        | "application/x-dosexec"
                        | "application/vnd.microsoft.portable-executable"
                ) | (
                    "msi",
                    "application/x-msi"
                        | "application/x-ms-installer"
                        | "application/x-ole-storage"
                )
            ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASES: &str = "https://github.com/CH3NGYZ/Overcooked-2-MOD-Manager/releases";

    fn update(version: &str, installer_type: &str, installer_name: &str) -> AppUpdateInfo {
        let mirror = release_list_url(InstallerRegion::MainlandChina);
        serde_json::from_value(serde_json::json!({
            "version": version,
            "current_version": "8.0.0",
            "installer_type": installer_type,
            "installer_name": installer_name,
            "provider": "gitee",
            "installer_url": format!("{mirror}/download/v{version}/{installer_name}"),
        }))
        .unwrap()
    }

    #[test]
    fn website_only_uses_supported_locale_values() {
        for locale in ["zh-CN", "zh-TW", "en", "ja", "ko"] {
            assert_eq!(
                website_recovery_url(locale),
                format!("https://oc2modmanager.ch3ng.top/?lang={locale}#download")
            );
        }
        assert_eq!(
            website_recovery_url("ja&extra=bad"),
            "https://oc2modmanager.ch3ng.top/?lang=en#download"
        );
    }

    #[tokio::test]
    async fn alternate_opens_github_when_the_candidate_is_not_a_verified_manual_installer() {
        for (region, os, kind, name) in [
            (
                InstallerRegion::International,
                "windows",
                "exe",
                "OC2ModManager_9.0.0_x64-setup.exe",
            ),
            (
                InstallerRegion::MainlandChina,
                "windows",
                "msi",
                "other package.msi",
            ),
            (
                InstallerRegion::MainlandChina,
                "macos",
                "app",
                "OC2ModManager_x64.app.tar.gz",
            ),
        ] {
            let target = UpdateTarget::from_parts(os, "x86_64", Some(kind)).unwrap();
            assert_eq!(
                alternate_recovery_url(region, target, &update("9.0.0", kind, name)).await,
                format!("{RELEASES}/tag/v9.0.0")
            );
        }
    }

    #[tokio::test]
    async fn alternate_falls_back_to_release_list_for_noncanonical_tags() {
        let target = UpdateTarget::from_parts("macos", "aarch64", Some("app")).unwrap();
        for version in ["9.0.0/escape", "9.0.0?x=y", "9.0.0-beta.1", "9.0.0+build"] {
            assert_eq!(
                alternate_recovery_url(
                    InstallerRegion::MainlandChina,
                    target,
                    &update(version, "app", "OC2ModManager_aarch64.app.tar.gz")
                )
                .await,
                RELEASES
            );
        }
    }

    #[test]
    fn gitee_probe_requires_the_checked_installer_and_exact_release_tag() {
        let version = Version::new(9, 0, 0);
        for (kind, name) in [
            ("exe", "OC2ModManager_9.0.0_x64-setup.exe"),
            ("msi", "OC2ModManager_9.0.0_x64_en-US.msi"),
        ] {
            let target = UpdateTarget::from_parts("windows", "x86_64", Some(kind)).unwrap();
            let mut info = update("9.0.0", kind, name);
            assert_eq!(
                gitee_recovery_package(target, &info, &version),
                info.installer_url
            );
            for bad_url in [
                format!("https://gitee.com/other/repo/releases/download/v9.0.0/{name}"),
                format!("{}?key=token", info.installer_url.as_deref().unwrap()),
                info.installer_url
                    .as_deref()
                    .unwrap()
                    .replace("v9.0.0", "latest"),
            ] {
                info.installer_url = Some(bad_url);
                assert_eq!(gitee_recovery_package(target, &info, &version), None);
            }
        }
    }

    #[test]
    fn gitee_probe_rejects_html_success_pages_and_empty_responses() {
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, "application/octet-stream".parse().unwrap());
        headers.insert(CONTENT_LENGTH, "42".parse().unwrap());
        assert!(has_installer_headers(&headers, "exe"));
        headers.insert(CONTENT_TYPE, "application/x-ole-storage".parse().unwrap());
        assert!(has_installer_headers(&headers, "msi"));
        assert!(!has_installer_headers(&headers, "exe"));
        headers.insert(CONTENT_TYPE, "text/html; charset=utf-8".parse().unwrap());
        assert!(!has_installer_headers(&headers, "exe"));
        headers.insert(CONTENT_TYPE, "application/x-msi".parse().unwrap());
        headers.insert(CONTENT_LENGTH, "0".parse().unwrap());
        assert!(!has_installer_headers(&headers, "msi"));
    }
}
