use super::loose_files::{LooseFile, exact_replacement};
use super::manifest_replacement::ManifestReplacement;
use super::missing_install::{MissingInstallRecord, repository_id_is_configured};
use super::*;
use crate::mod_changes::progress::PreparationError;
use crate::mod_changes::session::UntrackedOverwrite;

#[derive(Debug)]
pub(in crate::mod_changes) enum ReleaseTargetError {
    ForeignManifest { owner: String, message: String },
    Unverified(String),
}

impl From<String> for ReleaseTargetError {
    fn from(message: String) -> Self {
        Self::Unverified(message)
    }
}

impl From<ReleaseTargetError> for String {
    fn from(error: ReleaseTargetError) -> Self {
        match error {
            ReleaseTargetError::ForeignManifest { message, .. }
            | ReleaseTargetError::Unverified(message) => message,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::mod_changes) struct ReleaseTargetClaims {
    pub(super) loose: BTreeMap<(String, String), LooseFile>,
    missing_records: BTreeMap<String, MissingInstallRecord>,
    orphan_records: BTreeMap<String, ManifestReplacement>,
}

impl ReleaseTargetClaims {
    pub fn is_empty(&self) -> bool {
        self.loose.is_empty() && self.missing_records.is_empty() && self.orphan_records.is_empty()
    }

    pub fn overwrites(&self) -> Vec<UntrackedOverwrite> {
        self.loose
            .iter()
            .filter(|(_, file)| file.changes_content)
            .map(|((id, _), file)| UntrackedOverwrite {
                node_id: id.clone(),
                path: file.relative.clone(),
            })
            .collect()
    }

    pub(in crate::mod_changes::mutations) fn get(
        &self,
        owner: &str,
        key: &str,
    ) -> Option<&LooseFile> {
        self.loose.get(&(owner.into(), key.into()))
    }

    pub(in crate::mod_changes::mutations) fn missing_record(
        &self,
        owner: &str,
    ) -> Option<&MissingInstallRecord> {
        self.missing_records.get(owner)
    }

    pub(in crate::mod_changes::mutations) fn orphan_record(
        &self,
        owner: &str,
    ) -> Option<&ManifestReplacement> {
        self.orphan_records.get(owner)
    }

    pub fn check_orphan_replacements(
        &self,
        session: &PlanSession,
        roots: &TransactionRoots,
        changes: &[OwnedMutation],
    ) -> Result<(), PreparationError> {
        let intent = session.wire_request.manifest_overwrite.is_some();
        if intent
            && !self
                .orphan_records
                .values()
                .any(ManifestReplacement::fulfills_intent)
        {
            return Err("modChanges.targetChanged".into());
        }
        let mut pending = self
            .orphan_records
            .iter()
            .filter(|(_, record)| record.confirmation_required());
        self.verify_orphan_game_writes(roots, changes)?;
        if let Some((owner, record)) = pending.next() {
            return if !intent && pending.next().is_none() {
                Err(record.offer_error(owner, roots))
            } else {
                Err(InstallBlock::RecordExists
                    .report(owner, &record.relative)
                    .into())
            };
        }
        Ok(())
    }

    pub fn verify_orphan_game_writes(
        &self,
        roots: &TransactionRoots,
        changes: &[OwnedMutation],
    ) -> Result<(), String> {
        for (owner, record) in &self.orphan_records {
            record.verify_game_writes(owner, roots, changes)?;
        }
        Ok(())
    }

    pub fn orphan_original_files(
        &self,
    ) -> Vec<(TransactionPath, crate::file_metadata::FileIdentity)> {
        self.orphan_records
            .values()
            .map(|record| (data(&record.relative), record.stamp.identity.clone()))
            .collect()
    }

    pub fn consumes(&self, node: &str) -> bool {
        self.loose
            .values()
            .any(|file| file.loose_node.as_deref() == Some(node))
    }

    pub fn verify(
        &self,
        state: &AppState,
        session: &PlanSession,
        solved: &Inventory,
    ) -> Result<(), String> {
        let roots = transaction_roots(state, session)?;
        let actual = Self::collect(session, roots, solved)?;
        if self != &actual {
            if let Some(((owner, _), file)) = self
                .loose
                .iter()
                .chain(&actual.loose)
                .find(|(key, _)| self.loose.get(*key) != actual.loose.get(*key))
            {
                return Err(InstallBlock::VerificationChanged.report(owner, &file.relative));
            }
            if let Some((owner, record)) = self
                .missing_records
                .iter()
                .chain(&actual.missing_records)
                .find(|(key, _)| self.missing_records.get(*key) != actual.missing_records.get(*key))
            {
                return Err(InstallBlock::VerificationChanged.report(owner, &record.relative));
            }
            if let Some((owner, record)) = self
                .orphan_records
                .iter()
                .chain(&actual.orphan_records)
                .find(|(owner, _)| {
                    self.orphan_records.get(*owner) != actual.orphan_records.get(*owner)
                })
            {
                return Err(InstallBlock::VerificationChanged.report(owner, &record.relative));
            }
            return Err("modChanges.targetChanged".into());
        }
        Ok(())
    }

    pub fn reinstall_writes(&self, changes: &[OwnedMutation]) -> BTreeMap<String, String> {
        let mut written = BTreeMap::new();
        if !self.missing_records.is_empty() {
            for change in changes {
                if let Mutation::Tree(tree) = &change.mutation {
                    if tree.target.root == RootKind::Game && tree.source.is_some() {
                        let relative = &tree.target.relative;
                        written.insert(path_key(Path::new(relative)), relative.clone());
                    }
                }
                let Mutation::File(file) = &change.mutation else {
                    continue;
                };
                if file.target.root == RootKind::Game && file.source.is_some() {
                    let relative = &file.target.relative;
                    if change.items.iter().any(|item| {
                        self.missing_record(&item.node_id)
                            .is_some_and(|record| record.expects_missing(relative))
                    }) {
                        written.insert(path_key(Path::new(relative)), relative.clone());
                    }
                }
            }
        }
        written
    }

    pub fn verify_reinstalled_paths(
        &self,
        state: &AppState,
        roots: &TransactionRoots,
        written: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        let config = self
            .missing_records
            .values()
            .any(|record| record.retired_id.is_some())
            .then(|| state.config.load())
            .transpose()
            .map_err(|_| "modChanges.targetChanged")?;
        for (owner, record) in &self.missing_records {
            if let Some(id) = &record.retired_id
                && (config
                    .as_ref()
                    .is_some_and(|config| repository_id_is_configured(config, id))
                    || Fingerprint::capture(&data(&record.relative).resolve(roots)?)?
                        != Fingerprint::Missing)
            {
                return Err(InstallBlock::VerificationChanged.report(owner, &record.relative));
            }
            if let Some(relative) = record.unexpected_path(roots, written)? {
                return Err(InstallBlock::VerificationChanged.report(owner, &relative));
            }
        }
        for (owner, record) in &self.orphan_records {
            record.verify_vacancy(owner, roots)?;
        }
        Ok(())
    }

    pub fn collect(
        session: &PlanSession,
        roots: TransactionRoots,
        solved: &Inventory,
    ) -> Result<Self, ReleaseTargetError> {
        let mut releases = solved
            .nodes
            .iter()
            .filter_map(|node| selected_release(session, node));
        let Some(first) = releases.next() else {
            return Ok(Self::default());
        };
        let builder = Builder::indexed(session, roots, map_roots(session, solved)?)?;
        let mut result = Self::default();
        let mut seen = BTreeSet::new();
        let mut reserved_records = BTreeSet::new();
        for (node, payload) in std::iter::once(first).chain(releases) {
            let current = node
                .current
                .as_ref()
                .ok_or_else(|| "modChanges.targetChanged".to_owned())?;
            let levels = crate::mod_changes::level_trees::selected_bundled_level_root(
                &node.id, solved, current, payload,
            )?;
            if node.kind == NodeKind::Mod
                && let Some(recovery) =
                    builder.missing_install_record(&node.id, payload, levels.as_deref())?
            {
                let record = recovery.record;
                if !reserved_records.insert(path_key(Path::new(&record.relative))) {
                    return Err(InstallBlock::InstallRecord
                        .report(&node.id, &record.relative)
                        .into());
                }
                for (key, file) in recovery.files {
                    if !seen.insert(path_key(Path::new(&file.relative))) {
                        return Err(InstallBlock::PathCollision
                            .report(&node.id, &file.relative)
                            .into());
                    }
                    result.loose.insert((node.id.clone(), key), file);
                }
                result.missing_records.insert(node.id.clone(), record);
            } else if node.kind == NodeKind::Mod
                && let Some(record) = builder.orphan_manifest_replacement(&node.id, payload)?
            {
                if !reserved_records.insert(path_key(Path::new(&record.relative))) {
                    return Err(InstallBlock::InstallRecord
                        .report(&node.id, &record.relative)
                        .into());
                }
                result.orphan_records.insert(node.id.clone(), record);
            }
            builder.collect_loose_files(
                solved,
                node,
                payload,
                levels.as_deref(),
                &mut result,
                &mut seen,
            )?;
        }
        for (before, after) in session.trusted.inventory.nodes.iter().zip(&solved.nodes) {
            if before.id.starts_with("dll:")
                && before.current.is_some()
                && after.current.is_none()
                && !result.consumes(&before.id)
            {
                return Err(InstallBlock::Unverified
                    .report(&before.id, before.id.trim_start_matches("dll:"))
                    .into());
            }
        }
        Ok(result)
    }
}

// A downloaded candidate may be retained for retries after it loses the solve.
// Only the candidate actually selected in the current graph can claim files.
fn selected_release<'a>(
    session: &'a PlanSession,
    node: &'a ChangeNode,
) -> Option<(&'a ChangeNode, &'a Payload)> {
    let chosen = node.current.as_ref()?;
    let payload = session
        .verified_payloads
        .get(&(node.id.clone(), chosen.id.clone()))
        .or_else(|| {
            session
                .payloads
                .get(&node.id)
                .filter(|payload| payload.candidate.id == chosen.id)
        })?;
    let previous = session
        .trusted
        .inventory
        .nodes
        .iter()
        .find(|old| old.id == node.id)?;
    ((payload.downloaded
        && previous
            .current
            .as_ref()
            .is_none_or(|old| old.id != chosen.id))
        || exact_replacement(node, chosen, payload, previous))
    .then_some((node, payload))
}
