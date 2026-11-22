//! Extend Tao's delegate for the native Quit actions that bypass window close.
//! AppKit caches which delegate methods exist, so reattach the same instance
//! after adding the optional selector.

use objc2::runtime::{AnyObject, AnyProtocol, Bool, Imp, Sel};
use objc2::{MainThreadMarker, ffi, sel};
use objc2_app_kit::{NSApp, NSApplication, NSApplicationTerminateReply};
use std::sync::OnceLock;
use tauri::AppHandle;

static APP: OnceLock<AppHandle> = OnceLock::new();

type ShouldTerminate =
    extern "C-unwind" fn(&AnyObject, Sel, &NSApplication) -> NSApplicationTerminateReply;

pub(crate) fn install(app: &AppHandle) -> tauri::Result<()> {
    let main_thread = MainThreadMarker::new()
        .ok_or_else(|| std::io::Error::other("macOS quit hook requires the main thread"))?;
    if APP.get().is_some() {
        return Ok(());
    }
    let native_app = NSApp(main_thread);
    let delegate = native_app
        .delegate()
        .ok_or_else(|| std::io::Error::other("macOS application delegate is missing"))?;
    let object: &AnyObject = delegate.as_ref();
    let class = object.class();
    let selector = sel!(applicationShouldTerminate:);
    if class.instance_method(selector).is_some() {
        return Err(std::io::Error::other("macOS delegate already handles quit requests").into());
    }
    let protocol = AnyProtocol::get(c"NSApplicationDelegate")
        .ok_or_else(|| std::io::Error::other("macOS application delegate protocol is missing"))?;
    let method =
        unsafe { ffi::protocol_getMethodDescription(protocol, selector, Bool::NO, Bool::YES) };
    if method.types.is_null()
        || !unsafe {
            ffi::class_addMethod(
                std::ptr::from_ref(class).cast_mut(),
                selector,
                std::mem::transmute::<ShouldTerminate, Imp>(should_terminate),
                method.types,
            )
        }
        .as_bool()
    {
        return Err(std::io::Error::other("cannot register the macOS quit hook").into());
    }
    let _ = APP.set(app.clone());
    native_app.setDelegate(None);
    native_app.setDelegate(Some(&delegate));
    Ok(())
}

extern "C-unwind" fn should_terminate(
    _delegate: &AnyObject,
    _selector: Sel,
    _sender: &NSApplication,
) -> NSApplicationTerminateReply {
    if let Some(app) = APP.get()
        && let Err(error) = crate::main_webview::request_quit(app)
    {
        log::error!("无法请求关闭管理器：{error}");
    }
    NSApplicationTerminateReply::TerminateCancel
}
