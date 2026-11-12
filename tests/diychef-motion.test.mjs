import assert from 'node:assert/strict'
import test from 'node:test'
import { createRenderer, nextTick, shallowRef } from 'vue'
import { registerHooks } from 'node:module'
import { box, browserBoundary, chef, ElementBoundary } from './helpers/chef-motion-dom.mjs'
import { CHEF_MOVE_MS, CHEF_PREVIEW_MS, cloneChefCard, createChefCardMotion } from '../src/diychef/cardMotion.ts'
import { useChefMotion } from '../src/composables/useChefMotion.ts'
import { chefDropTarget } from '../src/diychef/reorder.ts'

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
    try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
  }
  return next(specifier, context)
} })
const { useChefReorder } = await import('../src/composables/useChefReorder.ts')
const renderer = createRenderer({ patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null })
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
function mounted(t, factory) {
  let value
  const app = renderer.createApp({ setup() { value = factory(); return () => null } })
  app.mount({})
  let stopped = false
  const stop = () => { if (!stopped) { stopped = true; app.unmount() } }
  t.chefMotionCleanups.unshift(stop)
  return { value, stop }
}
function setup(t) {
  const browser = browserBoundary(t), root = new ElementBoundary('section', box(30, 80, 800, 1000))
  browser.body.append(root)
  const lane = createChefCardMotion(() => root)
  t.chefMotionCleanups.unshift(lane.stop)
  return { ...browser, root, lane }
}
const last = element => element.animations.at(-1)

test('cross-grid enable flies the real portrait and shifts following cards across the row boundary', t => {
  const { root, lane } = setup(t), enabled = new ElementBoundary(), disabled = new ElementBoundary()
  root.append(enabled, disabled)
  const source = chef('fox', 170, 500, false), next = chef('owl', 310, 500, false), row = chef('cat', 30, 712, false)
  disabled.append(source, next, row)
  const image = new ElementBoundary('img'); Object.assign(image, { complete: true, naturalWidth: 120, naturalHeight: 150 }); source.append(image)
  const before = lane.capture('fox')
  source.remove()
  const destination = chef('fox', 450, 80); enabled.append(destination)
  next.layout = box(170, 500); row.layout = box(310, 500)
  lane.play(before)
  const bridge = root.querySelector('.chef-motion-bridge')
  assert.ok(bridge); assert.equal(bridge.querySelector('canvas').drawnFrom, image)
  assert.equal(bridge.querySelector('img'), null)
  assert.equal(destination.style.opacity, '0')
  assert.equal(last(bridge).options.duration, CHEF_MOVE_MS)
  assert.equal(last(bridge).keyframes[0].transform, 'translate(-280px, 420px)')
  assert.equal(last(next).keyframes[0].transform, 'translate(140px, 0px)')
  assert.equal(last(row).keyframes[0].transform, 'translate(-280px, 212px)')
  for (const card of [bridge, next, row]) last(card).finish()
  assert.equal(root.querySelector('.chef-motion-bridge'), null)
  assert.equal(destination.style.opacity, ''); assert.equal(root.dataset.chefMotion, 'idle')
})

test('quick reverse redirects from the visible bridge and cancels every previous flight', t => {
  const { root, lane } = setup(t), original = chef('fox', 30, 500, false)
  root.append(original)
  const first = lane.capture('fox'); original.remove()
  const enabled = chef('fox', 310, 80); root.append(enabled); lane.play(first)
  const oldBridge = root.querySelector('.chef-motion-bridge'), oldAnimation = last(oldBridge)
  oldAnimation.progress = .4
  const visible = oldBridge.getBoundingClientRect(), second = lane.capture('fox')
  assert.equal(oldAnimation.playState, 'idle'); assert.equal(oldBridge.parentElement, null); assert.equal(enabled.style.opacity, '')
  enabled.remove(); const disabled = chef('fox', 30, 500, false); root.append(disabled); lane.play(second)
  const bridge = root.querySelector('.chef-motion-bridge')
  assert.deepEqual(bridge.getBoundingClientRect(), visible)
  assert.equal(root.querySelectorAll('.chef-motion-bridge').length, 1)
  oldAnimation.onfinish?.()
  assert.equal(disabled.style.opacity, '0', 'a stale completion cannot reveal a new flight')
  last(bridge).finish()
  assert.equal(disabled.style.opacity, ''); assert.equal(root.querySelectorAll('.chef-motion-bridge').length, 0)
})

