import { ChangePreparationError, isPackageAction, type ChangeAction, type ChangeBackend, type ChangePlan, type ChangeRequest, type ChangeExecutionResult, type ManifestOverwriteOffer } from './types.ts'
import { isTransactionRecoveryError } from './transactionRecoveryTypes.ts'

export type ChangePhase = 'planning' | 'confirming' | 'preparing' | 'prepared' | 'reconfirming' | 'executing' | 'failed'
export interface ChangeWorkflowState {
  phase: ChangePhase
  plan: ChangePlan | null
  error: unknown
  silent?: boolean
  applyReview?: 'prepared' | 'changed'
  noteOnlyReview?: boolean
  manifestOverwrite?: ManifestOverwriteOffer
}

export function isChangePlanLocked(phase?: ChangePhase) {
  return phase === 'preparing' || phase === 'prepared' || phase === 'executing'
}

function needsReview(plan: ChangePlan) {
  return plan.status !== 'ready' || plan.blockers.length || plan.notices?.length
    || plan.untracked_overwrites?.length || needsRecoveryApproval(plan)
    || Object.values(plan.retired_mod_copies ?? {}).some(copies => copies.length)
}

export function needsRecoveryApproval(plan?: { recovery_conflicts?: readonly unknown[]; recovery_approval?: string } | null) {
  return !!(plan?.recovery_conflicts?.length || plan?.recovery_approval)
}

export function recoveryBlockedPlan(plan?: { status: string; blockers: readonly { code: string; message?: string }[] } | null) {
  return !!plan && plan.status !== 'ready' && plan.blockers.some(({ code, message }) =>
    isTransactionRecoveryError(message) || isTransactionRecoveryError(code))
}

