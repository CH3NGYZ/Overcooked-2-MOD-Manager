use crate::archive::{self, DownloadProgress, DownloadReporter, DownloadRoute};
use crate::models::NetworkConfig;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

pub(crate) mod control;
pub(crate) mod history;
pub(crate) mod planning;
mod race;
mod race_speed;

use race::race_candidates;
#[cfg(test)]
use race::{ActiveSource, leading_source};
#[cfg(test)]
use race_speed::{SourceSpeed, switching_candidate};

#[cfg(test)]
mod race_tests;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadCandidate {
    pub route: DownloadRoute,
    pub url: String,
    pub(crate) identity: Option<history::RouteIdentity>,
}

fn source_host(url: &str) -> Option<String> {
    let url = url::Url::parse(url).ok()?;
    url.host().map(|host| match url.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    })
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadExecution {
    pub max_parallelism: u32,
    pub race: bool,
}

pub struct DownloadContext<'a> {
    pub execution: Option<DownloadExecution>,
    pub label: &'a str,
    pub file_name: Option<&'a str>,
    pub expected_size: Option<u64>,
    pub(crate) race_session: Option<&'a history::RaceSession>,
}

pub trait DownloadOutput {
    fn discard(&self);
}

impl DownloadOutput for PathBuf {
    fn discard(&self) {
        let _ = std::fs::remove_file(self);
    }
}

impl DownloadOutput for Vec<u8> {
    fn discard(&self) {}
}

// A ready loser may already have closed its temporary file. Dropping its
// result must clean it just as dropping an in-flight archive future does.
struct CompletedDownload<T: DownloadOutput>(Option<T>);
impl<T: DownloadOutput> Drop for CompletedDownload<T> {
    fn drop(&mut self) {
        if let Some(value) = &self.0 {
            value.discard();
        }
    }
}

#[derive(Clone, Copy)]
struct RacePolicy {
    parallelism: usize,
    window: Duration,
    reference_speed: u64,
    tick: Duration,
    terminal_error: Option<&'static str>,
    soft_start: Duration,
    race: bool,
}

impl RacePolicy {
    fn from_network(network: &NetworkConfig) -> Self {
        Self {
            parallelism: 3,
            window: Duration::from_secs(10),
            reference_speed: 256 * 1024,
            tick: Duration::from_millis(250),
            terminal_error: None,
            soft_start: Duration::from_secs(10).min(network.first_byte_timeout() / 2),
            race: true,
        }
    }

    fn with_execution(mut self, execution: Option<DownloadExecution>) -> Result<Self, String> {
        if let Some(execution) = execution {
            if execution.max_parallelism == 0
                || execution.max_parallelism > 3
                || (!execution.race && execution.max_parallelism != 1)
            {
                return Err("proxy.validationFailed".into());
            }
            self.parallelism = execution.max_parallelism as usize;
            self.race = execution.race;
            if !execution.race {
                self.reference_speed = 0;
            }
        }
        Ok(self)
    }
}

impl Default for RacePolicy {
    fn default() -> Self {
        Self::from_network(&NetworkConfig::default())
    }
}

pub async fn try_download_candidates<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    is_cancelled: C,
    download: F,
    network: &NetworkConfig,
    context: Option<DownloadContext<'_>>,
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    try_download_candidates_with_progress(
        candidates,
        is_cancelled,
        download,
        network,
        context,
        archive::publish_download_progress,
    )
    .await
}

/// Uses the same race state machine with a caller-owned progress destination.
/// Multipart downloads aggregate one winning candidate per volume here.
pub(crate) async fn try_download_candidates_with_progress<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    is_cancelled: C,
    download: F,
    network: &NetworkConfig,
    context: Option<DownloadContext<'_>>,
    report_progress: impl FnMut(DownloadProgress),
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    let policy = RacePolicy::from_network(network)
        .with_execution(context.as_ref().and_then(|value| value.execution))?;
    race_candidates(
        candidates,
        is_cancelled,
        download,
        context,
        policy,
        report_progress,
    )
    .await
}

pub async fn try_download_candidates_until_error<T, F, Fut, C>(
    candidates: Vec<DownloadCandidate>,
    is_cancelled: C,
    download: F,
    terminal_error: &'static str,
    network: &NetworkConfig,
    execution: DownloadExecution,
) -> Result<T, String>
where
    T: DownloadOutput,
    F: FnMut(DownloadCandidate, Arc<DownloadReporter>) -> Fut,
    Fut: std::future::Future<Output = Result<T, String>>,
    C: FnMut() -> bool,
{
    race_candidates(
        candidates,
        is_cancelled,
        download,
        None,
        RacePolicy {
            terminal_error: Some(terminal_error),
            ..RacePolicy::from_network(network).with_execution(Some(execution))?
        },
        |_| {},
    )
    .await
}

#[cfg(test)]
#[path = "download_retention_tests.rs"]
mod retention_tests;

#[cfg(test)]
mod tests;