test('one real placeholder drives cross-row preview, expands the destination and cancels continuously', t => {
  const { root, lane } = setup(t), grid = new ElementBoundary('div', box(30, 80, 268, 412))
  grid.classList.add('chef-grid'); grid.gridColumns = 2; root.append(grid)
  const cards = [chef('default', 30, 80), chef('a', 170, 80), chef('b', 30, 292)]
  cards[0].classList.add('is-default'); grid.append(...cards)
  const positions = cards.map(element => ({ ...element.layout, id: element.dataset.chefId }))
  lane.preview('a', 'b', true, positions)
  const placeholder = root.querySelector('[data-chef-placeholder]')
  assert.equal(placeholder.parentElement, grid); assert.equal(placeholder.dataset.chefId, undefined)
  assert.equal(cards[1].isConnected, true); assert.equal(cards[1].style.position, 'absolute'); assert.equal(cards[1].style.opacity, '0')
  const moving = last(cards[2]); assert.equal(moving.options.duration, CHEF_PREVIEW_MS)
  assert.equal(moving.keyframes[0].transform, 'translate(-140px, 212px)')
  assert.equal(moving.keyframes[1].transform, 'translate(0px, 0px)')
  moving.progress = .5
  const visual = cards[2].getBoundingClientRect()
  lane.preview('a', null, false, positions)
  const returning = last(cards[2]); assert.equal(moving.playState, 'idle'); assert.deepEqual(cards[2].getBoundingClientRect(), visual)
  returning.progress = .4
  const captured = cards[2].getBoundingClientRect(), before = lane.capture('a')
  lane.play(before)
  assert.deepEqual(cards[2].getBoundingClientRect(), captured)
  assert.equal(root.querySelector('[data-chef-placeholder]'), null)
  assert.equal(cards[1].style.position, undefined); assert.equal(cards[1].dataset.chefDragSource, undefined)
})

test('disabled insertion uses the live destination grid and creates a real extra row until cleared', t => {
  const { root, lane } = setup(t)
  const enabled = new ElementBoundary('div', box(30, 80, 268, 200)), disabled = new ElementBoundary('div', box(30, 500, 268, 200))
  for (const grid of [enabled, disabled]) { grid.classList.add('chef-grid'); grid.gridColumns = 2 }
  root.append(enabled, disabled)
  const cards = [chef('a', 30, 80), chef('b', 170, 80)], source = chef('fox', 30, 500, false)
  enabled.append(...cards); disabled.append(source)
  const beforeHeight = enabled.getBoundingClientRect().height
  lane.preview('fox', 'a', false, cards.map(element => ({ ...element.layout, id: element.dataset.chefId })))
  assert.equal(enabled.querySelector('[data-chef-placeholder]').dataset.chefPlaceholder, 'fox')
  assert.equal(enabled.getBoundingClientRect().height, beforeHeight + 212)
  assert.ok(last(cards[1])); assert.equal(last(cards[1]).keyframes[0].transform, 'translate(140px, -212px)')
  lane.stop()
  assert.equal(enabled.getBoundingClientRect().height, beforeHeight)
  assert.equal(root.querySelector('[data-chef-placeholder]'), null)
  assert.equal(source.style.position, undefined)
  assert.ok(cards.every(element => element.style.order === undefined))
})

