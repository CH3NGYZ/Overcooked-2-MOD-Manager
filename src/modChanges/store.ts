import { computed, readonly, shallowReactive, shallowRef } from 'vue'
import { api } from '../services/tauri'
import { beginDownloadTask, cancelDownloadTask, endDownloadTask, updateDownloadProgress } from '../downloadTask'
import { createChangeWorkflow } from './workflow'
import { setEvaluationSnapshotReader } from './evaluationProjection'
import { installNoteKey, localizedInstallNotes, type InstallNote } from './planNotes'
import { changeStatusKey } from './presentation'
import type { ChangeWorkflowState } from './workflow'
import type { ChangeExecutionStage, ChangePreparationProgress, EvaluationSnapshot, EvaluationSubject } from './types'

const state = shallowRef<ChangeWorkflowState | null>(null)
const preparation = shallowRef<ChangePreparationProgress | null>(null)
const execution = shallowReactive(new Map<string, ChangeExecutionStage>())
const cancellingDownload = shallowRef(false)
const evaluation = shallowRef<EvaluationSnapshot | null>(null)
const evaluating = shallowRef(false)
const readNotes = shallowRef<ReadonlySet<string>>(new Set())
const installNotes = computed<readonly InstallNote[]>(() => {
  const current = state.value
  if (current?.plan?.status !== 'ready' || (current.phase !== 'confirming' && current.phase !== 'reconfirming')) return []
  return localizedInstallNotes(current.plan.install_notes)
    .filter(note => !readNotes.value.has(installNoteKey(note)))
})
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
  execute: async planId => {
    let committed = false
    let active = true
    execution.clear()
    try {
      const result = await api.executeModChanges(planId, progress => {
        if (!active || progress.plan_id !== planId) return
        for (const { node_id, member_key, stage } of progress.items) execution.set(changeStatusKey(node_id, member_key), stage)
      })
      committed = result.ok
      return result
    } finally {
      active = false
      // A write verified before commit can be rolled back. Never leave a stale
      // checkmark if the command fails before the final Channel message arrives.
      if (!committed) for (const [key, stage] of execution) if (stage !== 'applied') execution.set(key, 'failed')
    }
  },
  cancel: planId => api.cancelModChanges(planId),
}, value => {
  state.value = value
  if (!value && readNotes.value.size) readNotes.value = new Set()
  if (!value || value.phase === 'planning') { preparationSequence += 1; preparation.value = null; execution.clear() }
  if (value?.phase !== 'preparing') cancellingDownload.value = false
})

function acceptInstallNotes(notes: readonly InstallNote[]) {
  const pending = installNotes.value
  const read = notes.map(installNoteKey)
  if (!read.length || pending.length !== read.length || pending.some((note, i) => installNoteKey(note) !== read[i])) return
  // Use source and text, since package verification can change the plan ID or release.
  readNotes.value = new Set([...readNotes.value, ...read])
}

async function confirm() {
  if (!installNotes.value.length) await workflow.confirm()
}

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
    return result
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

export function useModChanges() {
  return {
    state: readonly(state), preparation: readonly(preparation), execution: readonly(execution), cancellingDownload: readonly(cancellingDownload),
    canCancelDownload: computed(canCancelPreparation), cancelPreparationDownload,
    installNotes, acceptInstallNotes,
    evaluation: readonly(evaluation), evaluating: readonly(evaluating),
    busy: computed(() => state.value !== null),
    run: workflow.run, refresh, invalidate, clearEvaluation, confirm, cancel: workflow.cancel, retry: workflow.retry,
  }
}
