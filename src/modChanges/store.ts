import { computed, readonly, shallowRef } from 'vue'
import { api } from '../services/tauri'
import { beginDownloadTask, cancelDownloadTask, endDownloadTask, updateDownloadProgress } from '../downloadTask'
import { createChangeWorkflow } from './workflow'
import { setEvaluationSnapshotReader } from './evaluationProjection'
import type { ChangeWorkflowState } from './workflow'
import type { ChangePreparationProgress, ChangeRequest, EvaluationSnapshot, EvaluationSubject } from './types'

const state = shallowRef<ChangeWorkflowState | null>(null)
const preparation = shallowRef<ChangePreparationProgress | null>(null)
const cancellingDownload = shallowRef(false)
const evaluation = shallowRef<EvaluationSnapshot | null>(null)
const evaluating = shallowRef(false)
setEvaluationSnapshotReader(() => evaluation.value)
let revision = 0
let pending: Promise<void> | undefined
let invalidated = false
const registeredSubjects = new Map<string, EvaluationSubject>()
let subjectSequence = 0
let preparationSequence = 0
let preparationTaskId: number | undefined

export function registerEvaluationSubject(subject: EvaluationSubject) {
  const token = String(++subjectSequence)
  registeredSubjects.set(token, subject)
  void invalidate().catch(() => undefined)
  return () => { registeredSubjects.delete(token) }
}

const workflow = createChangeWorkflow({
  plan: request => api.planModChanges(request),
  prepare: async planId => {
    const sequence = ++preparationSequence
    preparation.value = null
    const token = beginDownloadTask({ label: 'modChanges.title', retry: () => workflow.retry(),
      progressSource: 'caller', cancel: cancelPreparationDownload })
    preparationTaskId = token
    let active = true
    try { return await api.prepareModChanges(planId, progress => {
      if (!active || sequence !== preparationSequence || progress.plan_id !== planId) return
      preparation.value = progress
      const item = progress.items.find(item => item.node_id === progress.active_node_id && item.stage === 'downloading')
      // Each snapshot is complete: verification, candidate replacement and failure clear old bytes.
      updateDownloadProgress(progress.stage === 'downloading' ? item?.download ?? null : null, token)
    }) }
    finally {
      active = false
      if (preparationTaskId === token) preparationTaskId = undefined
      endDownloadTask(token)
    }
  },
  execute: planId => api.executeModChanges(planId),
  cancel: planId => api.cancelModChanges(planId),
}, value => {
  state.value = value
  if (!value || value.phase === 'planning') { preparationSequence += 1; preparation.value = null }
  if (value?.phase !== 'preparing') cancellingDownload.value = false
})

function canCancelPreparation() {
  return state.value?.phase === 'preparing' && preparation.value?.stage === 'downloading' && !cancellingDownload.value
}

/** Cancels only the active download; the plan waits for its failure and cleanup. */
async function cancelPreparationDownload() {
  const token = preparationTaskId
  if (token === undefined || !canCancelPreparation()) return
  cancellingDownload.value = true
  try {
    await api.cancelDownload()
    cancelDownloadTask(token)
  } catch {
    if (preparationTaskId === token) cancellingDownload.value = false
  }
}

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
    state: readonly(state), preparation: readonly(preparation), cancellingDownload: readonly(cancellingDownload),
    canCancelDownload: computed(canCancelPreparation), cancelPreparationDownload,
    evaluation: readonly(evaluation), evaluating: readonly(evaluating),
    busy: computed(() => state.value !== null),
    run, refresh, invalidate, clearEvaluation, confirm: workflow.confirm, cancel: workflow.cancel, retry: workflow.retry,
  }
}