test('same-node rapid reorder reads current transform and stale completions cannot clear the next move', t => {
  const { root, lane } = setup(t), card = chef('fox', 30, 80)
  root.append(card)
  const first = lane.capture('fox'); card.layout.left = 310; lane.play(first)
  const previous = last(card); previous.progress = .5
  const second = lane.capture('fox'); card.layout.left = 450; lane.play(second)
  assert.equal(last(card).keyframes[0].transform, 'translate(-280px, 0px)')
  previous.onfinish?.(); assert.equal(card.dataset.chefMotionState, 'move')
  lane.stop(); assert.equal(last(card).playState, 'idle'); assert.equal(card.dataset.chefMotionState, undefined)
})

test('decoded canvases survive another interruption and clones contain no focus/tutorial identity', t => {
  browserBoundary(t)
  const source = chef('fox', 0), button = new ElementBoundary('button'), canvas = new ElementBoundary('canvas')
  source.setAttribute('id', 'original'); button.setAttribute('data-chef-focus', 'fox'); button.setAttribute('data-tutorial', 'chef')
  canvas.width = 120; canvas.height = 150; button.append(canvas); source.append(button)
  const clone = cloneChefCard(source)
  assert.equal(clone.dataset.chefId, undefined); assert.equal(clone.querySelector('[data-chef-focus]'), null)
  assert.equal(clone.querySelector('[data-tutorial]'), null); assert.equal(clone.attributes.has('id'), false)
  assert.equal(clone.querySelector('canvas').drawnFrom, canvas); assert.equal(clone.inert, true)
})

test('reduced motion commits positions without WAAPI, clones, hidden targets or preview residue', t => {
  const { root, lane, setReduced } = setup(t), source = chef('fox', 30, 500, false), neighbour = chef('cat', 170, 500, false)
  root.append(source, neighbour); setReduced(true)
  const before = lane.capture('fox'); source.remove(); const destination = chef('fox', 310, 80); root.append(destination)
  neighbour.layout.left = 30; lane.play(before)
  lane.preview('cat', null, false, [{ ...neighbour.layout, id: 'cat' }])
  assert.equal(root.querySelector('.chef-motion-bridge'), null); assert.equal(destination.style.opacity, '')
  assert.equal(neighbour.animations.length, 0); assert.equal(root.dataset.chefMotion, 'idle')
})

test('focus scroll completes before final sampling and both endpoints use page coordinates', async t => {
  const { root } = setup(t), card = chef('fox', 30, 500), instance = mounted(t, () => useChefMotion(shallowRef(root)))
  root.append(card)
  await instance.value.change('fox', () => { card.layout.top = 80 }, async () => {
    await nextTick(); root.layout.top -= 120; card.layout.top -= 120
  })
  assert.equal(last(card).keyframes[0].transform, 'translate(0px, 420px)')
  instance.stop(); assert.equal(last(card).playState, 'idle'); assert.equal(root.dataset.chefMotion, 'idle')
})

test('an adopted fixed ghost keeps its release position through focus scroll while neighbours follow the page', async t => {
  const { root } = setup(t), source = chef('fox', 30, 500, false), neighbour = chef('cat', 170, 500)
  const instance = mounted(t, () => useChefMotion(shallowRef(root)))
  root.append(source, neighbour)
  const ghost = cloneChefCard(source)
  Object.assign(ghost.style, { position: 'fixed', left: '30px', top: '500px', width: '128px', height: '200px' })
  document.body.append(ghost)
  const released = ghost.getBoundingClientRect(), destination = chef('fox', 310, 80)
  instance.value.adopt('fox', ghost)
  const changing = instance.value.change('fox', () => {
    source.remove(); root.append(destination); neighbour.layout.top = 280
  }, async () => {
    await nextTick(); root.layout.top -= 120; destination.layout.top -= 120; neighbour.layout.top -= 120
  })
  ghost.remove()
  await changing
  const bridge = root.querySelector('.chef-motion-bridge')
  assert.deepEqual(bridge.getBoundingClientRect(), released, 'focus scroll cannot move the fixed release point')
  assert.equal(last(bridge).keyframes[0].transform, 'translate(-280px, 540px)')
  assert.equal(last(neighbour).keyframes[0].transform, 'translate(0px, 220px)')
  assert.equal(neighbour.getBoundingClientRect().top, 380, 'an ordinary card follows the 120px page scroll')
  last(bridge).finish(); last(neighbour).finish()
  assert.equal(destination.style.opacity, ''); assert.equal(root.querySelector('.chef-motion-bridge'), null)
})

