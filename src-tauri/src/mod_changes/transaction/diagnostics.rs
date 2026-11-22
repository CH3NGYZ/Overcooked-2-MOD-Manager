use super::types::*;
use crate::file_check::log_field;
use std::{cell::RefCell, fmt, path::Path};

pub(super) const TARGET: &str = "app_lib::mod_changes::transaction::diagnostics";

thread_local! {
    static SCOPE: RefCell<Option<Context>> = const { RefCell::new(None) };
}

struct Context {
    id: String,
    step: Option<Step>,
}

type Step = (usize, &'static str);

pub(super) struct TransactionScope(Option<Context>);

impl Drop for TransactionScope {
    fn drop(&mut self) {
        SCOPE.with(|scope| scope.replace(self.0.take()));
    }
}

pub(super) fn enter(id: &str) -> TransactionScope {
    TransactionScope(SCOPE.with(|scope| {
        scope.replace(Some(Context {
            id: log_field(id),
            step: None,
        }))
    }))
}

pub(super) struct RecordScope(Option<Step>);

impl Drop for RecordScope {
    fn drop(&mut self) {
        SCOPE.with(|scope| {
            if let Some(context) = scope.borrow_mut().as_mut() {
                context.step = self.0;
            }
        });
    }
}

pub(super) fn step(index: usize, phase: &'static str) -> RecordScope {
    RecordScope(SCOPE.with(|scope| {
        scope
            .borrow_mut()
            .as_mut()?
            .step
            .replace((index + 1, phase))
    }))
}

pub(super) struct Location;

impl fmt::Display for Location {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        SCOPE.with(|scope| {
            let scope = scope.borrow();
            if let Some(context) = scope.as_ref() {
                write!(output, "事务={}，", context.id)?;
                if let Some((index, phase)) = context.step {
                    write!(output, "记录号={index}，阶段={phase}，")?;
                }
            }
            Ok(())
        })
    }
}

pub(super) use crate::file_check::log_path as path;

pub(super) fn endpoint(roots: &TransactionRoots, endpoint: &TransactionPath) -> String {
    let root = match endpoint.root {
        RootKind::Game => &roots.game_root,
        RootKind::Data => &roots.data_root,
    };
    path(&root.join(&endpoint.relative))
}

pub(super) struct Stamp<'a>(pub &'a Fingerprint);

impl fmt::Display for Stamp<'_> {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Fingerprint::Missing => output.write_str("missing"),
            Fingerprint::File { size, sha256, .. } => {
                write!(output, "file(bytes={size}, sha256={})", digest(sha256))
            }
            Fingerprint::Directory { sha256, .. } => {
                write!(output, "directory(tree_sha256={})", digest(sha256))
            }
        }
    }
}

fn digest(value: &str) -> &str {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        value
    } else {
        "<invalid digest>"
    }
}

pub(super) fn record(journal: &Journal, directory: &Path, index: usize, stage: &str, error: &str) {
    if !log::log_enabled!(target: TARGET, log::Level::Warn) {
        return;
    }
    let record = &journal.records[index];
    let absolute = |relative: &TransactionPath| endpoint(&journal.roots, relative);
    let private = |relative: &str| path(&directory.join(relative));
    let (kind, target, source, backup, temporary, before, after) = match &record.operation {
        RecordedMutation::File {
            target,
            payload,
            backup,
            temporary,
            before,
            after,
            ..
        } => (
            "file",
            absolute(target),
            payload.as_deref().map(private),
            backup.as_deref().map(private),
            Some(absolute(temporary)),
            before,
            after,
        ),
        RecordedMutation::Tree {
            target,
            payload,
            backup,
            temporary,
            before,
            after,
        } => (
            "tree",
            absolute(target),
            payload.as_deref().map(private),
            Some(absolute(backup)),
            Some(absolute(temporary)),
            before,
            after,
        ),
        RecordedMutation::Move { from, to, before } => (
            "move",
            absolute(to),
            Some(absolute(from)),
            None,
            None,
            &Fingerprint::Missing,
            before,
        ),
    };
    log::warn!(
        target: TARGET,
        "整链事务记录失败：事务={}，记录号={}，阶段={stage}，记录状态={:?}，类型={kind}，计划目标={target:?}，计划源={source:?}，备份={backup:?}，临时={temporary:?}，初态={}，计划结果={}，错误={:?}",
        log_field(&journal.id), index + 1, record.phase, Stamp(before), Stamp(after), crate::locale_tag::diagnostic_tag(error),
    );
}

pub(super) fn transaction(journal: &Journal, directory: &Path, stage: &str, error: &str) {
    if !log::log_enabled!(target: TARGET, log::Level::Warn) {
        return;
    }
    let count = |phase| {
        journal
            .records
            .iter()
            .filter(|record| record.phase == phase)
            .count()
    };
    let restored = if stage == "rolled_back" {
        count(RecordPhase::Reverted) + count(RecordPhase::Purging)
    } else {
        count(RecordPhase::Reverted)
    };
    log::warn!(
        target: TARGET,
        "整链事务诊断：事务={}，目录={:?}，阶段={stage}，状态={:?}，未开始={}，可能部分写入={}，已应用={}，回滚中={}，已回滚={}，清理中={}，创建父目录={}，错误={:?}",
        log_field(&journal.id), path(directory), journal.state,
        count(RecordPhase::Pending), count(RecordPhase::Applying), count(RecordPhase::Applied),
        count(RecordPhase::Reverting), restored, count(RecordPhase::Purging),
        journal.created_directory_count, crate::locale_tag::diagnostic_tag(error),
    );
}
