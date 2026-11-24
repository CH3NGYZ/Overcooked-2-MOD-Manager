//! Turn the verified final inventory into one explicit filesystem transaction.
use super::{
    application_progress::{ItemKey, OwnedMutation},
    inventory,
    model::*,
    session::PlanSession,
    transaction::*,
};
use crate::{app_state::AppState, archive, mods};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

mod install_block;
mod order;
mod physical;
mod records;
mod release_ownership;
mod retirements;
mod roots;
pub(super) use install_block::{InstallBlock, package_identity_conflict, report_install_block};
use records::{InstallRecords, install_record_path};
pub(super) use release_ownership::LooseReleaseSearch;
pub(super) use release_ownership::ReleaseTargetClaims;
pub(super) use release_ownership::{prove_loose_dll, verify_missing_dll_variant};
pub(super) use roots::{final_framework_package, final_package};

pub(super) struct Builder<'a> {
    pub roots: TransactionRoots,
    pub session: &'a PlanSession,
    pub writes: BTreeMap<String, (String, OwnedMutation)>,
    pub moves: Vec<OwnedMutation>,
    pub ownership: BTreeMap<String, BTreeSet<String>>,
    ownership_paths: BTreeSet<String>,
    records: InstallRecords,
    pub protected_maps: Vec<String>,
    pub map_paths: Vec<(String, String)>,
}

pub(super) fn transaction_roots(
    state: &AppState,
    session: &PlanSession,
) -> Result<TransactionRoots, String> {
    let data_root = state
        .config
        .config_path()
        .parent()
        .ok_or("archive.pathEscape")?
        .to_path_buf();
    Ok(TransactionRoots {
        game_root: session.trusted.game_root.clone(),
        data_root,
    })
}

/// Both the old and solved levels roots belong to maps during a root migration.
pub(super) fn map_roots(session: &PlanSession, solved: &Inventory) -> Result<Vec<String>, String> {
    let mut roots = installed_map_roots(session)?;
    if let Some(package) = final_package(solved)? {
        roots.push(format!("{package}/levels"));
    }
    Ok(roots)
}

fn installed_map_roots(session: &PlanSession) -> Result<Vec<String>, String> {
    let game_root = &session.trusted.game_root;
    let mut roots = vec![inventory::relative(
        game_root,
        &crate::level_sets::disabled_root(game_root),
    )?];
    if let Some(package) = session.trusted.level_roots.package_root() {
        roots.push(inventory::relative(game_root, &package.join("levels"))?);
    }
    Ok(roots)
}

pub(super) fn node_label<'a>(trusted: &'a inventory::TrustedInventory, id: &'a str) -> &'a str {
    trusted
        .files
        .get(id)
        .map(|files| files.summary.label.as_str())
        .filter(|label| !label.is_empty())
        .unwrap_or(id)
}

pub(super) fn assemble(
    session: &PlanSession,
    transaction_roots: TransactionRoots,
) -> Result<Vec<OwnedMutation>, String> {
    let solved = session.solved.as_ref().ok_or("modChanges.notPrepared")?;
    if session.plan.retired_mod_copies
        != session
            .trusted
            .duplicate_releases
            .selected(&session.trusted.inventory, solved)?
    {
        // The generic replacement must never remove a copy absent from the confirmed list.
        return Err(MULTIPLE_INSTALLED_COPIES.into());
    }
    let mut builder = Builder::indexed(session, transaction_roots, map_roots(session, solved)?)?;
    let migration = roots::prepare(&mut builder, solved)?;
    for node in &solved.nodes {
        if session.release_target_claims.consumes(&node.id) {
            continue;
        }
        if session
            .map_payload
            .as_ref()
            .is_some_and(|batch| batch.node_ids.contains(&node.id))
        {
            continue;
        }
        let old = session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|old| old.id == node.id);
        if node.kind == NodeKind::Map {
            roots::toggle_map(&mut builder, node, migration.as_ref())?;
            continue;
        }
        let replaced = old
            .and_then(|old| old.current.as_ref())
            .map(|current| &current.id)
            != node.current.as_ref().map(|current| &current.id);
        if replaced {
            builder.replace_mod(node, migration.as_ref())?;
        } else if let Some(current) = &node.current {
            let old_current = old
                .and_then(|old| old.current.as_ref())
                .ok_or("modChanges.targetChanged")?;
            for member in &current.provides {
                let key = member.member_key.as_deref().ok_or_else(|| {
                    report_install_block(
                        "modChanges.installTargetOwnershipUnverified",
                        &node.id,
                        &[("name", node_label(&session.trusted, &node.id))],
                    )
                })?;
                let before = old_current
                    .provides
                    .iter()
                    .find(|before| before.member_key.as_deref() == Some(key))
                    .ok_or("modChanges.targetChanged")?;
                let enabled = node.enabled && member.enabled;
                if before.enabled != enabled {
                    builder.switch_member(&node.id, key, enabled)?;
                }
            }
        }
    }
    roots::update_registry(&mut builder, migration.as_ref())?;
    let mut result = builder.moves;
    result.extend(retirements::ordered_writes(
        builder.writes.into_values(),
        &session.plan.retired_mod_copies,
    ));
    if session.map_payload.is_some() {
        result.extend(super::map_payload::mutations(&builder.roots, session)?);
    }
    Ok(order::for_apply(session, result))
}

