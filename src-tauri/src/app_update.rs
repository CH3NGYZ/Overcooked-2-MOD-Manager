use crate::models::AppUpdateInfo;
use crate::models::{ReleaseAsset, ReleaseInfo};
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
        let bundle = match bundle_type() {
            Some(BundleType::Nsis) => Some("exe"),
            Some(BundleType::Msi) => Some("msi"),
            Some(BundleType::Deb) => Some("deb"),
            Some(BundleType::Rpm) => Some("rpm"),
            Some(BundleType::AppImage) => Some("appimage"),
            Some(BundleType::App) => Some("app"),
            _ if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some() => {
                Some("appimage")
            }
            _ => None,
        };
        Self::from_parts(std::env::consts::OS, std::env::consts::ARCH, bundle)
    }

    fn from_parts(os: &str, arch: &str, bundle: Option<&str>) -> Result<Self, String> {
        let arch = match arch {
            "x86_64" => "x86_64",
            "aarch64" => "aarch64",
            _ => return Err("当前平台或架构不支持自动更新，请手动更新".into()),
        };
        let (os, kind) = match (os, bundle) {
            ("windows", Some("msi")) => ("windows", "msi"),
            ("windows", None | Some("exe")) => ("windows", "exe"),
            ("linux", Some("appimage")) => ("linux", "appimage"),
            ("linux", Some("deb")) => ("linux", "deb"),
            ("linux", Some("rpm")) => ("linux", "rpm"),
            ("macos", None | Some("app")) => ("darwin", "app"),
            _ => return Err("当前安装格式不支持自动更新，请手动更新".into()),
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

    pub(crate) fn select_asset<'a>(
        self,
        release: &'a ReleaseInfo,
        version: &Version,
    ) -> Option<&'a ReleaseAsset> {
        release.assets.iter().find(|asset| {
            self.matches_name(&asset.name, version) && !asset.download_url.trim().is_empty()
        })
    }

    fn matches_name(self, name: &str, version: &Version) -> bool {
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
            ("linux", "appimage" | "deb") => arches
                .iter()
                .any(|arch| name == format!("{prefix}{arch}.{}", self.kind)),
            ("linux", "rpm") => {
                let rpm_prefix = format!("oc2modmanager-{version}-").to_ascii_lowercase();
                let rpm_arch = if self.arch == "x86_64" {
                    "x86_64"
                } else {
                    "aarch64"
                };
                name.strip_prefix(&rpm_prefix)
                    .and_then(|value| value.strip_suffix(&format!(".{rpm_arch}.rpm")))
                    .is_some_and(|revision| {
                        !revision.is_empty()
                            && revision
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || b"._+~".contains(&byte))
                    })
            }
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
        .map_err(|_| "更新签名公钥未配置")?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("更新签名公钥未配置")?;
    verify_with_key(bytes, signature, key, version)
}

