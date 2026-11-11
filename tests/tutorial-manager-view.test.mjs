import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef } from 'vue'

// Compile the production SFC setup and mount it with the real Vue lifecycle.
// Only application boundaries vary; Node does not emulate modal animations.
const boundary = { sections: [], calls: [], notices: [], reads: 0, config: async () => ({}) }
globalThis.__tutorialManagerViewBoundary = boundary
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url.endsWith('/src/components/tutorial/TutorialManager.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'tutorial-manager-view-test' }).content }
    }
    const modules = {
      '/src/components/AppModal.vue': 'export default {}',
      '/src/services/tauri.ts': `export const api = {config: () => {
        globalThis.__tutorialManagerViewBoundary.reads++
        return globalThis.__tutorialManagerViewBoundary.config()
      }}`,
      '/src/tutorial/steps.ts': 'export const tutorialSections = globalThis.__tutorialManagerViewBoundary.sections',
      '/src/tutorial/engine.ts': `
        export const startTutorial = options => globalThis.__tutorialManagerViewBoundary.calls.push(options)
        export const replayTutorial = () => globalThis.__tutorialManagerViewBoundary.calls.push('all')
      `,
      '/src/useNotify.ts': 'export const useNotify = () => ({error: value => globalThis.__tutorialManagerViewBoundary.notices.push(value)})',
      '/src/locale/backend.ts': 'export const backendMessageOf = value => `translated:${value instanceof Error ? value.message : String(value)}`',
      '/src/i18n.ts': 'export const t = key => key',
    }
    for (const [suffix, source] of Object.entries(modules)) {
      if (url.endsWith(suffix)) return { format: 'module', shortCircuit: true, source }
    }
    return next(url, context)
  },
})
const { default: TutorialManager } = await import('../src/components/tutorial/TutorialManager.vue')
const { tutorialManager } = await import('../src/locale/tutorialManager.ts')
const renderer = createRenderer({
  patchProp() {}, insert() {}, remove() {}, createElement: () => ({}), createText: () => ({}),
  createComment: () => ({}), setText() {}, setElementText() {}, parentNode: () => null, nextSibling: () => null,
})
const tick = async () => { await nextTick(); await new Promise(resolve => setImmediate(resolve)) }
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no }); return { promise, resolve, reject } }
const step = (id, extra = {}) => ({ id, titleKey: `tutorial.${id}.title`, bodyKey: `tutorial.${id}.body`, ...extra })
const section = (id, steps) => ({ id, titleKey: `nav.${id}`, steps })

function mounted(t, { show = true, sections = [section('chapter', [step('first'), step('second')])], config = async () => ({}) } = {}) {
  boundary.sections.splice(0, boundary.sections.length, ...sections)
  boundary.calls.length = 0
  boundary.notices.length = 0
  boundary.reads = 0
  boundary.config = config
  const visible = shallowRef(show)
  const component = { ...TutorialManager, render: () => null }
  const app = renderer.createApp({ setup: () => () => h(component, {
    show: visible.value, 'onUpdate:show': value => { visible.value = value },
  }) })
  app.mount({})
  const state = app._instance.subTree.component.setupState
  let disposed = false
  const dispose = () => { if (!disposed) { disposed = true; app.unmount() } }
  t.after(dispose)
  return { state, visible, dispose }
}

test('opening reads progress once and counts only platform-visible catalog steps', async t => {
  const { state, visible } = mounted(t, { show: false, config: async () => ({ tutorial_watched_steps: [' first ', 'first', 'hidden', 'removed-step'] }),
    sections: [section('chapter', [step('first'), step('second'), step('hidden', { when: () => false })]),
      section('hidden-chapter', [step('hidden-platform', { when: () => false })])],
  })
  assert.equal(boundary.reads, 0)
  visible.value = true
  await tick()
  assert.equal(boundary.reads, 1)
  assert.deepEqual(state.sections.map(row => [row.id, row.steps.length, row.seen]), [['chapter', 2, 1]])
  assert.deepEqual(state.totals, { seen: 1, total: 2 })
  assert.deepEqual(boundary.calls, [])
  assert.deepEqual(boundary.notices, [])
})

test('chapter and step actions wait for modal after-leave and play exactly once', async t => {
  const { state, visible } = mounted(t)
  await tick()
  state.play({ sectionId: 'chapter', fromStepId: 'second' })
  assert.deepEqual(boundary.calls, [])
  await tick()
  assert.equal(visible.value, false)
  state.afterLeave()
  state.afterLeave()
  assert.deepEqual(boundary.calls, [{ sectionId: 'chapter', fromStepId: 'second' }])
  visible.value = true
  await tick()
  state.play({ sectionId: 'chapter' })
  await tick()
  assert.equal(boundary.calls.length, 1)
  state.afterLeave()
  assert.deepEqual(boundary.calls[1], { sectionId: 'chapter' })
  assert.equal(boundary.reads, 2, 'one config read per opening; playback never writes progress')
})

