use super::missing_install::endpoints;
use super::*;
use crate::mod_changes::mutations::install_block::report_record_claim;
use crate::mod_changes::mutations::records::{InstallRecord, RecordStamp};
use crate::mod_changes::progress::{ManifestOverwriteOffer, PreparationError, SourceSwitch};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::mod_changes::mutations) struct ManifestReplacement {
    pub relative: String,
    pub stamp: RecordStamp,
    pub(super) confirmation: Option<OverwriteConfirmation>,
    pub(super) recorded_paths: Vec<String>,
    vacant_paths: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct OverwriteConfirmation {
    offer: ManifestOverwriteOffer,
    pub(super) approved: bool,
}

pub(super) struct RecordedEndpoint {
    pub(super) relative: String,
    pub(super) path: PathBuf,
    pub(super) missing: bool,
}

pub(super) fn overlaps_claim(left: &str, right: &str) -> bool {
    let normalizer = icu_normalizer::ComposingNormalizer::new_nfc();
    let left = mods::manifest_dll_key(left);
    let right = mods::manifest_dll_key(right);
    paths_overlap(&normalizer.normalize(&left), &normalizer.normalize(&right))
}

fn vacant(roots: &TransactionRoots, root: &Path, relative: &str) -> bool {
    game(relative).resolve(roots).is_ok_and(|path| {
        fs::symlink_metadata(&path).is_err_and(|error| error.kind() == ErrorKind::NotFound)
            && crate::game::resolve_existing_ancestor(&path)
                .is_ok_and(|actual| actual == root.join(relative))
    })
}

pub(super) fn recorded_endpoints(
    roots: &TransactionRoots,
    owner: &str,
    files: &[String],
) -> Result<Vec<RecordedEndpoint>, String> {
    let root = fs::canonicalize(&roots.game_root).map_err(|_| "modChanges.targetChanged")?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for relative in files {
        let key = mods::manifest_dll_key(relative);
        if relative.contains('\\')
            || seen
                .iter()
                .any(|other: &String| overlaps_claim(&key, other))
        {
            return Err(InstallBlock::AliasesOrLink.report(owner, relative));
        }
        for relative in endpoints(&mods::canonical_dll_name(relative)) {
            let path = game(&relative).resolve(roots)?;
            let actual = crate::game::resolve_existing_ancestor(&path)?;
            if actual != root.join(&relative) {
                return Err(InstallBlock::AliasesOrLink.report(owner, &relative));
            }
            let missing = match fs::symlink_metadata(&path) {
                Ok(meta) if meta.is_file() => {
                    if !fs::File::open(&path).is_ok_and(|file| has_single_link(&file)) {
                        return Err(InstallBlock::AliasesOrLink.report(owner, &relative));
                    }
                    false
                }
                Ok(meta) if meta.is_dir() => {
                    return Err(InstallBlock::Directory.report(owner, &relative));
                }
                Ok(_) => return Err(InstallBlock::Unsafe.report(owner, &relative)),
                Err(error) if error.kind() == ErrorKind::NotFound => true,
                Err(_) => return Err(InstallBlock::Unreadable.report(owner, &relative)),
            };
            result.push(RecordedEndpoint {
                relative,
                path: actual,
                missing,
            });
        }
        seen.insert(key);
    }
    Ok(result)
}

pub(super) fn overlapping_endpoint<'a>(
    roots: &TransactionRoots,
    recorded: &'a [RecordedEndpoint],
    claim: &str,
) -> Result<Option<&'a RecordedEndpoint>, String> {
    let physical = crate::game::resolve_existing_ancestor(
        &game(&mods::canonical_dll_name(claim)).resolve(roots)?,
    )?;
    Ok(recorded.iter().find(|endpoint| {
        overlaps_claim(&endpoint.relative, claim)
            || overlaps_claim(
                &endpoint.path.to_string_lossy(),
                &physical.to_string_lossy(),
            )
    }))
}

