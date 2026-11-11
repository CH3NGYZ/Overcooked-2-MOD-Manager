import type { ChefDraft, ChefEditorSnapshot, ChefEntry, ChefReference, HatReference } from './types.ts'

export interface ChefEditState {
  draft: ChefDraft
  /** Hats for disabled chefs belong to this editor session, never to prefer.txt. */
  disabledHats: Record<string, HatReference>
}
export type ChefEditResult = { state: ChefEditState; error?: never } | { state?: never; error: 'lastChef' | 'unavailable' | 'nativeDefaultRequired' }

export function isUsableResource(state: string): boolean {
  return state === 'valid' || state === 'known_name'
}

export function createChefDraft(snapshot: ChefEditorSnapshot): ChefDraft {
  return {
    base_revision: snapshot.source.revision,
    catalog_revision: snapshot.catalog_revision,
    target_generation: snapshot.target.generation,
    entries: snapshot.selection.entries.map(copyEntry),
    lobby_switch: snapshot.selection.lobby_switch,
    removed_problem_row_ids: [],
    acknowledged_change_ids: [],
    reset_intent: null,
  }
}

export function copyEntry(entry: ChefEntry): ChefEntry {
  return { ...entry, chef_ref: { ...entry.chef_ref }, hat_ref: { ...entry.hat_ref } }
}

export function copyEditState(state: ChefEditState): ChefEditState {
  return {
    draft: {
      ...state.draft,
      entries: state.draft.entries.map(copyEntry),
      removed_problem_row_ids: [...state.draft.removed_problem_row_ids],
      acknowledged_change_ids: [...state.draft.acknowledged_change_ids],
    },
    disabledHats: Object.fromEntries(Object.entries(state.disabledHats).map(([id, hat]) => [id, { ...hat }])),
  }
}

function referenceValue(reference: ChefReference | HatReference) {
  if (reference.kind === 'unresolved') return reference
  return 'chef_id' in reference ? reference.chef_id : reference.hat_id
}

/** Identity/confirmation tokens are not user edits; semantic order and unresolved text are. */
export function draftSignature(draft: ChefDraft): string {
  return JSON.stringify({
    entries: draft.entries.map((entry) => [referenceValue(entry.chef_ref), referenceValue(entry.hat_ref)]),
    lobby: draft.lobby_switch,
    removed: [...draft.removed_problem_row_ids].sort(),
    reset: draft.reset_intent,
  })
}

export function usableEntries(draft: ChefDraft, snapshot: ChefEditorSnapshot): ChefEntry[] {
  const usable = new Set(snapshot.chefs.filter((chef) => isUsableResource(chef.resource_state)).map((chef) => chef.id))
  return draft.entries.filter((entry) => entry.chef_ref.kind === 'known' && usable.has(entry.chef_ref.chef_id))
}

export function canBeDefault(source: string, snapshot: ChefEditorSnapshot): boolean {
  return source === 'native' || (source === 'custom' && snapshot.target.supports_custom_default === true)
}

function defaultEntries(draft: ChefDraft, snapshot: ChefEditorSnapshot): ChefEntry[] {
  const eligible = new Set(snapshot.chefs.filter(chef => canBeDefault(chef.source_kind, snapshot)).map(chef => chef.id))
  return usableEntries(draft, snapshot).filter(entry => entry.chef_ref.kind === 'known' && eligible.has(entry.chef_ref.chef_id))
}

export function hasValidDefault(draft: ChefDraft, snapshot: ChefEditorSnapshot): boolean {
  const first = usableEntries(draft, snapshot)[0]
  return !!first && defaultEntries(draft, snapshot)[0] === first
}

/** Only explicit edits reorder a legacy custom-first file; reading stays lossless. */
function promoteDefault(state: ChefEditState, snapshot: ChefEditorSnapshot) {
  if (hasValidDefault(state.draft, snapshot)) return
  const first = defaultEntries(state.draft, snapshot)[0]
  if (first) state.draft.entries = [first, ...state.draft.entries.filter(entry => entry !== first)]
}

