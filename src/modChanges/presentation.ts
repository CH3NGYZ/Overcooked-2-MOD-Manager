import type { ChangeAction, ChangeActionKind, ChangeActionReason, ChangePlan, ChangePreparationItem, ChangePreparationProgress, DiagnosticStep } from './types'
import type { DeepReadonly } from 'vue'

const isVersionChange = ({ kind }: DeepReadonly<ChangeAction>) => kind !== 'enable' && kind !== 'disable'
export const changeStatusKey = (nodeId: string, memberKey?: string | null) => JSON.stringify([nodeId, memberKey ?? null])

function versionPair(from?: string | null, to?: string | null) {
  if (from || to) return { from, to }
}

function planLabels(plan: DeepReadonly<ChangePlan>) {
  return new Map([...plan.roots, ...plan.nodes].filter(({ label }) => label.trim()).map(({ node_id, label }) => [node_id, label]))
}

function namedPlugin(path: string | null | undefined) {
  const name = path?.trim().split(/[\\/]/).pop()
  return path && name && /\.dll(?:\.disabled)?$/i.test(name) ? { name: name.replace(/\.disabled$/i, ''), path } : undefined
}

function relatedLabels(ids: readonly string[], nodeId: string, labels: ReadonlyMap<string, string>) {
  return [...new Set(ids.filter(id => id !== nodeId).map(id => labels.get(id)).filter((label): label is string => !!label))]
}

function describeReasons(reasons: readonly DeepReadonly<ChangeActionReason>[], nodeId: string, labels: ReadonlyMap<string, string>) {
  const seen = new Set<string>()
  return reasons.flatMap(({ kind, related_node_ids, target_guid, requirement, prerelease, condition_on_related }) => {
    const related = relatedLabels(related_node_ids, nodeId, labels)
    if (!related.length && target_guid?.trim()) related.push(target_guid)
    if (kind !== 'requested' && !related.length) return []
    const conditionOnRelated = condition_on_related === true
    const key = JSON.stringify([kind, related, requirement || null, prerelease ?? null, conditionOnRelated])
    if (seen.has(key)) return []
    seen.add(key)
    return [{ key, kind, related, requirement, prerelease, conditionOnRelated }]
  })
}

interface ChangePlanDetailRow {
  key: string
  nodeId: string
  statusKey?: string
  kind?: ChangeActionKind | 'retire'
  label?: string
  member?: ReturnType<typeof namedPlugin>
  memberOnly?: boolean
  bundled?: boolean
  retired?: string
  bundleReason?: 'enabled' | 'disabled' | 'included'
  selected?: boolean
  reason?: string
  reasons?: ReturnType<typeof describeReasons>
  causes?: string[]
  version?: ReturnType<typeof versionPair>
  preparationStage?: ChangePreparationItem['stage']
  diagnostic?: {
    start?: boolean
    path?: string
    depth?: number
    edge?: { step: DeepReadonly<DiagnosticStep>; ownerLabel?: string }
    terminal?: DeepReadonly<DiagnosticStep>
    providers?: { label: string; version?: string | null; enabled: boolean }[]
  }
}

type FailureStep = DeepReadonly<DiagnosticStep>
interface DiagnosticBranch {
  step: FailureStep
  terminal: boolean
  children: Map<string, DiagnosticBranch>
}

function diagnosticEdgeKey({ owner_id, owner_version, relation, targets, state, reason }: FailureStep) {
  return JSON.stringify([owner_id, owner_version ?? null, state, reason,
    relation ? [relation.kind, relation.guid, relation.version, relation.prerelease, relation.optional] : null,
    targets.map(({ node_id, version, enabled }) => [node_id, version ?? null, enabled]),
  ])
}

