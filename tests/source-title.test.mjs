import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'
import { compileScript, parse } from '@vue/compiler-sfc'
import { createRenderer, h, nextTick, shallowRef } from 'vue'

// Render the production SFC with the real pickI18n and locale state. Naive's
// unrelated component locale objects are the only browser package boundary.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'source-title:naive', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.(?:[cm]?[jt]s|vue)$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* normal resolution */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url === 'source-title:naive') return { format: 'module', shortCircuit: true,
      source: 'export const dateEnUS={},dateJaJP={},dateKoKR={},dateZhCN={},dateZhTW={},enUS={},jaJP={},koKR={},zhCN={},zhTW={}' }
    if (url.endsWith('/src/components/SourceTitle.vue')) {
      const { descriptor } = parse(readFileSync(new URL(url), 'utf8'))
      return { format: 'module-typescript', shortCircuit: true,
        source: compileScript(descriptor, { id: 'source-title-test', inlineTemplate: true }).content }
    }
    return next(url, context)
  },
})

const { default: SourceTitle } = await import('../src/components/SourceTitle.vue')
const { locale } = await import('../src/locale/state.ts')
const renderer = createRenderer({
  createElement: tag => ({ tag, props: {}, children: [] }),
  createText: text => ({ text }), createComment: () => ({}),
  patchProp: (node, key, _, value) => { node.props[key] = value },
  insert(node, parent, anchor) {
    if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1)
    node.parent = parent
    const before = parent.children.indexOf(anchor)
    parent.children.splice(before < 0 ? parent.children.length : before, 0, node)
  },
  remove(node) { if (node.parent) node.parent.children.splice(node.parent.children.indexOf(node), 1) },
  setText: (node, text) => { node.text = text },
  setElementText(node, text) { node.text = text; node.children = [] },
  parentNode: node => node.parent,
  nextSibling: node => node.parent?.children[node.parent.children.indexOf(node) + 1] ?? null,
})
function find(node, predicate) {
  if (predicate(node)) return node
  return node.children?.map(child => find(child, predicate)).find(Boolean)
}
const noteNode = root => find(root, node => node.props?.class === 'source-note')
const textOf = node => (node?.text ?? '') + (node?.children?.map(textOf).join('') ?? '')

test('production source titles react to all five locales and source-note replacement, fallback and removal', async () => {
  const originalLocale = locale.value
  const translations = { 'zh-CN': '源说明', 'zh-TW': '來源說明', en: 'Source note', ko: '소스 설명', ja: 'ソース説明' }
  const note = shallowRef(translations)
  const root = { children: [] }
  const app = renderer.createApp({ setup: () => () => h(SourceTitle, { label: 'Source title', noteI18n: note.value }) })
  try {
    app.mount(root)
    for (const [language, expected] of Object.entries(translations)) {
      locale.value = language
      await nextTick()
      assert.equal(textOf(noteNode(root)), ` · ${expected}`)
      assert.ok(textOf(root).startsWith('Source title'))
      assert.equal(find(root, node => node.tag === 'a'), undefined)
    }
    for (const [messages, expected] of [
      [{ ja: ' ', en: 'English fallback', 'zh-CN': '中文' }, 'English fallback'],
      [{ 'zh-CN': '中文回退' }, '中文回退'],
      [{ ko: '첫 번째 값' }, '첫 번째 값'],
      [{ ja: '  replacement  ' }, 'replacement'],
    ]) {
      note.value = messages
      await nextTick()
      assert.equal(textOf(noteNode(root)), ` · ${expected}`)
    }
    for (const missing of [undefined, {}, { en: '', 'zh-CN': '   ' }]) {
      note.value = missing
      await nextTick()
      assert.equal(noteNode(root), undefined)
      assert.equal(textOf(root), 'Source title')
    }
  } finally {
    app.unmount()
    locale.value = originalLocale
    await nextTick()
  }
})

test('both installation pages pass only the current source note through the shared source directory', () => {
  const read = path => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8')
  assert.match(read('components/SourceDirectory.vue'), /<SourceTitle :label="node.label" :note-i18n="node.note_i18n"/)
  assert.match(read('views/ModsOnlineView.vue'), /note_i18n: source\.note_i18n/)
  assert.match(read('views/ModsOnlineView.vue'), /<SourceTitle :label="node.label" :note-i18n="node.note_i18n"/)
  assert.match(read('views/MapsOnlineView.vue'), /note_i18n: node\.note_i18n/)
  assert.doesNotMatch(read('components/SourceTitle.vue'), /sourceRepository|source-origin|sourceUrl|RepositoryCoordinates/)
})
