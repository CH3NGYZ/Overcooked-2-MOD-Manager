import type { ChefDiagnostic, ChefDocumentRow, ChefEditorSnapshot, ChefEntry, ChefItem, HatReference } from './types.ts'
import { hatForChef, hasValidDefault, toggleChefError, usableEntries, type ChefEditState } from './draft.ts'

export interface ChefFilters {
  query: string
  sources: Array<'native' | 'custom'>
  statuses: Array<'enabled' | 'disabled'>
}
export interface ChefTileItem {
  chef: ChefItem
  enabled: boolean
  isDefault: boolean
  entry: ChefEntry | null
  hat: HatReference
  canToggle: boolean
  toggleError: 'lastChef' | 'nativeDefaultRequired' | 'unavailable' | null
}
export interface ChefProblem {
  row: ChefDocumentRow
  diagnostics: ChefDiagnostic[]
  chefId: string | null
}

export function hasChefFilters(filters: ChefFilters): boolean {
  return !!filters.query.trim() || filters.sources.length > 0 || filters.statuses.length > 0
}

export function projectChefs(snapshot: ChefEditorSnapshot, state: ChefEditState, filters: ChefFilters) {
  const chefById = new Map(snapshot.chefs.map((chef) => [chef.id, chef]))
  const knownEntries = state.draft.entries.filter((entry) => entry.chef_ref.kind === 'known')
  const selected = new Set(knownEntries.map((entry) => entry.chef_ref.kind === 'known' ? entry.chef_ref.chef_id : ''))
  const usable = usableEntries(state.draft, snapshot)
  const first = usable[0]?.chef_ref
  const defaultId = first?.kind === 'known' && hasValidDefault(state.draft, snapshot) ? first.chef_id : null
  const filtered = hasChefFilters(filters)
  const query = filters.query.trim().toLocaleLowerCase()
  const matches = (chef: ChefItem, enabled: boolean) => {
    if (filters.sources.length && !filters.sources.some(source => source === chef.source_kind)) return false
    if (filters.statuses.length && !filters.statuses.includes(enabled ? 'enabled' : 'disabled')) return false
    return !query || [chef.display_name, chef.config_name, chef.head_name].some((text) => text.toLocaleLowerCase().includes(query))
  }
  const tile = (chef: ChefItem, entry: ChefEntry | null): ChefTileItem => {
    const toggleError = toggleChefError(state.draft, snapshot, chef.id) ?? null
    return {
    chef, entry, enabled: !!entry, isDefault: chef.id === defaultId,
    hat: hatForChef(state, snapshot, chef.id),
    canToggle: !toggleError, toggleError,
    }
  }
  const enabled = knownEntries.flatMap((entry) => {
    const chef = entry.chef_ref.kind === 'known' ? chefById.get(entry.chef_ref.chef_id) : undefined
    return chef && matches(chef, true) ? [tile(chef, entry)] : []
  })
  const disabled = snapshot.chefs.filter((chef) => !selected.has(chef.id) && matches(chef, false)).map((chef) => tile(chef, null))
  const filterCounts = snapshot.chefs.reduce((counts, chef) => {
    if (chef.source_kind === 'native' || chef.source_kind === 'custom') counts[chef.source_kind] += 1
    counts[selected.has(chef.id) ? 'enabled' : 'disabled'] += 1
    return counts
  }, { native: 0, custom: 0, enabled: 0, disabled: 0 })
  return {
    enabled, disabled, defaultId, filterCounts, defaultChef: defaultId ? chefById.get(defaultId) ?? null : null,
    defaultHidden: !!defaultId && !enabled.some((item) => item.isDefault),
    enabledCount: knownEntries.length, filtered,
  }
}

export function projectProblems(snapshot: ChefEditorSnapshot, state: ChefEditState, diagnostics: ChefDiagnostic[]): ChefProblem[] {
  const removed = new Set(state.draft.removed_problem_row_ids)
  const unresolved = new Set(state.draft.entries.filter((entry) => entry.chef_ref.kind === 'unresolved' || entry.hat_ref.kind === 'unresolved').map((entry) => entry.row_id))
  return snapshot.document_rows.filter((row) => !removed.has(row.row_id) && (
    row.category === 'duplicate' || unresolved.has(row.row_id) || diagnostics.some((item) => item.row_id === row.row_id)
  )).map((row) => {
    const entry = state.draft.entries.find((item) => item.row_id === row.row_id)
    return { row, diagnostics: diagnostics.filter((item) => item.row_id === row.row_id), chefId: entry?.chef_ref.kind === 'known' ? entry.chef_ref.chef_id : null }
  })
}
