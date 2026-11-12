import { computed, onBeforeUnmount, shallowRef, watch } from 'vue'
import { api } from '../services/tauri'
import { useDashboard } from './useDashboardStore'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import type { ChefChange, ChefEditorOpen, ChefEditorSnapshot, ChefValidation, HatReference } from '../diychef/types'
import {
  changeChefHat, copyEditState, createChefDraft, draftSignature, moveChef,
  rebaseInitializedDraft, removeProblemRow, resetChefDraft, setDefaultChef, toggleChef, placeChef,
  type ChefEditResult, type ChefEditState,
} from '../diychef/draft'

interface EditorOptions {
  confirmChanges: (changes: ChefChange[]) => Promise<boolean>
  cancelChanges: () => void
  demoSnapshot?: () => ChefEditorSnapshot | null
}

/** Disk snapshots and editable state are independent until a successful write and read-back. */
export function useDiyChefEditor(options: EditorOptions) {
  const dashboard = useDashboard()
  const notify = useNotify()
  const snapshot = shallowRef<ChefEditorSnapshot | null>(null)
  const state = shallowRef<ChefEditState | null>(null)
  const opened = shallowRef<ChefEditorOpen | null>(null)
  const loading = shallowRef(false)
  const saving = shallowRef(false)
  const initializing = shallowRef(false)
  const operationError = shallowRef('')
  const validationError = shallowRef('')
  const error = computed(() => operationError.value || validationError.value)
  const needsReload = shallowRef(false)
  const validation = shallowRef<ChefValidation | null>(null)
  const undoState = shallowRef<ChefEditState | null>(null)
  const demonstrating = computed(() => !!options.demoSnapshot?.())
  const gameRunning = computed(() => !demonstrating.value && (dashboard.game.value?.kitchen_running ?? false))
  const readOnly = computed(() => snapshot.value?.target.compatibility === 'read_only')
  const stale = computed(() => !demonstrating.value && !!snapshot.value && dashboard.modsGeneration.value !== snapshot.value.target.generation)
  const busy = computed(() => loading.value || saving.value || initializing.value)
  const editable = computed(() => !!state.value && !busy.value && !readOnly.value)
  const baselineSignature = computed(() => snapshot.value ? draftSignature(createChefDraft(snapshot.value)) : '')
  const dirty = computed(() => !demonstrating.value && !!state.value && draftSignature(state.value.draft) !== baselineSignature.value)
  const canSave = computed(() => dirty.value && !busy.value && !readOnly.value && !gameRunning.value && !stale.value && !needsReload.value && snapshot.value?.source.state === 'present')
  const diagnostics = computed(() => validation.value?.diagnostics ?? snapshot.value?.diagnostics ?? [])
  let generation = 0
  let validationGeneration = 0
  let validationTimer: ReturnType<typeof setTimeout> | null = null
  let disposed = false
  const pendingDiskOperations = new Set<symbol>()
  let suspended: ReturnType<typeof captureEditor> | null = null

  function captureEditor() {
    return {
      snapshot: snapshot.value, state: state.value, opened: opened.value, undo: undoState.value,
      validation: validation.value, operationError: operationError.value, validationError: validationError.value,
      needsReload: needsReload.value, diskWasPending: pendingDiskOperations.size > 0,
    }
  }

  function invalidateRequests() {
    generation += 1
    validationGeneration += 1
    if (validationTimer) clearTimeout(validationTimer)
    validationTimer = null
    options.cancelChanges()
    loading.value = false
    saving.value = false
    initializing.value = false
  }

  function acceptSnapshot(value: ChefEditorSnapshot, preserveDraft = false) {
    const previous = state.value
    const wasDirty = dirty.value
    snapshot.value = value
    state.value = preserveDraft && wasDirty && previous
      ? rebaseInitializedDraft(previous, value)
      : { draft: createChefDraft(value) }
    if (preserveDraft && undoState.value) undoState.value = rebaseInitializedDraft(undoState.value, value)
    else undoState.value = null
    validation.value = null
    operationError.value = ''
    validationError.value = ''
    needsReload.value = false
  }

  function reportOperationFailure(reason: unknown) {
    const raw = reason instanceof Error ? reason.message : String(reason)
    // Match the raw IPC tag before translation; a written file or stale revision
    // must be re-read before another save can use the old baseline credentials.
    const tag = raw.split('?', 1)[0]
    if (['diychef.conflict', 'diychef.writtenReadFailed', 'diychef.targetChanged', 'diychef.sessionExpired', 'diychef.staleScan'].includes(tag!)) needsReload.value = true
    operationError.value = backendMessageOf(reason)
  }

  async function initializeMissing() {
    const current = snapshot.value
    if (disposed || demonstrating.value || needsReload.value || !current || current.source.state !== 'missing' || gameRunning.value || readOnly.value || busy.value || error.value) return
    const ticket = generation
    const operation = Symbol()
    pendingDiskOperations.add(operation)
    initializing.value = true
    try {
      const result = await api.diyChefInitialize(current.session_id, current.source.revision)
      if (!disposed && ticket === generation) acceptSnapshot(result, true)
    } catch (reason) {
      if (!disposed && ticket === generation) reportOperationFailure(reason)
    } finally {
      pendingDiskOperations.delete(operation)
      if (!disposed && ticket === generation) {
        initializing.value = false
        scheduleValidation()
      }
    }
  }

  async function open(targetId?: string, rescan = false): Promise<void> {
    if (disposed || demonstrating.value || busy.value) return
    const ticket = ++generation
    validationGeneration += 1
    loading.value = true
    operationError.value = ''
    validationError.value = ''
    try {
      await dashboard.ensureLoaded()
      if (disposed || ticket !== generation) return
      if (rescan) await dashboard.loadLocalMods()
      if (disposed || ticket !== generation) return
      await dashboard.ensureLocalModsLoaded()
      if (disposed || ticket !== generation) return
      const result = await api.diyChefOpen(dashboard.modsGeneration.value, targetId)
      if (disposed || ticket !== generation) return
      opened.value = result
      if (result.snapshot) acceptSnapshot(result.snapshot)
      else { snapshot.value = null; state.value = null; undoState.value = null }
    } catch (reason) {
      if (!disposed && ticket === generation) reportOperationFailure(reason)
    } finally {
      if (!disposed && ticket === generation) {
        loading.value = false
        await initializeMissing()
      }
    }
  }

  async function validateCurrent() {
    const current = snapshot.value
    const edit = state.value
    if (disposed || demonstrating.value || !current || !edit || busy.value || readOnly.value || stale.value) return
    const ticket = ++validationGeneration
    try {
      const result = await api.diyChefValidate(current.session_id, edit.draft)
      if (!disposed && ticket === validationGeneration && edit === state.value) {
        validation.value = result
        validationError.value = ''
      }
    } catch (reason) {
      if (!disposed && ticket === validationGeneration && edit === state.value) validationError.value = backendMessageOf(reason)
    }
  }

  function scheduleValidation() {
    if (disposed || demonstrating.value) return
    validationGeneration += 1
    if (validationTimer) clearTimeout(validationTimer)
    validationTimer = setTimeout(() => { validationTimer = null; void validateCurrent() }, 280)
  }

  function edit(next: ChefEditState) {
    if (!editable.value || !state.value || next === state.value) return
    undoState.value = copyEditState(state.value)
    state.value = next
    validation.value = null
    validationError.value = ''
    scheduleValidation()
  }

  function applyResult(result: ChefEditResult) {
    if (result.error) notify.info(t(`diychef.${result.error}`))
    else if (result.state) edit(result.state)
  }

  function toggle(chefId: string) {
    if (editable.value && snapshot.value && state.value) applyResult(toggleChef(state.value, snapshot.value, chefId))
  }
  function setDefault(chefId: string) {
    if (editable.value && snapshot.value && state.value) applyResult(setDefaultChef(state.value, snapshot.value, chefId))
  }
  function move(chefId: string, targetChefId: string) {
    if (state.value && snapshot.value) edit(moveChef(state.value, snapshot.value, chefId, targetChefId))
  }
  function place(chefId: string, targetChefId: string | null, after: boolean) {
    if (editable.value && state.value && snapshot.value) applyResult(placeChef(state.value, snapshot.value, chefId, targetChefId, after))
  }
  function changeHat(chefIds: string | readonly string[], hat: HatReference) {
    if (state.value) edit(changeChefHat(state.value, chefIds, hat))
  }
  function setLobby(value: boolean) {
    if (!state.value) return
    const next = copyEditState(state.value)
    next.draft.lobby_switch = value
    next.draft.acknowledged_change_ids = []
    edit(next)
  }
  function removeProblem(rowId: string) {
    if (state.value && snapshot.value) applyResult(removeProblemRow(state.value, snapshot.value, rowId))
  }
  function reset(mode: 'keep_black_cat' | 'restore_default') {
    if (!state.value || !snapshot.value || !editable.value) return
    try { edit(resetChefDraft(state.value, snapshot.value, mode)) }
    catch (reason) { notify.error(backendMessageOf(reason)) }
  }
  function undo() {
    if (!editable.value || !undoState.value) return
    state.value = undoState.value
    undoState.value = null
    validation.value = null
    scheduleValidation()
  }

  async function save(): Promise<boolean> {
    if (disposed || demonstrating.value) return false
    if (!canSave.value || !snapshot.value || !state.value) return !dirty.value
    const current = snapshot.value
    const pending = copyEditState(state.value)
    const ticket = generation
    const operation = Symbol()
    saving.value = true
    operationError.value = ''
    validationError.value = ''
    validationGeneration += 1
    try {
      const checked = await api.diyChefValidate(current.session_id, pending.draft)
      if (disposed || demonstrating.value || ticket !== generation) return false
      validation.value = checked
      if (checked.save_blocking) { notify.error(t('diychef.validationBlocked')); return false }
      if (checked.changes.length && !await options.confirmChanges(checked.changes)) return false
      if (disposed || demonstrating.value || ticket !== generation) return false
      pending.draft.acknowledged_change_ids = checked.changes.map((change) => change.id)
      pendingDiskOperations.add(operation)
      const result = await api.diyChefSave(current.session_id, pending.draft)
      if (disposed || ticket !== generation) return false
      acceptSnapshot(result)
      notify.success(t('diychef.saved'))
      return true
    } catch (reason) {
      if (!disposed && ticket === generation) reportOperationFailure(reason)
      return false
    } finally {
      pendingDiskOperations.delete(operation)
      if (!disposed && ticket === generation) saving.value = false
    }
  }

  watch(() => options.demoSnapshot?.() ?? null, (demo) => {
    if (disposed) return
    if (demo) {
      suspended ??= captureEditor()
      invalidateRequests()
      opened.value = { status: 'ready', candidates: [], snapshot: demo }
      acceptSnapshot(demo)
    } else if (suspended) {
      const real = suspended
      suspended = null
      invalidateRequests()
      snapshot.value = real.snapshot
      state.value = real.state
      opened.value = real.opened
      undoState.value = real.undo
      validation.value = real.validation
      operationError.value = real.operationError
      validationError.value = real.validationError
      needsReload.value = real.needsReload
      // A write already sent before the tutorial cannot be cancelled. Retain its
      // draft, but require a fresh baseline rather than reuse old revision tokens.
      if (real.diskWasPending) reportOperationFailure('diychef.conflict')
      if (!real.opened) void open()
      else if (!real.diskWasPending) { void initializeMissing(); if (dirty.value) scheduleValidation() }
    }
  }, { immediate: true, flush: 'sync' })
  watch(gameRunning, (running) => { if (!running) void initializeMissing() })
  onBeforeUnmount(() => {
    disposed = true
    generation += 1
    validationGeneration += 1
    if (validationTimer) clearTimeout(validationTimer)
    options.cancelChanges()
  })
  return {
    snapshot, state, opened, loading, saving, initializing, error, validation, diagnostics,
    gameRunning, readOnly, stale, busy, editable, dirty, canSave, demonstrating,
    canUndo: computed(() => !!undoState.value && editable.value),
    open, save, toggle, setDefault, move, place, changeHat, setLobby, removeProblem, reset, undo,
  }
}
