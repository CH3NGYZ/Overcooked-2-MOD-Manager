import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * MOD 安装「先处理同身份冲突、再问要不要装」的接线（`src/composables/useModInstallFlow.ts`）。
 *
 * 为什么要在 Node 里真跑这个 composable：它是五个安装入口的唯一收口点，判错的
 * 代价都很实在 ——
 *   · 有冲突却直接开安装详情 = 用户在详情里点「立即安装」才失败；
 *   · 检查通过了却没清 `conflicts` = 详情弹窗继续喊「卸载并安装」，把人往
 *     第二次卸载上引；
 *   · 取消后不通知宿主重扫 = 列表里还挂着已经进了回收站的旧装项。
 *
 * 这里不是读源码做正则断言，而是**执行真实 composable**：只把两处浏览器依赖
 * 换成替身 —— `services/tauri.ts`（IPC）与 `useNotify`（需要 n-message-provider）。
 * 两个替身都很薄：记录调用、返回固定结果，不模拟业务。
 */
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try {
        return nextResolve(`${specifier}.ts`, context)
      } catch {
        // 不是单文件模块，继续试目录索引
      }
      try {
        return nextResolve(`${specifier}/index.ts`, context)
      } catch {
        // 交给 Node 自己报错，错误里带原始 specifier 更好查
      }
    }
    return nextResolve(specifier, context)
  },
  load(url, context, nextLoad) {
    if (url.includes('/src/services/tauri.ts')) {
      return { format: 'module', shortCircuit: true, source: FAKE_API }
    }
    if (url.includes('/src/composables/useDashboardStore.ts')) {
      return { format: 'module', shortCircuit: true, source: `export const useDashboard = () => ({game: {get value() { return {kitchen_running: !!globalThis.__gameRunning} }},config:{value:null},mods:{value:[]}})` }
    }
    if (url.includes('/src/composables/useMapSources.ts')) {
      return { format: 'module', shortCircuit: true, source: `export const useMapSources = () => ({maps:{value:[]}})` }
    }
    if (url.includes('/src/useNotify.ts')) {
      return { format: 'module', shortCircuit: true, source: FAKE_NOTIFY }
    }
    return nextLoad(url, context)
  },
})

/** IPC 替身：只记录调用并按需回一个成功结果。 */
const FAKE_API = `
const calls = () => globalThis.__installCalls
export const watchLevelSetZipProgress = async () => () => {}
export const api = {
  stageMapSource: async () => ({path:'map.zip', staged:false, sets:globalThis.__mapSets}),
  importLevelSets: async () => { calls().push(['importLevelSets']); return {installed:['map']} },
  chooseModFile: async () => null,
  analyzeModSource: async (kind, value) => {
    calls().push(['analyzeModSource', kind, value])
    return null
  },
  cancelModPreview: async (id) => {
    calls().push(['cancelModPreview', id])
    return { ok: true, message: '' }
  },
  confirmModInstall: async (id) => {
    calls().push(['confirmModInstall', id])
    return { ok: true, message: '' }
  },
  uninstallLocalMod: async (handle, allowUnmanaged) => {
    calls().push(['uninstallLocalMod', handle, allowUnmanaged])
    if (globalThis.__uninstallFails) return { ok: false, message: 'mods.uninstallFailed' }
    return { ok: true, message: '' }
  },
  deleteLevelSet: async (handle, enabled) => {
    calls().push(['deleteLevelSet', handle, enabled])
    return { ok: true, message: '' }
  },
}
`

/** 通知替身：naive-ui 的 message 需要 provider，Node 里没有，只记录文案。 */
const FAKE_NOTIFY = `
const push = (level) => (content) => {
  globalThis.__notices.push([level, String(content)])
  return {}
}
export function useNotify() {
  return {
    create: push('create'),
    info: push('info'),
    success: push('success'),
    warning: push('warning'),
    error: push('error'),
    loading: push('loading'),
  }
}
`

const { useModInstallFlow } = await import('../src/composables/useModInstallFlow.ts')

const conflict = (handle, name, version = '2.1.0') => ({
  handle,
  name,
  version,
  enabled: true,
  same_version: true,
})

function modPreview(overrides = {}) {
  return {
    preview_id: 'preview-1',
    kind: 'single',
    package_name: 'Chef Rotation',
    file_name: 'OC2ChefRotation.dll',
    total_dlls: 1,
    mod_count: 1,
    dependency_count: 0,
    entries: [{ path: 'OC2ChefRotation.dll', role: 'mod', name: 'Chef Rotation' }],
    conflicts: [],
    origin: 'manual',
    release_tag: '',
    release_body: '',
    ...overrides,
  }
}