for (const type of ['pointercancel', 'blur', 'resize', 'Escape']) test(`real drag ${type} cancels without mutation, animates home, and releases listeners`, async t => {
  const browser = setup(t), { root, dispatch, flushFrame } = browser
  const enabled = new ElementBoundary('section', box(30, 80, 600, 220)), card = chef('fox', 310, 80)
  enabled.dataset.chefGroup = 'enabled'; enabled.classList.add('chef-grid'); root.append(enabled); enabled.append(card)
  let mutations = 0
  const instance = mounted(t, () => {
    const motion = useChefMotion(shallowRef(root))
    return useChefReorder({ root: shallowRef(root), enabled: shallowRef(true), canBeDefault: () => true,
      place: () => { mutations++ }, disable: () => { mutations++ }, motion })
  })
  instance.value.start({ button: 0, isPrimary: true, pointerId: 1, clientX: 350, clientY: 120, target: card, currentTarget: card }, 'fox')
  dispatch('pointermove', { pointerId: 1, clientX: 390, clientY: 130 }); flushFrame()
  assert.ok(document.body.querySelector('.chef-drag-preview')); assert.equal(instance.value.draggingId.value, 'fox')
  dispatch(type === 'Escape' ? 'keydown' : type, { key: 'Escape' }); await tick()
  assert.equal(mutations, 0); assert.equal(card.captured, null); assert.equal(instance.value.draggingId.value, null)
  assert.equal(document.body.querySelector('.chef-drag-preview'), null)
  const bridge = root.querySelector('.chef-motion-bridge'); assert.ok(bridge); last(bridge).finish()
  for (const name of ['pointermove', 'pointerup', 'pointercancel', 'blur', 'resize', 'keydown']) {
    if (type === 'Escape' && name === 'pointerup') dispatch('pointerup')
    assert.equal(browser.listenerCount(name), 0, name)
  }
  assert.equal(browser.frameCount(), 0); assert.equal(root.querySelector('.chef-motion-bridge'), null)
})


test('pickup interrupts a moving neighbour before stable slots are measured and resumes from its visual frame', t => {
  const { root, lane } = setup(t), grid = new ElementBoundary('div', box(30, 80, 268, 200))
  grid.classList.add('chef-grid'); grid.gridColumns = 2; root.append(grid)
  const first = chef('a', 30, 80), second = chef('b', 170, 80); grid.append(first, second)
  const before = lane.capture('a'); grid.children.reverse(); lane.play(before)
  const previous = last(first); previous.progress = .4
  const visible = first.getBoundingClientRect(), pickup = lane.pickup('a')
  assert.equal(previous.playState, 'idle'); assert.deepEqual(pickup.bounds, { left: visible.left, top: visible.top, width: visible.width, height: visible.height })
  assert.equal(first.getBoundingClientRect().left, 170, 'hit testing starts from the stable final slot')
  pickup.resume()
  assert.equal(root.querySelectorAll('[data-chef-placeholder]').length, 1)
  assert.equal(first.style.visibility, 'hidden')
  lane.stop(); assert.equal(first.style.visibility, undefined)
})

test('pickup during cross-grid flight starts at the visible bridge and removes the old clone', t => {
  const { root, lane } = setup(t), original = chef('fox', 30, 500, false)
  const grid = new ElementBoundary('div', box(30, 80, 268, 200)); grid.classList.add('chef-grid'); root.append(original, grid)
  const before = lane.capture('fox'); original.remove(); const target = chef('fox', 170, 80); grid.append(target); lane.play(before)
  const bridge = root.querySelector('.chef-motion-bridge'); last(bridge).progress = .4
  const visible = bridge.getBoundingClientRect(), pickup = lane.pickup('fox')
  assert.deepEqual(pickup.bounds, { left: visible.left, top: visible.top, width: visible.width, height: visible.height })
  assert.equal(bridge.isConnected, false); assert.equal(target.style.opacity, '')
  pickup.resume(); assert.equal(target.style.visibility, 'hidden')
  assert.equal(root.querySelectorAll('.chef-motion-bridge').length, 0)
})

