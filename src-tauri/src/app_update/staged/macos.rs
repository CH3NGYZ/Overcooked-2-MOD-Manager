use crate::app_update::download::VerifiedInstallerDownload;
use crate::app_update::receipt::{Attempt, Outcome, bundle_root};
use crate::models::AppUpdateInfo;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

mod validation;

pub(super) struct Prepared {
    download: VerifiedInstallerDownload,
    executable: PathBuf,
    bundle_identifier: String,
}

impl Prepared {
    pub(super) fn prepare(
        download: VerifiedInstallerDownload,
        info: &AppUpdateInfo,
    ) -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|_| "update.stageFailed")?;
        let bundle = bundle_root(&executable).ok_or("update.formatUnsupported")?;
        let bundle_identifier = validation::installed_bundle_identifier(bundle)
            .and_then(|identifier| {
                validation::verify_archive(&download.bytes, &info.version, &identifier)?;
                Ok(identifier)
            })
            .map_err(|error| {
                log::error!("macOS 更新包预检失败：版本={}，原因={error}", info.version);
                "update.stageFailed"
            })?;
        Ok(Self {
            download,
            executable,
            bundle_identifier,
        })
    }

    pub(super) fn source(&self) -> &Path {
        &self.executable
    }

    pub(super) fn destination(&self) -> &Path {
        &self.executable
    }

    pub(super) fn apply(&self, app: &AppHandle, attempt: &Attempt) -> Result<(), String> {
        let bundle = bundle_root(&self.executable).ok_or("update.formatUnsupported")?;
        let _lock = attempt.lock_work()?;
        if let Some(window) = app.get_webview_window("main") {
            if let Err(error) = window.hide() {
                log::warn!("无法在应用更新前隐藏主窗口：{error}");
            }
        }
        let verified_install = self
            .download
            .update
            .install(&self.download.bytes)
            .map_err(|error| crate::app_update::updater_error_details(&error))
            .and_then(|()| {
                validation::verify_installed_app(bundle, &attempt.version, &self.bundle_identifier)
                    .map_err(|error| error.to_string())
            });
        if let Err(error) = verified_install {
            log::error!(
                "macOS 更新失败：尝试={}，版本={}，原因={error}",
                attempt.id,
                attempt.version
            );
            let _ = attempt.record_outcome(Outcome::Failed);
            restore_window(app);
            return Err("update.installFailedManual".into());
        }

        let _ = attempt.record_outcome(Outcome::Installed);
        if attempt.restart {
            super::release_single_instance(app);
            if let Err(error) = reopen(bundle) {
                log::error!(
                    "macOS 更新已安装，重新打开失败：尝试={}，原因={error}",
                    attempt.id
                );
                let _ = attempt.record_outcome(Outcome::RestartFailed);
                let locale = crate::manager_data::native_notice_language(None);
                let path = crate::display_path::display_path(bundle);
                let message = super::native_message("restartFailed", &locale)
                    .replace("{reason}", &format!("{path}\n{error}"));
                app.dialog()
                    .message(message)
                    .title(app.package_info().name.clone())
                    .kind(MessageDialogKind::Warning)
                    .blocking_show();
            }
            log::logger().flush();
            std::process::exit(0);
        }
        Ok(())
    }
}

fn restore_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Err(error) = window.show() {
            log::warn!("无法重新显示更新窗口：{error}");
        }
        if let Err(error) = window.set_focus() {
            log::warn!("无法聚焦更新窗口：{error}");
        }
    }
}

fn reopen(bundle: &Path) -> std::io::Result<()> {
    let status = Command::new("/usr/bin/open")
        .arg("-n")
        .arg(bundle)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!("Launch Services: {status}")))
    }
}
