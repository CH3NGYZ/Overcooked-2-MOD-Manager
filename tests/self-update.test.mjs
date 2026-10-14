import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'
import * as Vue from 'vue'
import { compile } from '@vue/compiler-dom'
import { renderToString } from '@vue/server-renderer'

function declarations(source, names) {
  const file = ts.createSourceFile('fixture.ts', source, ts.ScriptTarget.Latest, true)
  return file.statements.flatMap((statement) => {
    if (ts.isVariableStatement(statement)) return statement.declarationList.declarations.some((node) => names.includes(node.name.getText(file))) ? [statement.getText(file)] : []
    return ts.isFunctionDeclaration(statement) && names.includes(statement.name?.text) ? [statement.getText(file)] : []
  }).join('\n')
}

async function importTypescript(source) {
  const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`)
}

const app = await readFile(new URL('../src/App.vue', import.meta.url), 'utf8')
const script = app.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1]
const runtime = await importTypescript(`
const t = (key) => key
${declarations(script, ['initialUpdateCheck', 'updateChannelName', 'updateChannelStatus'])}
export { initialUpdateCheck, updateChannelName, updateChannelStatus }
`)
const i18n = await readFile(new URL('../src/i18n.ts', import.meta.url), 'utf8')
const translations = await importTypescript(`
${declarations(i18n, ['entries', 'zhCnEntries', 'translateFor'])}
export { translateFor }
`)

const sourceFile = ts.createSourceFile('App.ts', script, ts.ScriptTarget.Latest, true)
function listenerSource(name) {
  let callback
  function visit(node) {
    if (ts.isCallExpression(node) && node.expression.getText(sourceFile) === name) callback = node.arguments[0].getText(sourceFile)
    ts.forEachChild(node, visit)
  }
  visit(sourceFile)
  return callback
}
const startupRuntime = await importTypescript(`
export function createStartupHarness(ref, computed, result = { ok: false, message: '当前已是最新版本' }) {
  const t = key => key
  const appUnmounted = false
  const STARTUP_IPC_TIMEOUT_MS = 8000
  const repos = ref([])
  const repositoryReleaseReady = {}
  const beginDownloadProgress = () => undefined
  const clearDownloadProgress = () => undefined
  const clearUpdateSkipTimer = () => undefined
  const resetInstalledModExpansion = () => undefined
  const startInitialRepositoryRefresh = () => undefined
  const tutorialRuntime = { startInitialOnboarding: async () => false }
  const withTimeout = promise => promise
  let resolveDashboard, rejectDashboard, dashboardStarted
  const enteredDashboard = new Promise(resolve => { dashboardStarted = resolve })
  const dashboardResult = new Promise((resolve, reject) => { resolveDashboard = resolve; rejectDashboard = reject })
  const loadDashboard = () => { dashboardStarted(); return dashboardResult }
  ${declarations(script, ['appUpdate', 'appUpdateCheck', 'startupUpdateChecking', 'startupLoading', 'startupLoadingError', 'updateCheckFailed', 'updateSkipped',
    'startupUpdatePhase', 'startupStage', 'startupError', 'updatePageError', 'updateSkipCountdown', 'visibleStartupUpdateChannels'])}
  const markUpdateSkipped = message => { updateSkipped.value = true; updateCheckFailed.value = true; startupUpdatePhase.value = 'skipped'; startupError.value = message }
  const api = { installAppUpdate: async () => {
    appUpdateCheck.value = { region: 'mainland-china', channels: [
      { id: 'gitee-direct', provider: 'gitee', status: 'latest' },
      { id: 'github-direct', provider: 'github', status: 'skipped' },
      ...Array.from({ length: 15 }, (_, index) => ({ id: 'github-proxy:' + index, provider: 'github', status: 'skipped' })),
    ] }
    if (result instanceof Error) throw result
    return result
  } }
  ${declarations(script, ['initialUpdateCheck', 'startupChecks', 'finishStartupUpdate', 'receiveAppUpdateChannel', 'receiveAppUpdatePhase', 'continueAfterUpdateSkip',
    'updateChannelName', 'updateChannelStatus', 'updateRegionLabel'])}
  const repositorySourceRegionLabel = value => value
  const sendChannel = ${listenerSource('watchAppUpdateChannel')}
  const sendPhase = ${listenerSource('watchAppUpdatePhase')}
  return {
    start: startupChecks, enteredDashboard, resolveDashboard, rejectDashboard, sendChannel, sendPhase, continueAfterUpdateSkip,
    state: { t, startupUpdateChecking, updateCheckFailed, updateSkipped, startupUpdatePhase, startupStage, startupError,
      appUpdateCheck, appUpdate, updateChannelName, updateChannelStatus, updateRegionLabel,
      updateSkipCountdown, updatePageError, updatePageOpening: false, displayProgress: {},
      continueAfterUpdateSkip, openManualUpdatePage: () => undefined, api: { exitApp: () => undefined },
      ...(typeof startupLoading !== 'undefined' ? { startupLoading, startupLoadingError } : {}),
      ...(typeof visibleStartupUpdateChannels !== 'undefined' ? { visibleStartupUpdateChannels } : {}),
    },
  }
}
`)
const overlayTemplate = app.slice(app.indexOf('    <div v-if="startupUpdateChecking || updateCheckFailed"'), app.indexOf('    <TutorialExperience'))
const overlayRender = new Function('Vue', compile(overlayTemplate, { prefixIdentifiers: true }).code)(Vue)
async function startupHtml(harness) {
  return renderToString(Vue.createSSRApp({ setup: () => harness.state, render: overlayRender }))
}

test('update UI starts with no fictitious API/token routes and names bundled proxies distinctly', () => {
  assert.deepEqual(runtime.initialUpdateCheck().channels, [])
  assert.equal(runtime.initialUpdateCheck().region, 'detecting')
  assert.equal(runtime.updateChannelName('gitee-direct'), 'Gitee 更新清单')
  assert.equal(runtime.updateChannelName('github-direct'), 'GitHub 更新清单')
  assert.equal(runtime.updateChannelName('github-proxy:gh-ddlc'), 'GitHub 内置代理 (gh-ddlc)')
  assert.equal(runtime.updateChannelName('github-proxy:oc2gh-release'), 'GitHub 内置代理 (oc2gh-release)')
  assert.equal(runtime.updateChannelStatus('cancelled'), '已取消')
  assert.match(app, /startupUpdatePhase === 'manifest'/)
  assert.doesNotMatch(app, /gitee-token|github-token|Github API [123]|Gitee API [123]/)
})

test('manifest errors and route labels have all supported UI translations', () => {
  assert.equal(translations.translateFor('已取消', 'en'), 'Cancelled')
  assert.notEqual(translations.translateFor('已取消', 'ko'), '已取消')
  for (const locale of ['zh-TW', 'en', 'ko']) {
    for (const key of ['Gitee 更新清单', 'GitHub 更新清单', 'GitHub 内置代理', '更新清单请求失败', '更新清单格式无效', '更新清单不是稳定版本', '更新清单读取失败', '更新清单过大', '读取厨房状态', '初始化失败，请重新启动 MOD 管理器。']) {
      assert.notEqual(translations.translateFor(key, locale), key, `${locale}: ${key}`)
    }
    assert.notEqual(translations.translateFor('更新清单 HTTP 429', locale), '更新清单 HTTP 429')
    assert.match(translations.translateFor('更新清单 HTTP 429', locale), /429/)
  }
})

test('no-update transitions out of the updater before pending kitchen loading and cannot flash skipped proxies', async () => {
  const harness = startupRuntime.createStartupHarness(Vue.ref, Vue.computed)
  const pending = harness.start()
  await harness.enteredDashboard
  try {
    assert.equal(harness.state.startupUpdateChecking.value, false, 'Kitchen loading must not remain in updater checking state')
    const html = await startupHtml(harness)
    assert.doesNotMatch(html, /update-channel-statuses|update-region|GitHub 内置代理/)
    assert.match(html, /startup-loading-overlay/)
    assert.match(html, /读取厨房状态/)
    harness.sendChannel({ region: 'mainland-china', channel: { id: 'github-proxy:late', provider: 'github', status: 'skipped' } })
    harness.sendPhase({ phase: 'manifest', region: 'mainland-china' })
    assert.doesNotMatch(await startupHtml(harness), /update-channel-statuses|update-region|GitHub 内置代理/)
  } finally {
    harness.resolveDashboard()
    await pending
  }
})

test('update UI never renders unrequested routes, while a failed update retains the manual decision', async () => {
  const harness = startupRuntime.createStartupHarness(Vue.ref, Vue.computed)
  harness.state.appUpdateCheck.value = { region: 'mainland-china', channels: [
    { id: 'gitee-direct', provider: 'gitee', status: 'checking' },
    { id: 'github-proxy:unused', provider: 'github', status: 'skipped' },
  ] }
  assert.doesNotMatch(await startupHtml(harness), /github-proxy:unused|GitHub 内置代理|未请求/)
  const failed = startupRuntime.createStartupHarness(Vue.ref, Vue.computed, { ok: false, message: 'Please update manually' })
  const pending = failed.start()
  await failed.enteredDashboard
  assert.equal(failed.state.updateCheckFailed.value, true)
  assert.match(await startupHtml(failed), /Please update manually/)
  failed.resolveDashboard()
  await pending
  failed.continueAfterUpdateSkip()
  assert.equal(failed.state.updateCheckFailed.value, false)
})

test('kitchen load failure releases its own busy state without reviving update channels', async () => {
  const harness = startupRuntime.createStartupHarness(Vue.ref, Vue.computed)
  const pending = harness.start()
  await harness.enteredDashboard
  harness.rejectDashboard(new Error('dashboard timeout'))
  await assert.rejects(pending, /dashboard timeout/)
  assert.equal(harness.state.startupLoading.value, false)
  assert.equal(harness.state.startupUpdateChecking.value, false)
  assert.equal(harness.state.updateCheckFailed.value, false)
  assert.equal(harness.state.appUpdateCheck.value, null)
})