function canApplyToggle(request: ChangeRequest, plan: ChangePlan) {
  if (!request.roots) return false
  const root = request.roots[0]
  if (!root || request.roots.length !== 1 || request.map_import || root.candidate_id != null || root.asset_name != null
    || needsReview(plan) || plan.install_notes.length
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

function isPackageRequest(request: ChangeRequest) {
  return !request.toggle_scope && !request.bulk_toggle && (!!request.repair_dependencies_of || !!request.map_import
    || request.roots?.some(root => root.candidate_id != null || root.node_id.startsWith('preview:')))
}

function keepsUpgradeTarget(previous: ChangePlan, plan: ChangePlan, upgrade: ChangeAction) {
  const { node_id, from_candidate_id, from_tag, to_candidate_id } = upgrade
  if (!from_tag || !previous.actions.some(action => action.kind === 'upgrade' && action.node_id === node_id
    && action.member_key == null && action.from_candidate_id === from_candidate_id && action.from_tag === from_tag
    && action.to_candidate_id === to_candidate_id)) return false
  const previousFiles = previous.nodes.find(node => node.node_id === node_id)?.dll_files
  const currentFiles = plan.nodes.find(node => node.node_id === node_id)?.dll_files
  if (!previousFiles?.length || !currentFiles?.length) return false
  const previousPaths = new Set(previousFiles), currentPaths = new Set(currentFiles)
  return previousPaths.size === currentPaths.size && [...currentPaths].every(path => previousPaths.has(path))
}

function canApplyPackage(request: ChangeRequest, plan: ChangePlan, previous?: ChangePlan) {
  if (needsReview(plan) || request.map_import) return false
  const root = request.roots?.length === 1 ? request.roots[0] : undefined
  const owner = request.repair_dependencies_of ?? root?.node_id
  if (!owner || plan.roots.length !== 1 || plan.roots[0]?.node_id !== owner) return false
  const candidate = root?.candidate_id
  if (root && (!root.enabled || root.member_key != null || !root.node_id.startsWith('mod:')
    || !candidate || !(candidate.startsWith(`release:${owner.slice(4)}:`) || candidate === `raw:${owner.slice(4)}`))) return false

  const dependencies: { node_id: string; owners: string[] }[] = []
  const reachableFrom = new Map<string, string[]>()
  function connect(from: string, to: string) {
    const targets = reachableFrom.get(from)
    if (targets) targets.push(to)
    else reachableFrom.set(from, [to])
  }
  for (const action of plan.actions) {
    const { node_id, kind, from_candidate_id, to_candidate_id } = action
    if (node_id === owner) {
      if (!root || action.member_key != null || to_candidate_id !== candidate
        || !(kind === 'install' && from_candidate_id == null
          || kind === 'upgrade' && from_candidate_id != null && from_candidate_id !== candidate
          || kind === 'enable' && from_candidate_id === candidate)) return false
      if (kind === 'upgrade' && previous && !keepsUpgradeTarget(previous, plan, action)) return false
      continue
    }
    if (!to_candidate_id || (kind === 'install' ? from_candidate_id != null
      : kind !== 'enable' || from_candidate_id !== to_candidate_id)) return false
    const reasons = action.reason_details
    if (!reasons?.length || reasons.some(reason => reason.kind !== 'required_by' || !reason.related_node_ids.length)) return false
    const owners = reasons.flatMap(reason => reason.related_node_ids).filter(id => id !== node_id)
    dependencies.push({ node_id, owners })
    for (const parent of owners) connect(parent, node_id)
  }
  for (const { node_id, runtime } of plan.evaluations) {
    for (const edge of runtime) {
      if (edge.state === 'satisfied' && (edge.relation.kind ?? 'requires') === 'requires' && !edge.relation.optional
        && edge.targets.length === 1 && edge.targets[0] !== undefined) connect(node_id, edge.targets[0])
    }
  }
  const reachable = new Set([owner])
  for (const node of reachable) {
    for (const next of reachableFrom.get(node) ?? []) reachable.add(next)
  }
  const actionTargets = new Set(plan.actions.map(action => action.node_id)).add(owner)
  return dependencies.every(dependency => dependency.owners.some(id => reachable.has(id)))
    && plan.impacts.every(id => actionTargets.has(id))
}

function isStalePreparation(error: unknown) {
  const message = error instanceof Error ? error.message : error
  return typeof message === 'string' && /^modChanges\.(?:inventoryChanged|targetChanged)(?:$|[?:：])/.test(message)
}

export function createChangeWorkflow(backend: ChangeBackend, publish: (state: ChangeWorkflowState | null) => void,
  hasUnreadNotes = (plan: ChangePlan) => plan.install_notes.length > 0) {
  let generation = 0
  let current: ChangeWorkflowState | null = null
  let settle: ((value: ChangeExecutionResult | null) => void) | undefined
  let originalRequest: ChangeRequest | undefined
  let targetScope: ChangePlan['replan_scope']
  let automaticReplanUsed = false

  function update(state: ChangeWorkflowState | null) { current = state; publish(state) }
  function finish(value: ChangeExecutionResult | null) {
    const resolve = settle
    settle = undefined
    originalRequest = undefined
    targetScope = undefined
    update(null)
    resolve?.(value)
  }

  async function run(request: ChangeRequest): Promise<ChangeExecutionResult | null> {
    if (current) throw new Error('modChanges.busy')
    ensureOperationsAllowed()
    // IPC input is JSON data and may contain Vue proxies; detach it before retry.
    const submitted = JSON.parse(JSON.stringify(request)) as ChangeRequest
    originalRequest = submitted
    automaticReplanUsed = false
    const mine = ++generation
    const result = new Promise<ChangeExecutionResult | null>((resolve) => { settle = resolve })
    await planRequest(submitted, mine)
    return result
  }

  function ensureOperationsAllowed() {
    if (backend.operationsBlocked?.()) throw new Error('update.inProgress')
  }

  async function planRequest(request: ChangeRequest, mine: number, previous?: ChangePlan, requireReview = false,
    manifestOverwrite?: ManifestOverwriteOffer) {
    update({ phase: 'planning', plan: null, error: null, silent: !previous && !!request.toggle_scope })
    try {
      if (previous) await backend.cancel(previous.plan_id).catch(() => undefined)
      if (mine !== generation) return
      ensureOperationsAllowed()
      const plan = await backend.plan(request)
      if (mine !== generation || (targetScope && plan.replan_scope !== targetScope)) {
        await backend.cancel(plan.plan_id).catch(() => undefined)
        if (mine === generation) update({ phase: 'failed', plan: null, error: 'modChanges.targetChanged' })
        return
      }
      targetScope ??= plan.replan_scope
      if (plan.status === 'ready' && (isPackageRequest(request) || manifestOverwrite)) {
        await apply(plan, 'package', undefined, requireReview, previous, manifestOverwrite)
      } else if (!requireReview && request.toggle_scope && canApplyToggle(request, plan)) await apply(plan, 'toggle')
      else if (plan.prepared && needsRecoveryApproval(plan)) update({ phase: 'reconfirming', plan, error: null, applyReview: 'prepared' })
      else update({ phase: 'confirming', plan, error: null })
    } catch (error) {
      if (mine === generation) update({ phase: 'failed', plan: null, error })
    }
  }

  async function apply(plan: ChangePlan, mode?: 'package' | 'toggle', applyReview?: ChangeWorkflowState['applyReview'],
    requireReview = false, previous?: ChangePlan, manifestOverwrite?: ManifestOverwriteOffer, recoveryApproval?: string) {
    if (plan.status !== 'ready') return
    const mine = generation
    const request = originalRequest!
    const initialPlanId = plan.plan_id
    const silent = mode === 'toggle'
    let preparing = false
    let replanAvailable = false
    try {
      ensureOperationsAllowed()
      let prepared = plan
      if (!plan.prepared) {
        update({ phase: 'preparing', plan, error: null, silent })
        preparing = true
        prepared = await backend.prepare(plan.plan_id)
        preparing = false
        if (mine !== generation) { await backend.cancel(prepared.plan_id).catch(() => undefined); return }
      }
      const changed = prepared.plan_id !== initialPlanId || (mode === 'toggle' && !canApplyToggle(request, prepared))
      plan = prepared
      if (targetScope && plan.replan_scope !== targetScope) throw new Error('modChanges.targetChanged')
      if (prepared.status !== 'ready') {
        update({ phase: 'reconfirming', plan: prepared, error: null, applyReview: 'changed' })
        return
      }
      if (!prepared.prepared) throw new Error('modChanges.notPrepared')
      const hasPackages = prepared.actions.some(isPackageAction)
      if (!applyReview && hasPackages) {
        update({ phase: 'prepared', plan: prepared, error: null })
        await backend.presentPrepared?.()
        if (mine !== generation) return
      }
      if (needsRecoveryApproval(prepared) && (!recoveryApproval || recoveryApproval !== prepared.recovery_approval)) {
        update({ phase: 'reconfirming', plan: prepared, error: null,
          applyReview: changed || applyReview === 'changed' ? 'changed' : 'prepared', manifestOverwrite })
        return
      }
      const routinePackage = !applyReview && mode === 'package' && !requireReview && canApplyPackage(request, prepared, previous)
      if (!applyReview && (manifestOverwrite || (mode === 'package' ? !routinePackage || hasUnreadNotes(prepared)
        : changed || hasPackages || prepared.untracked_overwrites?.length))) {
        update({ phase: 'reconfirming', plan: prepared, error: null,
          applyReview: mode !== 'package' && prepared.plan_id !== initialPlanId ? 'changed' : 'prepared',
          noteOnlyReview: routinePackage && !manifestOverwrite, manifestOverwrite })
        return
      }
      ensureOperationsAllowed()
      update({ phase: 'executing', plan, error: null, silent, applyReview })
      const executionArgs: [planId: string, recoveryApproval?: string] = recoveryApproval ? [plan.plan_id, recoveryApproval] : [plan.plan_id]
      const result = await backend.execute(...executionArgs)
      if (mine !== generation) return
      if (!result.ok) {
        replanAvailable = result.replan_available === true
        throw result.message
      }
      finish(result)
    } catch (error) {
      if (mine !== generation) return
      if (!automaticReplanUsed && plan.replan_scope && (replanAvailable || preparing && isStalePreparation(error))) {
        automaticReplanUsed = true
        const { manifestOverwrite: _expired, ...intent } = request
        await planRequest(intent, mine, plan, requireReview || !!applyReview || !!manifestOverwrite)
      } else {
        update({ phase: 'failed', plan, error, applyReview,
          manifestOverwrite: error instanceof ChangePreparationError ? error.manifestOverwrite : undefined })
      }
    }
  }

  async function confirm() {
    const state = current
    if (state?.plan && (state.phase === 'confirming' || state.phase === 'reconfirming')
      && !(state.plan.prepared && needsRecoveryApproval(state.plan))) {
      await apply(state.plan, undefined, state.applyReview, false, undefined, state.manifestOverwrite)
    }
  }

  async function confirmRecovery(planId: string, approval: string) {
    const state = current
    const plan = state?.plan
    if (state?.phase !== 'reconfirming' || !state.applyReview || plan?.status !== 'ready' || !plan.prepared
      || !approval || plan.plan_id !== planId || plan.recovery_approval !== approval) return
    await apply(plan, undefined, state.applyReview, false, undefined, state.manifestOverwrite, approval)
  }

  function invalidateRecoveryApproval() {
    const state = current
    if ((state?.phase === 'confirming' || state?.phase === 'reconfirming') && needsRecoveryApproval(state.plan)) {
      update({ phase: 'failed', plan: state.plan, error: 'modChanges.targetChanged' })
    }
  }

  async function cancel() {
    if (isChangePlanLocked(current?.phase)) return
    const plan = current?.plan
    generation += 1
    finish(null)
    if (plan) await backend.cancel(plan.plan_id).catch(() => undefined)
  }

  /** A failure is replanned and reviewed; it never blindly retries a commit. */
  async function retry() {
    if (!current || !originalRequest || isChangePlanLocked(current.phase)
      || (current.phase !== 'failed' && !recoveryBlockedPlan(current.plan) && !missingRecoveryApproval(current.plan))) return
    await planRequest(originalRequest, ++generation, current.plan ?? undefined, true)
  }

  async function forceOverwriteManifest() {
    const state = current
    const offer = state?.manifestOverwrite
    if (state?.phase !== 'failed' || !offer || !originalRequest) return
    const { repositoryId, fingerprint } = offer
    await planRequest({ ...originalRequest, manifestOverwrite: { repositoryId, fingerprint } },
      ++generation, state.plan ?? undefined, true, offer)
  }

  return { run, confirm, confirmRecovery, invalidateRecoveryApproval, cancel, retry, forceOverwriteManifest }
}

export function missingRecoveryApproval(plan?: { prepared: boolean; recovery_conflicts?: readonly unknown[]; recovery_approval?: string } | null) {
  return !!(plan?.prepared && needsRecoveryApproval(plan) && !plan.recovery_approval)
}
