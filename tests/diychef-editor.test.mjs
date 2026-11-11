import assert from 'node:assert/strict'
import test from 'node:test'
import {
  changeChefHat, copyEditState, createChefDraft, draftSignature, hatForChef, moveChef, placeChef,
  rebaseInitializedDraft, removeProblemRow, resetChefDraft, setDefaultChef, toggleChef,
} from '../src/diychef/draft.ts'
import { projectChefs, projectProblems } from '../src/diychef/projection.ts'

const hat = (id = 'hat:') => ({ kind: 'known', hat_id: id })
const selected = (id, row = id, choice = hat()) => ({
  entry_id: `entry:${id}`, row_id: row,
  chef_ref: { kind: 'known', chef_id: id }, hat_ref: choice,
})
const missing = { entry_id: 'missing', row_id: 'missing', chef_ref: { kind: 'unresolved', raw_key: 'DeletedChef', row_id: 'missing' }, hat_ref: hat() }
const chef = (id, source = 'custom', resource = 'valid') => ({
  id, source_kind: source, display_name: id, config_name: id === 'cat' ? 'Chef_Black_cat' : id,
  head_name: id, custom_id: source === 'custom' ? 100 : null, resource_state: resource, preview_state: 'unavailable',
})
function fixture(entries = [selected('cat'), selected('fox')]) {
  const snapshot = {
    session_id: 'session',
    target: { id: 'target', guid: 'dev.gua.overcooked.diychef', version: '2.1.0', display_path: 'fixture', enabled: true, generation: 7, compatibility: 'read_only', supports_custom_default: false },
    source: { state: 'present', filename: 'prefer.txt', revision: 'revision', encoding: 'utf8', newline: 'crlf' },
    catalog_revision: 'catalog',
    chefs: [chef('cat', 'native', 'known_name'), chef('beard', 'native', 'known_name'), chef('fox'), chef('owl'), chef('broken', 'custom', 'invalid')],
    hats: [
      { id: 'hat:', kind: 'auto', config_value: '', display_name: 'auto', resource_state: 'valid', preview_state: 'unavailable' },
      { id: 'hat:None', kind: 'none', config_value: 'None', display_name: 'None', resource_state: 'valid', preview_state: 'unavailable' },
      { id: 'hat:Wizard', kind: 'custom', config_value: 'Wizard', display_name: 'Wizard', resource_state: 'valid', preview_state: 'valid' },
    ],
    document_rows: [
      { row_id: 'cat', category: 'chef', raw_display: 'Chef_Black_cat', duplicate_group_id: null },
      { row_id: 'fox', category: 'chef', raw_display: 'fox', duplicate_group_id: null },
      { row_id: 'missing', category: 'chef', raw_display: 'DeletedChef', duplicate_group_id: null },
      { row_id: 'duplicate', category: 'duplicate', raw_display: 'fox HAT=None', duplicate_group_id: 'fox' },
      { row_id: 'blank', category: 'blank', raw_display: '', duplicate_group_id: null },
    ],
    selection: { entries, lobby_switch: true }, diagnostics: [],
  }
  return { snapshot, state: { draft: createChefDraft(snapshot), disabledHats: {} } }
}
const unfiltered = { query: '', sources: [], statuses: [] }

test('DIYChef 2.2 supports custom opening chefs through setting, dragging and projection', () => {
  const { snapshot, state } = fixture()
  snapshot.target.supports_custom_default = true
  snapshot.target.version = '2.2.0'
  const direct = setDefaultChef(state, snapshot, 'fox')
  assert.equal(direct.error, undefined)
  assert.equal(projectChefs(snapshot, direct.state, unfiltered).defaultId, 'fox')
  const dragged = placeChef(state, snapshot, 'owl', 'cat', false)
  assert.equal(dragged.error, undefined)
  assert.equal(projectChefs(snapshot, dragged.state, unfiltered).defaultId, 'owl')
  assert.deepEqual(projectChefs(snapshot, state, unfiltered).enabled.map(item => item.chef.id), ['cat', 'fox'])
})