/** 让 `openPreview` 里的 promise 链跑完（它刻意不 await）。 */
const flush = () => new Promise((resolve) => setTimeout(resolve, 5))

/**
 * 模拟检查弹窗「给出结论并收起」。
 *
 * 真实组件是 `emit('finished', ready)` 之后紧接着 `emit('update:show', false)`，
 * 宿主再通过 `v-model:show` 把 `open` 置 false；只调 `finish` 把它留在打开态，
 * 与真实顺序不符。
 */
function settleCheck(flow, ready) {
  flow.finishCheck(ready)
  flow.checkOpen.value = false
}

test.beforeEach(() => {
  globalThis.__gameRunning = false
  globalThis.__installCalls = []
  globalThis.__notices = []
  globalThis.__uninstallFails = false
})

test('运行中点击 MOD 来源：弹全局提示且不进入来源或 IPC', async () => {
  const { gameOperationNotice } = await import('../src/composables/useGameOperationGate.ts')
  const flow = useModInstallFlow()
  globalThis.__gameRunning = true
  gameOperationNotice.value = false
  flow.openSource()
  await flow.handleSourceSelect({ kind: 'url', value: 'https://example.com/mod.zip' })
  assert.equal(gameOperationNotice.value, true)
  assert.equal(flow.sourceOpen.value, false)
  assert.deepEqual(globalThis.__installCalls, [])
})

test('预览返回前游戏启动：只丢弃暂存，不进入安装详情', async () => {
  const flow = useModInstallFlow()
  globalThis.__gameRunning = true
  flow.openPreview(modPreview())
  await flush()
  assert.equal(flow.previewOpen.value, false)
  assert.deepEqual(globalThis.__installCalls, [['cancelModPreview', 'preview-1']])
})

test('冲突确认期间游戏启动：执行时复核，不调用卸载 IPC', async () => {
  const flow = useModInstallFlow()
  flow.openPreview(modPreview({ conflicts: [conflict('OC2ChefRotation.dll', '厨师旋转')] }))
  globalThis.__gameRunning = true
  await assert.rejects(flow.executeCheck(flow.checkModel.value.rows[0]), /game.running/)
  assert.deepEqual(globalThis.__installCalls, [])
})

test('运行中地图无待处理关系放行；缺少 MOD 依赖（包括没有来源）时拦截', async () => {
  const { useInstallPlanGate } = await import('../src/composables/useInstallPlanGate.ts')
  let checks = 0
  const gate = useInstallPlanGate({
    mods: () => [], repositories: () => [],
    gameBlocked: () => { checks += 1; return true }, onApplied: () => {},
  })
  const repo = { id: 'map', name: 'Map', cached_releases: [], relations: [] }
  assert.equal(await gate.run(repo), true)
  assert.equal(checks, 0)
  assert.equal(await gate.run({ ...repo, relations: [{ guid: 'missing.runtime', kind: 'requires' }] }), false)
  assert.equal(checks, 1)
  assert.equal(gate.open.value, false)
})

test('真实地图导入流程：运行中纯地图可确认导入，随包插件和无来源依赖在确认前拦截', async () => {
  const { useLevelSetImport } = await import('../src/composables/useLevelSetImport.ts')
  globalThis.__gameRunning = true
  const base = {dir_name:'Map', name:'Map', author:'Author', version:'1', plugins:[], relations:[], conflicts:[]}
  for (const extra of [{plugins:[{name:'Bundled plugin'}]}, {relations:[{guid:'missing.runtime',kind:'requires'}]}]) {
    globalThis.__mapSets = [{...base,...extra}]
    const flow = useLevelSetImport()
    await flow.importSource({kind:'file',value:'map.zip'})
    assert.equal(flow.previewOpen.value, false)
    assert.deepEqual(globalThis.__installCalls, [])
  }
  globalThis.__mapSets = [base]
  const flow = useLevelSetImport()
  const importing = flow.importSource({kind:'file',value:'map.zip'})
  await flush()
  assert.equal(flow.previewOpen.value, true)
  flow.confirmImport()
  await importing
  assert.deepEqual(globalThis.__installCalls, [['importLevelSets']])
})

test('没有同身份冲突：直接进安装详情，不弹检查窗', async () => {
  const changed = []
  const flow = useModInstallFlow(() => { changed.push('changed') })

  flow.openPreview(modPreview())

  assert.equal(flow.checkOpen.value, false)
  assert.equal(flow.previewOpen.value, true)
  assert.equal(flow.preview.value.preview_id, 'preview-1')
  assert.deepEqual(globalThis.__installCalls, [])
  assert.deepEqual(changed, [])
})

