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

mod order;
mod physical;
mod release_ownership;
mod retirements;
mod roots;
pub(super) use roots::{final_framework_package, final_package};

pub(super) fn file_conflict(owner: &str, relative: &str) -> String {
    log::warn!("整链安装无法确认文件归属：节点={owner}，路径={relative}");
    crate::locale_tag::with_params("modChanges.fileOwnershipConflict", &[("path", relative)])
}

pub(super) struct Builder<'a> {
    pub roots: TransactionRoots,
    pub session: &'a PlanSession,
    pub writes: BTreeMap<String, (String, OwnedMutation)>,
    pub moves: Vec<OwnedMutation>,
    pub ownership: BTreeMap<String, BTreeSet<String>>,
    ownership_paths: BTreeSet<String>,
    pub protected_maps: Vec<String>,
    pub map_paths: Vec<(String, String)>,
}

pub(super) fn build(state: &AppState, session: &PlanSession) -> Result<Vec<OwnedMutation>, String> {
    let data_root = state
        .config
        .config_path()
        .parent()
        .ok_or("archive.pathEscape")?
        .to_path_buf();
    assemble(
        session,
        TransactionRoots {
            game_root: session.trusted.game_root.clone(),
            data_root,
        },
    )
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
    let mut builder = Builder {
        roots: transaction_roots,
        session,
        writes: BTreeMap::new(),
        moves: Vec::new(),
        ownership: BTreeMap::new(),
        ownership_paths: BTreeSet::new(),
        protected_maps: Vec::new(),
        map_paths: Vec::new(),
    };
    builder.index_ownership()?;
    let migration = roots::prepare(&mut builder, solved)?;
    for node in &solved.nodes {
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
                let key = member
                    .member_key
                    .as_deref()
                    .ok_or("modChanges.ambiguousOwnership")?;
                let before = old_current
                    .provides
                    .iter()
                    .find(|before| before.member_key.as_deref() == Some(key))
                    .ok_or("modChanges.targetChanged")?;
                let enabled = node.enabled && member.enabled;
                if before.enabled != enabled {
                    let from = builder.physical(&node.id, key)?;
                    let canonical = mods::canonical_dll_name(&from);
                    let to = if enabled {
                        canonical
                    } else {
                        mods::disabled_dll_file(&canonical)
                    };
                    builder.ensure_owner(&from, &node.id)?;
                    builder.add_move(&node.id, Some(key), &from, &to)?;
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
    fn index_ownership(&mut self) -> Result<(), String> {
        if let Some(package) = self
            .session
            .solved
            .as_ref()
            .map(final_package)
            .transpose()?
            .flatten()
        {
            self.protected_maps.push(format!("{package}/levels"));
        }
        if let Some(package) = self.session.trusted.level_roots.package_root() {
            self.protected_maps.push(inventory::relative(
                &self.roots.game_root,
                &package.join("levels"),
            )?);
        }
        self.protected_maps.push(inventory::relative(
            &self.roots.game_root,
            &crate::level_sets::disabled_root(&self.roots.game_root),
        )?);
        let manifests = self.roots.data_root.join("manifests");
        if manifests.exists() {
            for entry in fs::read_dir(&manifests).map_err(|error| error.to_string())? {
                let path = entry.map_err(|error| error.to_string())?.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("toml"))
                {
                    let text =
                        fs::read_to_string(&path).map_err(|_| "modChanges.ambiguousOwnership")?;
                    let manifest = toml::from_str::<archive::InstallManifest>(&text)
                        .map_err(|_| "modChanges.ambiguousOwnership")?;
                    let owners = self
                        .session
                        .trusted
                        .files
                        .iter()
                        .filter(|(_, node)| {
                            node.manifests
                                .iter()
                                .any(|owned| owned.repository_id == manifest.repository_id)
                        })
                        .map(|(id, _)| id.clone())
                        .collect::<Vec<_>>();
                    let owner = if owners.len() == 1 {
                        owners[0].clone()
                    } else {
                        format!("unknown-package:{}", manifest.repository_id)
                    };
                    for file in manifest.files {
                        self.ownership
                            .entry(inventory::key(&mods::canonical_dll_name(&file)))
                            .or_default()
                            .insert(owner.clone());
                        self.ownership_paths.insert(file);
                    }
                }
            }
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
        Ok(())
    }

    pub fn ensure_owner(&self, relative: &str, owner: &str) -> Result<(), String> {
        let key = inventory::key(&mods::canonical_dll_name(relative));
        if self
            .ownership
            .get(&key)
            .is_some_and(|owners| owners.len() == 1 && owners.contains(owner))
        {
            Ok(())
        } else {
            log::warn!(
                "整链安装归属不明确：节点={owner}，路径={relative}，已知归属={:?}",
                self.ownership.get(&key)
            );
            Err("modChanges.ambiguousOwnership".into())
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
        if expected == Fingerprint::Missing
            || Fingerprint::capture(&target)? != Fingerprint::Missing
        {
            log::warn!(
                "整链移动路径冲突：节点={owner}，原路径={from}，目标路径={to}，原因=原文件缺失或目标已存在"
            );
            return Err("modChanges.pathCollision".into());
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
                        return Err("modChanges.pathCollision".into());
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
                    return Err("modChanges.pathCollision".into());
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
            log::warn!(
                "整链写入路径冲突：节点={owner}，路径={}，原因=目标是目录",
                target.relative
            );
            return Err("modChanges.pathCollision".into());
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
                    return Err("modChanges.pathCollision".into());
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
                return Err("modChanges.ambiguousOwnership".into());
            }
            if let Mutation::File(previous_file) = &mut previous.mutation {
                if previous_file.expected != *expected {
                    return Err("modChanges.targetChanged".into());
                }
                if previous_file.source.is_some() && source.is_some() {
                    log::warn!(
                        "整链写入路径冲突：节点={owner}，路径={}，原因=同一目标重复写入",
                        target.relative
                    );
                    return Err("modChanges.pathCollision".into());
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
                if copies
                    .iter()
                    .any(|copy| copy.member_key == inventory::key(&canonical))
                {
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
                self.remove(
                    &node.id,
                    data(&format!("manifests/{}.toml", old.repository_id)),
                )?;
            }
        }
        let mut final_files = Vec::new();
        let release_directory = std::cell::OnceCell::new();
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
            let destination = if is_dll {
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
                log::warn!(
                    "整链安装路径冲突：节点={}，路径={}，原因=MOD资产试图覆盖地图目录",
                    node.id,
                    destination
                );
                return Err("modChanges.pathCollision".into());
            }
            let ownership_key = inventory::key(&mods::canonical_dll_name(&destination));
            let owners = self.ownership.get(&ownership_key);
            if owners.is_some_and(|owners| owners.len() != 1 || !owners.contains(&node.id)) {
                return Err(file_conflict(&node.id, &destination));
            }
            if is_dll {
                let canonical = mods::canonical_dll_name(&destination);
                for variant in [&canonical, &mods::disabled_dll_file(&canonical)] {
                    if Fingerprint::capture(&game(variant).resolve(&self.roots)?)?
                        != Fingerprint::Missing
                        && owners.is_none()
                    {
                        return Err(file_conflict(&node.id, variant));
                    }
                }
            }
            let target = game(&destination);
            let target_path = target.resolve(&self.roots)?;
            let expected = Fingerprint::capture(&target_path)?;
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
            let adopting = expected != Fingerprint::Missing && owners.is_none();
            if adopting {
                let identical_resource = !is_dll
                    && matches!(expected, Fingerprint::File { .. })
                    && expected.same_content(&source.fingerprint)
                    && release_directory
                        .get_or_init(|| self.release_directory(&node.id, payload, &manifest))
                        .as_ref()
                        .is_some_and(|directory| {
                            self.release_path_unclaimed(&destination, &target_path, directory)
                        });
                if !identical_resource {
                    return Err(file_conflict(&node.id, &destination));
                }
                log::info!(
                    "整链安装将一致的 Release 资源补入清单：节点={}，路径={destination}",
                    node.id
                );
            }
            // Do not re-sample either side after authorizing the untracked target.
            self.write(
                &node.id,
                Some(ItemKey::new(&node.id, None)),
                FileChange {
                    target,
                    source: Some(source),
                    expected,
                    require_single_link: adopting,
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
        let manifest_target = data(&format!("manifests/{}.toml", manifest.repository_id));
        let expected = Fingerprint::capture(&manifest_target.resolve(&self.roots)?)?;
        if expected != Fingerprint::Missing
            && !old_files.is_some_and(|old| {
                old.manifests
                    .iter()
                    .any(|old| old.repository_id == manifest.repository_id)
            })
        {
            return Err("modChanges.ambiguousOwnership".into());
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
                require_single_link: false,
            },
        )
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
