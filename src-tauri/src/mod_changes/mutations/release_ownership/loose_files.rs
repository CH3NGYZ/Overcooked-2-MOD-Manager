//! Freeze existing loose endpoints for the selected Release and its final solve.
use super::*;
use crate::mod_changes::session::UntrackedOverwrite;

#[derive(Debug)]
pub(in crate::mod_changes) enum LooseClaimError {
    ForeignManifest { owner: String, message: String },
    Unverified(String),
}

impl From<String> for LooseClaimError {
    fn from(message: String) -> Self {
        Self::Unverified(message)
    }
}

impl From<LooseClaimError> for String {
    fn from(error: LooseClaimError) -> Self {
        match error {
            LooseClaimError::ForeignManifest { message, .. }
            | LooseClaimError::Unverified(message) => message,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::mod_changes) struct LooseFileClaims(BTreeMap<(String, String), LooseFile>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mod_changes::mutations) struct LooseFile {
    pub relative: String,
    pub expected: Fingerprint,
    loose_node: Option<String>,
    changes_content: bool,
}

impl LooseFileClaims {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn overwrites(&self) -> Vec<UntrackedOverwrite> {
        self.0
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
        self.0.get(&(owner.into(), key.into()))
    }

    pub fn consumes(&self, node: &str) -> bool {
        self.0
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
                .0
                .iter()
                .chain(&actual.0)
                .find(|(key, _)| self.0.get(*key) != actual.0.get(*key))
            {
                return Err(InstallBlock::VerificationChanged.report(owner, &file.relative));
            }
            return Err("modChanges.targetChanged".into());
        }
        Ok(())
    }

    pub fn collect(
        session: &PlanSession,
        roots: TransactionRoots,
        solved: &Inventory,
    ) -> Result<Self, LooseClaimError> {
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
            builder.collect_loose_files(solved, node, payload, &mut result, &mut seen)?;
        }
        // Only the final selected Release may account for an omitted loose node.
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
    exact_replacement(
        node,
        chosen,
        payload,
        session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|old| old.id == node.id)?,
    )
    .then_some((node, payload))
}

pub(super) fn exact_replacement(
    node: &ChangeNode,
    chosen: &Candidate,
    payload: &Payload,
    old: &ChangeNode,
) -> bool {
    node.kind == NodeKind::Mod
        && !chosen.raw
        && chosen.tag.is_some()
        && old
            .current
            .as_ref()
            .is_none_or(|prior| prior.id != chosen.id)
        && payload.candidate.source_id.is_some()
        && !payload.candidate.verification_pending
}