fn verify_with_key(
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
        "更新签名校验失败，已停止安装".into()
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
    {
        return Err("更新清单与所选平台安装包不一致，已停止安装".into());
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
    let directory = executable.parent().ok_or("安装目录无效")?;
    let target_directory = target_executable.parent().ok_or("安装目录无效")?;
    if directory.as_os_str().is_empty()
        || target_directory.as_os_str().is_empty()
        || [directory, target_directory, log].iter().any(|path| {
            path.as_os_str()
                .to_string_lossy()
                .contains(['"', '\r', '\n', '\0'])
        })
    {
        return Err("安装目录无效".into());
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
        _ => Err("当前安装格式不支持自动更新，请手动更新".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(os: &str, arch: &str, kind: &str) -> UpdateTarget {
        UpdateTarget::from_parts(os, arch, Some(kind)).unwrap()
    }

    #[test]
    fn selects_the_supplied_platform_architecture_and_package_names() {
        let version = Version::new(1, 2, 0);
        let cases = [
            (
                target("windows", "x86_64", "exe"),
                "OC2ModManager_1.2.0_x64-setup.exe",
            ),
            (
                target("windows", "x86_64", "msi"),
                "OC2ModManager_1.2.0_x64_en-US.msi",
            ),
            (
                target("linux", "x86_64", "appimage"),
                "OC2ModManager_1.2.0_amd64.AppImage",
            ),
            (
                target("linux", "x86_64", "deb"),
                "OC2ModManager_1.2.0_amd64.deb",
            ),
            (
                target("linux", "x86_64", "rpm"),
                "OC2ModManager-1.2.0-1.x86_64.rpm",
            ),
            (
                target("macos", "aarch64", "app"),
                "OC2ModManager_aarch64.app.tar.gz",
            ),
            (
                target("macos", "x86_64", "app"),
                "OC2ModManager_x64.app.tar.gz",
            ),
        ];
        for (selected, name) in cases {
            assert!(
                selected.matches_name(name, &version),
                "{selected:?}: {name}"
            );
            for (other, other_name) in cases {
                if other != selected {
                    assert!(
                        !selected.matches_name(other_name, &version),
                        "wrong platform: {other_name}"
                    );
                }
            }
        }
    }

    #[test]
    fn mac_updates_use_an_arch_specific_app_archive_not_a_dmg() {
        let arm = target("macos", "aarch64", "app");
        let intel = target("macos", "x86_64", "app");
        let version = Version::new(1, 2, 0);
        assert!(intel.matches_name("OC2ModManager_1.2.0_x86_64.app.tar.gz", &version));
        assert!(arm.matches_name("OC2ModManager_1.2.0_aarch64.app.tar.gz", &version));
        for name in [
            "OC2ModManager_1.2.0_aarch64.dmg",
            "OC2ModManager.app.tar.gz",
            "OC2ModManager_x64.app.tar.gz.sig",
        ] {
            assert!(!arm.matches_name(name, &version));
        }
    }

    #[test]
    fn rejects_other_versions_and_keeps_rpm_revision_separate_from_app_version() {
        let rpm = target("linux", "x86_64", "rpm");
        let version = Version::new(1, 2, 0);
        assert!(rpm.matches_name("OC2ModManager-1.2.0-2.x86_64.rpm", &version));
        assert!(!rpm.matches_name("OC2ModManager-1.2.0-1.aarch64.rpm", &version));
        assert!(!rpm.matches_name("OC2ModManager-1.2.1-1.x86_64.rpm", &version));
        assert!(!rpm.matches_name("OC2ModManager-1.2.0-.x86_64.rpm", &version));
        assert!(
            !target("windows", "x86_64", "msi")
                .matches_name("OC2ModManager_1.2.0_x64_msi.msi", &version)
        );
    }

    #[test]
    fn manifest_keys_never_mix_linux_package_formats_or_mac_architectures() {
        for (os, arch, kind, key) in [
            ("windows", "x86_64", "exe", "windows-x86_64-nsis"),
            ("windows", "x86_64", "msi", "windows-x86_64-msi"),
            ("linux", "x86_64", "deb", "linux-x86_64-deb"),
            ("linux", "x86_64", "rpm", "linux-x86_64-rpm"),
            ("linux", "x86_64", "appimage", "linux-x86_64-appimage"),
            ("macos", "aarch64", "app", "darwin-aarch64-app"),
            ("macos", "x86_64", "app", "darwin-x86_64-app"),
        ] {
            assert_eq!(target(os, arch, kind).manifest_key(), key);
        }
        assert!(UpdateTarget::from_parts("linux", "x86_64", None).is_err());
        assert!(UpdateTarget::from_parts("windows", "i686", None).is_err());
    }

    #[test]
    fn tauri_windows_arguments_preserve_paths_and_only_use_success_hook_relaunch() {
        use std::path::Path;
        let executable = Path::new("C:/Users/Test User/管理器/OC2ModManager.exe");
        let log = Path::new("C:/Users/Test User/OC2ModManager/logs/update-msi.log");
        let nsis = windows_installer_args("exe", executable, executable, log).unwrap();
        assert_eq!(nsis, ["/OC2AutoLaunch", "/D=C:/Users/Test User/管理器"]);
        let raw = nsis.join(std::ffi::OsStr::new(" "));
        assert_eq!(raw, "/OC2AutoLaunch /D=C:/Users/Test User/管理器");
        assert!(!raw.to_string_lossy().contains('"'));
        assert_eq!(
            windows_installer_args("msi", executable, executable, log).unwrap(),
            [
                "AUTOLAUNCHAPP=1",
                "INSTALLDIR=\"C:/Users/Test User/管理器\"",
                "/norestart",
                "/L*v",
                "\"C:/Users/Test User/OC2ModManager/logs/update-msi.log\"",
            ]
        );
        for (kind, exe, log) in [
            ("app", executable, log),
            ("exe", Path::new("OC2ModManager.exe"), log),
            ("exe", Path::new("C:/bad\"path/OC2ModManager.exe"), log),
            ("msi", executable, Path::new("C:/bad\npath/update.log")),
        ] {
            assert!(windows_installer_args(kind, exe, exe, log).is_err());
        }
    }

    #[test]
    fn relocation_arguments_keep_the_old_directory_for_installer_safety_hooks() {
        use std::path::Path;
        let executable = Path::new("C:/game/BepInEx/plugins/Manager/OC2ModManager.exe");
        let target = Path::new("C:/Users/Test/AppData/Local/OC2ModManager/OC2ModManager.exe");
        let log = Path::new("C:/Users/Test/OC2ModManager/logs/update-msi.log");
        for kind in ["exe", "msi"] {
            let args = windows_installer_args(kind, executable, target, log).unwrap();
            assert!(
                args.iter()
                    .any(|arg| arg.to_string_lossy().contains("BepInEx/plugins/Manager"))
            );
            assert!(
                args.iter()
                    .any(|arg| arg == "/OC2AutoLaunch" || arg == "AUTOLAUNCHAPP=1")
            );
            assert!(!args.iter().any(|arg| arg == "/R" || arg == "/ARGS"));
            if kind == "msi" {
                assert!(
                    args.iter().any(
                        |arg| arg == "INSTALLDIR=\"C:/Users/Test/AppData/Local/OC2ModManager\""
                    )
                );
                assert!(
                    args.iter()
                        .any(|arg| arg == "OC2MIGRATEFROM=\"C:/game/BepInEx/plugins/Manager\"")
                );
            }
        }
        let alias = Path::new("C:/game/alias/Manager/OC2ModManager.exe");
        assert!(
            windows_installer_args("msi", alias, target, log)
                .unwrap()
                .iter()
                .any(|arg| arg == "OC2MIGRATEFROM=\"C:/game/alias/Manager\"")
        );
    }

    #[test]
    fn tauri_manifest_metadata_covers_all_desktop_formats_and_rejects_mismatches() {
        let cases = [
            (
                "windows",
                "x86_64",
                "exe",
                "OC2ModManager_1.2.0_x64-setup.exe",
            ),
            (
                "windows",
                "x86_64",
                "msi",
                "OC2ModManager_1.2.0_x64_en-US.msi",
            ),
            (
                "linux",
                "x86_64",
                "appimage",
                "OC2ModManager_1.2.0_amd64.AppImage",
            ),
            ("linux", "x86_64", "deb", "OC2ModManager_1.2.0_amd64.deb"),
            ("linux", "x86_64", "rpm", "OC2ModManager-1.2.0-1.x86_64.rpm"),
            (
                "macos",
                "aarch64",
                "app",
                "OC2ModManager_aarch64.app.tar.gz",
            ),
            ("macos", "x86_64", "app", "OC2ModManager_x64.app.tar.gz"),
        ];
        let platforms = cases
            .iter()
            .map(|(os, arch, kind, name)| {
                (
                    target(os, arch, kind).manifest_key(),
                    serde_json::json!({
                        "url": format!("https://example.com/releases/download/v1.2.0/{name}"),
                        "signature": TEST_SIGNATURE,
                    }),
                )
            })
            .collect::<serde_json::Map<_, _>>();
        let release: tauri_plugin_updater::RemoteRelease =
            serde_json::from_value(serde_json::json!({
                "version": "1.2.0", "platforms": platforms,
            }))
            .unwrap();
        for (os, arch, kind, name) in cases {
            let key = target(os, arch, kind).manifest_key();
            let url = release.download_url(&key).unwrap();
            let signature = release.signature(&key).unwrap();
            let info: AppUpdateInfo = serde_json::from_value(serde_json::json!({
                "version": "1.2.0", "current_version": "1.1.0",
                "installer_name": name, "installer_type": kind,
            }))
            .unwrap();
            assert!(validate_update_metadata("1.2.0", url, signature, &info).is_ok());
            assert!(validate_update_metadata("1.2.1", url, signature, &info).is_err());
            assert!(validate_update_metadata("1.2.0", url, " ", &info).is_err());
            let other = url::Url::parse("https://example.com/wrong-platform.exe").unwrap();
            assert!(validate_update_metadata("1.2.0", &other, signature, &info).is_err());
        }
        assert!(release.download_url("android-aarch64").is_err());
        assert!(release.download_url("ios-aarch64").is_err());
    }

    // Public fixtures generated solely for this test; no production private key.
    const TEST_KEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHB1YmxpYyB0ZXN0IGZpeHR1cmUKUldSUFF6SjBaWE4wSVZJQldNanBhWnBtUnlyRnhXS1JOL1NqY1d4NFd2K0hIZjQ4ZEFhd1pwQUsK";
    const TEST_SIGNATURE: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHB1YmxpYyB0ZXN0IGZpeHR1cmUKUlVSUFF6SjBaWE4wSVVZeUJxWmlZWlYzOVMraGtXWmIrQmtpT3EzVFNmdWcvYmd6UGFhZ0JMN28vZnRWMXMwSDNheGtwbXFxS3g5RmpMa0xzRUtPN2FXaDFFQkMxVjdWVHdJPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxCWZpbGU6T0MyTW9kTWFuYWdlcl9hYXJjaDY0LmFwcC50YXIuZ3oJdmVyc2lvbjoxLjIuMApzZ2JpVUw5YWIveVdXOVA0d0lNaVpsZzVJNlBHZzEzU3FudnVXV3B4N2x3dDBOTHRLRHUrcGY4MkhaUXRtSGs5emlrd2YwUEcvNnl4TGFyMG1iTUdCdz09Cg==";

    #[test]
    fn verifies_real_signatures_payloads_and_authenticated_versions() {
        let bytes = b"OC2 signed update fixture";
        assert!(verify_with_key(bytes, TEST_SIGNATURE, TEST_KEY, "1.2.0").is_ok());
        assert!(verify_with_key(bytes, TEST_SIGNATURE, TEST_KEY, "v1.2.0").is_ok());
        assert!(verify_with_key(b"tampered", TEST_SIGNATURE, TEST_KEY, "1.2.0").is_err());
        assert!(verify_with_key(bytes, TEST_SIGNATURE, TEST_KEY, "9.0.0").is_err());
        let comment = String::from_utf8(B64.decode(TEST_SIGNATURE).unwrap()).unwrap();
        for changed in [
            comment.replace("version:1.2.0", "version:9.0.0"),
            comment.replace("\tversion:1.2.0", ""),
        ] {
            assert!(verify_with_key(bytes, &B64.encode(changed), TEST_KEY, "9.0.0").is_err());
        }
        assert!(verify_with_key(bytes, "not a signature", TEST_KEY, "1.2.0").is_err());
    }
}
