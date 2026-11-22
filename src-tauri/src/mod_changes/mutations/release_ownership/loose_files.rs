//! Freeze existing loose endpoints for the selected Release and its final solve.
use super::claims::ReleaseTargetError;
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mod_changes::mutations) struct LooseFile {
    pub relative: String,
    pub expected: Fingerprint,
    pub(super) loose_node: Option<String>,
    pub(super) changes_content: bool,
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
    pub(super) fn collect_loose_files(
        &self,
        solved: &Inventory,
        node: &ChangeNode,
        payload: &Payload,
        bundled_levels: Option<&str>,
        result: &mut ReleaseTargetClaims,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), ReleaseTargetError> {
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
        let registry = std::cell::OnceCell::new();
        for source in &payload.files {
            // Existing user maps are preserved by the shared tree composer.
            if bundled_levels.is_some_and(|levels| within(source, levels)) {
                continue;
            }
            let canonical = mods::canonical_dll_name(source);
            let key = inventory::key(&canonical);
            let is_dll = canonical.to_ascii_lowercase().ends_with(".dll");
            let scoped_release =
                current.tag.is_some() && !current.raw && payload.candidate.source_id.is_some();
            if !is_dll && !scoped_release {
                continue;
            }
            if result
                .missing_record(&node.id)
                .is_some_and(|record| record.owns_previous_path(&key))
            {
                continue;
            }
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
            if is_dll
                && payload.downloaded
                && own
                && old.manifests.is_empty()
                && old.summary.members.len() == 1
                && !self
                    .session
                    .trusted
                    .declared_bundle_switch_members
                    .contains_key(&node.id)
            {
                let incoming = current
                    .provides
                    .iter()
                    .find(|member| member.member_key.as_deref() == Some(&key))
                    .ok_or_else(|| {
                        InstallBlock::PluginIdentityUnknown.report(&node.id, &canonical)
                    })?;
                let (relative, expected) = self.prove_loose_dll(&node.id, &canonical, incoming)?;
                if !seen.insert(path_key(Path::new(&canonical))) {
                    return Err(InstallBlock::PathCollision
                        .report(&node.id, &relative)
                        .into());
                }
                result.loose.insert(
                    (node.id.clone(), key),
                    LooseFile {
                        relative,
                        expected,
                        loose_node: None,
                        changes_content: true,
                    },
                );
                continue;
            }
            if !scoped_release {
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
                && self.records.claims(&key).next().is_some();
            let Some((physical, before)) =
                live.or_else(|| foreign_manifest.then(|| (target.clone(), Fingerprint::Missing)))
            else {
                continue;
            };
            let report = |block: InstallBlock| block.report(&node.id, &physical);
            let unknown = || report(InstallBlock::Unverified);
            let claimed = || self.report_ownership_conflict(&key, &node.id, &physical);
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
                return Err(ReleaseTargetError::ForeignManifest {
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
            self.ensure_loose_path_unclaimed(
                &physical,
                scope.directory,
                registered,
                scan_owner,
                false,
                Some(&node.id),
            )
            .map_err(report)?;
            result.loose.insert(
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
        standalone: bool,
        requester: Option<&str>,
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
        if (!standalone && directory == plugins)
            || !directory.starts_with(&plugins)
            || !real_path.starts_with(&directory)
        {
            return Err(InstallBlock::OutsidePackage);
        }
        let key = inventory::key(&mods::canonical_dll_name(relative));
        let claims = self
            .records
            .claimed_files()
            .map(|(file, record)| (file, InstallBlock::InstallRecord, Some(record)))
            .chain(
                registered
                    .iter()
                    .map(|entry| (entry.dll.as_str(), InstallBlock::MapProtected, None)),
            )
            .chain(self.session.trusted.files.iter().flat_map(|(id, node)| {
                let key = &key;
                node.summary
                    .members
                    .iter()
                    .filter(move |member| Some(id.as_str()) != scan_owner || member.key != *key)
                    .map(|m| (m.dll_file.as_str(), InstallBlock::PathOwned, None))
            }));
        self.ensure_physically_unclaimed(relative, &real_path, claims, requester)
    }

    pub(super) fn verify_member(
        &self,
        incoming: &ProvidedPlugin,
        physical: &str,
    ) -> Result<(), InstallBlock> {
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
