import assert from 'node:assert/strict'
import test, { afterEach, beforeEach } from 'node:test'
import { registerHooks } from 'node:module'

// Exercise the production singleton, Vue refs and progress/geometry helpers.
// Only its application boundaries (routes, step content, demos and storage) vary.
const dependencies = {
  steps: [],
  sections: [],
  router: { currentRoute: { value: { name: 'overview' } }, async push() {} },
  demoActive: false,
  completed: false,
}
globalThis.__tutorialEngineDependencies = dependencies
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* try directory entry */ }
      try { return next(`${specifier}/index.ts`, context) } catch { /* use original error */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const boundary = {
      '/src/router/index.ts': 'export const router = globalThis.__tutorialEngineDependencies.router',
      '/src/tutorial/steps.ts': `
        export const tutorialSteps = globalThis.__tutorialEngineDependencies.steps
        export const tutorialSections = globalThis.__tutorialEngineDependencies.sections
      `,
      '/src/tutorial/demo.ts': `
        export const startTutorialDemo = () => { globalThis.__tutorialEngineDependencies.demoActive = true }
        export const stopTutorialDemo = () => { globalThis.__tutorialEngineDependencies.demoActive = false }
      `,
      '/src/tutorial/storage.ts': `
        export const clearTutorialProgress = () => { globalThis.__tutorialEngineDependencies.completed = false }
        export const isTutorialCompleted = () => globalThis.__tutorialEngineDependencies.completed
        export const markTutorialCompleted = () => { globalThis.__tutorialEngineDependencies.completed = true }
      `,
    }
    for (const [suffix, source] of Object.entries(boundary)) {
      if (url.endsWith(suffix)) return { format: 'module', shortCircuit: true, source }
    }
    return next(url, context)
  },
})

const { tutorial, startTutorial, nextStep, prevStep, exitTutorial, currentTutorialStepId, setTutorialProgressSink } =
  await import('../src/tutorial/engine.ts')