impl Builder<'_> {
    fn collect_loose_files(
        &self,
        solved: &Inventory,
        node: &ChangeNode,
        payload: &Payload,
        result: &mut LooseFileClaims,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), LooseClaimError> {
        let old = self
            .session
            .trusted
            .files
            .get(&node.id)
            .ok_or_else(|| "modChanges.targetChanged".to_string())?;
        let current = node
            .current
            .as_ref()
            .ok_or_else(|| "modChanges.targetChanged".to_string())?;
        let manifest_keys = old
            .manifests
            .iter()
            .flat_map(|manifest| &manifest.files)
            .map(|file| inventory::key(&mods::canonical_dll_name(file)))
            .collect::<BTreeSet<_>>();
        let retirement = self
            .session
            .trusted
            .duplicate_releases
            .copies_for(&node.id, &current.id);
        let package = std::cell::OnceCell::new();
        let bundled_levels =
            crate::mod_changes::level_trees::selected_bundled_level_root(solved, current, payload)?;
        let registry = std::cell::OnceCell::new();
        for source in &payload.files {
            // Existing user maps are preserved by the shared tree composer.
            if bundled_levels
                .as_ref()
                .is_some_and(|levels| within(source, levels))
            {
                continue;
            }
            let canonical = mods::canonical_dll_name(source);
            let key = inventory::key(&canonical);
            let is_dll = canonical.to_ascii_lowercase().ends_with(".dll");
            // Existing manifest writes and explicitly retired copies already
            // have their own transaction authority and are not loose claims.
            let own = self
                .ownership
                .get(&key)
                .is_some_and(|owners| owners.len() == 1 && owners.contains(&node.id));
            if (own && manifest_keys.contains(&key))
                || retirement.is_some_and(|copies| copies.iter().any(|copy| copy.member_key == key))
            {
                continue;
            }
            let target = if is_dll { &canonical } else { source };
            let proof = package.get_or_init(|| self.package_scope(&node.id, payload));
            if own && proof.is_err() {
                // Existing scanned owners still use the ordinary replacement
                // checks when a scoped claim is unavailable (e.g. no digest).
                continue;
            }
            let live = self
                .live_endpoint(target, is_dll)
                .map_err(|error| error.report(&node.id))?;
            // A manifest still owns a missing file. Keep that path in the
            // conflict check once the endpoint lookup has succeeded.
            let foreign_manifest = !own
                && self.ownership.contains_key(&key)
                && self
                    .external_claims
                    .iter()
                    .any(|claim| inventory::key(&mods::canonical_dll_name(claim)) == key);
            let Some((physical, before)) =
                live.or_else(|| foreign_manifest.then(|| (target.clone(), Fingerprint::Missing)))
            else {
                continue;
            };
            let report = |block: InstallBlock| block.report(&node.id, &physical);
            let unknown = || report(InstallBlock::Unverified);
            let claimed = || report(self.ownership_block(&key, &node.id));
            let scope = proof.as_ref().map_err(|failure| {
                if foreign_manifest {
                    claimed()
                } else {
                    failure.report(&node.id, &physical)
                }
            })?;
            let changes_content = scope
                .changes_file(payload, source, &key, &physical, &before, own)
                .map_err(|reason| {
                    if foreign_manifest {
                        claimed()
                    } else {
                        report(reason)
                    }
                })?;
            if !seen.insert(path_key(Path::new(&canonical))) {
                return Err(report(InstallBlock::PathCollision).into());
            }
            let registered = registry
                .get_or_init(|| self.registered_map_plugins())
                .as_ref()
                .map_err(|_| report(InstallBlock::MapRecordsUnreadable))?;
            // Only a positive on-disk manifest claim makes this branch eligible
            // for another version. Unclear scope, aliases and read failures do not.
            if foreign_manifest {
                return Err(LooseClaimError::ForeignManifest {
                    owner: node.id.clone(),
                    message: claimed(),
                });
            }
            let loose_node = if is_dll {
                let incoming = payload
                    .candidate
                    .provides
                    .iter()
                    .find(|member| member.member_key.as_deref() == Some(key.as_str()))
                    .ok_or_else(unknown)?;
                if own {
                    let original = self.physical(&node.id, &key).map_err(|_| unknown())?;
                    if original != physical {
                        return Err(report(InstallBlock::AliasesOrLink).into());
                    }
                    self.verify_member(incoming, &physical).map_err(report)?;
                    None
                } else {
                    let loose = self
                        .loose_owner(&node.id, payload, incoming, &key, &physical)
                        .map_err(report)?;
                    let after = solved
                        .nodes
                        .iter()
                        .find(|entry| entry.id == loose.id)
                        .ok_or_else(unknown)?;
                    if (after != loose && (after.current.is_some() || after.enabled))
                        || !keeps_provider(node, &key, loose)
                    {
                        return Err(report(InstallBlock::PathOwned).into());
                    }
                    Some(loose.id.clone())
                }
            } else {
                if self.ownership.contains_key(&key) {
                    return Err(report(InstallBlock::PathOwned).into());
                }
                None
            };
            let scan_owner = loose_node.as_deref().or(own.then_some(node.id.as_str()));
            self.ensure_loose_path_unclaimed(&physical, scope.directory, registered, scan_owner)
                .map_err(report)?;
            result.0.insert(
                (node.id.clone(), key),
                LooseFile {
                    relative: physical,
                    expected: before,
                    loose_node,
                    changes_content,
                },
            );
        }
        Ok(())
    }

    pub(super) fn ensure_loose_path_unclaimed(
        &self,
        relative: &str,
        directory: &str,
        registered: &[crate::level_sets::RegisteredLevelSetPlugin],
        scan_owner: Option<&str>,
    ) -> Result<(), InstallBlock> {
        let resolve = |name: &str| -> Option<PathBuf> {
            fs::canonicalize(game(name).resolve(&self.roots).ok()?).ok()
        };
        let (Some(real_path), Some(directory), Some(plugins)) = (
            resolve(relative),
            resolve(directory),
            resolve(mods::file_state::PLUGINS_DIR),
        ) else {
            return Err(InstallBlock::Unverified);
        };
        if directory == plugins
            || !directory.starts_with(&plugins)
            || !real_path.starts_with(&directory)
        {
            return Err(InstallBlock::OutsidePackage);
        }
        let key = inventory::key(&mods::canonical_dll_name(relative));
        let claims = self
            .external_claims
            .iter()
            .map(|file| (file.as_str(), InstallBlock::InstallRecord))
            .chain(
                registered
                    .iter()
                    .map(|entry| (entry.dll.as_str(), InstallBlock::MapProtected)),
            )
            .chain(self.session.trusted.files.iter().flat_map(|(id, node)| {
                let key = &key;
                node.summary
                    .members
                    .iter()
                    .filter(move |member| Some(id.as_str()) != scan_owner || member.key != *key)
                    .map(|m| (m.dll_file.as_str(), InstallBlock::PathOwned))
            }));
        self.ensure_physically_unclaimed(relative, &real_path, claims)
    }

    fn verify_member(&self, incoming: &ProvidedPlugin, physical: &str) -> Result<(), InstallBlock> {
        let path = game(physical)
            .resolve(&self.roots)
            .map_err(|_| InstallBlock::Unverified)?;
        let old = mods::read_dll_details(&path, physical);
        let old_guid = old.guid.as_deref().unwrap_or_default();
        if same_guid(old_guid, &incoming.guid) {
            Ok(())
        } else if old_guid.trim().is_empty() || incoming.guid.trim().is_empty() {
            Err(InstallBlock::PluginIdentityUnknown)
        } else {
            Err(InstallBlock::PluginMismatch)
        }
    }

    pub(super) fn loose_owner(
        &self,
        owner: &str,
        payload: &Payload,
        verified: &ProvidedPlugin,
        key: &str,
        physical: &str,
    ) -> Result<&ChangeNode, InstallBlock> {
        let unknown = InstallBlock::Unverified;
        let owners = self.ownership.get(key).ok_or(InstallBlock::Unrecognized)?;
        let id = owners.first().ok_or(unknown)?;
        if owners.len() != 1
            || id == owner
            || !id.starts_with("dll:")
            || !self.session.trusted.toggle_unbound_nodes.contains(id)
            || self.session.request.repair_dependencies_of.as_ref() == Some(id)
            || self
                .session
                .request
                .roots
                .iter()
                .any(|root| &root.node_id == id)
        {
            if owners.iter().any(|id| {
                id.starts_with("map-dll:")
                    || self
                        .session
                        .trusted
                        .files
                        .get(id)
                        .is_some_and(|node| node.map_path.is_some())
            }) {
                return Err(InstallBlock::MapProtected);
            }
            return Err(InstallBlock::PathOwned);
        }
        let files = self.session.trusted.files.get(id).ok_or(unknown)?;
        let [member] = files.summary.members.as_slice() else {
            return Err(unknown);
        };
        let original = self
            .session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|old| &old.id == id)
            .ok_or(unknown)?;
        let other_bundle_claim = member.guid.as_deref().is_some_and(|guid| {
            self.session.trusted.config.repositories.iter().any(|repo| {
                Some(&repo.id) != payload.candidate.source_id.as_ref()
                    && repo
                        .bundled_plugin_guids
                        .iter()
                        .any(|candidate| crate::mod_changes::evaluate::same_guid(candidate, guid))
            })
        });
        if !original.mutable || !original.candidates.is_empty() || !files.manifests.is_empty() {
            return Err(InstallBlock::PathOwned);
        }
        if files.map_path.is_some() {
            return Err(InstallBlock::MapProtected);
        }
        if member.key != key || other_bundle_claim {
            return Err(unknown);
        }
        self.verify_member(verified, physical)?;
        if member.enabled == physical.to_ascii_lowercase().ends_with(".disabled") {
            return Err(InstallBlock::VerificationChanged);
        }
        Ok(original)
    }
}

/// A package replacement cannot silently switch an active standalone provider off.
pub(super) fn keeps_provider(owner: &ChangeNode, key: &str, loose: &ChangeNode) -> bool {
    owner
        .current
        .iter()
        .flat_map(|candidate| &candidate.provides)
        .any(|member| {
            member.member_key.as_deref() == Some(key)
                && (!loose.enabled || (owner.enabled && member.enabled))
        })
}