export function entryForChef(draft: ChefDraft, chefId: string): ChefEntry | undefined {
  return draft.entries.find((entry) => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === chefId)
}

export function autoHat(snapshot: ChefEditorSnapshot): HatReference {
  const hat = snapshot.hats.find((item) => item.kind === 'auto')
  if (!hat) throw new Error('diychef.catalogUnavailable')
  return { kind: 'known', hat_id: hat.id }
}

export function hatForChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string): HatReference {
  return entryForChef(state.draft, chefId)?.hat_ref ?? state.disabledHats[chefId] ?? autoHat(snapshot)
}

function newEntry(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string): ChefEntry {
  // Re-enabling an original row keeps its metadata for lossless backend editing.
  const original = snapshot.selection.entries.find((entry) => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === chefId)
  return {
    entry_id: original?.entry_id ?? `draft:${chefId}`,
    row_id: original?.row_id ?? null,
    chef_ref: { kind: 'known', chef_id: chefId },
    hat_ref: { ...(state.disabledHats[chefId] ?? original?.hat_ref ?? autoHat(snapshot)) },
  }
}

export function toggleChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string): ChefEditResult {
  const selected = entryForChef(state.draft, chefId)
  const error = toggleChefError(state.draft, snapshot, chefId)
  if (error) return { error }
  const next = copyEditState(state)
  next.draft.acknowledged_change_ids = []
  if (selected) {
    next.disabledHats[chefId] = { ...selected.hat_ref }
    next.draft.entries = next.draft.entries.filter((entry) => entry.entry_id !== selected.entry_id)
  } else {
    next.draft.entries.push(newEntry(state, snapshot, chefId))
    delete next.disabledHats[chefId]
  }
  promoteDefault(next, snapshot)
  return { state: next }
}

export function toggleChefError(draft: ChefDraft, snapshot: ChefEditorSnapshot, chefId: string): ChefEditResult['error'] | null {
  const selected = entryForChef(draft, chefId)
  const chef = snapshot.chefs.find((item) => item.id === chefId)
  if (!chef) return 'unavailable'
  if (!snapshot.target.supports_custom_default && selected && chef.source_kind === 'native' && isUsableResource(chef.resource_state) && defaultEntries(draft, snapshot).length <= 1) {
    return 'nativeDefaultRequired'
  }
  if (selected && isUsableResource(chef.resource_state) && usableEntries(draft, snapshot).length <= 1) {
    return 'lastChef'
  }
  return !selected && !isUsableResource(chef.resource_state) ? 'unavailable' : null
}

export function setDefaultChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string): ChefEditResult {
  const chef = snapshot.chefs.find((item) => item.id === chefId)
  if (!chef || !isUsableResource(chef.resource_state)) return { error: 'unavailable' }
  if (!canBeDefault(chef.source_kind, snapshot)) return { error: 'nativeDefaultRequired' }
  const next = copyEditState(state)
  const entry = entryForChef(next.draft, chefId) ?? newEntry(state, snapshot, chefId)
  next.draft.entries = [entry, ...next.draft.entries.filter((item) => item.entry_id !== entry.entry_id)]
  next.draft.acknowledged_change_ids = []
  delete next.disabledHats[chefId]
  return { state: next }
}

/** Both pointer and keyboard moves use full entries, preserving unresolved rows' relative order. */
export function moveChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string, targetChefId: string, after?: boolean): ChefEditState {
  if (!defaultEntries(state.draft, snapshot).length) return state
  const from = state.draft.entries.findIndex((entry) => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === chefId)
  const to = state.draft.entries.findIndex((entry) => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === targetChefId)
  if (from < 0 || to < 0 || from === to) return state
  const next = copyEditState(state)
  const [entry] = next.draft.entries.splice(from, 1)
  const target = to - (from < to ? 1 : 0)
  next.draft.entries.splice(target + Number(after ?? from < to), 0, entry!)
  promoteDefault(next, snapshot)
  if (draftSignature(next.draft) === draftSignature(state.draft)) return state
  next.draft.acknowledged_change_ids = []
  return next
}