test('DIYChef 2.2 can disable its final native chef while retaining the final usable chef safeguard', () => {
  const { snapshot, state } = fixture()
  snapshot.target.supports_custom_default = true
  snapshot.target.version = '2.2.0'
  const next = toggleChef(state, snapshot, 'cat')
  assert.equal(next.error, undefined)
  assert.equal(projectChefs(snapshot, next.state, unfiltered).defaultId, 'fox')
  assert.equal(toggleChef(next.state, snapshot, 'fox').error, 'lastChef')
})

test('DIYChef 2.2 accepts custom-first files without reordering and can populate an empty selection', () => {
  const { snapshot, state } = fixture([selected('fox'), selected('cat')])
  snapshot.target.supports_custom_default = true
  snapshot.target.version = '2.2.0'
  assert.equal(projectChefs(snapshot, state, unfiltered).defaultId, 'fox')
  const empty = { ...state, draft: { ...state.draft, entries: [] } }
  const next = placeChef(empty, snapshot, 'owl', null, false)
  assert.equal(next.error, undefined)
  assert.equal(projectChefs(snapshot, next.state, unfiltered).defaultId, 'owl')
  assert.equal(setDefaultChef(state, snapshot, 'broken').error, 'unavailable')
})

test('the final usable native chef cannot be disabled even when custom chefs remain enabled', () => {
  const { snapshot, state } = fixture([missing, selected('cat'), selected('broken')])
  assert.equal(toggleChef(state, snapshot, 'cat').error, 'nativeDefaultRequired')
  const enabledFox = toggleChef(state, snapshot, 'fox').state
  assert.equal(toggleChef(enabledFox, snapshot, 'cat').error, 'nativeDefaultRequired')
  const legacy = fixture([selected('fox')])
  assert.equal(toggleChef(legacy.state, legacy.snapshot, 'fox').error, 'lastChef')
})

test('historical custom-first records retain document order without marking a custom chef as default', () => {
  const { snapshot, state } = fixture([missing, selected('fox'), selected('cat')])
  const projected = projectChefs(snapshot, state, unfiltered)
  assert.equal(projected.defaultId, null)
  assert.deepEqual(projected.enabled.map((item) => item.chef.id), ['fox', 'cat'])
  assert.deepEqual(state.draft.entries[0], missing)
})

test('setting a disabled native chef as default enables it first while custom defaults are rejected', () => {
  const { snapshot, state } = fixture()
  const next = setDefaultChef(state, snapshot, 'beard').state
  assert.deepEqual(next.draft.entries.map((entry) => entry.chef_ref.chef_id), ['beard', 'cat', 'fox'])
  assert.equal(projectChefs(snapshot, next, unfiltered).defaultId, 'beard')
  assert.equal(setDefaultChef(state, snapshot, 'owl').error, 'nativeDefaultRequired')
  assert.equal(setDefaultChef(state, snapshot, 'fox').error, 'nativeDefaultRequired')
  assert.equal(setDefaultChef(state, snapshot, 'broken').error, 'unavailable')
})

test('disabling the native default promotes the next enabled native ahead of custom chefs', () => {
  const { snapshot, state } = fixture([selected('cat'), selected('fox'), selected('beard'), selected('owl')])
  const next = toggleChef(state, snapshot, 'cat').state
  assert.deepEqual(next.draft.entries.map(entry => entry.chef_ref.chef_id), ['beard', 'fox', 'owl'])
  assert.equal(projectChefs(snapshot, next, unfiltered).defaultId, 'beard')
  assert.equal(toggleChef(next, snapshot, 'beard').error, 'nativeDefaultRequired')
})

