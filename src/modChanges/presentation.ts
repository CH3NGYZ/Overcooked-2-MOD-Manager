import type { ChangeAction, ChangeNode, ChangePlan, ChangePreparationItem, ChangePreparationProgress } from './types'
import type { DeepReadonly } from 'vue'

export interface ChangePlanRow {
  nodeId: string
  label: string
  action?: DeepReadonly<ChangeAction>
  actions: DeepReadonly<ChangeAction>[]
  node?: DeepReadonly<ChangeNode>
  item?: DeepReadonly<ChangePreparationItem>
}

/** One readable row per package; member switches remain available in details. */
export function changePlanRows(plan: DeepReadonly<ChangePlan> | null | undefined, progress?: DeepReadonly<ChangePreparationProgress> | null): ChangePlanRow[] {
  if (!plan) return []
  const rows = new Map<string, ChangePlanRow>()
  for (const action of progress?.actions ?? plan.actions) {
    const row = rows.get(action.node_id)
    if (row) {
      row.actions.push(action)
      if (row.action?.member_key && !action.member_key) row.action = action
      continue
    }
    const node = plan.nodes.find(item => item.node_id === action.node_id)
    const root = plan.roots.find(item => item.node_id === action.node_id)
    rows.set(action.node_id, { nodeId: action.node_id, label: node?.label || root?.label || '',
      action, actions: [action], node })
  }
  for (const item of progress?.items ?? []) {
    const row = rows.get(item.node_id)
    if (row) { row.item = item; row.label = item.label || row.label }
    else rows.set(item.node_id, { nodeId: item.node_id, label: item.label, actions: [], item })
  }
  return [...rows.values()]
}

/** This measures verified packages, never pretends to measure solver progress. */
export function preparationCompletion(progress: DeepReadonly<ChangePreparationProgress> | null | undefined) {
  const total = progress?.items.length ?? 0
  const completed = progress?.items.filter(item => item.stage === 'ready').length ?? 0
  return { completed, total, percentage: total ? Math.floor(completed / total * 100) : null }
}
