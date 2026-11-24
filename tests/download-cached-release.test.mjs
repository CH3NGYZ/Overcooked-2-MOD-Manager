import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import test from 'node:test'

// Execute the real selection/composables; only UI and service boundaries are replaced.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'fixture:download-dialog', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* directory below */ }
      try { return next(`${specifier}/index.ts`, context) } catch { /* original error below */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    const fixture = (source) => ({ format: 'module', shortCircuit: true, source })
    if (url === 'fixture:download-dialog') return fixture(`
      export const useDialog = () => ({warning: options => {
        globalThis.__cachedDownload.dialogs.push(options)
        options.onPositiveClick()
      }})`)
    if (url.endsWith('/src/modChanges/store.ts')) return fixture(`export const useModChanges=()=>({run:async request=>{globalThis.__cachedDownload.plans.push(request);return {ok:true,message:''}}})`)
    if (url.endsWith('/src/composables/useDropRouting.ts')) return fixture(`
      export const useDropRouting = () => ({ acceptDrop: () => { throw new Error('Online releases do not enter local package routing') } })`)
    if (url.endsWith('/src/services/tauri.ts')) return fixture(`
      export const watchLevelSetZipProgress = async () => () => {}
      export const api = new Proxy({}, {get: (_, key) => (...args) => {
        const state = globalThis.__cachedDownload
        state.calls.push([key, ...args])
        if (key === 'stageMapRelease') return state.stage(...args)
        throw new Error('Unexpected API: ' + key)
      }})`)
    if (url.endsWith('/src/useNotify.ts')) return fixture(`
      export const useNotify = () => ({error: text => globalThis.__cachedDownload.errors.push(text),
        warning: () => {}, success: () => {}})`)
    if (url.endsWith('/src/i18n.ts')) return fixture(`
      export const t = key => key
      export const pickI18n = (messages, fallback) => messages?.['zh-CN'] || fallback`)
    if (url.endsWith('/src/locale/backend.ts')) return fixture(`
      export const backendMessageOf = String
      export const translateBackendMessage = String`)
    if (url.endsWith('/src/composables/useDashboardStore.ts')) return fixture(`
      export const useDashboard = () => ({config:{value:null},mods:{value:[]},game:{value:null}})`)
    if (url.endsWith('/src/composables/useMapSources.ts')) return fixture(`
      export const useMapSources = () => ({maps:{get value(){return globalThis.__cachedDownload.maps}}})`)
    return next(url, context)
  },
})

const { cachedInstallRelease } = await import('../src/repositoryAssets.ts')
const { useModRepositoryPreview } = await import('../src/composables/useModRepositoryPreview.ts')
const { useLevelSetImport } = await import('../src/composables/useLevelSetImport.ts')
const { downloadTaskState } = await import('../src/downloadTask.ts')
const asset = (name) => ({ name, size: 1, download_url: `https://example.com/${name}`, digest: null })
const release = (tag_name, overrides = {}) => ({
  tag_name, version: tag_name.replace(/^v/, ''), name: tag_name, body: '',
  prerelease: false, draft: false, assets: [asset('package.zip')], ...overrides,
})
const repo = (overrides = {}) => ({
  id: 'mod', name: 'MOD', download_mode: 'release', cached_releases: [release('v2.0')], ...overrides,
})
const map = (overrides = {}) => ({
  id: 'map', name: 'Map', latest_tag: 'v2.0', cached_releases: [release('v2.0')], ...overrides,
})
function reset() {
  globalThis.__cachedDownload = {
    calls: [], plans: [], errors: [], dialogs: [], maps: [],
    stage: async () => ({ staged: false, path: 'fixture.zip', sets: [] }),
  }
  downloadTaskState.running = null
  downloadTaskState.failure = null
  return globalThis.__cachedDownload
}

test('cached selection follows explicit tag, pinned tag/version, then first snapshot; no missing fallback', () => {
  const releases = [release('v2.0'), release('v1.0')]
  assert.equal(cachedInstallRelease(releases), releases[0])
  assert.equal(cachedInstallRelease(releases, 'v1.0', 'v2.0'), releases[1])
  assert.equal(cachedInstallRelease(releases, undefined, '1.0'), releases[1])
  assert.equal(cachedInstallRelease(releases, undefined, 'v1.0'), releases[1])
  for (const tag of ['', 'missing', '1.0']) assert.equal(cachedInstallRelease(releases, tag), undefined)
  for (const pin of ['', 'missing']) assert.equal(cachedInstallRelease(releases, undefined, pin), undefined)
  assert.equal(cachedInstallRelease([], 'v2.0'), undefined)
})

test('unset Rust Option values select the latest snapshot instead of a null-version release', () => {
  const releases = [release('v2.0'), release('custom-tag', { version: null })]
  assert.equal(cachedInstallRelease(releases, null, null), releases[0])
  assert.equal(cachedInstallRelease(releases, undefined, null), releases[0])
  assert.equal(cachedInstallRelease(releases, null, 'custom-tag'), releases[1])
  assert.equal(cachedInstallRelease(releases, '', null), undefined)
  assert.equal(cachedInstallRelease(releases, null, ''), undefined)
})

