use super::loose_files::{LooseFile, exact_replacement};
use super::missing_install::MissingInstallRecord;
use super::*;
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
}

impl ReleaseTargetClaims {
    pub fn is_empty(&self) -> bool {
        self.loose.is_empty() && self.missing_records.is_empty()
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
        roots: &TransactionRoots,
        written: &BTreeMap<String, String>,
    ) -> Result<(), String> {
        for (owner, record) in &self.missing_records {
            if let Some(relative) = record.unexpected_path(roots, written)? {
                return Err(InstallBlock::VerificationChanged.report(owner, &relative));
            }
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
        for (node, payload) in std::iter::once(first).chain(releases) {
            let current = node
                .current
                .as_ref()
                .ok_or_else(|| "modChanges.targetChanged".to_owned())?;
            let levels = crate::mod_changes::level_trees::selected_bundled_level_root(
                &node.id, solved, current, payload,
            )?;
            if let Some(record) =
                builder.missing_install_record(&node.id, payload, levels.as_deref())?
            {
                result.missing_records.insert(node.id.clone(), record);
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
