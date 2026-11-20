//! Prove ownership of same-byte residues when the core and its manifest are gone.
use super::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::mod_changes) struct ResidueAdoptions(BTreeMap<(String, String), Residue>);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mod_changes::mutations) struct Residue {
    pub relative: String,
    pub expected: Fingerprint,
    loose_node: Option<String>,
}

impl ResidueAdoptions {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn consumes_loose_nodes(&self) -> bool {
        self.0.values().any(|residue| residue.loose_node.is_some())
    }
    pub(in crate::mod_changes::mutations) fn get(
        &self,
        owner: &str,
        key: &str,
    ) -> Option<&Residue> {
        self.0.get(&(owner.into(), key.into()))
    }

    pub fn consumes(&self, node: &str) -> bool {
        self.0
            .values()
            .any(|residue| residue.loose_node.as_deref() == Some(node))
    }

    pub fn project(&self, solved: &mut Inventory) {
        for node in &mut solved.nodes {
            if self.consumes(&node.id) {
                node.current = None;
                node.enabled = false;
                node.candidates.clear();
            }
        }
    }

    pub fn verify(
        &self,
        state: &AppState,
        session: &PlanSession,
        solved: &Inventory,
    ) -> Result<(), String> {
        let actual = Self::collect(state, session, solved)?;
        if self != &actual {
            if let Some(((owner, _), residue)) = self
                .0
                .iter()
                .chain(&actual.0)
                .find(|(key, _)| self.0.get(*key) != actual.0.get(*key))
            {
                return Err(file_conflict(owner, &residue.relative));
            }
            return Err("modChanges.targetChanged".into());
        }
        Ok(())
    }

    pub fn collect(
        state: &AppState,
        session: &PlanSession,
        solved: &Inventory,
    ) -> Result<Self, String> {
        let mut releases = solved
            .nodes
            .iter()
            .filter_map(|node| fresh_release(session, node));
        let Some(first) = releases.next() else {
            return Ok(Self::default());
        };
        let data_root = state
            .config
            .config_path()
            .parent()
            .ok_or("archive.pathEscape")?
            .to_path_buf();
        let roots = TransactionRoots {
            game_root: session.trusted.game_root.clone(),
            data_root,
        };
        let builder = Builder::indexed(session, roots, solved)?;
        let mut result = Self::default();
        let mut seen = BTreeSet::new();
        for (node, payload) in std::iter::once(first).chain(releases) {
            builder.collect_residues(solved, node, payload, &mut result, &mut seen)?;
        }
        Ok(result)
    }
}

// A downloaded candidate may be retained for retries after it loses the solve.
// Only the candidate actually selected in the current graph can claim files.
fn fresh_release<'a>(
    session: &'a PlanSession,
    node: &'a ChangeNode,
) -> Option<(&'a ChangeNode, &'a Payload)> {
    let chosen = node.current.as_ref()?;
    if node.kind != NodeKind::Mod
        || chosen.raw
        || chosen.tag.is_none()
        || session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|old| old.id == node.id)?
            .current
            .is_some()
        || !session.trusted.files.get(&node.id)?.manifests.is_empty()
    {
        return None;
    }
    let payload = session
        .verified_payloads
        .get(&(node.id.clone(), chosen.id.clone()))
        .or_else(|| {
            session
                .payloads
                .get(&node.id)
                .filter(|payload| payload.candidate.id == chosen.id)
        })?;
    (payload.candidate.source_id.is_some() && !payload.candidate.verification_pending)
        .then_some((node, payload))
}