test('MOD empty/missing snapshots stop before dialogs, dependency handling, busy state or IPC', async () => {
  for (const [source, tag] of [
    [repo({ cached_releases: [] }), undefined],
    [repo(), 'missing'],
    [repo({ pinned_version: 'missing' }), undefined],
  ]) {
    const state = reset()
    let sideEffects = 0
    const flow = useModRepositoryPreview({
      onBusyChange: () => { sideEffects++ },
    })
    await flow.previewModRepository(source, tag)
    assert.equal(sideEffects, 0)
    assert.deepEqual(state.calls, [])
    assert.deepEqual(state.dialogs, [])
    assert.deepEqual(state.errors, ['source.noInstallableRelease'])
    assert.equal(downloadTaskState.running, null)
  }
})

test('map empty/missing selected snapshots stop before starting a download or calling IPC', async () => {
  for (const source of [map({ cached_releases: [] }), map({ latest_tag: 'missing' })]) {
    const state = reset()
    state.maps = [source]
    assert.equal(await useLevelSetImport().installRelease('map'), false)
    assert.deepEqual(state.calls, [])
    assert.deepEqual(state.errors, ['mapSource.releaseMissing'])
    assert.equal(downloadTaskState.running, null)
  }
})

test('map explicit versions and assets are forwarded directly from the cached snapshot', async () => {
  const state = reset()
  state.maps = [map({ cached_releases: [release('v2.0'), release('v1.0')] })]
  await useLevelSetImport().installRelease('map', 'v1.0', 'chosen.zip')
  const [[command, mapId, tag, assetName, onProgress]] = state.calls
  assert.equal(state.calls.length, 1)
  assert.deepEqual([command, mapId, tag, assetName], ['stageMapRelease', 'map', 'v1.0', 'chosen.zip'])
  assert.equal(typeof onProgress, 'function')
  assert.deepEqual(state.errors, [])
})

test('map missing explicit version never downloads the cached latest version instead', async () => {
  const state = reset()
  state.maps = [map()]
  assert.equal(await useLevelSetImport().installRelease('map', 'missing', 'chosen.zip'), false)
  assert.deepEqual(state.calls, [])
  assert.deepEqual(state.errors, ['mapSource.releaseMissing'])
})

test('map retry keeps its version when the cached latest changes, and refuses a removed version', async () => {
  const state = reset()
  const source = map()
  state.maps = [source]
  state.stage = async () => { throw new Error('download failed') }
  await useLevelSetImport().installRelease('map')
  const retry = downloadTaskState.failure.retry
  source.latest_tag = 'v3.0'
  source.cached_releases.unshift(release('v3.0'))
  retry()
  await new Promise(resolve => setImmediate(resolve))
  assert.deepEqual(state.calls.map(([command, mapId, tag, assetName, onProgress]) => [command, mapId, tag, assetName, typeof onProgress]),
    Array(2).fill(['stageMapRelease', 'map', 'v2.0', undefined, 'function']))
  assert.notEqual(state.calls[0][4], state.calls[1][4], 'each retry owns its progress callback')
  source.cached_releases = [release('v3.0')]
  retry()
  await new Promise(resolve => setImmediate(resolve))
  assert.equal(state.calls.length, 2)
  assert.equal(state.errors.at(-1), 'mapSource.releaseMissing')
})

test('MOD selection freezes the exact cached root version and asset in one batch request', async () => {
  for (const [source, tag, assetName, expected] of [
    [repo({pinned_version:null}), undefined, undefined, 'release:mod:v2.0'],
    [repo({pinned_version:'1.0',cached_releases:[release('v2.0'),release('v1.0',{prerelease:true})]}),undefined,undefined,'release:mod:v1.0'],
    [repo({cached_releases:[release('v2.0'),release('v1.0')]}),'v1.0','chosen.zip','release:mod:v1.0'],
    [repo({download_mode:'repository_file',cached_releases:[]}),undefined,undefined,'raw:mod'],
  ]) {
    const state=reset()
    await useModRepositoryPreview().previewModRepository(source,tag,assetName)
    assert.deepEqual(state.plans,[{roots:[{node_id:'mod:mod',candidate_id:expected,enabled:true,...(assetName?{asset_name:assetName}:{})}]}])
    assert.deepEqual(state.calls,[],'version planning and execution are handled by the backend batch')
    assert.deepEqual(state.errors,[]);assert.deepEqual(state.dialogs,[])
  }
})

test('both map pages reject missing cached versions before staging their exact selected tag', () => {
  for (const name of ['MapsOnlineView','LevelsView']) {
    const source=readFileSync(new URL(`../src/views/${name}.vue`,import.meta.url),'utf8')
    const start=source.indexOf(name==='MapsOnlineView'?'async function install(':'async function runCardAction(')
    const body=source.slice(start,source.indexOf('\n}',start))
    assert.ok(body.indexOf('cachedInstallRelease(')>=0)
    assert.ok(body.indexOf("message.error(t('mapSource.releaseMissing'))")<body.indexOf('installRelease('))
    assert.ok(body.indexOf('const targetTag = release.tag_name')<body.indexOf('installRelease('))
  }
})

test('the shared workflow commits one planId and the UI never executes individual action rows', () => {
  const workflow=readFileSync(new URL('../src/modChanges/workflow.ts',import.meta.url),'utf8')
  const ui=readFileSync(new URL('../src/modChanges/ChangePlanDialog.vue',import.meta.url),'utf8')
  assert.equal([...workflow.matchAll(/backend\.execute\(/g)].length,1)
  assert.match(workflow,/recoveryApproval \? \[plan\.plan_id, recoveryApproval\] : \[plan\.plan_id\]/)
  assert.match(workflow,/backend\.execute\(\.\.\.executionArgs\)/)
  assert.doesNotMatch(ui,/installModRepository|confirmModInstall|uninstallLocalMod|importLevelSets/)
})