function diagnosticDetails(plan: DeepReadonly<ChangePlan>, labels: ReadonlyMap<string, string>) {
  const starts = new Map<string, { step: FailureStep; children: Map<string, DiagnosticBranch> }>()
  let witness = 0
  for (const blocker of plan.blockers) {
    for (const trace of blocker.traces ?? []) {
      const first = trace[0]
      if (!first) continue
      const key = JSON.stringify([first.owner_id, first.owner_version ?? null])
      let start = starts.get(key)
      if (!start) starts.set(key, start = { step: first, children: new Map() })
      let siblings = start.children
      trace.forEach((step, index) => {
        const terminal = index === trace.length - 1
        // Merge evidence only inside the same prefix. A terminal witness must
        // still get its own cross, even if another witness continues here.
        const edgeKey = terminal ? `terminal:${witness}` : diagnosticEdgeKey(step)
        let branch = siblings.get(edgeKey)
        if (!branch) siblings.set(edgeKey, branch = { step, terminal, children: new Map() })
        siblings = branch.children
      })
      witness++
    }
  }

  const selected = new Set<DeepReadonly<ChangePlan['roots'][number]>>()
  const diagnosticRows: ChangePlanDetailRow[] = []
  function appendBranches(children: Map<string, DiagnosticBranch>, parentKey: string, path: string, depth: number) {
    let position = 0
    for (const branch of children.values()) {
      const { step } = branch
      const terminal = branch.terminal ? step : undefined
      const key = `${parentKey}:${++position}`
      const branchPath = children.size > 1 ? `${path}.${position}` : path
      const diagnostic = { depth, path: children.size > 1 ? branchPath : undefined, terminal }
      if (!step.relation) {
        diagnosticRows.push({ key, nodeId: step.owner_id, label: labels.get(step.owner_id),
          version: versionPair(undefined, step.owner_version), diagnostic })
      } else {
        const { guid } = step.relation
        const providers = step.targets.map(target => ({ label: labels.get(target.node_id) ?? guid, version: target.version, enabled: target.enabled }))
        const only = providers.length === 1 ? providers[0] : undefined
        diagnosticRows.push({ key, nodeId: only ? step.targets[0]!.node_id : step.owner_id, label: only?.label ?? guid,
          version: versionPair(undefined, only?.version), diagnostic: { ...diagnostic, edge: { step, ownerLabel: labels.get(step.owner_id) }, providers } })
      }
      appendBranches(branch.children, key, branchPath, depth + 1)
    }
  }

  let startNumber = 0
  for (const { step: first, children } of starts.values()) {
    const matchingRoots = plan.roots.filter(root => root.node_id === first.owner_id)
    const ownerVersion = first.owner_version || matchingRoots[0]?.tag
    // The exact Release can declare a different DLL version. Keep its selected
    // tag visible as a separate row when no evaluated start has that version.
    const coveredRoots = matchingRoots.filter(root => !root.tag || root.tag === ownerVersion)
    coveredRoots.forEach(root => selected.add(root))
    const path = String(++startNumber)
    const key = `path:${path}`
    const standalone = children.size === 1 && !first.relation && [...children.values()][0]?.terminal
    diagnosticRows.push({ key, nodeId: first.owner_id, label: labels.get(first.owner_id), selected: !!coveredRoots.length,
      version: versionPair(undefined, ownerVersion), diagnostic: { start: true, path, terminal: standalone ? first : undefined } })
    if (!standalone) appendBranches(children, key, path, 1)
  }
  const roots: ChangePlanDetailRow[] = plan.roots.flatMap((root, index) => selected.has(root) ? [] : ({
    key: `selected:${index}`, nodeId: root.node_id, label: root.label, version: versionPair(undefined, root.tag), selected: true,
  }))
  return { rows: [...roots, ...diagnosticRows] }
}

