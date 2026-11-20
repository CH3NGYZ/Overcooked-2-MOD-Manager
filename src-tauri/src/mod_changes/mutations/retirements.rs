use super::super::inventory::RetiredModCopy;
use super::{
    Builder, FileChange, Fingerprint, ItemKey, Mutation, OwnedMutation, RootKind, TransactionPath,
    data, file_conflict, game, inventory, within,
};
use crate::mods;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};

pub(super) fn ordered_writes(
    writes: impl IntoIterator<Item = (String, OwnedMutation)>,
    copies: &BTreeMap<String, Vec<RetiredModCopy>>,
) -> impl Iterator<Item = OwnedMutation> {
    let retired_paths = copies
        .values()
        .flatten()
        .map(|copy| inventory::key(&copy.dll_file))
        .collect::<BTreeSet<_>>();
    let mut removals = Vec::new();
    let mut replacements = Vec::new();
    let mut remaining = Vec::new();
    for (owner, owned) in writes {
        // All obsolete files of the consolidated upgrade leave first,
        // regardless of path order. A still-used destination is a single
        // atomic replacement and goes before unrelated dependent writes.
        let queue = match &owned.mutation {
            Mutation::File(change) if copies.contains_key(&owner) && change.source.is_none() => {
                &mut removals
            }
            Mutation::File(change)
                if change.target.root == RootKind::Game
                    && retired_paths.contains(&inventory::key(&change.target.relative)) =>
            {
                &mut replacements
            }
            _ => &mut remaining,
        };
        queue.push(owned);
    }
    removals.into_iter().chain(replacements).chain(remaining)
}

impl Builder<'_> {
    pub(super) fn retire_copy(
        &mut self,
        node_id: &str,
        copy: &RetiredModCopy,
        migration: Option<&super::roots::RootMigration>,
    ) -> Result<(), String> {
        let relative = &copy.dll_file;
        if migration.is_some_and(|migration| within(relative, &migration.from))
            || self.protected_maps.iter().any(|map| within(relative, map))
        {
            return Err(file_conflict(node_id, relative));
        }
        self.ensure_owner(relative, node_id)?;
        let canonical = mods::canonical_dll_name(relative);
        let opposite = if *relative == canonical {
            mods::disabled_dll_file(&canonical)
        } else {
            canonical
        };
        if Fingerprint::capture(&game(&opposite).resolve(&self.roots)?)? != Fingerprint::Missing {
            return Err("modChanges.targetChanged".into());
        }
        self.remove_retired_file(node_id, &copy.member_key, game(relative), &copy.fingerprint)?;
        match (&copy.manifest_id, &copy.manifest_fingerprint) {
            (Some(id), Some(fingerprint)) => self.remove_retired_file(
                node_id,
                &copy.member_key,
                data(&format!("manifests/{id}.toml")),
                fingerprint,
            ),
            (None, None) => Ok(()),
            _ => Err("modChanges.ambiguousOwnership".into()),
        }
    }

    fn remove_retired_file(
        &mut self,
        node_id: &str,
        member_key: &str,
        target: TransactionPath,
        fingerprint: &Fingerprint,
    ) -> Result<(), String> {
        let path = target.resolve(&self.roots)?;
        if Fingerprint::capture(&path)? != *fingerprint
            || !matches!(fingerprint, Fingerprint::File { .. })
            || !fs::File::open(&path).is_ok_and(|file| super::has_single_link(&file))
        {
            return Err("modChanges.targetChanged".into());
        }
        self.write(
            node_id,
            Some(ItemKey::new(node_id, Some(member_key))),
            FileChange {
                target,
                source: None,
                expected: fingerprint.clone(),
                require_single_link: true,
            },
        )
    }
}
