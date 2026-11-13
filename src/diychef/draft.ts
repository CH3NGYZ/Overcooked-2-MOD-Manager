import type { ChefDraft, ChefEditorSnapshot, ChefEntry, ChefReference, HatReference } from './types.ts'

export interface ChefEditState {
  draft: ChefDraft
}
export type ChefEditError = 'lastChef' | 'unavailable' | 'nativeDefaultRequired' | 'nativeFallbackRequired'
export type ChefEditResult = { state: ChefEditState; error?: never; notice?: 'nativeFallbackRetained' } | { state?: never; error: ChefEditError; notice?: never }

export function isUsableResource(state: string): boolean {
  return state === 'valid' || state === 'known_name'
}

export function createChefDraft(snapshot: ChefEditorSnapshot): ChefDraft {
  return {
    base_revision: snapshot.source.revision,
    catalog_revision: snapshot.catalog_revision,
    target_generation: snapshot.target.generation,
    entries: snapshot.selection.entries.map(copyEntry),
    hat_bindings: copyBindings(snapshot.selection.hat_bindings ?? Object.fromEntries(snapshot.selection.entries.flatMap((entry) =>
      entry.chef_ref.kind === 'known' ? [[entry.chef_ref.chef_id, entry.hat_ref]] : []))),
    lobby_switch: snapshot.selection.lobby_switch,
    removed_problem_row_ids: [],
    acknowledged_change_ids: [],
    reset_intent: null,
  }
}

export function copyEntry(entry: ChefEntry): ChefEntry {
  return { ...entry, chef_ref: { ...entry.chef_ref }, hat_ref: { ...entry.hat_ref } }
}

function copyBindings(bindings: Record<string, HatReference>): Record<string, HatReference> {
  return Object.fromEntries(Object.entries(bindings).map(([id, hat]) => [id, { ...hat }]))
}

export function copyEditState(state: ChefEditState): ChefEditState {
  return {
    draft: {
      ...state.draft,
      entries: state.draft.entries.map(copyEntry),
      hat_bindings: copyBindings(state.draft.hat_bindings),
      removed_problem_row_ids: [...state.draft.removed_problem_row_ids],
      acknowledged_change_ids: [...state.draft.acknowledged_change_ids],
    },
  }
}

function referenceValue(reference: ChefReference | HatReference) {
  if (reference.kind === 'unresolved') return reference
  return 'chef_id' in reference ? reference.chef_id : reference.hat_id
}

