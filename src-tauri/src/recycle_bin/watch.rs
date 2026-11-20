//! One OS subscription covers every domain that stages or removes recycled files.
use crate::directory_watch::DirectoryWatcher;
use std::path::PathBuf;
use tauri::{AppHandle, Emitter};

#[derive(Default)]
pub(crate) struct RecycleWatcher {
    directory: DirectoryWatcher<()>,
}

impl RecycleWatcher {
    pub(crate) fn watch(&self, app: &AppHandle, root: PathBuf) {
        let app = app.clone();
        self.directory.watch(
            (),
            vec![root.clone()],
            &[],
            move |event| {
                // A missing root is watched through its nearest existing parent.
                // Ignore unrelated user-data writes and all read-access events.
                event
                    .paths
                    .retain(|path| path.starts_with(&root) || root.starts_with(path));
                !event.paths.is_empty()
            },
            move |_, _| {
                if let Err(error) = app.emit("recycle-bin-changed", ()) {
                    log::debug!("发送回收站变化事件失败：{error}");
                }
            },
        );
    }
}