impl Builder<'_> {
    fn indexed<'a>(
        session: &'a PlanSession,
        roots: TransactionRoots,
        protected_maps: Vec<String>,
    ) -> Result<Builder<'a>, String> {
        let records = InstallRecords::read(&roots, session)?;
        let mut builder = Builder {
            roots,
            session,
            writes: BTreeMap::new(),
            moves: Vec::new(),
            ownership: BTreeMap::new(),
            ownership_paths: BTreeSet::new(),
            records,
            protected_maps,
            map_paths: Vec::new(),
        };
        builder.index_ownership();
        Ok(builder)
    }

    fn index_ownership(&mut self) {
        for (file, owner) in self.records.members() {
            self.ownership
                .entry(inventory::key(&mods::canonical_dll_name(file)))
                .or_default()
                .insert(owner.into());
            self.ownership_paths.insert(file.into());
        }
        for (id, node) in &self.session.trusted.files {
            for member in &node.summary.members {
                self.ownership
                    .entry(inventory::key(&mods::canonical_dll_name(&member.dll_file)))
                    .or_default()
                    .insert(id.clone());
                self.ownership_paths.insert(member.dll_file.clone());
            }
            if let Some(path) = &node.map_path {
                self.protected_maps.push(path.clone());
            }
        }
    }

    pub fn ensure_owner(&self, relative: &str, owner: &str) -> Result<(), String> {
        let key = inventory::key(&mods::canonical_dll_name(relative));
        match self.ownership.get(&key) {
            Some(owners) if owners.len() == 1 && owners.contains(owner) => Ok(()),
            Some(_) => Err(self.report_ownership_conflict(&key, owner, relative)),
            None => Err(InstallBlock::Unverified.report(owner, relative)),
        }
    }

    fn report_ownership_conflict(&self, key: &str, owner: &str, relative: &str) -> String {
        let claims: Vec<_> = self
            .records
            .claims(key)
            .filter(|record| record.owner != owner)
            .collect();
        if !claims.is_empty() {
            let message =
                install_block::report_record_claim(owner, relative, claims.iter().copied());
            self.diagnose_record_claim(
                owner,
                relative,
                Some(key),
                "different_or_unbound_owner",
                claims.into_iter(),
            );
            message
        } else {
            InstallBlock::PathOwned.report(owner, relative)
        }
    }

    pub fn add_move(
        &mut self,
        owner: &str,
        member: Option<&str>,
        from: &str,
        to: &str,
    ) -> Result<(), String> {
        let source = game(from).resolve(&self.roots)?;
        let target = game(to).resolve(&self.roots)?;
        let expected = Fingerprint::capture(&source)?;
        if expected == Fingerprint::Missing {
            return Err(InstallBlock::VerificationChanged.report(owner, from));
        }
        let existing = Fingerprint::capture(&target)?;
        if existing != Fingerprint::Missing {
            return Err(occupied_target(&existing).report(owner, to));
        }
        for owned in &self.moves {
            if let Mutation::Move(change) = &owned.mutation {
                for existing in [&change.from, &change.to] {
                    if paths_overlap(&existing.relative, from)
                        || paths_overlap(&existing.relative, to)
                    {
                        log::warn!(
                            "整链移动路径冲突：节点={owner}，原路径={from}，目标路径={to}，冲突路径={}，原因=移动端点重叠",
                            existing.relative
                        );
                        return Err(InstallBlock::PathCollision.report(owner, &existing.relative));
                    }
                }
            }
        }
        for (_, owned) in self.writes.values() {
            if let Mutation::File(change) = &owned.mutation {
                if change.target.root == RootKind::Game
                    && (paths_overlap(&change.target.relative, from)
                        || paths_overlap(&change.target.relative, to))
                {
                    log::warn!(
                        "整链移动路径冲突：节点={owner}，原路径={from}，目标路径={to}，冲突路径={}，原因=写入与移动重叠",
                        change.target.relative
                    );
                    return Err(InstallBlock::PathCollision.report(owner, &change.target.relative));
                }
            }
        }
        let mut items = vec![ItemKey::new(owner, None)];
        if member.is_some() {
            items.push(ItemKey::new(owner, member));
        }
        self.moves.push(OwnedMutation::new(
            Mutation::Move(MoveChange {
                from: game(from),
                to: game(to),
                expected,
            }),
            items,
        ));
        Ok(())
    }

    fn write(
        &mut self,
        owner: &str,
        items: impl IntoIterator<Item = ItemKey>,
        mut change: FileChange,
    ) -> Result<(), String> {
        let FileChange {
            target,
            source,
            expected,
            require_single_link,
        } = &mut change;
        let id = format!("{:?}:{}", target.root, inventory::key(&target.relative));
        if matches!(expected, Fingerprint::Directory { .. }) {
            return Err(InstallBlock::Directory.report(owner, &target.relative));
        }
        for owned in &self.moves {
            if let Mutation::Move(change) = &owned.mutation {
                if target.root == RootKind::Game
                    && (paths_overlap(&change.from.relative, &target.relative)
                        || paths_overlap(&change.to.relative, &target.relative))
                {
                    log::warn!(
                        "整链写入路径冲突：节点={owner}，路径={}，原因=写入与移动重叠",
                        target.relative
                    );
                    return Err(InstallBlock::PathCollision.report(owner, &target.relative));
                }
            }
        }
        let mut items = items.into_iter().collect::<BTreeSet<_>>();
        if let Some((previous_owner, previous)) = self.writes.get_mut(&id) {
            if previous_owner != owner {
                log::warn!(
                    "整链写入归属冲突：节点={owner}，路径={}，前一归属={previous_owner}",
                    target.relative
                );
                return Err(InstallBlock::PathOwned.report(owner, &target.relative));
            }
            if let Mutation::File(previous_file) = &mut previous.mutation {
                if previous_file.expected != *expected {
                    return Err(InstallBlock::VerificationChanged.report(owner, &target.relative));
                }
                if previous_file.source.is_some() && source.is_some() {
                    log::warn!(
                        "整链写入路径冲突：节点={owner}，路径={}，原因=同一目标重复写入",
                        target.relative
                    );
                    return Err(InstallBlock::PathCollision.report(owner, &target.relative));
                }
                if previous_file.source.is_some() {
                    previous_file.require_single_link |= *require_single_link;
                    previous.items.append(&mut items);
                    return Ok(());
                }
                // Reusing a retired DLL's path must keep its frozen fingerprint,
                // single-link check, and progress row on the replacement record.
                *require_single_link |= previous_file.require_single_link;
                items.append(&mut previous.items);
            }
        }
        if *expected == Fingerprint::Missing && source.is_none() {
            return Ok(());
        }
        self.writes.insert(
            id,
            (
                owner.into(),
                OwnedMutation {
                    mutation: Mutation::File(change),
                    items,
                },
            ),
        );
        Ok(())
    }

    fn remove(&mut self, owner: &str, target: TransactionPath) -> Result<(), String> {
        let expected = Fingerprint::capture(&target.resolve(&self.roots)?)?;
        self.write(
            owner,
            Some(ItemKey::new(owner, None)),
            FileChange {
                target,
                source: None,
                expected,
                require_single_link: false,
            },
        )
    }

    fn replace_mod(
        &mut self,
        node: &ChangeNode,
        migration: Option<&roots::RootMigration>,
    ) -> Result<(), String> {
        let payload = self
            .session
            .payloads
            .get(&node.id)
            .ok_or("modChanges.notPrepared")?;
        let current = node.current.as_ref().ok_or("modChanges.notPrepared")?;
        if payload.candidate.id != current.id {
            return Err("modChanges.targetChanged".into());
        }
        let mut manifest = payload.manifest.clone().ok_or("modChanges.notPrepared")?;
        let reinstall = self.session.release_target_claims.missing_record(&node.id);
        let bundled_levels = super::level_trees::bundled_level_root(self.session, &node.id)?;
        let copies = self
            .session
            .plan
            .retired_mod_copies
            .get(&node.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        for copy in copies {
            self.retire_copy(&node.id, copy, migration)?;
        }
        let old_files = self.session.trusted.files.get(&node.id);
        if let Some(old) = old_files {
            let old = old.clone();
            let mut files = old
                .manifests
                .iter()
                .flat_map(|manifest| manifest.files.clone())
                .chain(old.summary.dll_files.iter().cloned())
                .collect::<BTreeSet<_>>();
            files.extend(
                old.summary
                    .members
                    .iter()
                    .map(|member| member.dll_file.clone()),
            );
            for relative in files {
                let canonical = mods::canonical_dll_name(&relative);
                let member_key = inventory::key(&canonical);
                if self
                    .session
                    .release_target_claims
                    .get(&node.id, &member_key)
                    .is_some()
                {
                    // The approved claim already freezes its exact physical
                    // endpoint, including a possible disabled variant.
                    continue;
                }
                if self
                    .session
                    .trusted
                    .declared_bundle_switch_members
                    .get(&node.id)
                    .is_some_and(|members| members.contains(&member_key))
                {
                    if !payload
                        .candidate
                        .provides
                        .iter()
                        .any(|member| member.member_key.as_deref() == Some(member_key.as_str()))
                    {
                        let final_member = current
                            .provides
                            .iter()
                            .find(|member| {
                                member.member_key.as_deref() == Some(member_key.as_str())
                            })
                            .ok_or("modChanges.targetChanged")?;
                        self.switch_member(
                            &node.id,
                            &member_key,
                            node.enabled && final_member.enabled,
                        )?;
                    }
                    // Declaration-only files never enter the generic removals.
                    // Matching Release files go through the narrower write gate below.
                    continue;
                }
                if copies.iter().any(|copy| copy.member_key == member_key) {
                    continue;
                }
                if migration.is_some_and(|migration| within(&relative, &migration.from))
                    || self.protected_maps.iter().any(|map| within(&relative, map))
                {
                    continue;
                }
                self.ensure_owner(&relative, &node.id)?;
                for candidate in if canonical.to_ascii_lowercase().ends_with(".dll") {
                    vec![canonical.clone(), mods::disabled_dll_file(&canonical)]
                } else {
                    vec![relative]
                } {
                    self.remove(&node.id, game(&candidate))?;
                }
            }
            for old in old.manifests {
                if copies
                    .iter()
                    .any(|copy| copy.manifest_id.as_ref() == Some(&old.repository_id))
                {
                    continue;
                }
                self.remove(&node.id, install_record_path(&old.repository_id))?;
            }
        }
        let mut final_files = Vec::new();
        let source_roots = TransactionRoots {
            game_root: payload.stage_root.clone(),
            data_root: self.roots.data_root.clone(),
        };
        for source_relative in &payload.files {
            if bundled_levels
                .as_ref()
                .is_some_and(|levels| within(source_relative, levels))
            {
                // The shared map-tree composer preserves installed maps and seeds
                // absent bundled maps once; MOD manifests must not claim map files.
                continue;
            }
            let canonical = mods::canonical_dll_name(source_relative);
            let member_key = inventory::key(&canonical);
            let is_dll = canonical.to_ascii_lowercase().ends_with(".dll");
            let mut destination = if is_dll {
                let member = current
                    .provides
                    .iter()
                    .find(|member| member.member_key.as_deref() == Some(member_key.as_str()))
                    .ok_or("modChanges.targetChanged")?;
                if node.enabled && member.enabled {
                    canonical
                } else {
                    mods::disabled_dll_file(&canonical)
                }
            } else {
                source_relative.clone()
            };
            if self
                .protected_maps
                .iter()
                .any(|map| within(&destination, map))
            {
                return Err(InstallBlock::MapProtected.report(&node.id, &destination));
            }
            let ownership_key = inventory::key(&mods::canonical_dll_name(&destination));
            let loose = self
                .session
                .release_target_claims
                .get(&node.id, &ownership_key);
            let restoring_owned =
                reinstall.is_some_and(|record| record.owns_previous_path(&ownership_key));
            let restoring_missing =
                reinstall.is_some_and(|record| record.expects_missing(&destination));
            let owners = self.ownership.get(&ownership_key);
            let unowned = owners.is_none();
            if loose.is_none()
                && !restoring_owned
                && owners.is_some_and(|owners| owners.len() != 1 || !owners.contains(&node.id))
            {
                return Err(self.report_ownership_conflict(&ownership_key, &node.id, &destination));
            }
            if is_dll && loose.is_none() && !restoring_owned {
                let canonical = mods::canonical_dll_name(&destination);
                for variant in [&canonical, &mods::disabled_dll_file(&canonical)] {
                    let existing = Fingerprint::capture(&game(variant).resolve(&self.roots)?)?;
                    if existing != Fingerprint::Missing && unowned {
                        return Err(occupied_target(&existing).report(&node.id, variant));
                    }
                }
            }
            let release_stamp = payload
                .release_files
                .as_ref()
                .and_then(|files| files.get(&path_key(Path::new(source_relative))));
            let (target, expected, guarded) = if restoring_missing {
                (game(&destination), Fingerprint::Missing, false)
            } else if let Some(loose) = loose {
                let expected = if !is_dll
                    || inventory::key(&loose.relative) == inventory::key(&destination)
                {
                    destination.clone_from(&loose.relative);
                    loose.expected.clone()
                } else {
                    // Preserve the live spelling when moving between the
                    // enabled and disabled forms of the same verified DLL.
                    let live = mods::canonical_dll_name(&loose.relative);
                    destination = if destination.to_ascii_lowercase().ends_with(".dll.disabled") {
                        mods::disabled_dll_file(&live)
                    } else {
                        live
                    };
                    self.write(
                        &node.id,
                        Some(ItemKey::new(&node.id, None)),
                        FileChange {
                            target: game(&loose.relative),
                            source: None,
                            expected: loose.expected.clone(),
                            require_single_link: true,
                        },
                    )?;
                    Fingerprint::Missing
                };
                let guarded = expected != Fingerprint::Missing;
                (game(&destination), expected, guarded)
            } else if let Some((physical, expected)) = self.verify_declared_bundle_write(
                &node.id,
                &member_key,
                &destination,
                release_stamp,
            )? {
                destination = physical;
                (game(&destination), expected, true)
            } else {
                let target = game(&destination);
                let expected = Fingerprint::capture(&target.resolve(&self.roots)?)?;
                (target, expected, false)
            };
            let path = game(source_relative).resolve(&source_roots)?;
            let source = if let Some(files) = &payload.release_files {
                let fingerprint = files
                    .get(&path_key(Path::new(source_relative)))
                    .ok_or("modChanges.payloadChanged")?;
                StagedFile {
                    path,
                    fingerprint: fingerprint.clone(),
                }
            } else {
                StagedFile::capture(path)?
            };
            if expected != Fingerprint::Missing && unowned && loose.is_none() {
                return Err(occupied_target(&expected).report(&node.id, &destination));
            }
            // Do not re-sample either side after authorizing the untracked target.
            self.write(
                &node.id,
                Some(ItemKey::new(&node.id, None)),
                FileChange {
                    target,
                    source: Some(source),
                    expected,
                    require_single_link: guarded,
                },
            )?;
            self.ownership
                .entry(ownership_key)
                .or_default()
                .insert(node.id.clone());
            self.ownership_paths.insert(destination.clone());
            final_files.push(destination);
        }
        manifest.files = final_files;
        let orphan = self.session.release_target_claims.orphan_record(&node.id);
        let (manifest_target, expected) = match (reinstall, orphan) {
            (Some(record), _) => (
                record.replacement.clone(),
                if record.retired_id.is_some() {
                    Fingerprint::Missing
                } else {
                    record.expected.clone()
                },
            ),
            (None, Some(record)) => (data(&record.relative), record.stamp.fingerprint.clone()),
            (None, None) => {
                let target = install_record_path(&manifest.repository_id);
                let expected = Fingerprint::capture(&target.resolve(&self.roots)?)?;
                (target, expected)
            }
        };
        if reinstall.is_none()
            && orphan.is_none()
            && !old_files.is_some_and(|old| {
                old.manifests
                    .iter()
                    .any(|old| old.repository_id == manifest.repository_id)
            })
            && (expected != Fingerprint::Missing
                || self
                    .records
                    .records_for_id(&manifest.repository_id)
                    .next()
                    .is_some())
        {
            let reason = if matches!(expected, Fingerprint::Directory { .. }) {
                InstallBlock::Directory
            } else {
                InstallBlock::RecordExists
            };
            self.diagnose_record_claim(
                &node.id,
                &manifest_target.relative,
                None,
                "install_record_path_occupied",
                self.records.at_relative(&manifest_target.relative),
            );
            return Err(reason.report(&node.id, &manifest_target.relative));
        }
        let staged = generated(
            self.session,
            "manifest",
            toml::to_string_pretty(&manifest)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )?;
        self.write(
            &node.id,
            Some(ItemKey::new(&node.id, None)),
            FileChange {
                target: manifest_target,
                source: Some(StagedFile::capture(staged)?),
                expected,
                require_single_link: orphan.is_some()
                    || reinstall.is_some_and(|record| record.retired_id.is_none()),
            },
        )?;
        if let Some(record) = reinstall.filter(|record| record.retired_id.is_some()) {
            self.remove_retired_file(&node.id, None, data(&record.relative), &record.expected)?;
        }
        Ok(())
    }
}

