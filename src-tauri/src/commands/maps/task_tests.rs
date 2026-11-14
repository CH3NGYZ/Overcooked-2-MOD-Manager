use crate::app_state::AppState;
use crate::models::MapInfo;
use std::future::Future;
use tauri::State;

#[test]
fn refresh_map_sources_task_fits_the_windows_ipc_stack_budget() {
    // Inspect the real command type without invoking it or constructing State.
    // Release IPC passes this type through several by-value dispatch layers on
    // the Windows main thread, whose default reserved stack is only 1 MiB.
    fn check<F: Future<Output = Result<Vec<MapInfo>, String>> + Send>(
        _: impl FnOnce(
            State<'static, AppState>,
            Option<String>,
            Option<bool>,
            Option<String>,
            tauri::ipc::Channel<crate::map_sources::progress::MapTreeProgress>,
        ) -> F,
    ) {
        let bytes = std::mem::size_of::<F>();
        assert!(
            bytes <= 64 * 1024,
            "refresh_map_sources task is {bytes} bytes"
        );
    }
    check(super::refresh_map_sources);
}
