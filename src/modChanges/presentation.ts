import type { ChangeAction, ChangePlan, ChangePreparationItem, ChangePreparationProgress } from './types'
import type { DeepReadonly } from 'vue'

export interface ChangePlanRow {
  nodeId: string
  label: string
  actions: DeepReadonly<ChangeAction>[]
  item?: DeepReadonly<ChangePreparationItem>
  version?: { from?: string | null; to?: string | null }
}

/** One readable row per package; member switches remain available in details. */
export function changePlanRows(plan: DeepReadonly<ChangePlan> | null | undefined, progress?: DeepReadonly<ChangePreparationProgress> | null): ChangePlanRow[] {
  if (!plan) return []
  const rows = new Map<string, ChangePlanRow>()
  for (const action of progress?.actions ?? plan.actions) {
    const row = rows.get(action.node_id)
    if (row) {
      row.actions.push(action)
      continue
    }
    const node = plan.nodes.find(item => item.node_id === action.node_id)
    const root = plan.roots.find(item => item.node_id === action.node_id)
    rows.set(action.node_id, { nodeId: action.node_id, label: node?.label || root?.label || '',
      actions: [action] })
  }
  for (const item of progress?.items ?? []) {
    const row = rows.get(item.node_id)
    if (row) { row.item = item; row.label = item.label || row.label }
    else rows.set(item.node_id, { nodeId: item.node_id, label: item.label, actions: [], item })
  }
  for (const row of rows.values()) {
    // Switch actions also carry the installed tag, even though the package isn't changing.
    const action = row.actions.find(({ kind }) => kind !== 'enable' && kind !== 'disable')
    if (!action) continue
    const from = action.from_tag
    const to = row.item?.tag || action.to_tag
    if (from || to) row.version = { from, to }
  }
  return [...rows.values()]
}

/** This measures verified packages, never pretends to measure solver progress. */
export function preparationCompletion(progress: DeepReadonly<ChangePreparationProgress> | null | undefined) {
  const total = progress?.items.length ?? 0
  const completed = progress?.items.filter(item => item.stage === 'ready').length ?? 0
  return { completed, total, percentage: total ? Math.floor(completed / total * 100) : null }
}
