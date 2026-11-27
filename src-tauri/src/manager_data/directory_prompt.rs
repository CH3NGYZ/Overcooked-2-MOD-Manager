use super::{handoff, notice};
use crate::display_path::display_path;
use std::{
    ffi::{OsStr, OsString},
    os::windows::ffi::{OsStrExt, OsStringExt},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use windows::{
    Win32::{
        Foundation::ERROR_CANCELLED,
        System::Com::{
            CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize,
        },
        UI::{
            Controls::{
                TASKDIALOG_BUTTON, TASKDIALOGCONFIG, TASKDIALOGCONFIG_0, TD_WARNING_ICON,
                TDCBF_CANCEL_BUTTON, TDF_ALLOW_DIALOG_CANCELLATION, TaskDialogIndirect,
            },
            Shell::{
                FOS_DONTADDTORECENT, FOS_FORCEFILESYSTEM, FOS_NODEREFERENCELINKS,
                FOS_PATHMUSTEXIST, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog,
                SIGDN_FILESYSPATH,
            },
        },
    },
    core::{HRESULT, PCWSTR, w},
};

static MESSAGES: LazyLock<serde_json::Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../src/locale/tools/dataDirectoryNative.json"
    ))
    .expect("bundled configuration directory translations")
});

#[derive(Clone, Copy)]
pub(super) enum Issue {
    Unsafe,
    InvalidSelection,
    Unavailable,
    RecoveryBlocked,
}

pub(super) enum Choice {
    Directory(PathBuf),
    Retry,
    Cancel,
}

fn message(key: &str, language: &str) -> &'static str {
    MESSAGES[key][language]
        .as_str()
        .expect("complete configuration directory translations")
}

impl Issue {
    fn copy(self) -> (&'static str, &'static str, Option<&'static str>) {
        match self {
            Self::Unsafe => ("unsafeTitle", "unsafeBody", Some("choose")),
            Self::InvalidSelection => ("invalidTitle", "invalidBody", Some("choose")),
            Self::Unavailable => ("unavailableTitle", "unavailableBody", Some("retry")),
            Self::RecoveryBlocked => ("recoveryTitle", "recoveryBody", None),
        }
    }
}

const CONFIRM: i32 = 100;

pub(super) fn choose(source: Option<&Path>, issue: Issue) -> Choice {
    let language = notice::system_language().unwrap_or("en");
    let (title, body, action) = issue.copy();
    let title = message(title, language);
    let body = match source {
        Some(path) => format!("{}\n\n{}", message(body, language), display_path(path)),
        None => message(body, language).to_owned(),
    };
    let Some(action) = action else {
        handoff::show_error(&format!("{title}\n\n{body}"));
        return Choice::Cancel;
    };
    let action = message(action, language);
    match confirm(title, &body, action) {
        Ok(true) => (),
        Ok(false) => return Choice::Cancel,
        Err(error) => {
            eprintln!(
                "Could not show configuration directory prompt: {}",
                error.code()
            );
            handoff::show_error(&format!("{title}\n\n{body}"));
            return Choice::Cancel;
        }
    }
    match issue {
        Issue::Unsafe | Issue::InvalidSelection => match pick_folder(action) {
            Ok(path) => Choice::Directory(path),
            Err(error) if error.code() == HRESULT::from_win32(ERROR_CANCELLED.0) => Choice::Cancel,
            Err(error) => {
                eprintln!(
                    "Could not open configuration directory picker: {}",
                    error.code()
                );
                handoff::show_error(message("pickerFailed", language));
                Choice::Cancel
            }
        },
        Issue::Unavailable => Choice::Retry,
        Issue::RecoveryBlocked => Choice::Cancel,
    }
}

fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(Some(0)).collect()
}

fn confirm(title: &str, body: &str, action: &str) -> windows::core::Result<bool> {
    let title = wide(title);
    let body = wide(body);
    let action = wide(action);
    let button = TASKDIALOG_BUTTON {
        nButtonID: CONFIRM,
        pszButtonText: PCWSTR(action.as_ptr()),
    };
    let config = TASKDIALOGCONFIG {
        cbSize: std::mem::size_of::<TASKDIALOGCONFIG>() as u32,
        pszWindowTitle: w!("OC2 MOD Manager"),
        Anonymous1: TASKDIALOGCONFIG_0 {
            pszMainIcon: TD_WARNING_ICON,
        },
        pszMainInstruction: PCWSTR(title.as_ptr()),
        pszContent: PCWSTR(body.as_ptr()),
        dwFlags: TDF_ALLOW_DIALOG_CANCELLATION,
        dwCommonButtons: TDCBF_CANCEL_BUTTON,
        cButtons: 1,
        pButtons: &button,
        nDefaultButton: CONFIRM,
        ..Default::default()
    };
    let mut selected = 0;
    unsafe { TaskDialogIndirect(&config, Some(&mut selected), None, None)? };
    Ok(selected == CONFIRM)
}

struct Apartment;

impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn pick_folder(title: &str) -> windows::core::Result<PathBuf> {
    let title = wide(title);
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE).ok()?;
        let _apartment = Apartment;
        let picker: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
        let options = picker.GetOptions()?
            | FOS_PICKFOLDERS
            | FOS_FORCEFILESYSTEM
            | FOS_PATHMUSTEXIST
            | FOS_NODEREFERENCELINKS
            | FOS_DONTADDTORECENT;
        picker.SetOptions(options)?;
        picker.SetTitle(PCWSTR(title.as_ptr()))?;
        picker.Show(None)?;
        let path = picker.GetResult()?.GetDisplayName(SIGDN_FILESYSPATH)?;
        let directory = PathBuf::from(OsString::from_wide(path.as_wide()));
        CoTaskMemFree(Some(path.0.cast()));
        Ok(directory)
    }
}
