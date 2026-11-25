use super::{installer, installer_directory, lock_file, matches_installed_version, wide};
use crate::app_update::receipt::{Attempt, Outcome};
use crate::app_update::recovery_links::{OFFICIAL_WEBSITE, official_release_url};
use crate::app_update::staged::native_message as message;
use crate::manager_data::native_notice_language;
use crate::shell_open::{self, WebPageKind};
use std::ffi::OsStr;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::process::Command;
use winapi::shared::winerror::WAIT_TIMEOUT;
use winapi::um::synchapi::WaitForSingleObject;
use winapi::um::winbase::WAIT_OBJECT_0;
use winapi::um::winuser::{MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND, MessageBoxW};
use windows::Win32::UI::Controls::{
    TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TD_ERROR_ICON, TaskDialogIndirect,
};
use windows::core::{PCWSTR, w};

const RELEASE_BUTTON: i32 = 100;
const WEBSITE_BUTTON: i32 = 101;

pub(super) fn after_install(attempt: &Attempt, outcome: Outcome) -> Result<(), String> {
    match outcome {
        Outcome::RebootRequired => {
            let locale = native_notice_language(None);
            show_notice(
                message("rebootRequired", &locale),
                MB_OK | MB_ICONINFORMATION,
            );
            return Ok(());
        }
        Outcome::Failed => {
            offer_manual_update(attempt, None);
            return Err("update.installFailedManual".into());
        }
        Outcome::Installed if attempt.restart => {}
        _ => return Ok(()),
    }
    let Err(error) = restart_verified(&attempt.destination, &attempt.version) else {
        return Ok(());
    };
    let receipt_error = attempt.record_outcome(Outcome::RestartFailed).err();
    installer::write_diagnostic(
        attempt,
        "windows-restart-result.txt",
        &format!("outcome={outcome:?} launch_error={error} receipt_error={receipt_error:?}"),
    );
    let locale = native_notice_language(None);
    let destination = crate::display_path::display_path(&attempt.destination);
    show_notice(
        &message("restartFailed", &locale).replace("{reason}", &format!("{destination}\n{error}")),
        MB_OK | MB_ICONINFORMATION,
    );
    Err(error)
}

fn restart_verified(path: &Path, version: &str) -> Result<(), String> {
    let _locked = lock_file(path).map_err(|error| error.to_string())?;
    if !matches_installed_version(path, version) {
        return Err(format!("Program does not match expected version {version}"));
    }
    let mut child = Command::new(path)
        .current_dir(installer_directory(path)?)
        .spawn()
        .map_err(|error| error.to_string())?;
    match unsafe { WaitForSingleObject(child.as_raw_handle().cast(), 1000) } {
        WAIT_TIMEOUT => Ok(()),
        WAIT_OBJECT_0 => Err(format!("Manager exited during startup: {:?}", child.wait())),
        _ => Err(io::Error::last_os_error().to_string()),
    }
}

pub(super) fn offer_manual_update(attempt: &Attempt, error: Option<&str>) {
    let locale = native_notice_language(None);
    let title = message("installFailedManual", &locale);
    let release = official_release_url(&attempt.version);
    let diagnostic =
        crate::display_path::display_path(&attempt.work_path().join(installer::WINDOWS_RESULT));
    let detail = error.map_or(diagnostic.clone(), |error| format!("{error}\n{diagnostic}"));
    let mut body = format!("{detail}\n\n{release}\n{OFFICIAL_WEBSITE}");
    loop {
        let button = match manual_dialog(title, &body, &locale) {
            Ok(button) => button,
            Err(error) => {
                eprintln!("OC2 update supervisor: could not show the update dialog: {error}");
                show_notice(&format!("{title}\n\n{body}"), MB_OK | MB_ICONERROR);
                return;
            }
        };
        let Some(url) = download_url(button, &release) else {
            return;
        };
        let Err(error) = shell_open::open_web_page(url, WebPageKind::Release) else {
            return;
        };
        eprintln!("OC2 update supervisor: could not open the update page: {error}");
        let reason = error.split_once('?').and_then(|(_, params)| {
            url::form_urlencoded::parse(params.as_bytes())
                .find(|(key, _)| key == "reason")
                .map(|(_, value)| value.into_owned())
        });
        let failed = message("downloadPageOpenFailed", &locale)
            .replace("{reason}", reason.as_deref().unwrap_or(&error));
        body = format!("{failed}\n\n{detail}\n\n{release}\n{OFFICIAL_WEBSITE}");
    }
}

fn download_url(button: i32, release: &str) -> Option<&str> {
    match button {
        RELEASE_BUTTON => Some(release),
        WEBSITE_BUTTON => Some(OFFICIAL_WEBSITE),
        _ => None,
    }
}

fn manual_dialog(title: &str, body: &str, locale: &str) -> windows::core::Result<i32> {
    let title = wide(OsStr::new(title));
    let body = wide(OsStr::new(body));
    let release = wide(OsStr::new(message("openReleasePage", locale)));
    let website = wide(OsStr::new(message("openWebsite", locale)));
    let buttons = [
        TASKDIALOG_BUTTON {
            nButtonID: RELEASE_BUTTON,
            pszButtonText: PCWSTR(release.as_ptr()),
        },
        TASKDIALOG_BUTTON {
            nButtonID: WEBSITE_BUTTON,
            pszButtonText: PCWSTR(website.as_ptr()),
        },
    ];
    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        pszWindowTitle: w!("OC2 MOD Manager"),
        Anonymous1: TASKDIALOGCONFIG_0 {
            pszMainIcon: TD_ERROR_ICON,
        },
        pszMainInstruction: PCWSTR(title.as_ptr()),
        pszContent: PCWSTR(body.as_ptr()),
        cButtons: buttons.len() as u32,
        pButtons: buttons.as_ptr(),
        nDefaultButton: RELEASE_BUTTON,
        ..Default::default()
    };
    let mut selected = 0;
    unsafe { TaskDialogIndirect(&config, Some(&mut selected), None, None)? };
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_manual_buttons_select_distinct_official_destinations() {
        let release = official_release_url("99.1.0");
        assert_eq!(
            download_url(RELEASE_BUTTON, &release),
            Some(release.as_str())
        );
        assert_eq!(
            download_url(WEBSITE_BUTTON, &release),
            Some(OFFICIAL_WEBSITE)
        );
        assert_eq!(download_url(0, &release), None);
    }
}

fn show_notice(body: &str, flags: u32) -> i32 {
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            wide(OsStr::new(body)).as_ptr(),
            wide(OsStr::new("OC2 MOD Manager")).as_ptr(),
            flags | MB_SETFOREGROUND,
        )
    }
}
