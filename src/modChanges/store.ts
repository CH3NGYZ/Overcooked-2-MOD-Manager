import { computed, readonly, shallowRef } from 'vue'
import { api } from '../services/tauri'
import { createChangeWorkflow } from './workflow'
import { setEvaluationSnapshotReader } from './evaluationProjection'
import type { ChangeWorkflowState } from './workflow'
import type { ChangeRequest, EvaluationSnapshot, EvaluationSubject } from './types'

const state = shallowRef<ChangeWorkflowState | null>(null)
const evaluation = shallowRef<EvaluationSnapshot | null>(null)
const evaluating = shallowRef(false)
setEvaluationSnapshotReader(() => evaluation.value)
let revision = 0
let pending: Promise<void> | undefined
let invalidated = false
const registeredSubjects = new Map<string, EvaluationSubject>()
let subjectSequence = 0

export function registerEvaluationSubject(subject: EvaluationSubject) {
  const token = String(++subjectSequence)
  registeredSubjects.set(token, subject)
  void invalidate().catch(() => undefined)
  return () => { registeredSubjects.delete(token) }
}

const workflow = createChangeWorkflow({
  plan: request => api.planModChanges(request),
  prepare: planId => api.prepareModChanges(planId),
  execute: planId => api.executeModChanges(planId),
  cancel: planId => api.cancelModChanges(planId),
}, value => { state.value = value })

/** Batch all visible subjects through one backend snapshot; newer refreshes win. */
async function refresh(subjects?: EvaluationSubject[]) {
  const mine = ++revision
  evaluating.value = true
  try {
    const allSubjects = [...registeredSubjects.values(), ...(subjects ?? [])]
    const unique = allSubjects.filter((subject, index) => allSubjects.findIndex(peer =>
      peer.repository_id === subject.repository_id && peer.map_id === subject.map_id
      && peer.tag === subject.tag && peer.dll_file === subject.dll_file
      && peer.map_dir_name === subject.map_dir_name) === index)
    const result = await api.evaluateModChanges(unique.length ? unique : undefined)
    if (mine === revision) evaluation.value = result
  } finally {
    if (mine === revision) evaluating.value = false
  }
}

function clearEvaluation() {
  revision += 1
  evaluation.value = null
  evaluating.value = false
}

/** Coalesce ordinary invalidations; no per-row IPC or relation solver runs here. */
function invalidate() {
  invalidated = true
  if (pending) return pending
  pending = Promise.resolve().then(async () => {
    do { invalidated = false; await refresh() } while (invalidated)
  }).finally(() => { pending = undefined })
  return pending
}

async function run(request: ChangeRequest) {
  const result = await workflow.run(request)
  if (result?.ok) await invalidate().catch(() => undefined)
  return result
}

export function useModChanges() {
  return {
    state: readonly(state), evaluation: readonly(evaluation), evaluating: readonly(evaluating),
    busy: computed(() => state.value !== null),
    run, refresh, invalidate, clearEvaluation, confirm: workflow.confirm, cancel: workflow.cancel,
  }
}
