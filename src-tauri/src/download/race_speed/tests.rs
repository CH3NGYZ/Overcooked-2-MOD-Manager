use super::*;
use crate::archive::{DownloadProgress, DownloadReporter, DownloadRoute};
use crate::download::DownloadCandidate;
use futures_util::future::AbortHandle;
use std::sync::Arc;

fn measured(progress: DownloadProgress, rate: u64, now: Instant) -> ActiveSource {
    let reporter = Arc::new(DownloadReporter::new("test"));
    reporter.update(progress);
    let (abort, _) = AbortHandle::new_pair();
    let window = Duration::from_secs(10);
    let mut speed = SourceSpeed::default();
    speed.sample(now - window, 1, window);
    speed.sample(now, 1 + rate * 10, window);
    ActiveSource {
        candidate: DownloadCandidate {
            route: DownloadRoute::Proxy,
            url: "https://proxy.example/file".into(),
        },
        reporter,
        abort,
        speed,
        started: now - window,
    }
}

#[test]
fn keeps_peers_at_half_speed_and_leaves_joint_stalls_to_transport_timeouts() {
    let now = Instant::now();
    let source = |rate| measured(DownloadProgress::connecting("test"), rate, now);
    for (fastest, other, expected) in [(100, 50, None), (100, 49, Some((1, 100))), (0, 0, None)] {
        let sources = HashMap::from([(0, source(fastest)), (1, source(other))]);
        assert_eq!(switching_candidate(&sources, Some(0), None, 1024), expected);
    }
}

#[test]
fn cannot_rotate_away_a_progress_leader_or_a_fully_received_file() {
    let now = Instant::now();
    let source = |downloaded, total, finished, rate| {
        let mut progress = DownloadProgress::connecting("test");
        progress.downloaded = downloaded;
        progress.total = total;
        progress.finished = finished;
        measured(progress, rate, now)
    };
    let mut sources = HashMap::from([
        (0, source(600, None, false, 350)),
        (1, source(900, None, false, 0)),
    ]);
    assert_eq!(switching_candidate(&sources, Some(1), None, 1024), None);
    // No Content-Length on the completed route, but the Release supplies it.
    sources.insert(2, source(1000, None, false, 0));
    assert_eq!(
        switching_candidate(&sources, Some(2), Some(1000), 1024),
        Some((1, 350))
    );
    // When the expected file size isn't available, the source can report completion.
    sources.insert(1, source(900, None, true, 0));
    sources.insert(2, source(1000, Some(1000), false, 0));
    assert_eq!(switching_candidate(&sources, Some(2), None, 1024), None);
}
