import type { ChangeAction, ChangeActionKind, ChangeActionReason, ChangePlan, ChangePreparationItem, ChangePreparationProgress } from './types'
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
  kind?: ChangeActionKind
  label?: string
  member?: ReturnType<typeof namedPlugin>
  memberOnly?: boolean
  bundled?: boolean
  bundleReason?: 'enabled' | 'disabled' | 'included'
  selected?: boolean
  reason?: string
  reasons?: ReturnType<typeof describeReasons>
  causes?: string[]
  version?: ReturnType<typeof versionPair>
  preparationStage?: ChangePreparationItem['stage']
}

/** Keep the order of package actions; related file rows follow each parent. */
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
