use super::PreparedTransaction;
use super::durable::{copy_verified, private_path, write_journal};
use super::paths::{CONFLICT, MODIFIED, UNSAFE, io_error, validate_mutations, validate_roots};
use super::types::*;
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
    fs::create_dir_all(transaction_root).map_err(io_error)?;
    let id = Uuid::new_v4().to_string();
    let directory = transaction_root.join(&id);
    fs::create_dir(&directory).map_err(io_error)?;
    let result = (|| {
        fs::create_dir(directory.join("before")).map_err(io_error)?;
        fs::create_dir(directory.join("after")).map_err(io_error)?;
        let mut records = Vec::new();
        let mut parents = Vec::new();
        let mut known_parents = HashSet::new();
        for (index, mutation) in mutations.into_iter().enumerate() {
            let operation = match mutation {
                Mutation::File(change) => {
                    let target = change.target.resolve(roots)?;
                    super::engine::verify_file_before(
                        &target,
                        &change.expected,
                        change.require_single_link,
                    )?;
                    if matches!(change.expected, Fingerprint::Directory { .. }) {
                        return Err(CONFLICT.into());
                    }
                    let backup = if change.expected == Fingerprint::Missing {
                        None
                    } else {
                        let relative = format!("before/{index}");
                        copy_verified(
                            &target,
                            &private_path(&directory, &relative)?,
                            &change.expected,
                        )?;
                        Some(relative)
                    };
                    let (payload, after) = if let Some(source) = change.source {
                        if !matches!(source.fingerprint, Fingerprint::File { .. }) {
                            return Err(UNSAFE.into());
                        }
                        let relative = format!("after/{index}");
                        let stored = private_path(&directory, &relative)?;
                        let mut after = copy_verified(&source.path, &stored, &source.fingerprint)?;
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
                    if Fingerprint::capture(&temporary.resolve(roots)?)? != Fingerprint::Missing {
                        return Err(CONFLICT.into());
                    }
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
                    let from = change.from.resolve(roots)?;
                    let to = change.to.resolve(roots)?;
                    if change.expected == Fingerprint::Missing
                        || Fingerprint::capture(&from)? != change.expected
                    {
                        return Err(MODIFIED.into());
                    }
                    if Fingerprint::capture(&to)? != Fingerprint::Missing {
                        return Err(CONFLICT.into());
                    }
                    collect_missing_parents(&change.to, roots, &mut parents, &mut known_parents)?;
                    RecordedMutation::Move {
                        from: change.from,
                        to: change.to,
                        before: change.expected,
                    }
                }
                Mutation::Tree(change) => {
                    let target = change.target.resolve(roots)?;
                    if Fingerprint::capture(&target)? != change.expected {
                        return Err(MODIFIED.into());
                    }
                    if !matches!(
                        change.expected,
                        Fingerprint::Missing | Fingerprint::Directory { .. }
                    ) {
                        return Err(CONFLICT.into());
                    }
                    let (payload, after) = if let Some(source) = change.source {
                        let expected = Fingerprint::capture(&source)?;
                        if !matches!(expected, Fingerprint::Directory { .. }) {
                            return Err(UNSAFE.into());
                        }
                        let relative = format!("after/{index}");
                        let stored = private_path(&directory, &relative)?;
                        super::tree::copy_tree(&source, &stored, &expected)?;
                        super::tree::preserve_replaced_permissions(&target, &stored)?;
                        let after = Fingerprint::capture(&stored)?;
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
                    for sibling in [&temporary, &backup] {
                        if Fingerprint::capture(&sibling.resolve(roots)?)? != Fingerprint::Missing {
                            return Err(CONFLICT.into());
                        }
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