test('disabled hat preferences never enable a chef, and follow-game differs from no-hat', () => {
  const { snapshot, state } = fixture()
  const next = changeChefHat(state, 'owl', hat('hat:None'))
  assert.equal(draftSignature(next.draft), draftSignature(state.draft))
  assert.deepEqual(hatForChef(next, snapshot, 'owl'), hat('hat:None'))
  const enabled = toggleChef(next, snapshot, 'owl').state
  assert.deepEqual(enabled.draft.entries.at(-1).hat_ref, hat('hat:None'))
  assert.equal(enabled.disabledHats.owl, undefined)
  assert.notDeepEqual(hatForChef(enabled, snapshot, 'owl'), hat())
})

test('changing all hats updates the full catalogue without enabling chefs or rewriting unresolved rows', () => {
  const { snapshot, state } = fixture([selected('cat'), missing, selected('fox', 'fox', hat('hat:None'))])
  state.disabledHats.owl = hat('hat:None')
  state.draft.acknowledged_change_ids = ['confirmed']
  const before = structuredClone(state)
  const choice = hat('hat:Wizard')
  const next = changeChefHat(state, snapshot.chefs.map(chef => chef.id), choice)
  assert.deepEqual(next.draft.entries.map(entry => entry.chef_ref), state.draft.entries.map(entry => entry.chef_ref))
  assert.deepEqual(next.draft.entries[1], missing)
  for (const chef of snapshot.chefs) assert.deepEqual(hatForChef(next, snapshot, chef.id), choice)
  assert.deepEqual(next.draft.acknowledged_change_ids, [])
  assert.deepEqual(state, before)
  assert.notEqual(next.draft.entries[0].hat_ref, next.draft.entries[2].hat_ref)
  choice.hat_id = 'hat:None'
  assert.deepEqual(next.disabledHats.owl, hat('hat:Wizard'))
  const enabled = toggleChef(next, snapshot, 'owl').state
  assert.deepEqual(enabled.draft.entries.at(-1).hat_ref, hat('hat:Wizard'))
})

test('toggle round-trip restores original row and hat metadata without a phantom dirty state', () => {
  const { snapshot, state } = fixture([selected('cat'), selected('fox', 'fox', hat('hat:Wizard'))])
  const removed = toggleChef(state, snapshot, 'fox').state
  const restored = toggleChef(removed, snapshot, 'fox').state
  assert.equal(draftSignature(restored.draft), draftSignature(state.draft))
  assert.equal(restored.draft.entries.at(-1).row_id, 'fox')
  assert.deepEqual(restored.draft.entries.at(-1).hat_ref, hat('hat:Wizard'))
})

test('bulk reset clears problem/duplicate rows and old black-cat hat while preserving full undo data', () => {
  const { snapshot, state } = fixture([selected('cat', 'cat', hat('hat:Wizard')), missing, selected('fox')])
  state.disabledHats.owl = hat('hat:None')
  state.draft.acknowledged_change_ids = ['old-ack']
  const undo = copyEditState(state)
  const kept = resetChefDraft(state, snapshot, 'keep_black_cat')
  assert.equal(kept.draft.lobby_switch, true)
  assert.equal(kept.draft.entries.length, 1)
  assert.deepEqual(kept.draft.entries[0].hat_ref, hat())
  assert.ok(kept.draft.removed_problem_row_ids.includes('missing'))
  assert.ok(kept.draft.removed_problem_row_ids.includes('duplicate'))
  assert.ok(!kept.draft.removed_problem_row_ids.includes('blank'))
  assert.deepEqual(kept.disabledHats, {})
  assert.deepEqual(kept.draft.acknowledged_change_ids, [])
  assert.deepEqual(state, undo)
  const reset = resetChefDraft(state, snapshot, 'restore_default')
  assert.equal(reset.draft.lobby_switch, true)
  assert.equal(reset.draft.reset_intent, 'restore_default')
  const lobbyOff = copyEditState(state)
  lobbyOff.draft.lobby_switch = false
  assert.equal(resetChefDraft(lobbyOff, snapshot, 'restore_default').draft.lobby_switch, true)
  assert.equal(resetChefDraft(lobbyOff, snapshot, 'keep_black_cat').draft.lobby_switch, false)
  assert.equal(lobbyOff.draft.lobby_switch, false, 'resets preserve the original state for undo')
})

