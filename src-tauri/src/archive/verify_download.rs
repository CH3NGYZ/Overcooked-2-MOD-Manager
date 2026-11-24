use super::DownloadReporter;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

struct CancelOnDrop(Arc<AtomicBool>);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

pub(crate) async fn verify_temp_download(
    path: PathBuf,
    reporter: &DownloadReporter,
    verify: impl FnOnce(&Path, &dyn Fn() -> bool) -> Result<(), String> + Send + 'static,
) -> Result<PathBuf, String> {
    let file = tempfile::TempPath::try_from_path(path).map_err(|_| "error.checkedFileMissing")?;
    reporter.verifying();
    let cancelled = Arc::new(AtomicBool::new(false));
    let _scope = CancelOnDrop(cancelled.clone());
    let file = tauri::async_runtime::spawn_blocking(move || {
        let is_cancelled = || cancelled.load(Ordering::Acquire);
        if is_cancelled() {
            return Err("download.cancelled".into());
        }
        verify(&file, &is_cancelled)?;
        Ok::<_, String>(file)
    })
    .await
    .map_err(|error| error.to_string())??;
    file.keep().map_err(|error| error.error.to_string())
}