impl OverwriteConfirmation {
    pub(super) fn new(
        builder: &Builder<'_>,
        owner: &str,
        repository_id: &str,
        record: &InstallRecord,
        proof: impl serde::Serialize,
        source_switch: Option<SourceSwitch>,
    ) -> Result<Self, String> {
        let session = builder.session;
        let bytes = serde_json::to_vec(&(
            repository_id,
            &builder.roots,
            &session.trusted.replan_scope,
            &record.relative,
            &source_switch,
            proof,
        ))
        .map_err(|_| "modChanges.targetChanged")?;
        let fingerprint = format!("{:x}", Sha256::digest(bytes));
        let intent = session
            .wire_request
            .manifest_overwrite
            .as_ref()
            .filter(|intent| intent.repository_id == repository_id);
        let approved = intent.is_some_and(|intent| intent.fingerprint == fingerprint);
        if intent.is_some() && !approved {
            return Err(InstallBlock::VerificationChanged.report(owner, &record.relative));
        }
        Ok(Self {
            offer: ManifestOverwriteOffer {
                repository_id: repository_id.into(),
                manifest_path: crate::display_path::display_path(
                    &builder.roots.data_root.join(&record.relative),
                ),
                recorded_paths: record.manifest.files.clone(),
                fingerprint,
                source_switch,
            },
            approved,
        })
    }

    pub(super) fn failure(&self, message: String, expose_offer: bool) -> PreparationError {
        PreparationError {
            message,
            manifest_overwrite: expose_offer.then(|| Box::new(self.offer.clone())),
        }
    }
}

impl ManifestReplacement {
    pub(super) fn verify_vacancy(
        &self,
        owner: &str,
        roots: &TransactionRoots,
    ) -> Result<(), String> {
        if !self.vacant_paths.is_empty() {
            let root =
                fs::canonicalize(&roots.game_root).map_err(|_| "modChanges.targetChanged")?;
            for path in &self.vacant_paths {
                if !vacant(roots, &root, path) {
                    return Err(InstallBlock::VerificationChanged.report(owner, path));
                }
            }
        }
        Ok(())
    }

    pub(super) fn verify_game_writes(
        &self,
        owner: &str,
        roots: &TransactionRoots,
        changes: &[OwnedMutation],
    ) -> Result<(), String> {
        let recorded = recorded_endpoints(roots, owner, &self.recorded_paths)?;
        let check = |target: &TransactionPath| -> Result<(), String> {
            if target.root != RootKind::Game {
                return Ok(());
            }
            let absolute = target.resolve(roots)?;
            let actual = crate::game::resolve_existing_ancestor(&absolute)?;
            for previous in &recorded {
                if overlaps_claim(&target.relative, &previous.relative)
                    || overlaps_claim(&actual.to_string_lossy(), &previous.path.to_string_lossy())
                {
                    return Err(InstallBlock::InstallRecord.report(owner, &target.relative));
                }
            }
            if absolute.is_file()
                && !fs::File::open(&absolute).is_ok_and(|file| has_single_link(&file))
            {
                return Err(InstallBlock::AliasesOrLink.report(owner, &target.relative));
            }
            Ok(())
        };
        for change in changes {
            match &change.mutation {
                Mutation::File(file) => check(&file.target)?,
                Mutation::Tree(tree) => check(&tree.target)?,
                Mutation::Move(move_) => {
                    check(&move_.from)?;
                    check(&move_.to)?;
                }
            }
        }
        Ok(())
    }
}

