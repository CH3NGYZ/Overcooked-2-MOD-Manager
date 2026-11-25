#[cfg(windows)]
pub(crate) mod admin_consent;
pub(crate) mod cache;
pub(crate) mod channels;
pub(crate) mod distribution;
pub(crate) mod download;
pub(crate) mod manifest;
pub(crate) mod receipt;
pub(crate) mod recovery_links;
pub(crate) mod staged;
#[cfg(windows)]
pub(crate) mod windows_session;

use crate::models::{AppUpdateInfo, AppUpdateInstallMode};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use tauri::utils::{config::BundleType, platform::bundle_type};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UpdateTarget {
    os: &'static str,
    arch: &'static str,
    pub(crate) kind: &'static str,
}

impl UpdateTarget {
    pub(crate) fn current() -> Result<Self, String> {
        distribution::require_standalone()?;
        // 只有两种安装包形态参与自动更新：Windows 的 NSIS / MSI 与 macOS 的 .app。
        // DEB / RPM / AppImage 随 Linux 支持一起移除，落到 `_ => None` 后由
        // `from_parts` 拒绝，不会尝试下载一个本项目不再产出的包。
        let bundle = match bundle_type() {
            Some(BundleType::Nsis) => Some("exe"),
            Some(BundleType::Msi) => Some("msi"),
            Some(BundleType::App) => Some("app"),
            _ => None,
        };
        Self::from_parts(std::env::consts::OS, std::env::consts::ARCH, bundle)
    }

    pub(crate) fn from_parts(os: &str, arch: &str, bundle: Option<&str>) -> Result<Self, String> {
        let arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return Err("update.platformUnsupported".into()),
        };
        let (os, kind) = match (os, bundle) {
            ("windows", Some("msi")) => ("windows", "msi"),
            ("windows", None | Some("exe")) => ("windows", "exe"),
            ("macos", None | Some("app")) => ("darwin", "app"),
            _ => return Err("update.formatUnsupported".into()),
        };
        Ok(Self { os, arch, kind })
    }

    pub(crate) fn manifest_key(self) -> String {
        let kind = if self.kind == "exe" {
            "nsis"
        } else {
            self.kind
        };
        format!("{}-{}-{kind}", self.os, self.arch)
    }

    pub(crate) fn install_mode(self) -> AppUpdateInstallMode {
        use AppUpdateInstallMode::{Deferred, Manual};

        if tauri::is_dev() {
            return Manual;
        }
        match (self.os, self.kind, bundle_type()) {
            #[cfg(windows)]
            ("windows", "exe", Some(BundleType::Nsis))
            | ("windows", "msi", Some(BundleType::Msi)) => {
                use windows_session::PermissionRoute;

                match windows_session::permission_route() {
                    Ok(PermissionRoute::User) => Deferred,
                    Ok(PermissionRoute::RestartAsUser) => AppUpdateInstallMode::RestartAsUser,
                    Ok(PermissionRoute::Administrator)
                        if self.kind == "exe" && staged::windows::can_install_as_admin() =>
                    {
                        AppUpdateInstallMode::InstallAsAdministrator
                    }
                    Ok(PermissionRoute::Administrator) => Manual,
                    Err(error) => {
                        log::warn!("[update] 无法确认 Windows 更新权限：{error}");
                        Manual
                    }
                }
            }
            ("darwin", "app", Some(BundleType::App))
                if std::env::current_exe()
                    .ok()
                    .is_some_and(|exe| receipt::bundle_root(&exe).is_some()) =>
            {
                Deferred
            }
            _ => Manual,
        }
    }

    pub(crate) fn matches_name(self, name: &str, version: &Version) -> bool {
        if name.contains(['/', '\\']) {
            return false;
        }
        let name = name.to_ascii_lowercase();
        let prefix = format!("oc2modmanager_{version}_").to_ascii_lowercase();
        let arches: &[&str] = match self.arch {
            "x86_64" => &["x64", "x86_64", "amd64"],
            "aarch64" => &["aarch64", "arm64"],
            _ => return false,
        };
        match (self.os, self.kind) {
            ("windows", "exe") => arches
                .iter()
                .any(|arch| name == format!("{prefix}{arch}-setup.exe")),
            ("windows", "msi") => arches.iter().any(|arch| {
                name.strip_prefix(&format!("{prefix}{arch}_"))
                    .and_then(|value| value.strip_suffix(".msi"))
                    .is_some_and(|locale| {
                        let bytes = locale.as_bytes();
                        bytes.len() == 5
                            && bytes[2] == b'-'
                            && bytes[..2].iter().all(u8::is_ascii_alphabetic)
                            && bytes[3..].iter().all(u8::is_ascii_alphabetic)
                    })
            }),
            ("darwin", "app") => arches.iter().any(|arch| {
                name == format!("oc2modmanager_{arch}.app.tar.gz")
                    || name == format!("{prefix}{arch}.app.tar.gz")
            }),
            _ => false,
        }
    }
}

pub(crate) fn verify_download(bytes: &[u8], signature: &str, version: &str) -> Result<(), String> {
    let config: serde_json::Value = serde_json::from_str(include_str!("../tauri.conf.json"))
        .map_err(|_| "update.publicKeyMissing")?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("update.publicKeyMissing")?;
    verify_with_key(bytes, signature, key, version)
}

