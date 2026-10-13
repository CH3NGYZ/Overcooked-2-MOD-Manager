import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'
import { createRenderer, h, nextTick, reactive, ref } from 'vue'

function moduleUrl(source, imports = {}) {
  let { outputText } = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  })
  for (const [name, target] of Object.entries(imports)) outputText = outputText.replaceAll(`from '${name}'`, `from '${target}'`)
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`
}

const focusUrl = moduleUrl(readFileSync(new URL('../src/composables/useListNavigationFocus.ts', import.meta.url), 'utf8'), {
  vue: import.meta.resolve('vue'),
})
const { useListNavigationFocus } = await import(focusUrl)
const view = readFileSync(new URL('../src/components/RepositoriesView.vue', import.meta.url), 'utf8')
const source = ts.createSourceFile('RepositoriesView.ts', view.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1], ts.ScriptTarget.Latest, true)
const names = new Set(['repositoryList', 'focusedRepositories'])
const declarations = source.statements.filter((statement) => ts.isVariableStatement(statement)
  && statement.declarationList.declarations.some((node) => names.has(node.name.getText(source)))).map((statement) => statement.getText(source))
assert.equal(declarations.length, names.size, 'Execute the actual repository view focus configuration')
const { createFocus } = await import(moduleUrl(`
import { ref } from 'vue'
import { useListNavigationFocus } from '${focusUrl}'
export function createFocus(props, list, emit) {
  ${declarations.join('\n')}
  repositoryList.value = list
  return focusedRepositories
}
`, { vue: import.meta.resolve('vue') }))

function fixture(context, targets, coordinates, scale = 1, configureFocus = createFocus) {
  context.mock.timers.enable({ apis: ['setTimeout'] })
  let frame
  const previousRequest = globalThis.requestAnimationFrame
  const previousCancel = globalThis.cancelAnimationFrame
  globalThis.requestAnimationFrame = (callback) => { frame = callback; return 1 }
  globalThis.cancelAnimationFrame = () => { frame = undefined }
  const focusCalls = []
  const finished = []
  const container = {
    scrollTop: 0, clientHeight: 600, scrollHeight: 2400,
    getBoundingClientRect: () => ({ top: 0, bottom: 600 * scale, height: 600 * scale }),
  }
  const list = {
    parentElement: container,
    closest: () => container,
    querySelectorAll: (selector) => selector === '[data-repository-id]' ? rows : [],
  }
  const rows = Object.entries(coordinates).map(([id, top]) => ({
    parentElement: list, isConnected: true,
    getAttribute: () => id,
    getBoundingClientRect: () => ({ top: (top - container.scrollTop) * scale, bottom: (top + 120 - container.scrollTop) * scale }),
    focus: (options) => focusCalls.push({ id, options }),
  }))
  const renderer = createRenderer({
    createElement: () => ({}), createText: () => ({}), createComment: () => ({}),
    insert: () => {}, remove: () => {}, setText: () => {}, setElementText: () => {}, patchProp: () => {},
    parentNode: () => null, nextSibling: () => null,
  })
  const props = reactive({ focusRepositoryIds: targets })
  let focused
  const app = renderer.createApp({ setup() { focused = configureFocus(props, list, (event) => finished.push(event)); return () => h('div') } })
  app.mount({})
  let unmounted = false
  function unmount() {
    if (unmounted) return
    unmounted = true
    app.unmount()
  }
  return {
    props, focused, container, focusCalls, finished, unmount,
    animationPending: () => frame !== undefined,
    navigate() {
      context.mock.timers.tick(450)
      assert.ok(frame, 'Navigation schedules its scroll animation after rendering')
      const animate = frame
      frame = undefined
      animate(performance.now() + 1000)
    },
    close() {
      unmount()
      context.mock.timers.reset()
      if (previousRequest) globalThis.requestAnimationFrame = previousRequest
      else delete globalThis.requestAnimationFrame
      if (previousCancel) globalThis.cancelAnimationFrame = previousCancel
      else delete globalThis.cancelAnimationFrame
    },
  }
}

for (const scale of [1, 1.5, 2]) {
  test(`repository navigation highlights official and offscreen community targets at ${scale}x content scale`, (context) => {
    const view = fixture(context, ['official', 'community'], { official: 100, community: 1800, unrelated: 2000 }, scale)
    try {
      view.navigate()
      assert.deepEqual(view.focused.value, ['official', 'community'], 'Community targets must not be discarded when they do not fit beside the official target')
      assert.deepEqual(view.focusCalls, [{ id: 'official', options: { preventScroll: true } }], 'Only the first target receives keyboard focus')
      assert.equal(view.container.scrollTop, 84)
      context.mock.timers.tick(4500)
      assert.deepEqual(view.focused.value, [])
      assert.deepEqual(view.finished, ['focus-finished'])
    } finally { view.close() }
  })
}

test('a single community target scrolls into view and a later request replaces the old highlight', async (context) => {
  const view = fixture(context, ['community'], { official: 100, community: 1800 })
  try {
    view.navigate()
    assert.deepEqual(view.focused.value, ['community'])
    assert.equal(view.container.scrollTop, 1784)
    view.props.focusRepositoryIds = ['official']
    await nextTick()
    assert.deepEqual(view.focused.value, [])
    view.navigate()
    assert.deepEqual(view.focused.value, ['official'])
    context.mock.timers.tick(4500)
    assert.deepEqual(view.finished, ['focus-finished'], 'A superseded request must not complete the newer navigation')
  } finally { view.close() }
})

test('an absent target finishes without highlighting unrelated rows', (context) => {
  const view = fixture(context, ['missing'], { official: 100, community: 1800 })
  try {
    context.mock.timers.tick(450)
    assert.deepEqual(view.focused.value, [])
    assert.deepEqual(view.finished, ['focus-finished'])
  } finally { view.close() }
})

test('unmount cancels the scheduled scroll and highlight completion', (context) => {
  const view = fixture(context, ['community'], { official: 100, community: 1800 })
  try {
    view.unmount()
    context.mock.timers.tick(5000)
    assert.equal(view.animationPending(), false)
    assert.deepEqual(view.focused.value, [])
    assert.deepEqual(view.focusCalls, [])
    assert.deepEqual(view.finished, ['focus-finished'])
  } finally { view.close() }
})

test('other list navigation keeps its default visible-target highlight behavior', (context) => {
  const view = fixture(context, ['official', 'community'], { official: 100, community: 1800 }, 1, (props, list, emit) => useListNavigationFocus({
    list: ref(list), targetIds: () => props.focusRepositoryIds, targetAttribute: 'data-repository-id',
    stickyHeadingSelector: ':scope > .table-head', onFinished: () => emit('focus-finished'),
  }))
  try {
    view.navigate()
    assert.deepEqual(view.focused.value, ['official'], 'The repository-only option must not change MOD list navigation')
  } finally { view.close() }
})
