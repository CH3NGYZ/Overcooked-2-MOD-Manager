import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { createRenderer } from 'vue'

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
    try { return next(`${specifier}.ts`, context) } catch { /* standard resolution below */ }
  }
  return next(specifier, context)
} })
const { isCardOverlayClick } = await import('../src/composables/useCardExpansion.ts')
const { useModsLocalCardExpansion } = await import('../src/composables/useModsLocalCardExpansion.ts')
const { useLevelSetCardExpansion } = await import('../src/composables/useLevelSetCardExpansion.ts')

class ElementBoundary {
  constructor(className = '', parentElement = null) {
    this.className = className
    this.parentElement = parentElement
  }
  matches(selector) {
    return selector.split(',').some(value => this.className.split(' ').includes(value.trim().slice(1)))
  }
  closest(selector) {
    return this.matches(selector) ? this : this.parentElement?.closest(selector) ?? null
  }
}
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})

function setup(t) {
  const document = new EventTarget()
  document.querySelector = () => null
  const replacements = { Element: ElementBoundary, HTMLElement: ElementBoundary, document, CSS: { escape: value => value } }
  const previous = Object.fromEntries(Object.keys(replacements).map(key => [key, Object.getOwnPropertyDescriptor(globalThis, key)]))
  for (const [key, value] of Object.entries(replacements)) Object.defineProperty(globalThis, key, { configurable: true, writable: true, value })
  const apps = []
  t.after(() => {
    for (const app of apps) app.unmount()
    for (const [key, descriptor] of Object.entries(previous)) {
      if (descriptor) Object.defineProperty(globalThis, key, descriptor)
      else delete globalThis[key]
    }
  })
  return {
    document,
    mount(factory) {
      let state
      const app = renderer.createApp({ setup() { state = factory(); return () => null } })
      app.mount({})
      apps.push(app)
      return state
    },
  }
}

function click(target, path = [target]) {
  const event = new Event('click', { bubbles: true })
  Object.defineProperties(event, { target: { value: target }, composedPath: { value: () => path } })
  return event
}

test('shared guard recognizes the modal container and clicks originating inside it', t => {
  setup(t)
  const modal = new ElementBoundary('n-modal-container')
  const button = new ElementBoundary('confirm-button', modal)
  assert.equal(isCardOverlayClick(click(modal)), true)
  assert.equal(isCardOverlayClick(click(button, [button, modal])), true)
})

test('shared guard leaves ordinary card details and non-Element paths available for blank collapse', t => {
  const { document } = setup(t)
  const card = new ElementBoundary('mod-card')
  const body = new ElementBoundary('card-detail', card)
  assert.equal(isCardOverlayClick(click(body, [body, card, document])), false)
  assert.equal(isCardOverlayClick(click(null, [null, undefined, {}, 'n-modal-container', document])), false)
})

test('shared guard uses the dispatch path after the original modal target was removed', t => {
  setup(t)
  const modal = new ElementBoundary('n-modal-container')
  const button = new ElementBoundary('confirm-button', modal)
  const event = click(button, [button, modal])
  button.parentElement = null
  assert.equal(button.closest('.n-modal-container'), null)
  assert.equal(isCardOverlayClick(event), true)
})

for (const [name, factory, stateOf, outerKey] of [
  ['MOD', useModsLocalCardExpansion, value => value.expansion, 'mod:demo.dll'],
  ['map', useLevelSetCardExpansion, value => value.levelExpansion, 'demo-map'],
]) {
  test(`${name} real document listener keeps card details open for modal clicks and closes on ordinary blank`, t => {
    const { document, mount } = setup(t)
    const expansion = stateOf(mount(factory))
    expansion.openOuter(outerKey)
    expansion.openInner(outerKey, 'child.dll')
    const modal = new ElementBoundary('n-modal-container')
    const button = new ElementBoundary('confirm-button', modal)
    document.dispatchEvent(click(button, [button, modal, document]))
    assert.equal(expansion.isOuterOpen(outerKey), true)
    assert.equal(expansion.isInnerOpen(outerKey, 'child.dll'), true)

    const detachedClick = click(button, [button, modal, document])
    button.parentElement = null
    document.dispatchEvent(detachedClick)
    assert.equal(expansion.isOuterOpen(outerKey), true)
    assert.equal(expansion.isInnerOpen(outerKey, 'child.dll'), true)

    const blank = new ElementBoundary('card-detail')
    document.dispatchEvent(click(blank, [blank, document]))
    assert.equal(expansion.isOuterOpen(outerKey), false)
    assert.equal(expansion.isInnerOpen(outerKey, 'child.dll'), false)
  })
}