pub(super) fn occupied_target(existing: &Fingerprint) -> InstallBlock {
    match existing {
        Fingerprint::Directory { .. } => InstallBlock::Directory,
        _ => InstallBlock::Unrecognized,
    }
}

pub(crate) fn game(relative: &str) -> TransactionPath {
    TransactionPath {
        root: RootKind::Game,
        relative: relative.replace('\\', "/"),
    }
}
pub(crate) fn data(relative: &str) -> TransactionPath {
    TransactionPath {
        root: RootKind::Data,
        relative: relative.replace('\\', "/"),
    }
}
pub(crate) fn within(path: &str, root: &str) -> bool {
    let path = inventory::key(path);
    let root = inventory::key(root);
    path == root || path.starts_with(&(root + "/"))
}
pub(crate) fn paths_overlap(left: &str, right: &str) -> bool {
    within(left, right) || within(right, left)
}

pub(crate) fn generated(
    session: &PlanSession,
    name: &str,
    bytes: &[u8],
) -> Result<PathBuf, String> {
    if !session.staging.is_absolute()
        || !matches!(
            Fingerprint::capture(&session.staging)?,
            Fingerprint::Directory { .. }
        )
    {
        return Err("archive.pathEscape".into());
    }
    let root = session.staging.join("generated");
    Fingerprint::capture(&root)?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let target = root.join(format!("{name}-{}", uuid::Uuid::new_v4()));
    fs::write(&target, bytes).map_err(|error| error.to_string())?;
    Ok(target)
}

#[cfg(test)]
mod tests;
