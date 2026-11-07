import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import vm from 'node:vm'
import ts from 'typescript'
import * as vue from 'vue'

const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8')
const defaultZoom = JSON.parse(read('src/uiZoomDefault.json'))
const code = ts.transpileModule(read('src/composables/useUiZoom.ts'), {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 },
}).outputText

async function loadZoom({ local = null, config, offline = false } = {}) {
  const storage = new Map(local === null ? [] : [['oc2-ui-zoom', local]])
  const applied = [], saved = []
  const exports = {}
  const dependencies = {
    vue,
    '@tauri-apps/api/webview': { getCurrentWebview: () => ({ setZoom: async value => { applied.push(value) } }) },
    '../services/tauri': { api: {
      config: async () => { if (offline) throw Error('offline'); return config },
      saveSettings: async patch => { saved.push(patch.ui_zoom) },
    } },
    '../runtime': { safeTauri: action => action() },
    '../uiZoomDefault.json': defaultZoom,
  }
  vm.runInNewContext(code, {
    exports,
    require: name => { assert.ok(name in dependencies, name); return dependencies[name] },
    localStorage: { getItem: key => storage.get(key) ?? null, setItem: (key, value) => storage.set(key, value) },
  })
  await new Promise(resolve => setImmediate(resolve))
  return { module: exports, storage, applied, saved }
}

test('启动无配置/缓存或缓存无效时使用 125%，不把空值转换成 100%', async () => {
  assert.equal(defaultZoom, 1.25)
  for (const local of [null, '', ' ', 'invalid', 'NaN', 'Infinity', '0', '-1']) {
    const state = await loadZoom({ local, offline: true })
    assert.equal(state.module.zoom.value, 1.25, String(local))
    assert.deepEqual(state.applied, [1.25])
    assert.equal(state.storage.get('oc2-ui-zoom'), '1.25')
    assert.deepEqual(state.saved, [], '恢复不覆盖用户配置')
  }
})

test('后端缩放优先，保留已存档位，旧值吸附仍有效', async () => {
  for (const [stored, expected] of [[1, 1], [1.25, 1.25], [1.5, 1.5], [1.75, 1.75], [2, 2], [1.3, 1.25], [1.8, 1.75]]) {
    const state = await loadZoom({ local: '2', config: { ui_zoom: stored } })
    assert.equal(state.module.zoom.value, expected)
    assert.deepEqual(state.applied, [expected])
    assert.deepEqual(state.saved, [])
  }
})

test('缺失或无效后端缩放保留本地选择，没有本地选择才用默认值', async () => {
  for (const ui_zoom of [undefined, null, '', 'broken', 0]) {
    assert.equal((await loadZoom({ local: '1.75', config: { ui_zoom } })).module.zoom.value, 1.75)
    assert.equal((await loadZoom({ config: { ui_zoom } })).module.zoom.value, 1.25)
  }
})

test('恢复默认落到 125%，原五档增减与持久化不变', async () => {
  const state = await loadZoom({ config: { ui_zoom: 2 } })
  state.module.resetZoom()
  await new Promise(resolve => setImmediate(resolve))
  assert.equal(state.module.zoomPercent.value, 125)
  assert.deepEqual(state.saved, [1.25])
  state.module.zoomIn()
  await new Promise(resolve => setImmediate(resolve))
  assert.equal(state.module.zoom.value, 1.5)
  state.module.zoomOut()
  await new Promise(resolve => setImmediate(resolve))
  assert.equal(state.module.zoom.value, 1.25)
  assert.equal(state.storage.get('oc2-ui-zoom'), '1.25')
})

test('后端创建、缺字段与界面重置共用前端默认资源', () => {
  assert.match(read('src-tauri/src/config/defaults.rs'), /include_str!\("\.\.\/\.\.\/\.\.\/src\/uiZoomDefault.json"\)/)
  assert.match(read('src-tauri/src/config/defaults.rs'), /ui_zoom: default_ui_zoom\(\)/)
  assert.match(read('src-tauri/src/models/config.rs'), /serde\(default = "crate::config::default_ui_zoom"\)/)
  assert.match(read('src-tauri/src/commands/settings.rs'), /config.ui_zoom = crate::config::default_ui_zoom\(\)/)
})