function dragFixture(t, ids = ['a', 'b', 'c'], columns = 3) {
  const browser = setup(t), { root, lane } = browser
  const enabled = new ElementBoundary('section', box(30, 80, columns * 140 - 12, 200))
  enabled.classList.add('chef-grid'); enabled.dataset.chefGroup = 'enabled'; enabled.gridColumns = columns
  const disabled = new ElementBoundary('section', box(30, 800, columns * 140 - 12, 200))
  disabled.classList.add('chef-grid'); disabled.gridColumns = columns; root.append(enabled, disabled)
  const cards = ids.map((id, index) => chef(id, 30 + index % columns * 140, 80 + Math.floor(index / columns) * 212))
  enabled.append(...cards)
  function source(id) { return cards.find(element => element.dataset.chefId === id) }
  function raise(id) {
    const raised = lane.pickup(id); raised.resume()
    return { ...raised.bounds, id, isDefault: false }
  }
  function choose(origin, left, top, currentTarget = null) {
    const positions = lane.snapshot(enabled)
    const target = chefDropTarget({ source: origin, sourceId: origin.id, positions, preserveDefault: false,
      deltaX: left - origin.left, deltaY: top - origin.top, scrollShift: 0,
      pointerX: left + origin.width / 2, pointerY: top + origin.height / 2, currentTarget })
    lane.preview(origin.id, target?.id ?? null, target?.after ?? false, positions, true)
    return target
  }
  const order = () => lane.snapshot(enabled).map(position => position.id)
  return { ...browser, enabled, disabled, cards, source, raise, choose, order }
}

test('live slot order reverses the same neighbour at 50 percent even before its preview animation finishes', t => {
  const { lane, enabled, source, raise, choose, order } = dragFixture(t)
  const origin = raise('a'), b = source('b')
  let target = choose(origin, 30 + 76, 80)
  assert.deepEqual(target, { id: 'b', after: true }); assert.deepEqual(order(), ['b', 'a', 'c'])
  const motion = last(b); motion.progress = .2
  assert.notEqual(b.getBoundingClientRect().left, 30, 'the neighbour is still visibly moving')
  assert.equal(lane.snapshot(enabled)[0].left, 30, 'hit testing uses its final real slot')
  target = choose(origin, 30 + 128 * .501, 80, target)
  assert.deepEqual(target, { id: 'b', after: true }, '49.9 percent keeps the existing slot')
  target = choose(origin, 30 + 64, 80, target)
  assert.deepEqual(target, { id: 'b', after: false }); assert.deepEqual(order(), ['a', 'b', 'c'])
  assert.equal(motion.playState, 'idle')
})

test('a same-drag cross-row move reverses using current placeholder reading order', t => {
  const { raise, choose, order } = dragFixture(t, ['a', 'b', 'c', 'd'], 2)
  const origin = raise('b')
  let target = choose(origin, 30, 292)
  assert.deepEqual(target, { id: 'c', after: true }); assert.deepEqual(order(), ['a', 'c', 'b', 'd'])
  target = choose(origin, 170, 80, target)
  assert.deepEqual(target, { id: 'c', after: false }); assert.deepEqual(order(), ['a', 'b', 'c', 'd'])
})

