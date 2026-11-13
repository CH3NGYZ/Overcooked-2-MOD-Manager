import assert from 'node:assert/strict'
import test from 'node:test'
import { createRenderer, nextTick, shallowRef } from 'vue'
import { useChefSelection } from '../src/composables/useChefSelection.ts'
import { chefDragMembers, chefSelectionRect, intersectsChef } from '../src/diychef/selection.ts'

const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const pointer = (x = 0, y = 0, extra = {}) => ({ target: { closest: () => null }, button: 0, isPrimary: true,
  pointerId: 7, clientX: x, clientY: y, ctrlKey: false, metaKey: false, preventDefault() {}, ...extra })

function setup(t) {
  const originals = new Map(), listeners = new Map(), frames = new Map()
  let serial = 0, selection
  const cards = [
    { dataset: { chefId: 'cat' }, getBoundingClientRect: () => ({ left: 10, top: 10, right: 80, bottom: 80 }) },
    { dataset: { chefId: 'fox' }, getBoundingClientRect: () => ({ left: 100, top: 10, right: 180, bottom: 80 }) },
    { dataset: { chefId: 'invalid' }, getBoundingClientRect: () => ({ left: 20, top: 20, right: 60, bottom: 60 }) },
  ]
  for (const [name, value] of Object.entries({
    window: { addEventListener(type, callback) { listeners.set(type, callback) },
      removeEventListener(type, callback) { if (listeners.get(type) === callback) listeners.delete(type) } },
    requestAnimationFrame: callback => { frames.set(++serial, callback); return serial },
    cancelAnimationFrame: id => frames.delete(id),
  })) {
    originals.set(name, Object.getOwnPropertyDescriptor(globalThis, name))
    Object.defineProperty(globalThis, name, { configurable: true, writable: true, value })
  }
  const options = { root: shallowRef({ querySelectorAll: () => cards }), enabled: shallowRef(true),
    filterKey: shallowRef(''), sessionId: shallowRef('session'), readingOrder: shallowRef(['cat', 'fox']),
    selectableIds: shallowRef(new Set(['cat', 'fox'])) }
  const app = renderer.createApp({ setup() { selection = useChefSelection(options); return () => null } })
  app.mount({})
  t.after(() => {
    app.unmount()
    for (const [name, descriptor] of originals) {
      if (descriptor) Object.defineProperty(globalThis, name, descriptor)
      else delete globalThis[name]
    }
  })
  return { selection, options, frames, listeners,
    dispatch: (type, event) => listeners.get(type)?.(event),
    flush() { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(callback => callback(0)) } }
}

test('selection end cancels queued rAF and stale callbacks cannot revive a rectangle or touch a new session', t => {
  const { selection, frames, dispatch, flush } = setup(t)
  selection.start(pointer())
  dispatch('pointermove', pointer(20, 20))
  const stale = [...frames.values()][0]
  assert.equal(frames.size, 1)
  dispatch('pointerup', pointer(20, 20))
  assert.deepEqual([...selection.selectedIds.value], ['cat'])
  assert.equal(selection.rectangle.value, null)
  assert.equal(frames.size, 0)
  stale()
  assert.equal(selection.rectangle.value, null)
  selection.start(pointer(90, 0))
  dispatch('pointermove', pointer(110, 20))
  stale()
  assert.equal(selection.rectangle.value, null)
  assert.equal(frames.size, 1)
  flush()
  assert.deepEqual([...selection.selectedIds.value], ['fox'])
})

test('rectangle starts at six pixels, selects intersecting valid cards, and blank clicks clear selection', t => {
  const { selection, dispatch, frames, flush } = setup(t)
  selection.toggle('fox')
  selection.start(pointer())
  dispatch('pointermove', pointer(5.9))
  assert.equal(frames.size, 0)
  assert.equal(selection.rectangle.value, null)
  dispatch('pointermove', pointer(6))
  assert.equal(frames.size, 1)
  dispatch('pointermove', pointer(10, 10))
  flush()
  assert.deepEqual([...selection.selectedIds.value], ['cat'], 'edge intersection is enough; invalid resources are excluded')
  dispatch('pointerup', pointer(10, 10))
  selection.start(pointer(200, 200))
  dispatch('pointerup', pointer(200, 200))
  assert.equal(selection.selectedIds.value.size, 0)
})

test('Ctrl and Meta add to selection, cancellation restores baseline, and filters clear selection', async t => {
  const { selection, options, dispatch, frames, flush } = setup(t)
  for (const modifier of ['ctrlKey', 'metaKey']) {
    selection.clear(); selection.toggle('fox')
    selection.start(pointer(0, 0, { [modifier]: true }))
    dispatch('pointermove', pointer(20, 20)); flush()
    assert.deepEqual([...selection.selectedIds.value], ['fox', 'cat'])
    dispatch('pointercancel', pointer())
    assert.deepEqual([...selection.selectedIds.value], ['fox'])
  }
  selection.start(pointer())
  dispatch('pointermove', pointer(20, 20))
  options.filterKey.value = 'filtered'
  assert.equal(frames.size, 0)
  assert.equal(selection.selectedIds.value.size, 0)
  selection.toggle('cat')
  options.enabled.value = false
  await nextTick()
  assert.equal(selection.selectedIds.value.size, 0)
  selection.toggle('cat')
  assert.equal(selection.selectedIds.value.size, 0)
})

test('group membership freezes in cross-section reading order and an unselected drag clears previous selection', t => {
  const { selection, options } = setup(t)
  options.readingOrder.value = ['cat', 'fox', 'owl', 'native']
  options.selectableIds.value = new Set(options.readingOrder.value)
  for (const id of ['native', 'owl', 'cat']) selection.toggle(id)
  assert.deepEqual(selection.members('owl'), ['cat', 'owl', 'native'])
  assert.deepEqual(selection.members('fox'), ['fox'])
  assert.equal(selection.selectedIds.value.size, 0)
  assert.deepEqual(chefDragMembers('owl', new Set(['native', 'owl']), options.readingOrder.value), ['owl', 'native'])
  assert.deepEqual(chefSelectionRect(20, 30, 10, 5), { left: 10, top: 5, right: 20, bottom: 30 })
  assert.equal(intersectsChef(chefSelectionRect(20, 30, 10, 5), { left: 20, top: 30, right: 40, bottom: 40 }), true)
})
