import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { LOCALE_CODES, LOCALE_LABELS, localeOptions } from '../src/locale/types.ts'

const read = path => readFileSync(new URL(`../src/${path}`, import.meta.url), 'utf8')
const storageKey = 'oc2-locale'
let instance = 0

function fixture(t, existing) {
  const store = new Map(existing ? [[storageKey, existing]] : [])
  const writes = []
  const original = Object.getOwnPropertyDescriptor(globalThis, 'localStorage')
  Object.defineProperty(globalThis, 'localStorage', { configurable: true, value: {
    getItem: key => store.get(key) ?? null,
    setItem: (key, value) => { writes.push([key, value]); store.set(key, value) },
  } })
  t.after(() => {
    if (original) Object.defineProperty(globalThis, 'localStorage', original)
    else delete globalThis.localStorage
  })
  return { writes, launch: () => import(`../src/locale/state.ts?run=${++instance}`) }
}

test('语言选择器与管理器语言目录共用 12 种语言及本族语标签', () => {
  const codes = ['en', 'de', 'fr', 'it', 'ko', 'es-ES', 'zh-CN', 'zh-TW', 'ru', 'ja', 'pl', 'pt-BR']
  assert.deepEqual(LOCALE_CODES, codes)
  assert.deepEqual(localeOptions.map(option => option.value), codes)
  assert.equal(new Set(localeOptions.map(option => option.label)).size, codes.length)
  assert.equal(LOCALE_LABELS['es-ES'], 'Español (España)')
  assert.equal(LOCALE_LABELS['pt-BR'], 'Português (Brasil)')
})

test('安装和界面预览均不写入，关闭重进仍是首启；确认不变的值也会持久化', async t => {
  const { writes, launch } = fixture(t)
  const first = await launch()
  for (const language of LOCALE_CODES) {
    first.preselectLocale(language, ['en-US'])
    assert.equal(first.locale.value, language)
    assert.equal(first.hasStoredLocale(), false)
  }
  first.locale.value = 'pl'
  assert.deepEqual(writes, [])
  const reopened = await launch()
  assert.equal(reopened.hasStoredLocale(), false)
  reopened.preselectLocale('pt-BR', ['it-IT'])
  reopened.setLocale(reopened.locale.value)
  assert.deepEqual(writes, [[storageKey, 'pt-BR']])
  const confirmed = await launch()
  confirmed.preselectLocale('de', ['zh-TW'])
  assert.equal(confirmed.locale.value, 'pt-BR')
  assert.equal(confirmed.hasStoredLocale(), true)
})

test('原有用户的存储值优先，标题栏改语言继续使用同一键', async t => {
  const { writes, launch } = fixture(t, 'ko')
  const state = await launch()
  state.preselectLocale('ru', ['en-GB'])
  assert.equal(state.locale.value, 'ko')
  assert.deepEqual(writes, [])
  state.setLocale('es-ES')
  assert.deepEqual(writes, [[storageKey, 'es-ES']])
  assert.equal((await launch()).locale.value, 'es-ES')
})

test('无有效安装语言时按系统语言预选，缺失或不支持时仍可启动', async t => {
  const { writes, launch } = fixture(t, 'invalid')
  const state = await launch()
  for (const [input, expected] of [
    [['nl-NL', 'DE-de'], 'de'], [['pt-PT'], 'pt-BR'], [['zh-Hant-HK'], 'zh-TW'],
    [['zh-Hans-TW'], 'zh-CN'], [['es-MX'], 'es-ES'], [['ru-RU-u-ca-gregory'], 'ru'],
    [['invalid_tag', 'pl-PL'], 'pl'], [[], 'zh-CN'],
  ]) {
    state.preselectLocale('constructor', input)
    assert.equal(state.locale.value, expected)
    assert.equal(state.hasStoredLocale(), false)
  }
  assert.deepEqual(writes, [])
})

test('首启完成模态框退场再启动教程和更新，教程结束前不触发启动刷新', () => {
  const main = read('main.ts')
  const app = read('App.vue')
  const dialog = read('components/StartupLanguageDialog.vue')
  assert.match(main, /if \(!hasStoredLocale\(\)\) \{\s*const installLanguage = isTauri\(\) \? await api\.installLanguage\(\)\.catch\(\(\) => null\) : null\s+preselectLocale\(installLanguage, navigator\.languages\)/)
  assert.ok(main.indexOf('preselectLocale(installLanguage') < main.indexOf('.mount('))
  assert.match(app, /<StartupLanguageDialog v-if="choosingLanguage" @confirmed="finishLanguageSelection"/)
  assert.match(app, /startTutorialIfFirstRun\(config\?\.tutorial_watched_steps \?\? \[\]\)\s+onTutorialSettled\(\(\) => \{ void startupPreflight\.start\(\) \}\)\s+if \(desktop\) initializeAppUpdates\(\)/)
  assert.match(dialog, /setLocale\(locale\.value\)\s+show\.value = false/)
  assert.match(dialog, /@after-leave="emit\('confirmed'\)"/)
  assert.match(dialog, /:closable="false"/)
})