test('a disabled insertion into a full grid follows shifted neighbours across rows and keeps its own slot', t => {
  const { lane, enabled, disabled, cards, raise, choose, order } = dragFixture(t, ['a', 'b', 'c', 'd'], 2)
  const x = chef('x', 30, 800, false); cards.push(x); disabled.append(x)
  const origin = raise('x'), originalHeight = enabled.getBoundingClientRect().height
  let target = choose(origin, 30, 80)
  assert.deepEqual(target, { id: 'a', after: false }); assert.deepEqual(order(), ['x', 'a', 'b', 'c', 'd'])
  assert.equal(enabled.getBoundingClientRect().height, originalHeight + 212)
  target = choose(origin, 170, 80, target)
  assert.deepEqual(target, { id: 'a', after: true }); assert.deepEqual(order(), ['a', 'x', 'b', 'c', 'd'])
  target = choose(origin, 30, 292, target)
  assert.deepEqual(target, { id: 'b', after: true }); assert.deepEqual(order(), ['a', 'b', 'x', 'c', 'd'])
  const slot = lane.snapshot(enabled).find(position => position.id === 'x')
  target = choose(origin, slot.left, slot.top, target)
  assert.deepEqual(target, { id: 'b', after: true }); assert.deepEqual(order(), ['a', 'b', 'x', 'c', 'd'])
})

for (const reduced of [false, true]) test(`entering another lane reserves a slot immediately, then swaps at 50 percent (reduced=${reduced})`, async t => {
  const { root, enabled, disabled, dispatch, flushFrame, setReduced } = dragFixture(t, ['a', 'b', 'c', 'd'], 2)
  setReduced(reduced)
  const card = chef('x', 30, 800, false); disabled.append(card)
  const calls = [], { value } = mounted(t, () => {
    const motion = useChefMotion(shallowRef(root))
    return useChefReorder({ root: shallowRef(root), enabled: shallowRef(true), canBeDefault: () => true,
      place: (...args) => calls.push(args), disable: () => assert.fail('a disabled source cannot disable'), motion })
  })
  const order = () => [...enabled.children].filter(element => element.style.position !== 'absolute')
    .sort((a, b) => Number(a.style.order ?? 0) - Number(b.style.order ?? 0))
    .map(element => element.dataset.chefId ?? element.dataset.chefPlaceholder)
  const move = (clientX, clientY) => { dispatch('pointermove', { pointerId: 1, clientX, clientY }); flushFrame() }
  const original = enabled.getBoundingClientRect()
  value.start({ button: 0, isPrimary: true, pointerId: 1, clientX: 94, clientY: 900, target: card, currentTarget: card }, 'x')
  move(94, original.bottom + 23)
  assert.equal(value.overEnabled.value, true)
  assert.deepEqual(order(), ['a', 'b', 'x', 'c', 'd'], '38.5 percent overlap already reserves the cross-lane slot')
  assert.equal(enabled.getBoundingClientRect().height, original.height + 212)
  assert.equal(root.querySelectorAll('[data-chef-placeholder]').length, 1)
  assert.deepEqual(calls, [], 'preview does not mutate the draft')
  move(94, 392)
  assert.deepEqual(order(), ['a', 'b', 'x', 'c', 'd'], 'resting on the new placeholder keeps its insertion')
  move(170 - .128, 392)
  assert.deepEqual(order(), ['a', 'b', 'x', 'c', 'd'], '49.9 percent cannot swap inside the lane')
  move(170, 392)
  assert.deepEqual(order(), ['a', 'b', 'c', 'x', 'd'], '50 percent swaps with the shifted neighbour')
  move(158 + .128, 392)
  assert.deepEqual(order(), ['a', 'b', 'c', 'x', 'd'], 'reverse 49.9 percent retains the slot')
  move(158, 392)
  assert.deepEqual(order(), ['a', 'b', 'x', 'c', 'd'], 'reverse 50 percent immediately restores it')
  move(94, enabled.getBoundingClientRect().bottom + 25)
  assert.equal(value.overEnabled.value, false); assert.equal(enabled.querySelector('[data-chef-placeholder]'), null)
  assert.equal(enabled.getBoundingClientRect().height, original.height)
  move(94, original.bottom + 23)
  assert.deepEqual(order(), ['a', 'b', 'x', 'c', 'd'], 'reentry reserves a new slot without waiting for 50 percent')
  move(94, 392)
  dispatch('pointerup', { pointerId: 1, clientX: 94, clientY: 392 }); await tick()
  assert.deepEqual(calls, [['x', 'c', false]])
  assert.equal(root.querySelector('[data-chef-placeholder]'), null)
  if (reduced) assert.ok(root.querySelectorAll('[data-chef-id]').every(element => element.animations.length === 0))
})

