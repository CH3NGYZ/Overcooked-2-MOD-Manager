//! Check verified loose-file replacements inside the branch that selected them.
use super::claims::ReleaseTargetError;
use super::loose_files::{exact_replacement, keeps_provider};
use super::*;

pub(in crate::mod_changes) struct LooseReleaseSearch<'a> {
    session: &'a PlanSession,
    roots: TransactionRoots,
    by_owner: BTreeMap<usize, BTreeMap<String, Vec<Provider>>>,
    accepted_claims: Option<ReleaseTargetClaims>,
    rejection: Option<String>,
}

struct Provider {
    index: usize,
    key: String,
}

impl<'a> LooseReleaseSearch<'a> {
    pub fn new(state: &AppState, session: &'a PlanSession) -> Result<Self, String> {
        let mut search = Self {
            session,
            roots: transaction_roots(state, session)?,
            by_owner: BTreeMap::new(),
            accepted_claims: None,
            rejection: None,
        };
        search.collect_providers(state)?;
        Ok(search)
    }

    fn collect_providers(&mut self, state: &AppState) -> Result<(), String> {
        let session = self.session;
        let nodes = &session.trusted.inventory.nodes;
        let loose_nodes = nodes
            .iter()
            .enumerate()
            .filter_map(|(index, node)| {
                node.id
                    .strip_prefix("dll:")
                    .filter(|_| session.trusted.toggle_unbound_nodes.contains(&node.id))
                    .map(|key| (key, index))
            })
            .collect::<BTreeMap<_, _>>();
        if loose_nodes.is_empty() {
            return Ok(());
        }
        let payloads = session
            .verified_payloads
            .iter()
            .map(|((node, _), payload)| (node, payload))
            .chain(&session.payloads)
            .filter(|(_, payload)| payload.release_files.is_some() && !payload.candidate.raw);
        let mut builder = None;
        let registry = std::cell::OnceCell::new();
        for (id, payload) in payloads {
            let Some((owner, old)) = nodes.iter().enumerate().find(|(_, node)| &node.id == id)
            else {
                continue;
            };
            let candidate = &payload.candidate;
            if !exact_replacement(old, candidate, payload, old) {
                continue;
            }
            let matching = payload.files.iter().filter_map(|source| {
                let dll = mods::canonical_dll_name(source);
                let key = inventory::key(&dll);
                let &index = loose_nodes.get(key.as_str())?;
                Some((source, dll, key, index))
            });
            let scope = std::cell::OnceCell::new();
            for (source, dll, key, index) in matching {
                let builder = match &builder {
                    Some(builder) => builder,
                    None => builder.insert(Builder::indexed(
                        session,
                        self.roots.clone(),
                        installed_map_roots(session)?,
                    )?),
                };
                let Ok(scope) = scope.get_or_init(|| builder.package_scope(id, payload)) else {
                    break;
                };
                let Some(incoming) = candidate
                    .provides
                    .iter()
                    .find(|p| p.member_key.as_deref() == Some(&key))
                else {
                    continue;
                };
                let Ok(Some((physical, before))) = builder.live_endpoint(&dll, true) else {
                    continue;
                };
                let Ok(replacement) =
                    scope.file_replacement(payload, source, &key, &physical, &before, false)
                else {
                    continue;
                };
                if !builder
                    .loose_owner(id, payload, incoming, &key, &physical, replacement)
                    .is_ok_and(|loose| loose.id == nodes[index].id)
                {
                    continue;
                }
                let Ok(registered) = registry.get_or_init(|| builder.registered_map_plugins())
                else {
                    continue;
                };
                if builder
                    .ensure_loose_path_unclaimed(
                        &physical,
                        scope.directory,
                        registered,
                        Some(AllowedClaim::Scanned(&nodes[index].id)),
                        false,
                        None,
                    )
                    .is_err()
                {
                    continue;
                }
                let providers = self
                    .by_owner
                    .entry(owner)
                    .or_default()
                    .entry(candidate.id.clone())
                    .or_default();
                if !providers.iter().any(|provider| provider.index == index) {
                    providers.push(Provider { index, key });
                }
            }
        }
        if registry.get().is_some_and(Result::is_err) {
            // A failed read can change which branch the solver chooses, even
            // before it encounters a conflict in an otherwise valid solution.
            // Recheck the inventory before requesting any alternate downloads.
            crate::mod_changes::preparation::ensure_fresh(state, &session.trusted.revision)?;
        }
        Ok(())
    }

    fn claims(&self, solved: &Inventory) -> Result<ReleaseTargetClaims, ReleaseTargetError> {
        // An earlier asset may be verified before DIYLevel has an actual DLL
        // path. Neither a search branch nor the final plan can guess its roots.
        if solved.nodes.iter().any(|node| {
            node.current
                .as_ref()
                .is_some_and(|candidate| candidate.verification_pending)
        }) {
            return Ok(ReleaseTargetClaims::default());
        }
        ReleaseTargetClaims::collect(self.session, self.roots.clone(), solved)
    }

    pub fn accept(&mut self, solved: &Inventory) -> Result<bool, String> {
        match self.claims(solved) {
            Ok(claims) => {
                self.accepted_claims = Some(claims);
                Ok(true)
            }
            Err(ReleaseTargetError::ForeignManifest { owner, message })
                if !self
                    .session
                    .request
                    .roots
                    .iter()
                    .any(|root| root.node_id == owner) =>
            {
                self.rejection.get_or_insert(message);
                Ok(false)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn finish(
        self,
        state: &AppState,
        solved: Option<&Inventory>,
    ) -> Result<ReleaseTargetClaims, String> {
        if self.rejection.is_some() {
            // Before proceeding with a different plan or a different download,
            // distinguish a stable conflict from a stale game/manifest snapshot.
            crate::mod_changes::preparation::ensure_fresh(state, &self.session.trusted.revision)?;
            crate::mod_changes::preparation::verify_fingerprints(
                &self.session.payload_fingerprints,
            )?;
        }
        if let Some(claims) = self.accepted_claims {
            return Ok(claims);
        }
        match solved {
            Some(inventory) => self.claims(inventory).map_err(Into::into),
            None => self
                .rejection
                .map_or_else(|| Ok(ReleaseTargetClaims::default()), Err),
        }
    }

    pub fn project(
        &self,
        inventory: &mut Inventory,
        original: &Inventory,
        projected: &mut BTreeSet<usize>,
    ) {
        if self.by_owner.is_empty() {
            return;
        }
        // A later branch can turn the selected owner off or restore its old
        // candidate. Recompute the projection before checking relations again.
        for index in std::mem::take(projected) {
            inventory.nodes[index].clone_from(&original.nodes[index]);
        }
        let final_levels = final_package(inventory)
            .ok()
            .flatten()
            .map(|root| format!("{root}/levels"));
        for (&owner, releases) in &self.by_owner {
            let node = &inventory.nodes[owner];
            let Some(providers) = node.current.as_ref().and_then(|c| releases.get(&c.id)) else {
                continue;
            };
            for provider in providers {
                let loose = &inventory.nodes[provider.index];
                if loose == &original.nodes[provider.index]
                    && keeps_provider(node, &provider.key, loose)
                    && final_levels
                        .as_ref()
                        .is_none_or(|levels| !within(&provider.key, levels))
                {
                    projected.insert(provider.index);
                }
            }
        }
        for &index in projected.iter() {
            let loose = &mut inventory.nodes[index];
            loose.current = None;
            loose.enabled = false;
        }
    }
}