/** Dropping from the disabled section enables and positions in one undoable edit. */
export function placeChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string, targetChefId: string | null, after: boolean): ChefEditResult {
  if (targetChefId === null) {
    if (usableEntries(state.draft, snapshot).length) return { state }
    const chef = snapshot.chefs.find(chef => chef.id === chefId)
    if (!chef || !isUsableResource(chef.resource_state)) return { error: 'unavailable' }
    if (!canBeDefault(chef.source_kind, snapshot)) return { error: 'nativeDefaultRequired' }
    return toggleChef(state, snapshot, chefId)
  }
  if (!entryForChef(state.draft, targetChefId)) return { state }
  const selected = entryForChef(state.draft, chefId) ? { state } : toggleChef(state, snapshot, chefId)
  if (selected.error) return selected
  return { state: moveChef(selected.state!, snapshot, chefId, targetChefId, after) }
}

export function changeChefHat(state: ChefEditState, chefIds: string | readonly string[], hat: HatReference): ChefEditState {
  const next = copyEditState(state)
  const remaining = new Set(typeof chefIds === 'string' ? [chefIds] : chefIds)
  for (const entry of next.draft.entries) {
    if (entry.chef_ref.kind === 'known' && remaining.has(entry.chef_ref.chef_id)) {
      entry.hat_ref = { ...hat }
      remaining.delete(entry.chef_ref.chef_id)
    }
  }
  for (const chefId of remaining) next.disabledHats[chefId] = { ...hat }
  next.draft.acknowledged_change_ids = []
  return next
}

export function removeProblemRow(state: ChefEditState, snapshot: ChefEditorSnapshot, rowId: string): ChefEditResult {
  const next = copyEditState(state)
  next.draft.entries = next.draft.entries.filter((entry) => entry.row_id !== rowId)
  if (defaultEntries(state.draft, snapshot).length && !defaultEntries(next.draft, snapshot).length) return { error: snapshot.target.supports_custom_default ? 'lastChef' : 'nativeDefaultRequired' }
  if (!usableEntries(next.draft, snapshot).length) return { error: 'lastChef' }
  promoteDefault(next, snapshot)
  next.draft.removed_problem_row_ids = [...new Set([...next.draft.removed_problem_row_ids, rowId])]
  next.draft.acknowledged_change_ids = []
  return { state: next }
}

export function resetChefDraft(state: ChefEditState, snapshot: ChefEditorSnapshot, mode: 'keep_black_cat' | 'restore_default'): ChefEditState {
  const blackCat = snapshot.chefs.find((chef) => chef.source_kind === 'native' && chef.config_name === 'Chef_Black_cat')
  if (!blackCat) throw new Error('diychef.catalogUnavailable')
  const next = copyEditState(state)
  next.draft.entries = [{ ...newEntry(state, snapshot, blackCat.id), hat_ref: autoHat(snapshot) }]
  next.draft.lobby_switch = mode === 'restore_default' || state.draft.lobby_switch
  next.draft.removed_problem_row_ids = snapshot.document_rows.filter((row) => row.category !== 'blank').map((row) => row.row_id)
  next.draft.acknowledged_change_ids = []
  next.draft.reset_intent = mode
  next.disabledHats = {}
  return next
}

/** Creating a missing file establishes a new baseline without replacing an edited draft. */
export function rebaseInitializedDraft(state: ChefEditState, snapshot: ChefEditorSnapshot): ChefEditState {
  const next = copyEditState(state)
  next.draft.base_revision = snapshot.source.revision
  next.draft.catalog_revision = snapshot.catalog_revision
  next.draft.target_generation = snapshot.target.generation
  next.draft.acknowledged_change_ids = []
  return next
}
