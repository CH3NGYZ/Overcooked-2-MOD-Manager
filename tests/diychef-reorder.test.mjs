import assert from 'node:assert/strict'
import test from 'node:test'
import { chefDropTarget } from '../src/diychef/reorder.ts'

const card = (id, left, top = 0, width = 128, isDefault = false) => ({ id, left, top, width, height: 220, isDefault })
const positions = [card('default', 0, 0, 268, true), card('a', 280), card('b', 420), card('c', 560)]
function target(id, deltaX, deltaY = 0, overrides = {}) {
  const source = positions.find(position => position.id === id)
  return chefDropTarget({ sourceId: id, source, preserveDefault: false, positions, deltaX, deltaY, scrollShift: 0,
    pointerX: source.left + source.width / 2 + deltaX, pointerY: source.top + source.height / 2 + deltaY, ...overrides })
}

test('one pixel of overlap exchanges adjacent cards before either pointer or card center reaches the neighbour', () => {
  assert.deepEqual(target('a', 13), { id: 'b', after: true })
  assert.deepEqual(target('b', -13), { id: 'a', after: false })
  assert.ok(280 + 64 + 13 < 420, 'pointer is still to the left of the neighbour')
  assert.ok(420 + 64 - 13 > 408, 'pointer is still to the right of the neighbour')
})

test('a stationary card and a move within the gap retain the source slot', () => {
  assert.equal(target('a', 0).id, 'a')
  assert.equal(target('a', 12).id, 'a', 'touching edges without area is not overlap')
  assert.equal(target('b', -12).id, 'b')
})

test('the card bounds determine an overlap even when grabbed far from the center', () => {
  assert.deepEqual(target('a', 13, 0, { pointerX: 294, pointerY: 30 }), { id: 'b', after: true })
  assert.deepEqual(target('b', -13, 0, { pointerX: 534, pointerY: 190 }), { id: 'a', after: false })
})

test('overlap in either row and direction uses reading order for insertion', () => {
  const rows = [card('a', 0), card('b', 140), card('c', 0, 232), card('d', 140, 232)]
  assert.deepEqual(target('a', 0, 13, { positions: rows, source: rows[0], pointerX: 64, pointerY: 123 }), { id: 'c', after: true })
  assert.deepEqual(target('c', 0, -13, { positions: rows, source: rows[2], pointerX: 64, pointerY: 329 }), { id: 'a', after: false })
})

test('auto-scrolling shifts cached targets but not the floating card', () => {
  const rows = [card('a', 0), card('b', 0, 232)]
  const input = { positions: rows, source: rows[0], scrollShift: 100, pointerX: 64, pointerY: 10 }
  assert.deepEqual(target('a', 0, -87, input), { id: 'b', after: true })
  assert.equal(target('a', 0, -88, input).id, 'a')
})

test('when multiple cards overlap the largest intersection wins', () => {
  assert.deepEqual(target('a', 215), { id: 'c', after: true })
  assert.deepEqual(target('c', -215), { id: 'a', after: false })
})

test('disabled chefs insert by the floating center and preserve a restricted default only when requested', () => {
  const source = card('disabled', 280, 400)
  assert.deepEqual(target('a', 120, -400, { source, sourceId: source.id, pointerX: 464, pointerY: 110 }), { id: 'b', after: false })
  assert.deepEqual(target('a', -270, -400, { source, sourceId: source.id, preserveDefault: true, pointerX: 74, pointerY: 110 }), { id: 'default', after: true })
  assert.deepEqual(target('a', -270, -400, { source, sourceId: source.id, pointerX: 74, pointerY: 110 }), { id: 'default', after: false })
})

test('an empty enabled lane has no target, leaving native-default validation to the draft', () => {
  assert.equal(target('a', 0, 0, { positions: [] }), null)
})