/** Identity/confirmation tokens are not user edits; semantic order and unresolved text are. */
export function draftSignature(draft: ChefDraft): string {
  return JSON.stringify({
    entries: draft.entries.map((entry) => [referenceValue(entry.chef_ref), entry.chef_ref.kind === 'unresolved' ? referenceValue(entry.hat_ref) : null]),
    bindings: Object.entries(draft.hat_bindings).filter(([, hat]) => hat.kind !== 'known' || hat.hat_id !== 'hat:')
      .sort(([left], [right]) => left.localeCompare(right)).map(([id, hat]) => [id, referenceValue(hat)]),
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

export function defaultEntries(draft: ChefDraft, snapshot: ChefEditorSnapshot): ChefEntry[] {
  const eligible = new Set(snapshot.chefs.filter(chef => canBeDefault(chef.source_kind, snapshot)).map(chef => chef.id))
  return usableEntries(draft, snapshot).filter(entry => entry.chef_ref.kind === 'known' && eligible.has(entry.chef_ref.chef_id))
}

/** Enabled native chefs are the stable fallback even when a 2.2+ custom chef opens first. */
export function nativeEntries(draft: ChefDraft, snapshot: ChefEditorSnapshot): ChefEntry[] {
  const native = new Set(snapshot.chefs.filter(chef => chef.source_kind === 'native').map(chef => chef.id))
  return usableEntries(draft, snapshot).filter(entry => entry.chef_ref.kind === 'known' && native.has(entry.chef_ref.chef_id))
}

export function nativeFallbackError(draft: ChefDraft, snapshot: ChefEditorSnapshot): ChefEditError | null {
  if (nativeEntries(draft, snapshot).length) return null
  return snapshot.target.supports_custom_default ? 'nativeFallbackRequired' : 'nativeDefaultRequired'
}

export function hasValidDefault(draft: ChefDraft, snapshot: ChefEditorSnapshot): boolean {
  const first = usableEntries(draft, snapshot)[0]
  return !!first && defaultEntries(draft, snapshot)[0] === first
}

export function chefSelectionError(draft: ChefDraft, snapshot: ChefEditorSnapshot): ChefEditError | null {
  return nativeFallbackError(draft, snapshot) ?? (hasValidDefault(draft, snapshot) ? null : 'nativeDefaultRequired')
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
  return state.draft.hat_bindings[chefId] ?? autoHat(snapshot)
}

function newEntry(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string): ChefEntry {
  // Re-enabling an original row keeps its metadata for lossless backend editing.
  const original = snapshot.selection.entries.find((entry) => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === chefId)
  return {
    entry_id: original?.entry_id ?? `draft:${chefId}`,
    row_id: original?.row_id ?? null,
    chef_ref: { kind: 'known', chef_id: chefId },
    hat_ref: { ...hatForChef(state, snapshot, chefId) },
  }
}

export function toggleChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string): ChefEditResult {
  const selected = entryForChef(state.draft, chefId)
  const error = toggleChefError(state.draft, snapshot, chefId)
  if (error) return { error }
  const next = copyEditState(state)
  next.draft.acknowledged_change_ids = []
  if (selected) {
    next.draft.entries = next.draft.entries.filter((entry) => entry.entry_id !== selected.entry_id)
  } else {
    next.draft.entries.push(newEntry(state, snapshot, chefId))
  }
  const fallbackError = nativeFallbackError(next.draft, snapshot)
  if (fallbackError) return { error: fallbackError }
  promoteDefault(next, snapshot)
  return { state: next }
}

export function toggleChefError(draft: ChefDraft, snapshot: ChefEditorSnapshot, chefId: string): ChefEditResult['error'] | null {
  const selected = entryForChef(draft, chefId)
  const chef = snapshot.chefs.find((item) => item.id === chefId)
  if (!chef) return 'unavailable'
  if (selected && chef.source_kind === 'native' && isUsableResource(chef.resource_state) && nativeEntries(draft, snapshot).length <= 1) {
    return snapshot.target.supports_custom_default ? 'nativeFallbackRequired' : 'nativeDefaultRequired'
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
  const fallbackError = nativeFallbackError(next.draft, snapshot)
  if (fallbackError) return { error: fallbackError }
  next.draft.acknowledged_change_ids = []
  return { state: next }
}

/** Both pointer and keyboard moves use full entries, preserving unresolved rows' relative order. */
export function moveChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefId: string, targetChefId: string, after?: boolean): ChefEditState {
  if (nativeFallbackError(state.draft, snapshot)) return state
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

/** Group order is frozen by the caller; validate the complete result, never an intermediate member. */
export function placeChef(state: ChefEditState, snapshot: ChefEditorSnapshot, chefIds: string | readonly string[], targetChefId: string | null, after: boolean): ChefEditResult {
  const ids = [...new Set(typeof chefIds === 'string' ? [chefIds] : chefIds)]
  if (!ids.length) return { state }
  const selected = new Set(ids)
  if (targetChefId === null && usableEntries(state.draft, snapshot).some(entry => entry.chef_ref.kind === 'known' && !selected.has(entry.chef_ref.chef_id))) return { state }
  if (ids.some(id => !snapshot.chefs.some(chef => chef.id === id && isUsableResource(chef.resource_state)))) return { error: 'unavailable' }
  if (targetChefId !== null && (selected.has(targetChefId) || !entryForChef(state.draft, targetChefId))) return { state }
  const next = copyEditState(state)
  const members = ids.map(id => entryForChef(next.draft, id) ?? newEntry(state, snapshot, id))
  next.draft.entries = next.draft.entries.filter(entry => entry.chef_ref.kind !== 'known' || !selected.has(entry.chef_ref.chef_id))
  const target = next.draft.entries.findIndex(entry => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === targetChefId)
  next.draft.entries.splice(target < 0 ? next.draft.entries.length : target + Number(after), 0, ...members)
  if (!usableEntries(next.draft, snapshot).length) return { error: 'lastChef' }
  const fallbackError = nativeFallbackError(next.draft, snapshot)
  if (fallbackError) return { error: fallbackError }
  if (!defaultEntries(next.draft, snapshot).length) return { error: 'nativeDefaultRequired' }
  promoteDefault(next, snapshot)
  if (draftSignature(next.draft) === draftSignature(state.draft)) return { state }
  next.draft.acknowledged_change_ids = []
  return { state: next }
}

/** Retain the current opening chef when needed, plus a native fallback, without changing retained order. */
export function disableChefs(state: ChefEditState, snapshot: ChefEditorSnapshot, chefIds: readonly string[]): ChefEditResult {
  const ids = new Set(chefIds), next = copyEditState(state), retained = new Set<string>()
  if (ids.size === 1) {
    const error = toggleChefError(state.draft, snapshot, chefIds[0]!)
    if (error) return { error }
  }
  next.draft.entries = next.draft.entries.filter(entry => entry.chef_ref.kind !== 'known' || !ids.has(entry.chef_ref.chef_id))
  if (!defaultEntries(next.draft, snapshot).length) {
    const current = usableEntries(state.draft, snapshot)[0]
    if (!current || !hasValidDefault(state.draft, snapshot)) return { error: snapshot.target.supports_custom_default ? 'lastChef' : 'nativeDefaultRequired' }
    retained.add(current.entry_id)
  }
  const natives = nativeEntries(state.draft, snapshot)
  if (!natives.length) return { error: nativeFallbackError(state.draft, snapshot)! }
  if (!nativeEntries(next.draft, snapshot).length && !natives.some(entry => retained.has(entry.entry_id))) retained.add(natives[0]!.entry_id)
  next.draft.entries = next.draft.entries.length || retained.size
    ? state.draft.entries.filter(entry => retained.has(entry.entry_id) || entry.chef_ref.kind !== 'known' || !ids.has(entry.chef_ref.chef_id)).map(copyEntry)
    : []
  const error = nativeFallbackError(next.draft, snapshot)
  if (error) return { error }
  const notice = nativeEntries(next.draft, snapshot).some(entry => entry.chef_ref.kind === 'known' && ids.has(entry.chef_ref.chef_id))
    ? 'nativeFallbackRetained' as const : undefined
  promoteDefault(next, snapshot)
  if (draftSignature(next.draft) === draftSignature(state.draft)) return { state, notice }
  next.draft.acknowledged_change_ids = []
  return { state: next, notice }
}

/** Bulk enable follows the disabled section's stable custom → native reading order. */
export function enableAllChefs(state: ChefEditState, snapshot: ChefEditorSnapshot): ChefEditResult {
  const ids = ['custom', 'native'].flatMap(source => snapshot.chefs.filter(chef => chef.source_kind === source
    && isUsableResource(chef.resource_state) && !entryForChef(state.draft, chef.id)).map(chef => chef.id))
  const last = [...state.draft.entries].reverse().find(entry => entry.chef_ref.kind === 'known')?.chef_ref
  return placeChef(state, snapshot, ids, last?.kind === 'known' ? last.chef_id : null, true)
}

export function disableAllChefs(state: ChefEditState, snapshot: ChefEditorSnapshot): ChefEditResult {
  if (!hasValidDefault(state.draft, snapshot)) return { error: 'nativeDefaultRequired' }
  const error = nativeFallbackError(state.draft, snapshot)
  if (error) return { error }
  const current = usableEntries(state.draft, snapshot)[0]!
  const keep = new Set([current.entry_id, nativeEntries(state.draft, snapshot)[0]!.entry_id])
  const next = copyEditState(state)
  next.draft.entries = next.draft.entries.filter(entry => keep.has(entry.entry_id) || entry.chef_ref.kind !== 'known')
  if (draftSignature(next.draft) === draftSignature(state.draft)) return { state }
  next.draft.acknowledged_change_ids = []
  return { state: next }
}

export function changeChefHat(state: ChefEditState, chefIds: string | readonly string[], hat: HatReference): ChefEditState {
  const next = copyEditState(state)
  const ids = new Set(typeof chefIds === 'string' ? [chefIds] : chefIds)
  for (const chefId of ids) next.draft.hat_bindings[chefId] = { ...hat }
  // Entries carry row metadata and an output mirror; bindings own each known chef's choice.
  for (const entry of next.draft.entries) {
    if (entry.chef_ref.kind === 'known' && ids.has(entry.chef_ref.chef_id)) entry.hat_ref = { ...next.draft.hat_bindings[entry.chef_ref.chef_id]! }
  }
  next.draft.acknowledged_change_ids = []
  return next
}

export function removeProblemRow(state: ChefEditState, snapshot: ChefEditorSnapshot, rowId: string): ChefEditResult {
  const next = copyEditState(state)
  next.draft.entries = next.draft.entries.filter((entry) => entry.row_id !== rowId)
  if (defaultEntries(state.draft, snapshot).length && !defaultEntries(next.draft, snapshot).length) return { error: snapshot.target.supports_custom_default ? 'lastChef' : 'nativeDefaultRequired' }
  if (!usableEntries(next.draft, snapshot).length) return { error: 'lastChef' }
  if (nativeEntries(state.draft, snapshot).length && !nativeEntries(next.draft, snapshot).length) return { error: nativeFallbackError(next.draft, snapshot)! }
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
  next.draft.hat_bindings = { [blackCat.id]: autoHat(snapshot) }
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
