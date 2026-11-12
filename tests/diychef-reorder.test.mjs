import assert from 'node:assert/strict'
import test from 'node:test'
import { CHEF_SWAP_OVERLAP, chefDropTarget } from '../src/diychef/reorder.ts'

const card = (id, left, top = 0, isDefault = false) => ({ id, left, top, width: 128, height: 220, isDefault })
const positions = [card('default', 0, 0, true), card('a', 140), card('b', 280), card('c', 420)]
function target(id, deltaX, deltaY = 0, overrides = {}) {
  const source = positions.find(position => position.id === id)
  return chefDropTarget({ sourceId: id, source, preserveDefault: false, positions, deltaX, deltaY, scrollShift: 0,
    pointerX: source.left + source.width / 2 + deltaX, pointerY: source.top + source.height / 2 + deltaY, ...overrides })
}

test('50 percent of the smaller area is the single threshold in both directions', () => {
  assert.equal(CHEF_SWAP_OVERLAP, .5)
  assert.equal(target('a', 12 + 128 * .499), null)
  assert.equal(target('b', -(12 + 128 * .499)), null)
  assert.deepEqual(target('a', 76), { id: 'b', after: true })
  assert.deepEqual(target('b', -76), { id: 'a', after: false })
})

test('stationary cards and touching edges retain their source slot', () => {
  assert.equal(target('a', 0).id, 'a')
  assert.equal(target('a', 12).id, 'a')
  assert.equal(target('b', -12).id, 'b')
})

test('a pointer leading the card cannot bypass the area threshold', () => {
  assert.equal(target('a', 13, 0, { pointerX: 300, pointerY: 30 }), null)
  assert.equal(target('a', 12, 0, { pointerX: 300, pointerY: 30 }), null)
  assert.equal(target('a', 12 + 128 * .499, 0, { pointerX: 300, pointerY: 30 }), null)
  assert.deepEqual(target('a', 76, 0, { pointerX: 186, pointerY: 30 }), { id: 'b', after: true })
  const source = card('disabled', 140, 400)
  assert.equal(target('a', 140, -600, { source, sourceId: source.id, pointerX: 300, pointerY: 30 }), null)
})

test('subthreshold overlap retains an existing candidate without removing default protection', () => {
  assert.deepEqual(target('a', 13, 0, { currentTarget: { id: 'b', after: true } }), { id: 'b', after: true })
  assert.deepEqual(target('a', 12 + 128 * .499, 0, { currentTarget: { id: 'b', after: false } }), { id: 'b', after: false })
  assert.deepEqual(target('a', -13, 0, { preserveDefault: true, currentTarget: { id: 'default', after: false } }), { id: 'default', after: true })
  assert.equal(target('a', 13, 0, { currentTarget: { id: 'missing', after: true } }), null)
})

test('vertical overlap uses the same area threshold and reading order across rows', () => {
  const rows = [card('a', 0), card('b', 140), card('c', 0, 232), card('d', 140, 232)]
  assert.equal(target('a', 0, 12 + 220 * .499, { positions: rows, source: rows[0] }), null)
  assert.deepEqual(target('a', 0, 122, { positions: rows, source: rows[0] }), { id: 'c', after: true })
  assert.deepEqual(target('c', 0, -122, { positions: rows, source: rows[2] }), { id: 'a', after: false })
})

test('auto-scrolling shifts cached targets but not the floating card', () => {
  const rows = [card('a', 0), card('b', 0, 232)]
  const input = { positions: rows, source: rows[0], scrollShift: 100, pointerX: 64, pointerY: 10 }
  assert.deepEqual(target('a', 0, 22, input), { id: 'b', after: true })
  assert.equal(target('a', 0, 21.9, input), null)
})

test('the largest qualified intersection wins and smaller bounds define the ratio', () => {
  assert.deepEqual(target('a', 216), { id: 'c', after: true })
  assert.deepEqual(target('c', -216), { id: 'a', after: false })
  const source = { ...positions[1], width: 64 }
  assert.equal(target('a', 107.9, 0, { source }), null)
  assert.deepEqual(target('a', 108, 0, { source }), { id: 'b', after: true })
})

test('disabled chefs insert by floating center and restricted defaults remain protected', () => {
  const source = card('disabled', 140, 400)
  assert.deepEqual(target('a', 120, -400, { source, sourceId: source.id }), { id: 'b', after: false })
  assert.deepEqual(target('a', -130, -400, { source, sourceId: source.id, preserveDefault: true }), { id: 'default', after: true })
  assert.deepEqual(target('a', -130, -400, { source, sourceId: source.id }), { id: 'default', after: true })
  assert.deepEqual(target('a', -150, -400, { source, sourceId: source.id }), { id: 'default', after: false })
})

test('entering the enabled lane inserts below 50 percent, then lane swaps keep the threshold', () => {
  const source = card('disabled', 140, 400)
  const input = { source, sourceId: source.id, pointerX: 204, pointerY: 215 }
  assert.equal(target('a', 0, -190, input), null)
  const entering = target('a', 0, -190, { ...input, allowRegionEntry: true })
  assert.deepEqual(entering, { id: 'a', after: false })
  assert.deepEqual(target('a', 0, -190, { ...input, currentTarget: entering }), entering)
  assert.equal(target('a', 140, -190, { ...input, pointerX: 344 }), null)
  assert.deepEqual(target('a', 140, -190, { ...input, pointerX: 344, currentTarget: entering }), entering)
})

test('a region entry with a leading pointer reserves its nearest slot while protecting a legacy default', () => {
  const source = card('disabled', 140, 400)
  const input = { source, sourceId: source.id, pointerX: 5, pointerY: 215, preserveDefault: true }
  assert.equal(target('a', 140, -100, input), null)
  assert.deepEqual(target('a', 140, -100, { ...input, allowRegionEntry: true }), { id: 'default', after: true })
})

test('a genuinely empty area permits nearest-slot insertion; an empty lane has no target', () => {
  assert.deepEqual(target('a', 0, 300), { id: 'a', after: false })
  assert.equal(target('a', 0, 0, { positions: [] }), null)
})
