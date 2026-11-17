import type { ChangeBackend, ChangePlan, ChangeRequest, ChangeExecutionResult } from './types'

export type ChangePhase = 'planning' | 'confirming' | 'preparing' | 'reconfirming' | 'executing' | 'failed'
export interface ChangeWorkflowState {
  phase: ChangePhase
  plan: ChangePlan | null
  error: unknown
}

/** One batch is prepared and committed once; a revised plan always needs consent. */
export function createChangeWorkflow(backend: ChangeBackend, publish: (state: ChangeWorkflowState | null) => void) {
  let generation = 0
  let current: ChangeWorkflowState | null = null
  let settle: ((value: ChangeExecutionResult | null) => void) | undefined

  function update(state: ChangeWorkflowState | null) { current = state; publish(state) }
  function finish(value: ChangeExecutionResult | null) {
    const resolve = settle
    settle = undefined
    update(null)
    resolve?.(value)
  }

  async function run(request: ChangeRequest): Promise<ChangeExecutionResult | null> {
    if (current) throw new Error('modChanges.busy')
    const mine = ++generation
    update({ phase: 'planning', plan: null, error: null })
    const result = new Promise<ChangeExecutionResult | null>((resolve) => { settle = resolve })
    try {
      const plan = await backend.plan(request)
      if (mine !== generation) { await backend.cancel(plan.plan_id).catch(() => undefined); return result }
      update({ phase: 'confirming', plan, error: null })
    } catch (error) {
      if (mine === generation) update({ phase: 'failed', plan: null, error })
    }
    return result
  }

  async function confirm() {
    const state = current
    if (!state?.plan || !['confirming', 'reconfirming'].includes(state.phase)) return
    if (state.plan.status !== 'ready') return
    const mine = generation
    try {
      let plan = state.plan
      if (!plan.prepared) {
        update({ phase: 'preparing', plan, error: null })
        const prepared = await backend.prepare(plan.plan_id)
        if (mine !== generation) { await backend.cancel(prepared.plan_id).catch(() => undefined); return }
        // Backend changes plan_id whenever root, actions, dependencies or package contents change.
        if (prepared.plan_id !== plan.plan_id || prepared.status !== 'ready') {
          update({ phase: 'reconfirming', plan: prepared, error: null })
          return
        }
        plan = prepared
      }
      update({ phase: 'executing', plan, error: null })
      const result = await backend.execute(plan.plan_id)
      if (mine !== generation) return
      if (!result.ok) { update({ phase: 'failed', plan, error: result.message }); return }
      finish(result)
    } catch (error) {
      if (mine === generation) update({ phase: 'failed', plan: current?.plan ?? state.plan, error })
    }
  }

  async function cancel() {
    if (current?.phase === 'executing' || current?.phase === 'preparing') return
    const plan = current?.plan
    generation += 1
    finish(null)
    if (plan) await backend.cancel(plan.plan_id).catch(() => undefined)
  }

  return { run, confirm, cancel }
}