/** Keep the order of package actions, or, for a blocked plan, explain the tried paths instead of showing proposed work. */
export function changePlanDetails(plan: DeepReadonly<ChangePlan>, progress?: DeepReadonly<ChangePreparationProgress> | null) {
  const labels = planLabels(plan)
  if (plan.status !== 'ready') return diagnosticDetails(plan, labels)
  const nodes = new Map(plan.nodes.map(node => [node.node_id, node]))
  const items = new Map(progress?.items.map(item => [item.node_id, item]))
  for (const [nodeId, item] of items) {
    if (item.label.trim()) labels.set(nodeId, item.label)
  }
  const actions = progress?.actions ?? plan.actions
  const coveredNodes = new Set<string>()
  const coveredTags = new Map<string, string>()
  const parentActions = new Map<string, DeepReadonly<ChangeAction>>()
  const memberActions = new Map<string, DeepReadonly<ChangeAction>[]>()
  const explicitMembers = new Set<string>()
  for (const action of actions) {
    const { node_id: nodeId, member_key: memberKey } = action
    if (memberKey) {
      explicitMembers.add(changeStatusKey(nodeId, memberKey))
      const group = memberActions.get(nodeId)
      if (group) group.push(action)
      else memberActions.set(nodeId, [action])
    } else if (!parentActions.has(nodeId) || isVersionChange(action)) parentActions.set(nodeId, action)
  }
  const actionRow = (action: DeepReadonly<ChangeAction>, bundled = false): ChangePlanDetailRow => {
    const { node_id: nodeId, member_key: memberKey, kind } = action
    const item = items.get(nodeId)
    const changesVersion = isVersionChange(action)
    // A switch may carry a version in the DTO, but it leaves that version installed.
    const version = changesVersion ? versionPair(action.from_tag, item?.tag || action.to_tag) : undefined
    const node = nodes.get(nodeId)
    const member = memberKey && node?.members?.find(member => member.key === memberKey)
    const plugin = memberKey ? (member ? namedPlugin(member.dll_file) : undefined) ?? namedPlugin(memberKey) : undefined
    const reasons = describeReasons(action.reason_details ?? [], nodeId, labels)
    const causes = reasons.length ? [] : relatedLabels(action.caused_by, nodeId, labels)
    const statusKey = changeStatusKey(nodeId, memberKey)
    coveredNodes.add(nodeId)
    if (version?.to) coveredTags.set(nodeId, version.to)
    return { key: JSON.stringify([nodeId, kind, memberKey ?? null]), nodeId, statusKey, kind, label: labels.get(nodeId),
      member: plugin, memberOnly: !!memberKey, bundled, reason: action.reason, reasons, causes, version,
      preparationStage: changesVersion ? item?.stage : undefined }
  }
  const rows: ChangePlanDetailRow[] = []
  for (const action of actions) {
    const { node_id: nodeId, member_key: memberKey, kind } = action
    const parent = parentActions.get(nodeId)
    if (memberKey && parent) continue
    const row = actionRow(action)
    rows.push(row)
    if (action !== parent) continue

    if (kind === 'upgrade') {
      for (const copy of plan.retired_mod_copies?.[nodeId] ?? []) {
        const statusKey = changeStatusKey(nodeId, copy.member_key)
        const file = namedPlugin(copy.dll_file)
        rows.push({ key: `retire:${statusKey}`, nodeId, statusKey, kind: 'retire', retired: copy.member_key,
          label: copy.label.trim() || file?.name, member: file, version: versionPair(undefined, copy.version) })
      }
    }

    // Only the node's true internal members follow a package operation. In
    // particular, a separately registered map DLL is its own plan node.
    const node = nodes.get(nodeId)
    const children = node?.members ?? []
    const changesVersion = isVersionChange(action)
    const previousMembers = plan.previous_enabled_members?.[nodeId]
    const includedBeforeDisable = previousMembers && new Set(previousMembers)
    if (node?.kind === 'map' || children.length > 1) {
      for (const child of children) {
        const file = namedPlugin(child.dll_file)
        const childKey = changeStatusKey(nodeId, child.key)
        const affected = kind === 'disable' && includedBeforeDisable ? includedBeforeDisable.has(child.key) : child.enabled
        if (!file || explicitMembers.has(childKey) || (!changesVersion && !affected)) continue
        explicitMembers.add(childKey)
        rows.push({ key: childKey, nodeId, statusKey: row.statusKey, member: file, bundled: true,
          kind: changesVersion ? undefined : kind, bundleReason: changesVersion ? 'included' : kind === 'enable' ? 'enabled' : 'disabled' })
      }
    }
    for (const memberAction of memberActions.get(nodeId) ?? []) rows.push(actionRow(memberAction, true))
  }
  for (const [nodeId, item] of items) {
    if (!coveredNodes.has(nodeId)) rows.push({ key: `pending:${nodeId}`, nodeId, label: labels.get(nodeId),
      version: versionPair(undefined, item.tag), preparationStage: item.stage })
  }
  for (const root of plan.roots) {
    const nodeId = root.node_id
    if (!root.label.trim() || (coveredNodes.has(nodeId) && (!root.tag || root.tag === coveredTags.get(nodeId)))
      || (!coveredNodes.has(nodeId) && items.has(nodeId) && (!root.tag || root.tag === items.get(nodeId)?.tag))) continue
    rows.push({ key: `selected:${nodeId}`, nodeId, label: root.label, version: versionPair(undefined, root.tag), selected: true })
  }
  return { rows }
}

/** This measures verified packages, never pretends to measure solver progress. */
export function preparationCompletion(progress: DeepReadonly<ChangePreparationProgress> | null | undefined) {
  const total = progress?.items.length ?? 0
  const completed = progress?.items.filter(item => item.stage === 'ready').length ?? 0
  return { completed, total, percentage: total ? Math.floor(completed / total * 100) : null }
}