const tick = () => new Promise(resolve => setImmediate(resolve))
const deferred = () => {
  let resolve, reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
const step = (id, extra = {}) => ({ id, titleKey: `tutorial.${id}.title`, bodyKey: `tutorial.${id}.body`, ...extra })

// Targetless steps do not need DOM emulation. Keep the follow loop queued until
// exit cancels it, so no browser timer or test-specific engine is involved.
let frameId = 0
const frames = new Map()
const originalFrame = Object.getOwnPropertyDescriptor(globalThis, 'requestAnimationFrame')
const originalCancel = Object.getOwnPropertyDescriptor(globalThis, 'cancelAnimationFrame')
beforeEach(() => {
  globalThis.requestAnimationFrame = callback => { const id = ++frameId; frames.set(id, callback); return id }
  globalThis.cancelAnimationFrame = id => { frames.delete(id) }
  dependencies.steps.length = 0
  dependencies.sections.length = 0
  setTutorialProgressSink(null)
  dependencies.demoActive = false
  dependencies.completed = false
})
afterEach(async () => {
  exitTutorial()
  setTutorialProgressSink(null)
  await tick()
  assert.equal(frames.size, 0, 'exiting must cancel the target-follow frame')
  for (const [name, descriptor] of [
    ['requestAnimationFrame', originalFrame], ['cancelAnimationFrame', originalCancel],
  ]) {
    if (descriptor) Object.defineProperty(globalThis, name, descriptor)
    else delete globalThis[name]
  }
})

test('step cleanup locks repeated next/previous input until the next step is ready', async () => {
  const cleanup = deferred()
  let cleanups = 0, prepared = 0
  dependencies.steps.push(step('first'), step('middle', {
    cleanup: () => { cleanups++; return cleanup.promise },
  }), step('last', { prepare: () => { prepared++ } }))
  startTutorial({ fromStepId: 'middle', watchedStepIds: [] })
  await tick()
  assert.equal(tutorial.busy.value, false)

  nextStep()
  try {
    nextStep()
    nextStep()
    prevStep()
    assert.equal(cleanups, 1, 'rapid input must not start parallel cleanup/prepare chains')
    assert.equal(tutorial.busy.value, true, 'cleanup is already part of the locked transition')
    assert.equal(currentTutorialStepId(), 'middle')
    assert.equal(prepared, 0)
  } finally {
    cleanup.resolve()
    await tick()
  }
  assert.equal(currentTutorialStepId(), 'last')
  assert.equal(prepared, 1)
  assert.equal(tutorial.busy.value, false)
})

test('a rejected cleanup still advances once and releases the transition lock', async () => {
  const cleanup = deferred()
  let prepared = 0
  dependencies.steps.push(step('first', { cleanup: () => cleanup.promise }),
    step('last', { prepare: () => { prepared++ } }))
  startTutorial({ watchedStepIds: [] })
  await tick()
  nextStep()
  cleanup.reject(new Error('cleanup fixture'))
  await tick()
  assert.equal(currentTutorialStepId(), 'last')
  assert.equal(prepared, 1)
  assert.equal(tutorial.busy.value, false)
})

test('exiting during cleanup prevents its pending transition from entering the next step', async () => {
  const cleanup = deferred()
  let prepared = 0
  dependencies.steps.push(step('first', { cleanup: () => cleanup.promise }),
    step('last', { prepare: () => { prepared++ } }))
  startTutorial({ watchedStepIds: [] })
  await tick()
  nextStep()
  exitTutorial()
  cleanup.resolve()
  await tick()
  assert.equal(currentTutorialStepId(), null)
  assert.equal(prepared, 0)
  assert.equal(tutorial.active.value, false)
  assert.equal(tutorial.busy.value, false)
  assert.equal(dependencies.demoActive, false)
})

test('cleanup from an exited run cannot advance or unlock a newly started tutorial', async () => {
  const cleanup = deferred(), preparation = deferred()
  let stalePreparations = 0, currentPreparations = 0
  dependencies.steps.push(step('first', { cleanup: () => cleanup.promise }),
    step('stale', { prepare: () => { stalePreparations++ } }),
    step('current', { prepare: () => { currentPreparations++; return preparation.promise } }))
  startTutorial({ watchedStepIds: [] })
  await tick()
  nextStep()
  exitTutorial()
  startTutorial({ fromStepId: 'current', watchedStepIds: [] })
  assert.equal(currentPreparations, 1)
  assert.equal(tutorial.busy.value, true)
  cleanup.resolve()
  try {
    await tick()
    assert.equal(currentTutorialStepId(), 'current', 'old cleanup cannot overwrite the new run index')
    assert.equal(stalePreparations, 0)
    assert.equal(currentPreparations, 1)
    assert.equal(tutorial.busy.value, true, 'only the current prepare owns the busy flag')
  } finally {
    preparation.resolve()
    await tick()
  }
  assert.equal(currentTutorialStepId(), 'current')
  assert.equal(tutorial.busy.value, false)
  assert.equal(dependencies.demoActive, true)
})

test('a chapter replay ignores watched filtering, respects the platform and ends before another chapter', async () => {
  const first = step('chapter-first'), hidden = step('mac-only', { when: () => false }), last = step('chapter-last')
  dependencies.steps.push(step('before'), first, hidden, last, step('after'))
  dependencies.sections.push({ id: 'chapter', titleKey: 'chapter', steps: [first, hidden, last] })
  const writes = []
  setTutorialProgressSink(ids => writes.push([...ids]))
  startTutorial({ sectionId: 'chapter', watchedStepIds: ['chapter-first', 'already-watched'] })
  await tick()
  assert.equal(tutorial.total.value, 2)
  assert.equal(currentTutorialStepId(), 'chapter-first')
  nextStep(); await tick()
  assert.equal(currentTutorialStepId(), 'chapter-last')
  nextStep(); await tick()
  assert.equal(tutorial.active.value, false)
  assert.deepEqual(writes.at(-1), ['chapter-first', 'already-watched', 'chapter-last'])
  assert.equal(dependencies.demoActive, false)
})

test('starting within a chapter only plays and records that chapter suffix', async () => {
  const first = step('skipped'), middle = step('start-here'), last = step('end-here')
  dependencies.steps.push(first, middle, last, step('other-chapter'))
  dependencies.sections.push({ id: 'chapter', titleKey: 'chapter', steps: [first, middle, last] })
  const writes = []
  setTutorialProgressSink(ids => writes.push([...ids]))
  writes.length = 0
  startTutorial({ sectionId: 'chapter', fromStepId: 'start-here', watchedStepIds: [] })
  await tick()
  assert.equal(tutorial.index.value, 0)
  assert.equal(tutorial.total.value, 2)
  assert.equal(currentTutorialStepId(), 'start-here')
  prevStep(); await tick()
  assert.equal(currentTutorialStepId(), 'start-here')
  nextStep(); await tick()
  assert.equal(currentTutorialStepId(), 'end-here')
  nextStep(); await tick()
  assert.deepEqual(writes.at(-1), ['start-here', 'end-here'])
})

test('unknown chapters and steps outside the selected chapter leave progress and demo state alone', () => {
  dependencies.steps.push(step('other'))
  dependencies.sections.push({ id: 'chapter', titleKey: 'chapter', steps: [step('inside')] })
  dependencies.completed = true
  for (const options of [
    { sectionId: 'missing', restart: true },
    { sectionId: 'chapter', fromStepId: 'other', restart: true },
  ]) {
    startTutorial(options)
    assert.equal(tutorial.active.value, false)
    assert.equal(dependencies.completed, true)
    assert.equal(dependencies.demoActive, false)
  }
})