test('missing-file initialization updates credentials while retaining edited order and session hats', () => {
  const { snapshot, state } = fixture()
  const edited = setDefaultChef(changeChefHat(state, 'owl', hat('hat:None')), snapshot, 'beard').state
  const initialized = { ...snapshot, source: { ...snapshot.source, revision: 'created' }, catalog_revision: 'current', target: { ...snapshot.target, generation: 8 } }
  const rebased = rebaseInitializedDraft(edited, initialized)
  assert.equal(rebased.draft.base_revision, 'created')
  assert.equal(rebased.draft.catalog_revision, 'current')
  assert.equal(rebased.draft.target_generation, 8)
  assert.deepEqual(rebased.draft.entries, edited.draft.entries)
  assert.deepEqual(rebased.disabledHats, edited.disabledHats)
  assert.notEqual(draftSignature(rebased.draft), draftSignature(createChefDraft(snapshot)))
})

test('search and source/status tags project the catalogue and retain the filtered state used by drag gating', () => {
  const { snapshot, state } = fixture()
  const before = copyEditState(state)
  const projected = projectChefs(snapshot, state, { query: 'FOX', sources: ['custom'], statuses: ['enabled'] })
  assert.deepEqual(projected.enabled.map((item) => item.chef.id), ['fox'])
  assert.equal(projected.filtered, true)
  assert.equal(projected.defaultHidden, true)
  assert.equal(projected.defaultId, 'cat')
  assert.deepEqual(projected.disabled, [])
  assert.deepEqual(state, before)
  const rawName = projectChefs(snapshot, state, { ...unfiltered, query: 'Chef_Black' })
  assert.deepEqual(rawName.enabled.map((item) => item.chef.id), ['cat'])
  assert.equal(rawName.filtered, true)
})

test('one source tag and one status tag intersect without hiding other catalogue data from the draft', () => {
  const { snapshot, state } = fixture()
  const before = copyEditState(state)
  for (const [source, status, enabled, disabled] of [
    ['native', 'enabled', ['cat'], []],
    ['native', 'disabled', [], ['beard']],
    ['custom', 'enabled', ['fox'], []],
    ['custom', 'disabled', [], ['owl', 'broken']],
  ]) {
    const projected = projectChefs(snapshot, state, { query: '', sources: [source], statuses: [status] })
    const context = `${source} and ${status}`
    assert.deepEqual(projected.enabled.map(item => item.chef.id), enabled, context)
    assert.deepEqual(projected.disabled.map(item => item.chef.id), disabled, context)
    assert.equal(projected.enabledCount, 2, context)
    assert.equal(projected.defaultId, 'cat', context)
    assert.equal(projected.defaultHidden, !enabled.includes('cat'), context)
    assert.equal(projected.filtered, true, context)
  }
  assert.deepEqual(state, before)
})

test('tags within each group are alternatives while any selected tags mark the projection filtered', () => {
  const { snapshot, state } = fixture()
  for (const [sources, statuses, enabled, disabled] of [
    [['native', 'custom'], ['enabled'], ['cat', 'fox'], []],
    [['custom'], ['enabled', 'disabled'], ['fox'], ['owl', 'broken']],
    [['native', 'custom'], ['enabled', 'disabled'], ['cat', 'fox'], ['beard', 'owl', 'broken']],
    [['native', 'custom'], [], ['cat', 'fox'], ['beard', 'owl', 'broken']],
    [[], ['enabled', 'disabled'], ['cat', 'fox'], ['beard', 'owl', 'broken']],
  ]) {
    const filters = { query: '', sources, statuses }
    const projected = projectChefs(snapshot, state, filters)
    const context = JSON.stringify(filters)
    assert.deepEqual(projected.enabled.map(item => item.chef.id), enabled, context)
    assert.deepEqual(projected.disabled.map(item => item.chef.id), disabled, context)
    assert.equal(projected.filtered, true, context)
  }
})