impl Builder<'_> {
    pub(super) fn orphan_manifest_replacement(
        &self,
        owner: &str,
        payload: &Payload,
    ) -> Result<Option<ManifestReplacement>, String> {
        if !payload.downloaded || payload.candidate.raw || payload.candidate.verification_pending {
            return Ok(None);
        }
        let Ok((repo, binding)) = self.release_repository(payload) else {
            return Ok(None);
        };
        let Some(record) = self.records.exact_orphan(&repo.id, self.session) else {
            return Ok(None);
        };
        if record
            .manifest
            .repository_binding
            .as_ref()
            .is_some_and(|prior| !prior.matches_identity(repo) || prior.channel != binding.channel)
            || self.records.bound_records().any(|(other, prior)| {
                other.relative != record.relative
                    && prior.matches_identity(repo)
                    && prior.channel == binding.channel
            })
            || self
                .session
                .trusted
                .config
                .map_repositories
                .iter()
                .any(|map| map.id.eq_ignore_ascii_case(&repo.id))
        {
            return Ok(None);
        }
        let stamp = record.exact_stamp(&self.roots, owner)?;
        let recorded = recorded_endpoints(&self.roots, owner, &record.manifest.files)?;
        let registry = self
            .registered_map_plugins()
            .map_err(|_| InstallBlock::MapRecordsUnreadable.report(owner, &record.relative))?;
        let find_claim = |claim| overlapping_endpoint(&self.roots, &recorded, claim);
        if !recorded.is_empty() {
            for (claim, other) in self.records.claimed_files() {
                if other.relative != record.relative
                    && let Some(path) = find_claim(claim)?
                {
                    return Err(report_record_claim(
                        owner,
                        &path.relative,
                        std::iter::once(other),
                    ));
                }
            }
            for claim in self
                .protected_maps
                .iter()
                .map(String::as_str)
                .chain(registry.iter().map(|entry| entry.dll.as_str()))
            {
                if let Some(path) = find_claim(claim)? {
                    return Err(InstallBlock::MapProtected.report(owner, &path.relative));
                }
            }
        }
        let intent = self
            .session
            .wire_request
            .manifest_overwrite
            .as_ref()
            .filter(|intent| intent.repository_id == repo.id);
        let automatic = intent.is_none()
            && recorded
                .iter()
                .all(|path| path.missing && within(&path.relative, mods::file_state::PLUGINS_DIR))
            && self.can_reclaim_orphan(owner, payload, record, &registry);
        let (confirmation, vacant_paths) = if automatic {
            (
                None,
                recorded.into_iter().map(|path| path.relative).collect(),
            )
        } else {
            (
                Some(OverwriteConfirmation::new(
                    self,
                    owner,
                    &repo.id,
                    record,
                    (
                        binding,
                        &payload.candidate.id,
                        &payload.stage_fingerprint,
                        &stamp,
                    ),
                    None,
                )?),
                BTreeSet::new(),
            )
        };
        Ok(Some(ManifestReplacement {
            relative: record.relative.clone(),
            stamp,
            confirmation,
            recorded_paths: record.manifest.files.clone(),
            vacant_paths,
        }))
    }

    fn can_reclaim_orphan(
        &self,
        owner: &str,
        payload: &Payload,
        record: &InstallRecord,
        registry: &[crate::level_sets::RegisteredLevelSetPlugin],
    ) -> bool {
        let Ok(core) = self.release_core(payload) else {
            return false;
        };
        let prior = &record.manifest;
        if prior.is_manual()
            || !prior.repository_binding.as_ref().is_some_and(|binding| {
                core.matches_prior_binding(binding) && binding.tag == prior.release_tag
            })
            || !prior
                .files
                .iter()
                .any(|path| mods::file_state::is_mod_file(Path::new(path)))
        {
            return false;
        }
        let mut providers = self
            .session
            .trusted
            .inventory
            .nodes
            .iter()
            .flat_map(|node| {
                node.current.iter().flat_map(move |candidate| {
                    candidate
                        .provides
                        .iter()
                        .map(move |member| (node, candidate, member))
                })
            })
            .filter(|(_, _, member)| same_guid(&member.guid, &core.repo.plugin_guid));
        let Some((node, candidate, member)) = providers.next() else {
            return false;
        };
        if providers.next().is_some()
            || node.id != owner
            || node.kind != NodeKind::Mod
            || !node.mutable
            || candidate.source_id.as_ref() != Some(&core.repo.id)
        {
            return false;
        }
        let Some(key) = member.member_key.as_deref() else {
            return false;
        };
        let Ok(path) = self.physical(owner, key) else {
            return false;
        };
        let parent = Path::new(&path).parent().and_then(Path::to_str);
        let seen = self
            .session
            .trusted
            .files
            .values()
            .flat_map(|files| &files.summary.members);
        self.verify_member(member, &path).is_ok()
            && parent.is_some_and(|parent| {
                self.ensure_loose_path_unclaimed(
                    &path,
                    parent,
                    registry,
                    Some(AllowedClaim::Scanned(owner)),
                    true,
                    Some(owner),
                )
                .is_ok()
            })
            && !seen.into_iter().any(|member| {
                prior
                    .files
                    .iter()
                    .any(|path| overlaps_claim(path, &member.dll_file))
            })
    }
}
