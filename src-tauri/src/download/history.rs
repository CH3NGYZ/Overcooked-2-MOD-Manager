use super::DownloadCandidate;
use crate::models::Provider;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use url::Url;

#[cfg(test)]
mod tests;

const HISTORY_LIFETIME_MS: u64 = 24 * 60 * 60 * 1000;
const MAX_ENTRIES: usize = 768;
const MAX_BYTES: u64 = 512 * 1024;
const EXPLORE_EVERY: u8 = 4;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) enum HistoryPurpose {
    Raw,
    Release,
    Installer,
    Framework,
}

impl HistoryPurpose {
    fn as_str(self) -> &'static str {
        match self {
            Self::Raw => "raw",
            Self::Release => "release",
            Self::Installer => "installer",
            Self::Framework => "framework",
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct RouteIdentity {
    scope: String,
    digest: String,
}

impl RouteIdentity {
    pub(crate) fn for_candidate(
        region: &str,
        provider: &Provider,
        purpose: HistoryPurpose,
        url: &str,
        proxy: Option<&crate::network_policy::Proxy>,
    ) -> Option<Self> {
        let region = if crate::config::region_matches(
            Some(crate::config::REGION_CHINESE_MAINLAND),
            region,
        ) {
            "cn"
        } else {
            "intl"
        };
        let provider = match provider {
            Provider::Github => "github",
            Provider::Gitee => "gitee",
            Provider::Gitcode => "gitcode",
        };
        let scope = format!("{region}:{provider}:{}", purpose.as_str());
        let mut hasher = Sha256::new();
        match proxy {
            Some(proxy) => {
                hasher.update(b"\0proxy\0");
                hasher.update(proxy.id.as_bytes());
                hasher.update(b"\0");
                hasher.update(proxy.template.as_bytes());
            }
            None => {
                let origin = Url::parse(url).ok()?.origin();
                if !matches!(origin, url::Origin::Tuple(..)) {
                    return None;
                }
                hasher.update(b"\0direct\0");
                hasher.update(origin.ascii_serialization());
            }
        }
        Some(Self {
            scope,
            digest: format!("{:x}", hasher.finalize()),
        })
    }

    pub(crate) fn with_purpose(&self, purpose: HistoryPurpose) -> Self {
        let (prefix, _) = self.scope.rsplit_once(':').unwrap();
        Self {
            scope: format!("{prefix}:{}", purpose.as_str()),
            digest: self.digest.clone(),
        }
    }

    fn key(&self) -> String {
        format!("{}:{}", self.scope, self.digest)
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
struct Timing {
    verified_at: u64,
    completion_order: u64,
    first_byte_ms: u64,
    bytes_per_second: u64,
}

impl Timing {
    fn is_current(self, now: u64) -> bool {
        self.verified_at <= now
            && now - self.verified_at < HISTORY_LIFETIME_MS
            && self.bytes_per_second > 0
    }

    fn score(self, expected: Option<u64>) -> (u128, Reverse<u64>, u64) {
        let estimate = expected.filter(|size| *size > 0).map_or(0, |size| {
            u128::from(self.first_byte_ms)
                + u128::from(size) * 1000 / u128::from(self.bytes_per_second)
        });
        (estimate, Reverse(self.bytes_per_second), self.first_byte_ms)
    }
}

#[derive(Default, Deserialize, Serialize)]
struct Entry {
    tried_at: u64,
    timing: Option<Timing>,
}

#[derive(Default, Deserialize, Serialize)]
struct Ledger {
    entries: BTreeMap<String, Entry>,
    cadence: BTreeMap<String, u8>,
    #[serde(skip)]
    completion_order: u64,
    #[serde(skip)]
    dirty: bool,
}

pub(crate) struct DownloadHistory {
    path: PathBuf,
    ledger: Mutex<Ledger>,
    writer: Mutex<()>,
}

#[derive(Default)]
struct Run {
    allow_exploration: Option<bool>,
    explored: bool,
    preferred: BTreeSet<RouteIdentity>,
    verified: BTreeMap<RouteIdentity, Timing>,
}

pub(crate) struct RaceSession {
    history: Arc<DownloadHistory>,
    run: Mutex<Run>,
}

impl DownloadHistory {
    pub(crate) fn new(cache: PathBuf) -> Self {
        let path = cache.join("download-routes").join("history.json");
        let mut ledger = read(&path).unwrap_or_default();
        ledger.completion_order = ledger
            .entries
            .values()
            .filter_map(|entry| entry.timing.map(|timing| timing.completion_order))
            .max()
            .unwrap_or_default();
        Self {
            path,
            ledger: Mutex::new(ledger),
            writer: Mutex::new(()),
        }
    }

    pub(crate) fn session(self: &Arc<Self>) -> RaceSession {
        RaceSession {
            history: self.clone(),
            run: Mutex::default(),
        }
    }

    fn flush(&self) {
        let _writer = lock(&self.writer);
        let bytes = {
            let mut ledger = lock(&self.ledger);
            if !ledger.dirty {
                return;
            }
            ledger.dirty = false;
            match serde_json::to_vec(&*ledger) {
                Ok(bytes) => bytes,
                Err(_) => return,
            }
        };
        if let Err(error) = write(&self.path, &bytes) {
            lock(&self.ledger).dirty = true;
            log::debug!("无法保存下载线路记录：{error}");
        }
    }
}

impl RaceSession {
    pub(crate) fn order(
        &self,
        candidates: &mut [(usize, DownloadCandidate)],
        expected: Option<u64>,
    ) -> Option<usize> {
        let first = candidates.first()?.1.identity.as_ref()?;
        if candidates.len() < 2
            || candidates.iter().any(|(_, candidate)| {
                candidate
                    .identity
                    .as_ref()
                    .is_none_or(|identity| identity.scope != first.scope)
            })
        {
            return None;
        }
        let now = now_ms();
        let mut run = lock(&self.run);
        let mut ledger = lock(&self.history.ledger);
        let allow_exploration = *run.allow_exploration.get_or_insert_with(|| {
            let count = ledger.cadence.entry(first.scope.clone()).or_default();
            *count = (*count % EXPLORE_EVERY + 1) % EXPLORE_EVERY;
            let explore = *count == 0;
            ledger.dirty = true;
            explore
        });
        let entry = |candidate: &DownloadCandidate| {
            ledger
                .entries
                .get(&candidate.identity.as_ref().unwrap().key())
        };
        let timing = |candidate: &DownloadCandidate| {
            entry(candidate)
                .and_then(|entry| entry.timing)
                .filter(|timing| timing.is_current(now))
        };
        let preferred = candidates
            .iter()
            .enumerate()
            .filter_map(|(index, (_, candidate))| {
                Some((timing(candidate)?.completion_order, index))
            })
            .max();
        if let Some((_, index)) = preferred {
            candidates[..=index].rotate_right(1);
            if let Some(identity) = &candidates[0].1.identity {
                run.preferred.insert(identity.clone());
            }
            candidates[1..].sort_by_cached_key(|(_, candidate)| {
                let verified = timing(candidate);
                (
                    verified.is_none(),
                    verified.map(|timing| timing.score(expected)),
                )
            });
        }
        if !allow_exploration || run.explored {
            return None;
        }
        let explore = candidates
            .iter()
            .enumerate()
            .skip(1)
            .min_by_key(|(_, (id, candidate))| {
                (entry(candidate).map_or(0, |record| record.tried_at), *id)
            })?
            .0;
        candidates[1..=explore].rotate_right(1);
        Some(candidates[1].0)
    }

    pub(crate) fn history_preferred(&self, identity: &RouteIdentity) -> bool {
        lock(&self.run).preferred.contains(identity)
    }

    pub(crate) fn verified_speed(&self, identity: &RouteIdentity) -> Option<u64> {
        let timing = lock(&self.history.ledger)
            .entries
            .get(&identity.key())?
            .timing?;
        timing
            .is_current(now_ms())
            .then_some(timing.bytes_per_second)
    }

    pub(crate) fn started(&self, identity: &RouteIdentity, exploratory: bool) -> bool {
        let mut run = lock(&self.run);
        if exploratory && run.explored {
            return false;
        }
        run.explored |= exploratory;
        let mut ledger = lock(&self.history.ledger);
        ledger.entries.entry(identity.key()).or_default().tried_at = now_ms();
        trim(&mut ledger);
        ledger.dirty = true;
        true
    }

    pub(crate) fn succeeded(
        &self,
        identity: &RouteIdentity,
        bytes: u64,
        first_byte: Option<Duration>,
        transfer: Duration,
    ) {
        let Some(first_byte) = first_byte else { return };
        if bytes == 0 {
            return;
        }
        let mut run = lock(&self.run);
        let completion_order = {
            let mut ledger = lock(&self.history.ledger);
            let Some(next) = ledger.completion_order.checked_add(1) else {
                return;
            };
            ledger.completion_order = next;
            next
        };
        let timing = Timing {
            verified_at: now_ms(),
            completion_order,
            first_byte_ms: first_byte.as_millis() as u64,
            bytes_per_second: (bytes as f64 / transfer.as_secs_f64().max(0.001)) as u64,
        };
        run.verified.insert(identity.clone(), timing);
    }

    pub(crate) fn commit(&self) {
        let verified = std::mem::take(&mut lock(&self.run).verified);
        {
            let mut ledger = lock(&self.history.ledger);
            for (identity, timing) in verified {
                let entry = ledger.entries.entry(identity.key()).or_default();
                if entry
                    .timing
                    .is_none_or(|previous| previous.completion_order < timing.completion_order)
                {
                    entry.timing = Some(timing);
                    ledger.dirty = true;
                }
            }
            trim(&mut ledger);
        }
    }
}

impl Drop for RaceSession {
    fn drop(&mut self) {
        if lock(&self.run).allow_exploration.is_some()
            && let Ok(runtime) = tokio::runtime::Handle::try_current()
        {
            let history = self.history.clone();
            runtime.spawn_blocking(move || history.flush());
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|error| error.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn trim(ledger: &mut Ledger) {
    while ledger.entries.len() > MAX_ENTRIES {
        let Some(key) = ledger
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.tried_at)
            .map(|(key, _)| key.clone())
        else {
            return;
        };
        ledger.entries.remove(&key);
    }
}

fn read(path: &Path) -> Option<Ledger> {
    let file = File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64 > MAX_BYTES {
        return None;
    }
    let ledger: Ledger = serde_json::from_slice(&bytes).ok()?;
    (ledger.entries.len() <= MAX_ENTRIES && ledger.cadence.len() <= 24).then_some(ledger)
}

fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let directory = path
        .parent()
        .ok_or_else(|| io::Error::other("missing cache directory"))?;
    fs::create_dir_all(directory)?;
    let mut output = tempfile::NamedTempFile::new_in(directory)?;
    output.write_all(bytes)?;
    output.as_file().sync_all()?;
    output.persist(path).map_err(|error| error.error)?;
    Ok(())
}