test('clearing tags and search restores the complete projection and ordering without losing draft entries or session hats', () => {
  const { snapshot, state } = fixture([selected('cat'), missing, selected('fox', 'fox', hat('hat:Wizard')), selected('beard')])
  state.disabledHats.owl = hat('hat:None')
  state.draft.acknowledged_change_ids = ['confirmed']
  const before = copyEditState(state)
  const originalSnapshot = structuredClone(snapshot)
  const complete = projectChefs(snapshot, state, unfiltered)
  const filters = { query: 'OWL', sources: ['custom'], statuses: ['disabled'] }
  const narrowed = projectChefs(snapshot, state, filters)
  assert.deepEqual(narrowed.enabled, [])
  assert.deepEqual(narrowed.disabled.map(item => item.chef.id), ['owl'])
  assert.equal(narrowed.filtered, true)
  filters.query = ''
  filters.sources = []
  filters.statuses = []
  const restored = projectChefs(snapshot, state, filters)
  assert.deepEqual(restored, complete)
  assert.deepEqual(restored.enabled.map(item => item.chef.id), ['cat', 'fox', 'beard'])
  assert.deepEqual(restored.disabled.map(item => item.chef.id), ['owl', 'broken'])
  assert.equal(restored.filtered, false)
  assert.deepEqual(state, before)
  assert.deepEqual(snapshot, originalSnapshot)
})

test('moving chefs uses the full sequence while preserving unresolved relative order', () => {
  const otherMissing = { ...missing, entry_id: 'missing2', row_id: 'missing2', chef_ref: { kind: 'unresolved', raw_key: 'OtherDeleted', row_id: 'missing2' } }
  const { snapshot, state } = fixture([selected('cat'), missing, selected('fox'), otherMissing, selected('owl')])
  const next = moveChef(state, snapshot, 'owl', 'cat')
  assert.deepEqual(projectChefs(snapshot, next, unfiltered).enabled.map((item) => item.chef.id), ['cat', 'owl', 'fox'])
  assert.deepEqual(next.draft.entries.filter((entry) => entry.chef_ref.kind === 'unresolved').map((entry) => entry.row_id), ['missing', 'missing2'])
  assert.equal(moveChef(state, snapshot, 'absent', 'fox'), state)
})

test('dropping a disabled custom chef inserts in one edit and clamps a front drop behind the native default', () => {
  for (const [target, after, order] of [
    ['cat', false, ['cat', 'owl', 'fox']],
    ['cat', true, ['cat', 'owl', 'fox']],
    ['fox', false, ['cat', 'owl', 'fox']],
    ['fox', true, ['cat', 'fox', 'owl']],
  ]) {
    const { snapshot, state } = fixture()
    state.disabledHats.owl = hat('hat:Wizard')
    state.disabledHats.broken = hat('hat:None')
    state.draft.acknowledged_change_ids = ['previous-confirmation']
    const before = structuredClone(state)
    const next = placeChef(state, snapshot, 'owl', target, after).state
    const context = `${after ? 'after' : 'before'} ${target}`
    assert.ok(next, context)
    assert.deepEqual(next.draft.entries.map(entry => entry.chef_ref.chef_id), order, context)
    assert.equal(projectChefs(snapshot, next, unfiltered).defaultId, order[0], context)
    const inserted = next.draft.entries.find(entry => entry.chef_ref.chef_id === 'owl')
    assert.equal(inserted.entry_id, 'draft:owl')
    assert.equal(inserted.row_id, null)
    assert.deepEqual(inserted.hat_ref, hat('hat:Wizard'))
    assert.deepEqual(next.disabledHats, { broken: hat('hat:None') })
    assert.deepEqual(next.draft.acknowledged_change_ids, [])
    assert.deepEqual(state, before, context)
  }
})

