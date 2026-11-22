use super::durable::{copy_verified, private_path, write_journal};
use super::paths::{UNSAFE, validate_mutations, validate_roots};
use super::types::*;
use super::{PreparedTransaction, check};
use crate::file_check;
use std::{collections::HashSet, fs, path::Path};
use uuid::Uuid;

/// Sources must already represent the complete validated download set. Only private staging
/// is written here; the write-ahead journal is published after all source copies verify.
pub(crate) fn prepare(
    transaction_root: &Path,
    roots: &TransactionRoots,
    mutations: Vec<Mutation>,
) -> Result<PreparedTransaction, String> {
    validate_roots(transaction_root, roots)?;
    validate_mutations(&mutations, transaction_root, roots)?;
    if mutations.is_empty() {
        return Err("recycle.noMovableFiles".into());
    }
    fs::create_dir_all(transaction_root).map_err(|error| {
        file_check::report_io(check::WRITE, transaction_root, error, file_check::WRITE)
    })?;
    let id = Uuid::new_v4().to_string();
    let directory = transaction_root.join(&id);
    create_dir(&directory, check::WRITE)?;
    let result = (|| {
        create_dir(&directory.join("before"), check::WRITE_BACKUP)?;
        create_dir(&directory.join("after"), check::WRITE)?;
        let mut records = Vec::new();
        let mut parents = Vec::new();
        let mut known_parents = HashSet::new();
        for (index, mutation) in mutations.into_iter().enumerate() {
            let operation = match mutation {
                Mutation::File(change) => {
                    let target = check::resolve(&change.target, roots, check::TARGET)?;
                    super::engine::verify_file_before(
                        &target,
                        &change.expected,
                        change.require_single_link,
                    )?;
                    if matches!(change.expected, Fingerprint::Directory { .. }) {
                        return Err(file_check::report(check::TARGET, &target, file_check::TYPE));
                    }
                    let backup = if change.expected == Fingerprint::Missing {
                        None
                    } else {
                        let relative = format!("before/{index}");
                        copy_verified(
                            &target,
                            &private_path(&directory, &relative, check::BACKUP)?,
                            &change.expected,
                            check::TARGET,
                            check::WRITE_BACKUP,
                        )?;
                        Some(relative)
                    };
                    let (payload, after) = if let Some(source) = change.source {
                        if !matches!(source.fingerprint, Fingerprint::File { .. }) {
                            let reason = if source.fingerprint == Fingerprint::Missing {
                                file_check::MISSING
                            } else {
                                file_check::TYPE
                            };
                            return Err(file_check::report(check::STAGED, &source.path, reason));
                        }
                        let relative = format!("after/{index}");
                        let stored = private_path(&directory, &relative, check::STAGED)?;
                        let mut after = copy_verified(
                            &source.path,
                            &stored,
                            &source.fingerprint,
                            check::STAGED,
                            check::WRITE,
                        )?;
                        // Replacement retains the destination's permissions, including its DACL.
                        let permissions = change
                            .expected
                            .permissions()
                            .or(source.fingerprint.permissions())
                            .ok_or(UNSAFE)?
                            .clone();
                        if let Fingerprint::File {
                            permissions: value, ..
                        } = &mut after
                        {
                            *value = permissions;
                        }
                        (Some(relative), after)
                    } else {
                        (None, Fingerprint::Missing)
                    };
                    let mut temporary = change.target.clone();
                    let parent = Path::new(&temporary.relative)
                        .parent()
                        .unwrap_or_else(|| Path::new(""));
                    temporary.relative = parent
                        .join(format!(".oc2-batch-{id}-{index}.tmp"))
                        .to_string_lossy()
                        .replace('\\', "/");
                    check::expect(
                        &check::resolve(&temporary, roots, check::STAGED)?,
                        check::STAGED,
                        &Fingerprint::Missing,
                    )?;
                    collect_missing_parents(
                        &change.target,
                        roots,
                        &mut parents,
                        &mut known_parents,
                    )?;
                    RecordedMutation::File {
                        target: change.target,
                        before: change.expected,
                        after,
                        backup,
                        payload,
                        temporary,
                        require_single_link: change.require_single_link,
                    }
                }
                Mutation::Move(change) => {
                    let from = check::resolve(&change.from, roots, check::TARGET)?;
                    let to = check::resolve(&change.to, roots, check::TARGET)?;
                    if change.expected == Fingerprint::Missing {
                        return Err(file_check::report(
                            check::TARGET,
                            &from,
                            file_check::MISSING,
                        ));
                    }
                    check::expect(&from, check::TARGET, &change.expected)?;
                    check::expect(&to, check::TARGET, &Fingerprint::Missing)?;
                    collect_missing_parents(&change.to, roots, &mut parents, &mut known_parents)?;
                    RecordedMutation::Move {
                        from: change.from,
                        to: change.to,
                        before: change.expected,
                    }
                }
                Mutation::Tree(change) => {
                    let target = check::resolve(&change.target, roots, check::TARGET)?;
                    check::expect(&target, check::TARGET, &change.expected)?;
                    if !matches!(
                        change.expected,
                        Fingerprint::Missing | Fingerprint::Directory { .. }
                    ) {
                        return Err(file_check::report(check::TARGET, &target, file_check::TYPE));
                    }
                    let (payload, after) = if let Some(source) = change.source {
                        let expected = check::capture(&source, check::STAGED)?;
                        if !matches!(expected, Fingerprint::Directory { .. }) {
                            let reason = if expected == Fingerprint::Missing {
                                file_check::MISSING
                            } else {
                                file_check::TYPE
                            };
                            return Err(file_check::report(check::STAGED, &source, reason));
                        }
                        let relative = format!("after/{index}");
                        let stored = private_path(&directory, &relative, check::STAGED)?;
                        super::tree::copy_tree(&source, &stored, &expected)?;
                        super::tree::preserve_replaced_permissions(&target, &stored)?;
                        let after = check::capture(&stored, check::STAGED)?;
                        (Some(relative), after)
                    } else {
                        (None, Fingerprint::Missing)
                    };
                    let mut temporary = change.target.clone();
                    let mut backup = change.target.clone();
                    let parent = Path::new(&change.target.relative)
                        .parent()
                        .unwrap_or_else(|| Path::new(""));
                    temporary.relative = parent
                        .join(format!(".oc2-batch-{id}-{index}.tree-new"))
                        .to_string_lossy()
                        .replace('\\', "/");
                    backup.relative = parent
                        .join(format!(".oc2-batch-{id}-{index}.tree-old"))
                        .to_string_lossy()
                        .replace('\\', "/");
                    for (sibling, stage) in [(&temporary, check::STAGED), (&backup, check::BACKUP)]
                    {
                        check::expect(
                            &check::resolve(sibling, roots, stage)?,
                            stage,
                            &Fingerprint::Missing,
                        )?;
                    }
                    collect_missing_parents(
                        &change.target,
                        roots,
                        &mut parents,
                        &mut known_parents,
                    )?;
                    RecordedMutation::Tree {
                        target: change.target,
                        before: change.expected,
                        after,
                        payload,
                        backup,
                        temporary,
                    }
                }
            };
            records.push(Record {
                operation,
                phase: RecordPhase::Pending,
            });
        }
        // Detect a changing scene across the preparation window before publishing a journal.
        for record in &records {
            super::engine::verify_before(record, roots)?;
        }
        super::paths::validate_records(&records, transaction_root, roots)?;
        let journal = Journal {
            version: 1,
            id,
            roots: roots.clone(),
            state: JournalState::Prepared,
            records,
            created_directories: parents,
            created_directory_count: 0,
            error: None,
        };
        write_journal(&directory, &journal)?;
        super::durable::sync_parent(&directory)?;
        Ok(PreparedTransaction {
            directory: directory.clone(),
            journal,
        })
    })();
    if result.is_err() {
        // No game/data target was touched. Recovery material is unnecessary until publication.
        if let Err(error) = super::recovery::remove_private_tree(&directory) {
            log::warn!("清理未发布事务失败：{error}");
        }
    }
    result
}

fn create_dir(path: &Path, stage: &str) -> Result<(), String> {
    fs::create_dir(path)
        .map_err(|error| file_check::report_io(stage, path, error, file_check::WRITE))
}

fn collect_missing_parents(
    path: &TransactionPath,
    roots: &TransactionRoots,
    output: &mut Vec<TransactionPath>,
    seen: &mut HashSet<String>,
) -> Result<(), String> {
    let target = path.resolve(roots)?;
    let root = match path.root {
        RootKind::Game => &roots.game_root,
        RootKind::Data => &roots.data_root,
    };
    let missing = target
        .parent()
        .into_iter()
        .flat_map(Path::ancestors)
        .take_while(|parent| !parent.exists())
        .collect::<Vec<_>>();
    for parent in missing.into_iter().rev() {
        let relative = parent.strip_prefix(root).map_err(|_| UNSAFE.to_string())?;
        if relative.as_os_str().is_empty() {
            return Err(UNSAFE.into());
        }
        if seen.insert(super::paths::key(parent)) {
            // Apply consumes this list as a durable prefix, at each parent's first use.
            output.push(TransactionPath {
                root: path.root,
                relative: relative.to_string_lossy().replace('\\', "/"),
            });
        }
    }
    Ok(())
}