test('replay-all delegates to the existing full tutorial action after closing', async t => {
  const { state, visible } = mounted(t)
  await tick()
  state.play('all')
  await tick()
  assert.equal(visible.value, false)
  assert.deepEqual(boundary.calls, [])
  state.afterLeave()
  assert.deepEqual(boundary.calls, ['all'])
})

test('ordinary close never starts playback, and reopening cancels a pending selection', async t => {
  const { state, visible } = mounted(t)
  await tick()
  state.close()
  await tick()
  state.afterLeave()
  assert.deepEqual(boundary.calls, [])
  visible.value = true
  await tick()
  state.play('all')
  await tick()
  visible.value = true
  await tick()
  state.afterLeave()
  assert.deepEqual(boundary.calls, [])
  state.close()
  await tick()
  state.afterLeave()
  assert.deepEqual(boundary.calls, [], 'a cancelled selection cannot return on the next close')
})

test('unmount clears pending replay and ignores a late progress response', async t => {
  const gate = deferred()
  const { state, dispose } = mounted(t, { config: () => gate.promise })
  state.play({ sectionId: 'chapter' })
  await tick()
  dispose()
  state.afterLeave()
  gate.reject(new Error('late response'))
  await tick()
  assert.deepEqual(boundary.calls, [])
  assert.deepEqual(boundary.notices, [])
})

test('an earlier opening cannot replace newer progress or display a late error', async t => {
  const first = deferred(), second = deferred()
  let reads = 0
  const { state, visible } = mounted(t, { config: () => ++reads === 1 ? first.promise : second.promise })
  visible.value = false
  await tick()
  visible.value = true
  await tick()
  second.resolve({ tutorial_watched_steps: ['second'] })
  await tick()
  first.reject(new Error('old opening'))
  await tick()
  assert.equal(state.watched.has('second'), true)
  assert.equal(state.watched.has('first'), false)
  assert.equal(state.error, '')
  assert.deepEqual(boundary.notices, [])
})

test('read failure reports through the shared translator, leaves progress unknown and allows retry', async t => {
  const { state } = mounted(t, { config: async () => { throw new Error('fixture failed') } })
  await tick()
  assert.equal(state.watched, null)
  assert.equal(state.loading, false)
  assert.equal(state.error, 'translated:fixture failed')
  assert.deepEqual(boundary.notices, ['translated:fixture failed'])
  boundary.config = async () => ({ tutorial_watched_steps: ['first', 'second'] })
  await state.loadProgress()
  assert.equal(state.error, '')
  assert.deepEqual(state.totals, { seen: 2, total: 2 })
})

test('reopening reevaluates platform conditions and newly added catalog chapters', async t => {
  let supported = false
  const { state, visible } = mounted(t, { sections: [section('chapter', [step('first'), step('conditional', { when: () => supported })])] })
  await tick()
  assert.equal(state.totals.total, 1)
  visible.value = false
  await tick()
  supported = true
  boundary.sections.push(section('future', [step('future-step')]))
  visible.value = true
  await tick()
  assert.equal(state.totals.total, 3)
  assert.deepEqual(state.sections.map(row => row.id), ['chapter', 'future'])
})

test('tools retain direct replay and wire the manager through the unified modal and shared expansion', () => {
  const tools = readFileSync(new URL('../src/views/ToolsView.vue', import.meta.url), 'utf8')
  const manager = readFileSync(new URL('../src/components/tutorial/TutorialManager.vue', import.meta.url), 'utf8')
  assert.match(tools, /@click="replayTutorial"/)
  assert.match(tools, /data-tutorial-manager-entry[^>]*@click="tutorialManagerOpen = true"/)
  assert.match(tools, /<TutorialManager v-model:show="tutorialManagerOpen"/)
  assert.match(manager, /<AppModal[^>]*@after-leave="afterLeave"/s)
  assert.match(manager, /useCardExpansion\(\)/)
  assert.match(manager, /collapseCardWithScroll/)
  assert.match(manager, /scrollExpandedCardIntoView/)
  assert.match(manager, /:data-tutorial-manager-play-section="section.id"/)
  assert.match(manager, /:data-tutorial-manager-play-step="step.id"/)
  assert.match(manager, /t\(step.titleKey\)/)
  assert.doesNotMatch(manager, /\{\{\s*(?:step|section)\.id\s*\}\}|v-html|api\.(?:save|update|set)/)
})

test('manager strings retain all five languages with matching placeholders', () => {
  const locales = ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']
  for (const [key, messages] of Object.entries(tutorialManager)) {
    assert.ok(key.startsWith('manager.'))
    assert.deepEqual(Object.keys(messages).sort(), [...locales].sort(), key)
    const placeholders = text => [...text.matchAll(/\{(\w+)\}/g)].map(match => match[1]).sort()
    for (const locale of locales) {
      assert.ok(messages[locale].trim(), `${key}:${locale}`)
      assert.deepEqual(placeholders(messages[locale]), placeholders(messages.en), `${key}:${locale}`)
    }
  }
})
