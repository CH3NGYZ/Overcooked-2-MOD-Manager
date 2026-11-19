import type { ChangeAction, ChangeActionReason, ChangePlan, ChangePreparationItem, ChangePreparationProgress } from './types'
import type { DeepReadonly } from 'vue'

export interface ChangePlanRow {
  nodeId: string
  label: string
  actions: DeepReadonly<ChangeAction>[]
  item?: DeepReadonly<ChangePreparationItem>
  version?: { from?: string | null; to?: string | null }
}

const isVersionChange = ({ kind }: DeepReadonly<ChangeAction>) => kind !== 'enable' && kind !== 'disable'

function changeVersion(action: DeepReadonly<ChangeAction>, item?: DeepReadonly<ChangePreparationItem>): ChangePlanRow['version'] {
  // Switch actions carry the installed tag, even though the package isn't changing.
  if (!isVersionChange(action)) return
  const from = action.from_tag
  const to = item?.tag || action.to_tag
  if (from || to) return { from, to }
}

/** One readable row per package; member switches remain available in details. */
export function changePlanRows(plan: DeepReadonly<ChangePlan> | null | undefined, progress?: DeepReadonly<ChangePreparationProgress> | null): ChangePlanRow[] {
  if (!plan) return []
  const rows = new Map<string, ChangePlanRow>()
  const labels = planLabels(plan)
  for (const action of progress?.actions ?? plan.actions) {
    const row = rows.get(action.node_id)
    if (row) {
      row.actions.push(action)
      continue
    }
    rows.set(action.node_id, { nodeId: action.node_id, label: labels.get(action.node_id) || '',
      actions: [action] })
  }
  for (const item of progress?.items ?? []) {
    const row = rows.get(item.node_id)
    if (row) { row.item = item; row.label = item.label || row.label }
    else rows.set(item.node_id, { nodeId: item.node_id, label: item.label, actions: [], item })
  }
  for (const row of rows.values()) {
    const action = row.actions.find(isVersionChange)
    if (action) row.version = changeVersion(action, row.item)
  }
  return [...rows.values()]
}

function planLabels(plan: DeepReadonly<ChangePlan>) {
  return new Map([...plan.roots, ...plan.nodes].filter(({ label }) => label.trim()).map(({ node_id, label }) => [node_id, label]))
}

function namedPlugin(path: string | null | undefined) {
  const name = path?.trim().split(/[\\/]/).pop()
  return path && name && /\.dll(?:\.disabled)?$/i.test(name) ? { name, path } : undefined
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

/** Keep authoritative action order and scope; don't infer causes from labels or opaque identifiers. */
export function changePlanDetails(plan: DeepReadonly<ChangePlan>, progress?: DeepReadonly<ChangePreparationProgress> | null) {
  const labels = planLabels(plan)
  const nodes = new Map(plan.nodes.map(node => [node.node_id, node]))
  const items = new Map(progress?.items.map(item => [item.node_id, item]))
  for (const [nodeId, item] of items) {
    if (item.label.trim()) labels.set(nodeId, item.label)
  }
  const actions = progress?.actions ?? plan.actions
  const coveredNodes = new Set<string>()
  const coveredTags = new Map<string, string>()
  const packageActions = new Set(actions.filter(isVersionChange).map(a => a.node_id))
  const rows = actions.map(action => {
    const { node_id: nodeId, member_key: memberKey, kind } = action
    const item = items.get(nodeId)
    const version = changeVersion(action, item)
    const member = memberKey && nodes.get(nodeId)?.members?.find(member => member.key === memberKey)
    const plugin = memberKey ? namedPlugin(member && member.dll_file) ?? namedPlugin(memberKey) : undefined
    const reasons = describeReasons(action.reason_details ?? [], nodeId, labels)
    const causes = reasons.length ? [] : relatedLabels(action.caused_by, nodeId, labels)
    // A package's preparation status belongs to its package action, or its first switch if it has no package action.
    const showStage = packageActions.has(nodeId) ? isVersionChange(action) : !coveredNodes.has(nodeId)
    coveredNodes.add(nodeId)
    if (version?.to) coveredTags.set(nodeId, version.to)
    return { key: JSON.stringify([nodeId, kind, memberKey]), kind, label: labels.get(nodeId), member: plugin, memberOnly: !!memberKey,
      reason: action.reason, reasons, causes, version, showStage, stage: showStage ? item?.stage : undefined }
  })
  const pending = [...items.values()].filter(item => !coveredNodes.has(item.node_id))
  const roots = plan.roots.filter(root => root.label.trim() && (!coveredNodes.has(root.node_id)
    || root.tag && root.tag !== coveredTags.get(root.node_id)))
  const packages = plan.nodes.filter(node => coveredNodes.has(node.node_id)).flatMap(node => {
    const plugins = (node.members ?? []).flatMap(member => {
      const file = namedPlugin(member.dll_file)
      return file ? [{ ...file, enabled: member.enabled }] : []
    })
    return plugins.length ? [{ nodeId: node.node_id, label: labels.get(node.node_id), plugins }] : []
  })
  return { rows, pending, roots, packages }
}

/** This measures verified packages, never pretends to measure solver progress. */
export function preparationCompletion(progress: DeepReadonly<ChangePreparationProgress> | null | undefined) {
  const total = progress?.items.length ?? 0
  const completed = progress?.items.filter(item => item.stage === 'ready').length ?? 0
  return { completed, total, percentage: total ? Math.floor(completed / total * 100) : null }
}
