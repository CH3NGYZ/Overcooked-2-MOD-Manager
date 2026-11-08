pub(crate) mod cache;
pub(crate) mod channels;
pub(crate) mod download;
pub(crate) mod manifest;

use crate::models::AppUpdateInfo;
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
    if version != info.version
        || downloaded_name != Some(expected_name)
        || signature.trim().is_empty()
        || info.installer_url.as_deref() != Some(download_url.as_str())
        || signature != info.installer_signature
    {
        return Err("update.manifestPlatformMismatch".into());
    }
    Ok(())
}

#[cfg(any(windows, test))]
pub(crate) fn windows_installer_args(
    kind: &str,
    executable: &std::path::Path,
    target_executable: &std::path::Path,
    log: &std::path::Path,
) -> Result<Vec<std::ffi::OsString>, String> {
    use std::ffi::OsString;
    let directory = executable
        .parent()
        .ok_or("migrate.invalidInstallDirectory")?;
    let target_directory = target_executable
        .parent()
        .ok_or("migrate.invalidInstallDirectory")?;
    if directory.as_os_str().is_empty()
        || target_directory.as_os_str().is_empty()
        || [directory, target_directory, log].iter().any(|path| {
            path.as_os_str()
                .to_string_lossy()
                .contains(['"', '\r', '\n', '\0'])
        })
    {
        return Err("migrate.invalidInstallDirectory".into());
    }
    match kind {
        "exe" => {
            let mut target = OsString::from("/D=");
            target.push(directory);
            // Updater joins raw parameters; NSIS /D must be last and unquoted.
            Ok(vec![OsString::from("/OC2AutoLaunch"), target])
        }
        "msi" => {
            let mut target = OsString::from("INSTALLDIR=\"");
            target.push(target_directory);
            target.push("\"");
            let mut log_path = OsString::from("\"");
            log_path.push(log);
            log_path.push("\"");
            let mut args = vec![
                OsString::from("AUTOLAUNCHAPP=1"),
                target,
                OsString::from("/norestart"),
                OsString::from("/L*v"),
                log_path,
            ];
            if directory != target_directory {
                // MSI's textual path test cannot detect junction aliases.
                let mut source = OsString::from("OC2MIGRATEFROM=\"");
                source.push(directory);
                source.push("\"");
                args.push(source);
            }
            Ok(args)
        }
        _ => Err("update.formatUnsupported".into()),
    }
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