pub(crate) fn verify_with_key(
    bytes: &[u8],
    encoded_signature: &str,
    key: &str,
    version: &str,
) -> Result<(), String> {
    let verify = || -> Result<(), String> {
        let key = String::from_utf8(B64.decode(key.trim()).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let signature = String::from_utf8(
            B64.decode(encoded_signature.trim())
                .map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let key = PublicKey::decode(&key).map_err(|e| e.to_string())?;
        let signature = Signature::decode(&signature).map_err(|e| e.to_string())?;
        key.verify(bytes, &signature, true)
            .map_err(|e| e.to_string())?;
        // The global signature authenticates the comment, including the version.
        let signed_version = signature
            .trusted_comment()
            .split('\t')
            .find_map(|field| field.strip_prefix("version:"))
            .ok_or("missing signed version")?;
        let signed =
            Version::parse(signed_version.trim_start_matches('v')).map_err(|e| e.to_string())?;
        let announced =
            Version::parse(version.trim_start_matches('v')).map_err(|e| e.to_string())?;
        if signed != announced {
            return Err("signed version does not match the selected release".into());
        }
        Ok(())
    };
    verify().map_err(|error| {
        log::warn!("程序更新签名校验失败：{error}");
        "update.signatureInvalid".into()
    })
}

pub(crate) fn updater_error_details(error: &tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error;
    match error {
        Error::Io(error) => format!(
            "io: kind={:?}, os_code={:?}",
            error.kind(),
            error.raw_os_error()
        ),
        Error::Reqwest(error) => format!(
            "http: status={:?}, timeout={}, connect={}, body={}, decode={}",
            error.status().map(|status| status.as_u16()),
            error.is_timeout(),
            error.is_connect(),
            error.is_body(),
            error.is_decode(),
        ),
        Error::Serialization(error) => format!(
            "json: kind={:?}, line={}, column={}",
            error.classify(),
            error.line(),
            error.column(),
        ),
        Error::PreviousAppNotRestored(path) => {
            format!(
                "previous_app_not_restored: backup={:?}",
                crate::file_check::log_path(path)
            )
        }
        _ => match error {
            Error::ReleaseNotFound => "release_not_found",
            Error::TargetNotFound(_) | Error::TargetsNotFound(_) => "target_not_found",
            Error::SignatureUtf8(_) | Error::Base64(_) | Error::Minisign(_) => "invalid_signature",
            Error::SignedVersionMismatch { .. } => "signed_version_mismatch",
            Error::MissingSignedVersion => "missing_signed_version",
            Error::BinaryNotFoundInArchive => "binary_not_found_in_archive",
            Error::InvalidUpdaterFormat => "invalid_updater_format",
            Error::TempDirNotFound | Error::TempDirNotOnSameMountPoint => {
                "temporary_directory_unavailable"
            }
            Error::InsecureTransportProtocol => "insecure_transport",
            Error::InvalidHeaderName(_) | Error::InvalidHeaderValue(_) => "invalid_header",
            Error::AuthenticationFailed => "authentication_failed",
            _ => "updater_error",
        }
        .to_owned(),
    }
}

pub(crate) fn validate_update_metadata(
    version: &str,
    download_url: &url::Url,
    signature: &str,
    info: &AppUpdateInfo,
) -> Result<(), String> {
    let expected_name = info.installer_name.as_str();
    let downloaded_name = download_url
        .path_segments()
        .and_then(|mut segments| segments.next_back());
    let version_matches = version == info.version;
    let file_matches = downloaded_name == Some(expected_name);
    let signature_present = !signature.trim().is_empty();
    let url_matches = info.installer_url.as_deref() == Some(download_url.as_str());
    let signature_matches = signature == info.installer_signature;
    if !(version_matches && file_matches && signature_present && url_matches && signature_matches) {
        log::warn!(
            "安装器更新清单复核未通过：version_matches={version_matches}，file_matches={file_matches}，signature_present={signature_present}，url_matches={url_matches}，signature_matches={signature_matches}"
        );
        return Err("update.manifestPlatformMismatch".into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
fn invalid_argument_path(path: &std::path::Path) -> bool {
    path.as_os_str()
        .to_string_lossy()
        .contains(['"', '\r', '\n', '\0'])
}

#[cfg(any(windows, test))]
fn installer_directory(executable: &std::path::Path) -> Result<&std::path::Path, String> {
    executable
        .parent()
        .filter(|path| !path.as_os_str().is_empty() && !invalid_argument_path(path))
        .ok_or_else(|| "migrate.invalidInstallDirectory".into())
}

#[cfg(any(windows, test))]
fn deferred_nsis_args(directory: &std::path::Path, pid: u32) -> std::ffi::OsString {
    let mut arguments = std::ffi::OsString::from(format!("/S /UPDATE /OC2WaitForPid={pid} /D="));
    arguments.push(directory);
    arguments
}

#[cfg(test)]
#[path = "app_update/test_support.rs"]
mod test_support;

#[cfg(test)]
pub(crate) use test_support::{TEST_KEY, TEST_SIGNATURE};

#[cfg(test)]
#[path = "app_update/tests.rs"]
mod tests;

#[cfg(test)]
mod installer_update_tests;
