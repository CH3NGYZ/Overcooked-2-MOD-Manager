//! This project's MSIX distribution is updated by Microsoft Store, never NSIS/MSI.
//! Package identity is independent of install path, architecture and bundle metadata.
use crate::models::{AppDistribution, AppUpdateCheckResult};
use std::sync::OnceLock;

static DISTRIBUTION: OnceLock<Result<AppDistribution, String>> = OnceLock::new();

pub(crate) const STORE_UPDATES_URI: &str = "ms-windows-store://downloadsandupdates";

pub(crate) fn current() -> Result<AppDistribution, String> {
    DISTRIBUTION.get_or_init(detect).clone()
}

pub(crate) fn require_standalone() -> Result<(), String> {
    match current()? {
        AppDistribution::Standalone => Ok(()),
        AppDistribution::MicrosoftStore => Err("update.microsoftStoreOnly".into()),
    }
}

pub(crate) fn store_check(region: &str) -> AppUpdateCheckResult {
    // No GitHub version is a statement about Store availability. This only tells
    // the UI which updater owns the process; it does not claim "up to date".
    AppUpdateCheckResult {
        update: None,
        channels: Vec::new(),
        all_failed: false,
        region: region.into(),
        distribution: AppDistribution::MicrosoftStore,
    }
}

#[cfg(windows)]
fn detect() -> Result<AppDistribution, String> {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentPackageFamilyName(length: *mut u32, name: *mut u16) -> i32;
    }
    let mut length = 0;
    // A sizing call is sufficient: INSUFFICIENT_BUFFER proves package identity.
    // A failed probe must not silently allow a standalone installer to run.
    let status = unsafe { GetCurrentPackageFamilyName(&mut length, std::ptr::null_mut()) };
    classify_probe(status, length)
}

#[cfg(not(windows))]
fn detect() -> Result<AppDistribution, String> {
    Ok(AppDistribution::Standalone)
}

#[cfg(any(windows, test))]
fn classify_probe(status: i32, length: u32) -> Result<AppDistribution, String> {
    match (status, length) {
        (15700, _) => Ok(AppDistribution::Standalone), // APPMODEL_ERROR_NO_PACKAGE
        (122, 2..) => Ok(AppDistribution::MicrosoftStore), // ERROR_INSUFFICIENT_BUFFER
        _ => Err(crate::locale_tag::with_params(
            "update.distributionUnavailable",
            &[(
                "reason",
                &format!("GetCurrentPackageFamilyName: {status}, length={length}"),
            )],
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_explicit_no_package_can_use_standalone_updater() {
        assert_eq!(
            classify_probe(15700, 0).unwrap(),
            AppDistribution::Standalone
        );
        assert_eq!(
            classify_probe(122, 48).unwrap(),
            AppDistribution::MicrosoftStore
        );
        for (status, length) in [(0, 0), (5, 0), (122, 0), (122, 1), (-1, 0)] {
            assert!(classify_probe(status, length).is_err());
        }
    }

    #[test]
    fn store_check_has_no_installer_channels_or_mandatory_update() {
        let result = store_check("cn");
        assert!(result.update.is_none());
        assert!(result.channels.is_empty());
        assert!(!result.all_failed);
        assert_eq!(result.region, "cn");
        assert_eq!(
            serde_json::to_value(&result).unwrap()["distribution"],
            "microsoft_store"
        );
        let legacy: AppUpdateCheckResult = serde_json::from_value(serde_json::json!({
            "update": null, "channels": [], "all_failed": false, "region": "global"
        }))
        .unwrap();
        assert_eq!(legacy.distribution, AppDistribution::Standalone);
    }

    #[test]
    fn unpackaged_test_host_is_detected_through_the_native_api() {
        assert_eq!(detect().unwrap(), AppDistribution::Standalone);
        assert_eq!(current().unwrap(), AppDistribution::Standalone);
    }
}
