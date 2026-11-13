//! Optional display metadata must never decide whether an update is valid.

use semver::Version;
use serde_json::Value;
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Included};

const MAX_ENTRIES: usize = 256;
const MAX_NOTE_BYTES: usize = 128 * 1024;
const MAX_TOTAL_NOTE_BYTES: usize = 768 * 1024;

/// `None` keeps the original notes, including the distinction between absent
/// and empty notes. Validate the entire extension before selecting a range so
/// malformed or oversized history cannot produce a misleading partial log.
pub(super) fn aggregate_notes(
    changelog: Option<&Value>,
    current: &Version,
    target: &Version,
    target_notes: Option<&str>,
) -> Option<String> {
    let entries = changelog?.as_array()?;
    if entries.len() > MAX_ENTRIES || !target.pre.is_empty() || !target.build.is_empty() {
        return None;
    }
    let mut history = BTreeMap::new();
    for entry in entries {
        let text = entry.get("version")?.as_str()?;
        let version = Version::parse(text).ok()?;
        if !version.pre.is_empty() || !version.build.is_empty() || version.to_string() != text {
            return None;
        }
        let notes = entry.get("notes")?.as_str()?;
        if notes.len() > MAX_NOTE_BYTES {
            return None;
        }
        // The first occurrence wins; the selected manifest's own notes are
        // authoritative for the target version regardless of this history.
        history.entry(version).or_insert(notes);
    }
    let target_notes = target_notes.unwrap_or_default();
    if target_notes.len() > MAX_NOTE_BYTES {
        return None;
    }
    history.insert(target.clone(), target_notes);
    if history.len() > MAX_ENTRIES
        || history.values().map(|notes| notes.len()).sum::<usize>() > MAX_TOTAL_NOTE_BYTES
        || current >= target
    {
        return None;
    }
    let selected = history.range((Excluded(current), Included(target)));
    if !selected.clone().any(|(version, _)| version < target) {
        return None;
    }
    Some(
        selected
            .map(|(version, notes)| format!("### v{version}\n\n{notes}"))
            .collect::<Vec<_>>()
            .join("\n\n"),
    )
}
