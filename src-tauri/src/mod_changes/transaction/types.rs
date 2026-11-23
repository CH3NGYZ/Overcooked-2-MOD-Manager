use crate::file_metadata::FileIdentity;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(super) const JOURNAL_VERSION: u32 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RootKind {
    Game,
    Data,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransactionRoots {
    pub game_root: PathBuf,
    pub data_root: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct TransactionPath {
    pub root: RootKind,
    pub relative: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PermissionsSnapshot {
    pub readonly: bool,
    #[cfg(unix)]
    pub mode: u32,
    #[cfg(windows)]
    pub security_descriptor: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Fingerprint {
    Missing,
    File {
        size: u64,
        sha256: String,
        permissions: PermissionsSnapshot,
    },
    Directory {
        sha256: String,
        permissions: PermissionsSnapshot,
    },
}

#[derive(Debug, Clone)]
pub(crate) struct StagedFile {
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
}

#[derive(Debug, Clone)]
pub(crate) struct FileChange {
    pub target: TransactionPath,
    /// A validated, fully downloaded/unpacked file. None removes the target.
    pub source: Option<StagedFile>,
    pub expected: Fingerprint,
    /// Adoption or retirement must not change permissions through another link.
    pub require_single_link: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct MoveChange {
    pub from: TransactionPath,
    pub to: TransactionPath,
    pub expected: Fingerprint,
}

#[derive(Debug, Clone)]
pub(crate) struct TreeChange {
    pub target: TransactionPath,
    /// A complete validated tree. None removes the whole target tree.
    pub source: Option<PathBuf>,
    pub expected: Fingerprint,
}

#[derive(Debug, Clone)]
pub(crate) enum Mutation {
    File(FileChange),
    Tree(TreeChange),
    /// Same-volume rename preserves directory contents and each member's permissions.
    Move(MoveChange),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JournalState {
    Prepared,
    Applying,
    Committed,
    Cleaned,
    RecoveryRequired,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Journal {
    pub version: u32,
    pub id: String,
    pub roots: TransactionRoots,
    pub state: JournalState,
    pub records: Vec<Record>,
    pub created_directories: Vec<TransactionPath>,
    /// Only successfully created parents may be removed during rollback. An
    /// interrupted mkdir before this counter persists may leave an empty folder.
    #[serde(default)]
    pub created_directory_count: usize,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Record {
    pub operation: RecordedMutation,
    pub phase: RecordPhase,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publication: Option<FilePublication>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum FilePublication {
    NotStaged,
    Staged(FileIdentity),
    Unpublished(FileIdentity),
    Restoring {
        published: FileIdentity,
        temporary: FileIdentity,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RecordPhase {
    Pending,
    Applying,
    Applied,
    Reverting,
    Reverted,
    Purging,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(super) enum RecordedMutation {
    File {
        target: TransactionPath,
        before: Fingerprint,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        original_identity: Option<FileIdentity>,
        after: Fingerprint,
        backup: Option<String>,
        payload: Option<String>,
        temporary: TransactionPath,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        require_single_link: bool,
    },
    Move {
        from: TransactionPath,
        to: TransactionPath,
        before: Fingerprint,
    },
    Tree {
        target: TransactionPath,
        before: Fingerprint,
        after: Fingerprint,
        payload: Option<String>,
        backup: TransactionPath,
        temporary: TransactionPath,
    },
}

impl RecordedMutation {
    pub(super) fn guards_original(&self) -> bool {
        matches!(
            self,
            Self::File {
                original_identity: Some(_),
                ..
            }
        )
    }

    pub(super) fn tracks_publication(&self) -> bool {
        self.creates_file() || self.guards_original()
    }

    pub(super) fn creates_file(&self) -> bool {
        matches!(
            self,
            Self::File {
                before: Fingerprint::Missing,
                after: Fingerprint::File { .. },
                ..
            }
        )
    }
}

#[derive(Debug, Default)]
pub(crate) struct RecoveryReport {
    pub recovered: Vec<String>,
    pub cleaned: Vec<String>,
    /// Nonempty means all game/data writes must remain blocked.
    pub blocked: Vec<String>,
}