test('an enabled chef returning to its lane still needs 50 percent to change its slot', t => {
  const { root, enabled, cards, dispatch, flushFrame } = dragFixture(t)
  const card = cards[0], { value } = mounted(t, () => {
    const motion = useChefMotion(shallowRef(root))
    return useChefReorder({ root: shallowRef(root), enabled: shallowRef(true), canBeDefault: () => true,
      place: () => assert.fail('preview cannot place'), disable: () => assert.fail('preview cannot disable'), motion })
  })
  value.start({ button: 0, isPrimary: true, pointerId: 1, clientX: 94, clientY: 180, target: card, currentTarget: card }, 'a')
  dispatch('pointermove', { pointerId: 1, clientX: 94, clientY: 600 }); flushFrame()
  assert.equal(value.overEnabled.value, false)
  dispatch('pointermove', { pointerId: 1, clientX: 234, clientY: 70 }); flushFrame()
  assert.equal(value.overEnabled.value, true); assert.equal(value.targetId.value, null)
  assert.equal(enabled.querySelector('[data-chef-placeholder]').style.order, '0')
  dispatch('pointermove', { pointerId: 1, clientX: 234, clientY: 180 }); flushFrame()
  assert.equal(value.targetId.value, 'b'); assert.equal(value.dropAfter.value, true)
})

test('fractional CSS widths do not stretch the placeholder height or the motion lane', t => {
  const { root, lane, enabled, cards, raise } = dragFixture(t, ['a', 'b'], 2)
  root.layout.width = 799.671875; root.offsetWidth = 800
  for (const card of cards) { card.layout.width = 139.671875; card.layout.height = 218; card.offsetWidth = 140 }
  const origin = raise('a'), placeholder = enabled.querySelector('[data-chef-placeholder]')
  assert.equal(origin.width, 139.671875); assert.equal(origin.height, 218)
  assert.equal(placeholder.style.height, '218px'); assert.equal(placeholder.style.minHeight, '218px')
  assert.equal(lane.snapshot(enabled)[0].height, 218)
})

test('live slot snapshots compensate current scroll once and ignore moving transforms', t => {
  const { root, lane, enabled, cards, raise, choose } = dragFixture(t)
  const origin = raise('a'), target = choose(origin, 106, 80)
  const b = cards[1]; last(b).progress = .3
  root.layout.top -= 120; enabled.layout.top -= 120
  const positions = lane.snapshot(enabled)
  assert.equal(positions[0].top, -40)
  assert.deepEqual(chefDropTarget({ source: origin, sourceId: 'a', positions, preserveDefault: false,
    deltaX: 64, deltaY: -120, scrollShift: 0, pointerX: 158, pointerY: 60, currentTarget: target }), { id: 'b', after: false })
})

test('releasing over the moved placeholder commits its current insertion exactly once', async t => {
  const { root, enabled, cards, dispatch, flushFrame } = dragFixture(t)
  const calls = [], card = cards[0]
  const { value } = mounted(t, () => {
    const motion = useChefMotion(shallowRef(root))
    return useChefReorder({ root: shallowRef(root), enabled: shallowRef(true), canBeDefault: () => true,
      place: (...args) => calls.push(args), disable: () => assert.fail('a valid placeholder drop cannot disable'), motion })
  })
  value.start({ button: 0, isPrimary: true, pointerId: 1, clientX: 94, clientY: 180, target: card, currentTarget: card }, 'a')
  dispatch('pointermove', { pointerId: 1, clientX: 170, clientY: 180 }); flushFrame()
  dispatch('pointermove', { pointerId: 1, clientX: 158.128, clientY: 180 }); flushFrame()
  assert.equal(value.targetId.value, 'b'); assert.equal(value.dropAfter.value, true)
  dispatch('pointermove', { pointerId: 1, clientX: 158, clientY: 180 }); flushFrame()
  assert.equal(value.targetId.value, 'b'); assert.equal(value.dropAfter.value, false)
  dispatch('pointermove', { pointerId: 1, clientX: 234, clientY: 180 }); flushFrame()
  assert.equal(enabled.querySelector('[data-chef-placeholder]').style.order, '1')
  assert.equal(value.targetId.value, 'b'); assert.equal(value.dropAfter.value, true)
  dispatch('pointerup', { pointerId: 1, clientX: 234, clientY: 180 }); await tick()
  assert.deepEqual(calls, [['a', 'b', true]])
  assert.equal(root.querySelector('[data-chef-placeholder]'), null)
})