test('有冲突：先弹检查窗、暂不开安装详情，清单来自后端 conflicts', async () => {
  const flow = useModInstallFlow()
  flow.openPreview(modPreview({
    package_name: 'Chef Rotation 重装',
    conflicts: [conflict('OC2ChefRotation.dll', '厨师旋转')],
  }))

  assert.equal(flow.checkOpen.value, true)
  assert.equal(flow.previewOpen.value, false)
  assert.equal(flow.preview.value, null, '检查没结束前不该把预览交给详情弹窗')

  const model = flow.checkModel.value
  assert.equal(model.rows.length, 1)
  assert.equal(model.rows[0].label, '厨师旋转')
  assert.equal(model.rows[0].actionLabel, '卸载')
  assert.equal(model.rows[0].sameVersion, true)
  assert.ok(model.lead.includes('Chef Rotation 重装'), `说明行应点名目标：${model.lead}`)
})

test('逐行执行：走整包语义卸载本地 MOD（allow_unmanaged = true）', async () => {
  const flow = useModInstallFlow()
  flow.openPreview(modPreview({ conflicts: [conflict('OC2ChefRotation.dll', '厨师旋转')] }))

  await flow.executeCheck(flow.checkModel.value.rows[0])

  assert.deepEqual(globalThis.__installCalls.at(-1),
    ['uninstallLocalMod', 'OC2ChefRotation.dll', true])
})

test('全部处理成功：清空 conflicts 再进详情，且不额外重扫', async () => {
  const changed = []
  const flow = useModInstallFlow(() => { changed.push('changed') })
  flow.openPreview(modPreview({ conflicts: [conflict('OC2ChefRotation.dll', '厨师旋转')] }))

  await flow.executeCheck(flow.checkModel.value.rows[0])
  settleCheck(flow, true)
  await flush()

  assert.equal(flow.previewOpen.value, true)
  assert.equal(flow.checkOpen.value, false)
  assert.deepEqual(flow.preview.value.conflicts, [], '卸完了详情弹窗不该再喊卸载并安装')
  assert.equal(flow.preview.value.preview_id, 'preview-1')
  assert.equal(flow.preview.value.total_dlls, 1)
  assert.deepEqual(changed, [], '成功路径由安装成功的 handleInstalled 刷新，这里不该多刷一次')
})

test('取消：清后端暂存并让宿主重扫（磁盘可能已经卸过旧装项）', async () => {
  const changed = []
  const flow = useModInstallFlow(() => { changed.push('changed') })
  flow.openPreview(modPreview({ conflicts: [conflict('OC2ChefRotation.dll', '厨师旋转')] }))

  settleCheck(flow, false)
  await flush()

  assert.equal(flow.checkOpen.value, false)
  assert.equal(flow.previewOpen.value, false)
  assert.deepEqual(globalThis.__installCalls, [['cancelModPreview', 'preview-1']])
  assert.deepEqual(changed, ['changed'])
})

test('卸载失败：这一行抛出、检查窗不关、不进安装详情', async () => {
  globalThis.__uninstallFails = true
  const flow = useModInstallFlow()
  flow.openPreview(modPreview({ conflicts: [conflict('OC2ChefRotation.dll', '厨师旋转')] }))

  await assert.rejects(flow.executeCheck(flow.checkModel.value.rows[0]))

  assert.equal(flow.checkOpen.value, true, '失败要留在原地，用户才能看到是哪一项没卸成')
  assert.equal(flow.previewOpen.value, false)
  assert.equal(flow.preview.value, null)
})

test('检查通过后放弃安装详情：也要通知宿主重扫（旧装项已经进回收站）', async () => {
  const changed = []
  const flow = useModInstallFlow(() => { changed.push('changed') })
  flow.openPreview(modPreview({ conflicts: [conflict('OC2ChefRotation.dll', '厨师旋转')] }))
  settleCheck(flow, true)
  await flush()
  assert.deepEqual(changed, [])

  await flow.discardPreview(flow.preview.value)

  assert.deepEqual(changed, ['changed'])
  assert.deepEqual(globalThis.__installCalls, [['cancelModPreview', 'preview-1']])
})

test('没走过检查的预览被放弃：只清暂存，不白刷一次列表', async () => {
  const changed = []
  const flow = useModInstallFlow(() => { changed.push('changed') })
  flow.openPreview(modPreview())

  await flow.discardPreview(flow.preview.value)

  assert.deepEqual(globalThis.__installCalls, [['cancelModPreview', 'preview-1']])
  assert.deepEqual(changed, [])
})