test('dropping a previously disabled row restores its metadata and most recent hat without sharing draft objects', () => {
  const { snapshot, state } = fixture([selected('cat'), selected('owl', 'original-owl-row', hat('hat:Wizard')), selected('fox')])
  const disabled = changeChefHat(toggleChef(state, snapshot, 'owl').state, 'owl', hat('hat:None'))
  const before = structuredClone(disabled)
  const originalSnapshot = structuredClone(snapshot)
  const next = placeChef(disabled, snapshot, 'owl', 'fox', false).state
  const restored = next.draft.entries[1]
  assert.deepEqual(next.draft.entries.map(entry => entry.chef_ref.chef_id), ['cat', 'owl', 'fox'])
  assert.equal(restored.entry_id, 'entry:owl')
  assert.equal(restored.row_id, 'original-owl-row')
  assert.deepEqual(restored.hat_ref, hat('hat:None'))
  assert.equal(next.disabledHats.owl, undefined)
  restored.hat_ref.hat_id = 'hat:Wizard'
  next.draft.entries[0].chef_ref.chef_id = 'edited-result-only'
  next.draft.removed_problem_row_ids.push('result-only')
  assert.deepEqual(disabled, before)
  assert.deepEqual(snapshot, originalSnapshot)
})

test('enabled chefs can move before or after targets in both directions without duplicating rows or changing hats', () => {
  const { snapshot, state } = fixture([selected('cat'), selected('fox', 'fox-row', hat('hat:Wizard')), selected('owl')])
  const before = structuredClone(state)
  for (const [from, target, after, order] of [
    ['cat', 'fox', false, ['cat', 'fox', 'owl']],
    ['cat', 'fox', true, ['cat', 'fox', 'owl']],
    ['cat', 'owl', false, ['cat', 'fox', 'owl']],
    ['cat', 'owl', true, ['cat', 'fox', 'owl']],
    ['owl', 'cat', false, ['cat', 'owl', 'fox']],
    ['owl', 'cat', true, ['cat', 'owl', 'fox']],
    ['owl', 'fox', false, ['cat', 'owl', 'fox']],
    ['owl', 'fox', true, ['cat', 'fox', 'owl']],
  ]) {
    const next = moveChef(state, snapshot, from, target, after)
    assert.deepEqual(next.draft.entries.map(entry => entry.chef_ref.chef_id), order, `${from} ${after ? 'after' : 'before'} ${target}`)
    assert.deepEqual(placeChef(state, snapshot, from, target, after).state, next)
    assert.deepEqual(next.draft.entries.find(entry => entry.chef_ref.chef_id === 'fox'), selected('fox', 'fox-row', hat('hat:Wizard')))
    assert.deepEqual(state, before)
  }
  assert.deepEqual(moveChef(state, snapshot, 'cat', 'fox').draft.entries.map(entry => entry.chef_ref.chef_id), ['cat', 'fox', 'owl'])
  assert.deepEqual(moveChef(state, snapshot, 'owl', 'fox').draft.entries.map(entry => entry.chef_ref.chef_id), ['cat', 'owl', 'fox'])
})

test('native source metadata determines who may take the first slot, independently of chef id spelling', () => {
  const { snapshot, state } = fixture([selected('cat'), selected('fox'), selected('beard')])
  const promoted = moveChef(state, snapshot, 'beard', 'cat', false)
  assert.deepEqual(promoted.draft.entries.map(entry => entry.chef_ref.chef_id), ['beard', 'cat', 'fox'])
  const movedAway = moveChef(state, snapshot, 'cat', 'beard', true)
  assert.deepEqual(movedAway.draft.entries.map(entry => entry.chef_ref.chef_id), ['beard', 'fox', 'cat'])
  const metadataOnly = { ...snapshot, chefs: snapshot.chefs.map(item => item.id === 'fox' ? { ...item, source_kind: 'native' } : item) }
  assert.equal(projectChefs(metadataOnly, setDefaultChef(state, metadataOnly, 'fox').state, unfiltered).defaultId, 'fox')
  assert.equal(setDefaultChef(state, snapshot, 'fox').error, 'nativeDefaultRequired')
})