test('a disabled chef can drop on the inserted placeholder in an initially empty enabled grid', async t => {
  const { root, enabled, disabled, dispatch, flushFrame } = dragFixture(t, [], 2)
  const card = chef('x', 30, 800, false); disabled.append(card)
  const calls = [], { value } = mounted(t, () => {
    const motion = useChefMotion(shallowRef(root))
    return useChefReorder({ root: shallowRef(root), enabled: shallowRef(true), canBeDefault: () => true,
      place: (...args) => calls.push(args), disable: () => assert.fail('disabled source cannot disable'), motion })
  })
  value.start({ button: 0, isPrimary: true, pointerId: 1, clientX: 94, clientY: 900, target: card, currentTarget: card }, 'x')
  dispatch('pointermove', { pointerId: 1, clientX: 94, clientY: 90 }); flushFrame()
  assert.equal(enabled.querySelector('[data-chef-placeholder]')?.dataset.chefPlaceholder, 'x')
  dispatch('pointermove', { pointerId: 1, clientX: 94, clientY: 180 }); flushFrame()
  assert.equal(enabled.querySelector('[data-chef-placeholder]')?.dataset.chefPlaceholder, 'x')
  dispatch('pointerup', { pointerId: 1, clientX: 94, clientY: 180 }); await tick()
  assert.deepEqual(calls, [['x', null, false]])
})

test('dragging into the new last row stays enabled and drops after its current neighbour', async t => {
  const { root, enabled, disabled, dispatch, flushFrame } = dragFixture(t, ['a', 'b', 'c', 'd'], 2)
  const card = chef('x', 30, 800, false); disabled.append(card)
  const calls = [], { value } = mounted(t, () => {
    const motion = useChefMotion(shallowRef(root))
    return useChefReorder({ root: shallowRef(root), enabled: shallowRef(true), canBeDefault: () => true,
      place: (...args) => calls.push(args), disable: () => assert.fail('the expanded real row is still enabled'), motion })
  })
  const oldBottom = enabled.getBoundingClientRect().bottom
  value.start({ button: 0, isPrimary: true, pointerId: 1, clientX: 94, clientY: 900, target: card, currentTarget: card }, 'x')
  dispatch('pointermove', { pointerId: 1, clientX: 94, clientY: 180 }); flushFrame()
  assert.ok(enabled.getBoundingClientRect().bottom > oldBottom)
  dispatch('pointermove', { pointerId: 1, clientX: 94, clientY: 604 }); flushFrame()
  assert.equal(value.overEnabled.value, true); assert.equal(value.targetId.value, 'd'); assert.equal(value.dropAfter.value, true)
  dispatch('pointerup', { pointerId: 1, clientX: 94, clientY: 604 }); await tick()
  assert.deepEqual(calls, [['x', 'd', true]])
})

test('slot snapshots read native DOMRect prototype properties without relying on object spread', t => {
  const { lane, enabled, source } = dragFixture(t)
  const card = source('a'), original = card.getBoundingClientRect.bind(card)
  card.getBoundingClientRect = () => {
    const rect = original(), prototype = {}
    for (const name of ['left', 'top', 'width', 'height', 'right', 'bottom']) Object.defineProperty(prototype, name, { get: () => rect[name] })
    return Object.create(prototype)
  }
  assert.deepEqual(lane.snapshot(enabled)[0], { id: 'a', left: 30, top: 80, width: 128, height: 200, isDefault: false })
})
