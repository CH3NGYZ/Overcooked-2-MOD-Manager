use std::sync::atomic::{AtomicU8, Ordering};

// Cancellation and update ownership share one atomic state to avoid a check/store race.
#[derive(Default)]
struct DownloadControl(AtomicU8);

const DOWNLOAD_IDLE: u8 = 0;

const DOWNLOAD_CANCELLED: u8 = 1;

const UPDATE_RUNNING: u8 = 2;

impl DownloadControl {
    fn start(&self) {
        let _ = self.0.compare_exchange(
            DOWNLOAD_CANCELLED,
            DOWNLOAD_IDLE,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }

    fn cancel(&self) -> bool {
        self.set_unless_updating(DOWNLOAD_CANCELLED)
    }

    fn set_unless_updating(&self, next: u8) -> bool {
        let mut state = self.0.load(Ordering::Acquire);
        while state != UPDATE_RUNNING {
            match self
                .0
                .compare_exchange_weak(state, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(actual) => state = actual,
            }
        }
        false
    }

    fn updating(&self) -> bool {
        self.0.load(Ordering::Acquire) == UPDATE_RUNNING
    }

    fn begin_update(&self) -> Option<AppUpdateGuard<'_>> {
        self.set_unless_updating(UPDATE_RUNNING)
            .then(|| AppUpdateGuard(self))
    }
}

pub struct AppUpdateGuard<'a>(&'a DownloadControl);

impl Drop for AppUpdateGuard<'_> {
    fn drop(&mut self) {
        self.0.0.store(DOWNLOAD_IDLE, Ordering::Release);
    }
}

static DOWNLOAD_CONTROL: DownloadControl = DownloadControl(AtomicU8::new(DOWNLOAD_IDLE));

pub fn cancel_download() -> bool {
    DOWNLOAD_CONTROL.cancel()
}

pub fn is_download_cancelled() -> bool {
    DOWNLOAD_CONTROL.0.load(Ordering::Acquire) == DOWNLOAD_CANCELLED
}

pub fn start_download() {
    log::debug!("开始新的下载任务，清除取消状态");
    DOWNLOAD_CONTROL.start();
}

pub fn app_update_in_progress() -> bool {
    DOWNLOAD_CONTROL.updating()
}

pub fn begin_app_update() -> Result<AppUpdateGuard<'static>, String> {
    DOWNLOAD_CONTROL
        .begin_update()
        .ok_or_else(|| "update.inProgress".into())
}

#[cfg(test)]
mod tests;