test('invalid or disappeared drop targets cannot enable a chef or alter existing draft state', () => {
  const { snapshot, state } = fixture([selected('cat'), missing, selected('fox')])
  state.disabledHats.owl = hat('hat:Wizard')
  state.draft.acknowledged_change_ids = ['unchanged-confirmation']
  const before = structuredClone(state)
  for (const target of ['', 'absent', 'owl', 'broken', 'missing']) {
    assert.equal(placeChef(state, snapshot, 'owl', target, false).state, state, target)
    assert.equal(moveChef(state, snapshot, 'cat', target, true), state, target)
  }
  for (const source of ['', 'absent', 'broken']) {
    assert.equal(placeChef(state, snapshot, source, 'cat', true).error, 'unavailable', source)
    assert.equal(moveChef(state, snapshot, source, 'cat', true), state, source)
  }
  assert.equal(placeChef(state, snapshot, 'cat', 'cat', true).state, state)
  assert.equal(moveChef(state, snapshot, 'cat', 'cat', false), state)
  assert.deepEqual(state, before)
})

test('dropping into an empty usable selection accepts a native chef and rejects a custom default', () => {
  for (const entries of [[], [missing], [missing, selected('broken')]]) {
    const { snapshot, state } = fixture(entries)
    state.disabledHats.beard = hat('hat:None')
    const before = structuredClone(state)
    const next = placeChef(state, snapshot, 'beard', null, false).state
    assert.ok(next)
    assert.deepEqual(next.draft.entries.slice(0, -1), state.draft.entries)
    assert.deepEqual(next.draft.entries.at(-1), {
      entry_id: 'draft:beard', row_id: null, chef_ref: { kind: 'known', chef_id: 'beard' }, hat_ref: hat('hat:None'),
    })
    assert.equal(projectChefs(snapshot, next, unfiltered).defaultId, 'beard')
    assert.deepEqual(next.disabledHats, {})
    assert.deepEqual(state, before)
    assert.equal(placeChef(state, snapshot, 'owl', null, false).error, 'nativeDefaultRequired')
    assert.equal(placeChef(state, snapshot, 'absent', null, false).error, 'unavailable')
  }
})

test('a null drop target is a no-op whenever an existing usable chef still occupies the enabled section', () => {
  const { snapshot, state } = fixture([missing, selected('cat')])
  state.disabledHats.owl = hat('hat:Wizard')
  const before = structuredClone(state)
  for (const source of ['owl', 'cat', 'absent']) {
    assert.equal(placeChef(state, snapshot, source, null, true).state, state)
  }
  assert.deepEqual(state, before)
})

test('problem removal preserves the last usable native and exposes only remaining problem rows', () => {
  const { snapshot, state } = fixture([missing, selected('cat')])
  assert.equal(removeProblemRow(state, snapshot, 'cat').error, 'nativeDefaultRequired')
  const next = removeProblemRow(state, snapshot, 'missing').state
  const problems = projectProblems(snapshot, next, [])
  assert.ok(problems.every((problem) => problem.row.row_id !== 'missing'))
  assert.ok(problems.some((problem) => problem.row.row_id === 'duplicate'))
})

test('dirty signature ignores baseline/confirmation credentials but notices an intentional reset', () => {
  const { snapshot, state } = fixture([selected('cat')])
  const baseline = draftSignature(state.draft)
  const acknowledged = copyEditState(state)
  acknowledged.draft.acknowledged_change_ids = ['confirmed']
  acknowledged.draft.base_revision = 'new-revision'
  assert.equal(draftSignature(acknowledged.draft), baseline)
  assert.notEqual(draftSignature(resetChefDraft(state, snapshot, 'keep_black_cat').draft), baseline)
})
