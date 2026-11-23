import { ChangePreparationError, isPackageAction, type ChangeBackend, type ChangePlan, type ChangeRequest, type ChangeExecutionResult, type ManifestOverwriteOffer } from './types.ts'

export type ChangePhase = 'planning' | 'confirming' | 'preparing' | 'prepared' | 'reconfirming' | 'executing' | 'failed'
export interface ChangeWorkflowState {
  phase: ChangePhase
  plan: ChangePlan | null
  error: unknown
  silent?: boolean
  applyReview?: 'prepared' | 'changed'
  manifestOverwrite?: ManifestOverwriteOffer
}

export function isChangePlanLocked(phase?: ChangePhase) {
  return phase === 'preparing' || phase === 'prepared' || phase === 'executing'
}

/** Only skip the UI when the trusted plan has no change outside the requested switch. */
function canApplyDirectly(request: ChangeRequest, plan: ChangePlan) {
  if (!request.roots) return false
  const root = request.roots[0]
  if (!root || request.roots.length !== 1 || request.map_import || root.candidate_id != null || root.asset_name != null
    || plan.status !== 'ready' || plan.blockers.length || plan.install_notes.length || plan.notices?.length
    || plan.roots.length !== 1 || plan.roots[0]?.node_id !== root.node_id
    || plan.impacts.some(id => id !== root.node_id)) return false
  const node = plan.nodes.find(node => node.node_id === root.node_id)
  if (!node || (node.kind !== 'map' && node.kind !== 'mod')) return false
  const soleDll = node.kind === 'mod' && node.dll_files.length === 1
  if (node.kind === 'mod' && root.member_key == null && !soleDll) return false
  const kind = root.enabled ? 'enable' : 'disable'
  return plan.actions.length <= 1 && plan.actions.every(action =>
    action.node_id === root.node_id && action.kind === kind && action.from_candidate_id === action.to_candidate_id
    && ((action.member_key ?? undefined) === root.member_key
      // The solver emits a node-level switch for a one-member MOD.
      || (action.member_key == null && soleDll && node.members?.[0]?.key === root.member_key)))
}

/** Package preparation and approval to write live files are separate steps. */
export function createChangeWorkflow(backend: ChangeBackend, publish: (state: ChangeWorkflowState | null) => void) {
  let generation = 0
  let current: ChangeWorkflowState | null = null
  let settle: ((value: ChangeExecutionResult | null) => void) | undefined
  let originalRequest: ChangeRequest | undefined

  function update(state: ChangeWorkflowState | null) { current = state; publish(state) }
  function finish(value: ChangeExecutionResult | null) {
    const resolve = settle
    settle = undefined
    originalRequest = undefined
    update(null)
    resolve?.(value)
  }

  async function run(request: ChangeRequest): Promise<ChangeExecutionResult | null> {
    if (current) throw new Error('modChanges.busy')
    // IPC input is JSON data and may contain Vue proxies; detach it before retry.
    const submitted = JSON.parse(JSON.stringify(request)) as ChangeRequest
    originalRequest = submitted
    const mine = ++generation
    update({ phase: 'planning', plan: null, error: null, silent: !!submitted.toggle_scope })
    const result = new Promise<ChangeExecutionResult | null>((resolve) => { settle = resolve })
    try {
      const plan = await backend.plan(submitted)
      if (mine !== generation) { await backend.cancel(plan.plan_id).catch(() => undefined); return result }
      if (submitted.toggle_scope && canApplyDirectly(submitted, plan)) await apply(plan, submitted)
      else update({ phase: 'confirming', plan, error: null })
    } catch (error) {
      if (mine === generation) update({ phase: 'failed', plan: null, error })
    }
    return result
  }

  async function apply(plan: ChangePlan, directRequest?: ChangeRequest, applyReview?: ChangeWorkflowState['applyReview'],
    manifestOverwrite?: ManifestOverwriteOffer) {
    if (plan.status !== 'ready') return
    const mine = generation
    const silent = !!directRequest
    try {
      let prepared = plan
      if (!plan.prepared) {
        update({ phase: 'preparing', plan, error: null, silent })
        prepared = await backend.prepare(plan.plan_id)
        if (mine !== generation) { await backend.cancel(prepared.plan_id).catch(() => undefined); return }
      }
      if (prepared.status !== 'ready') {
        update({ phase: 'reconfirming', plan: prepared, error: null, applyReview: 'changed' })
        return
      }
      if (!prepared.prepared) throw new Error('modChanges.notPrepared')
      const changed = prepared.plan_id !== plan.plan_id || !!(directRequest && !canApplyDirectly(directRequest, prepared))
      const hasPackages = prepared.actions.some(isPackageAction)
      if (!applyReview && (manifestOverwrite || changed || hasPackages || prepared.untracked_overwrites?.length)) {
        if (hasPackages) {
          update({ phase: 'prepared', plan: prepared, error: null })
          await backend.presentPrepared?.()
          if (mine !== generation) return
        }
        update({ phase: 'reconfirming', plan: prepared, error: null, applyReview: changed ? 'changed' : 'prepared', manifestOverwrite })
        return
      }
      plan = prepared
      update({ phase: 'executing', plan, error: null, silent, applyReview })
      const result = await backend.execute(plan.plan_id)
      if (mine !== generation) return
      if (!result.ok) { update({ phase: 'failed', plan, error: result.message, applyReview }); return }
      finish(result)
    } catch (error) {
      if (mine === generation) update({ phase: 'failed', plan, error, applyReview,
        manifestOverwrite: error instanceof ChangePreparationError ? error.manifestOverwrite : undefined })
    }
  }

  async function confirm() {
    const state = current
    if (state?.plan && (state.phase === 'confirming' || state.phase === 'reconfirming')) await apply(state.plan, undefined, state.applyReview)
  }

  async function cancel() {
    if (isChangePlanLocked(current?.phase)) return
    const plan = current?.plan
    generation += 1
    finish(null)
    if (plan) await backend.cancel(plan.plan_id).catch(() => undefined)
  }

  /** A failure is replanned and reviewed; it never blindly retries a commit. */
  async function retry(force: boolean) {
    if (current?.phase !== 'failed' || !originalRequest) return
    const previous = current.plan
    const offer = force ? current.manifestOverwrite : undefined
    if (force && !offer) return
    const request = offer ? { ...originalRequest, manifestOverwrite: { repositoryId: offer.repositoryId, fingerprint: offer.fingerprint } }
      : originalRequest
    const mine = ++generation
    update({ phase: 'planning', plan: null, error: null })
    if (previous) await backend.cancel(previous.plan_id).catch(() => undefined)
    if (mine !== generation) return
    try {
      const plan = await backend.plan(request)
      if (mine !== generation) { await backend.cancel(plan.plan_id).catch(() => undefined); return }
      if (offer && plan.status === 'ready') await apply(plan, undefined, undefined, offer)
      else update({ phase: 'confirming', plan, error: null })
    } catch (error) {
      if (mine === generation) update({ phase: 'failed', plan: null, error })
    }
  }

  return { run, confirm, cancel, retry: () => retry(false), forceOverwriteManifest: () => retry(true) }
}