impl Builder<'_> {
    fn collect_residues(
        &self,
        solved: &Inventory,
        node: &ChangeNode,
        payload: &Payload,
        result: &mut ResidueAdoptions,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        let repo = self
            .session
            .trusted
            .config
            .repositories
            .iter()
            .find(|repo| Some(&repo.id) == payload.candidate.source_id.as_ref())
            .ok_or("modChanges.targetChanged")?;
        let guid = repo.plugin_guid.trim();
        let mut primaries = payload
            .candidate
            .provides
            .iter()
            .filter(|p| !guid.is_empty() && p.guid.eq_ignore_ascii_case(guid));
        let primary_key = primaries
            .next()
            .and_then(|p| p.member_key.as_deref())
            .filter(|_| primaries.next().is_none());
        let primary = primary_key.and_then(|key| {
            payload
                .files
                .iter()
                .find(|path| inventory::key(&mods::canonical_dll_name(path)) == key)
        });
        let directory = primary
            .and_then(|path| Path::new(path).parent())
            .and_then(Path::to_str)
            .filter(|dir| {
                let plugins = mods::file_state::PLUGINS_DIR;
                within(dir, plugins) && path_key(Path::new(dir)) != path_key(Path::new(plugins))
            });
        let current = node.current.as_ref().ok_or("modChanges.targetChanged")?;
        let verified_binding = payload
            .manifest
            .as_ref()
            .and_then(|manifest| manifest.repository_binding.as_ref())
            .is_some_and(|binding| {
                binding.is_valid()
                    && binding.matches_repository(repo)
                    && Some(&binding.tag) == current.tag.as_ref()
            });
        let core_missing = !guid.is_empty()
            && !self
                .session
                .trusted
                .inventory
                .nodes
                .iter()
                .filter_map(|old| old.current.as_ref())
                .flat_map(|current| &current.provides)
                .any(|plugin| plugin.guid.eq_ignore_ascii_case(guid));
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
            let target = if is_dll { &canonical } else { source };
            let primary_path = primary_key == Some(key.as_str());
            let mut live = self.residue_file(&node.id, target)?;
            if is_dll {
                let disabled = mods::disabled_dll_file(&canonical);
                if let Some(inactive) = self.residue_file(&node.id, &disabled)? {
                    if live.is_some() {
                        return Err(file_conflict(&node.id, target));
                    }
                    live = Some(inactive);
                }
            }
            let Some((physical, before)) = live else {
                continue;
            };
            let rejected = || file_conflict(&node.id, &physical);
            let digest = payload
                .release_files
                .as_ref()
                .and_then(|files| files.get(&path_key(Path::new(source))))
                .ok_or_else(rejected)?;
            let dir = directory
                .filter(|dir| within(&physical, dir))
                .ok_or_else(rejected)?;
            if primary_path
                || !core_missing
                || !verified_binding
                || !before.same_content(digest)
                || !seen.insert(path_key(Path::new(&canonical)))
            {
                return Err(rejected());
            }
            let registered = registry
                .get_or_init(|| {
                    crate::level_sets::checked_registered_plugins(
                        &self.roots.data_root.join("manifests"),
                        true,
                    )
                })
                .as_ref()
                .map_err(|_| rejected())?;
            let loose_node = if is_dll {
                Some(
                    self.loose_owner(solved, node, &key, &physical)
                        .ok_or_else(rejected)?,
                )
            } else {
                if self.ownership.contains_key(&key) {
                    return Err(rejected());
                }
                None
            };
            if !self.residue_is_unclaimed(&physical, dir, registered, loose_node.as_deref()) {
                return Err(rejected());
            }
            result.0.insert(
                (node.id.clone(), key),
                Residue {
                    relative: physical,
                    expected: before,
                    loose_node,
                },
            );
        }
        Ok(())
    }

    fn residue_file(
        &self,
        owner: &str,
        relative: &str,
    ) -> Result<Option<(String, Fingerprint)>, String> {
        let path = game(relative)
            .resolve(&self.roots)
            .map_err(|_| file_conflict(owner, relative))?;
        let fingerprint =
            Fingerprint::capture(&path).map_err(|_| file_conflict(owner, relative))?;
        match fingerprint {
            Fingerprint::Missing => Ok(None),
            Fingerprint::File { .. } => Ok(Some((relative.to_owned(), fingerprint))),
            Fingerprint::Directory { .. } => Err(file_conflict(owner, relative)),
        }
    }

    fn residue_is_unclaimed(
        &self,
        relative: &str,
        directory: &str,
        registered: &[crate::level_sets::RegisteredLevelSetPlugin],
        loose: Option<&str>,
    ) -> bool {
        let resolve = |name: &str| -> Option<PathBuf> {
            fs::canonicalize(game(name).resolve(&self.roots).ok()?).ok()
        };
        let (Some(real_path), Some(directory), Some(plugins)) = (
            resolve(relative),
            resolve(directory),
            resolve(mods::file_state::PLUGINS_DIR),
        ) else {
            return false;
        };
        if directory == plugins
            || !directory.starts_with(&plugins)
            || !real_path.starts_with(&directory)
        {
            return false;
        }
        let claims = self
            .external_claims
            .iter()
            .map(String::as_str)
            .chain(registered.iter().map(|entry| entry.dll.as_str()))
            .chain(
                self.session
                    .trusted
                    .files
                    .iter()
                    .filter(|(id, _)| Some(id.as_str()) != loose)
                    .flat_map(|(_, node)| node.summary.members.iter().map(|m| m.dll_file.as_str())),
            );
        self.physically_unclaimed(relative, &real_path, claims)
    }

    fn loose_owner(
        &self,
        solved: &Inventory,
        owner: &ChangeNode,
        key: &str,
        physical: &str,
    ) -> Option<String> {
        let owners = self.ownership.get(key)?;
        let id = owners.first()?;
        if owners.len() != 1
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
            return None;
        }
        let files = self.session.trusted.files.get(id)?;
        let [member] = files.summary.members.as_slice() else {
            return None;
        };
        let original = self
            .session
            .trusted
            .inventory
            .nodes
            .iter()
            .find(|old| &old.id == id)?;
        let after = solved.nodes.iter().find(|entry| &entry.id == id)?;
        let stays_loose = after == original || (after.current.is_none() && !after.enabled);
        let incoming = owner
            .current
            .as_ref()?
            .provides
            .iter()
            .find(|p| p.member_key.as_deref() == Some(key))?;
        let other_bundle_claim = member.guid.as_deref().is_some_and(|guid| {
            self.session.trusted.config.repositories.iter().any(|repo| {
                Some(&repo.id)
                    != owner
                        .current
                        .as_ref()
                        .and_then(|candidate| candidate.source_id.as_ref())
                    && repo
                        .bundled_plugin_guids
                        .iter()
                        .any(|candidate| crate::mod_changes::evaluate::same_guid(candidate, guid))
            })
        });
        (stays_loose
            && original.mutable
            && files.manifests.is_empty()
            && files.map_path.is_none()
            && member.key == key
            && !other_bundle_claim
            && (!original.enabled || (owner.enabled && incoming.enabled))
            && member.enabled == !physical.to_ascii_lowercase().ends_with(".disabled"))
        .then(|| id.clone())
    }
}
