import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { setImmediate as tick } from 'node:timers/promises'
import test from 'node:test'
import ts from 'typescript'
import { effectScope } from 'vue'

function moduleUrl(source, imports = {}) {
  let { outputText } = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  })
  for (const [name, target] of Object.entries(imports)) outputText = outputText.replaceAll(`from '${name}'`, `from '${target}'`)
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`
}

const readSource = (path) => readFileSync(new URL(path, import.meta.url), 'utf8')
// The extracted engine composables call window.setTimeout like they do in the app.
globalThis.window ??= globalThis
const catalogueUrl = moduleUrl(readSource('../src/communityRepositories.ts'))
const stubs = {
  '../services/tauri': moduleUrl('export const api = {}\nexport function watchGameState() { return Promise.resolve(() => undefined) }\nexport function watchDownloadProgress() { return Promise.resolve(() => undefined) }\nexport function watchAppUpdateChannel() { return Promise.resolve(() => undefined) }\nexport function watchAppUpdatePhase() { return Promise.resolve(() => undefined) }'),
  '../i18n': moduleUrl("export const t = (key) => key\nexport const locale = { value: 'zh-CN' }\nexport function localizedRepositorySourceName(source) { return source.nickname }\nexport function useLocaleTranslation() { return { locale, setLocale: () => undefined } }"),
}
const sourcesUrl = moduleUrl(readSource('../src/repositorySources.ts'), { './i18n': stubs['../i18n'] })
const communityUrl = moduleUrl(readSource('../src/composables/useCommunityRepositories.ts'), {
  vue: import.meta.resolve('vue'), '../communityRepositories': catalogueUrl,
})
// The engine composables are executed for real; the Tauri api is injected per fixture.
const assetsUrl = moduleUrl(readSource('../src/repositoryAssets.ts'), { './communityRepositories': catalogueUrl })
const repositoryNamesUrl = moduleUrl(readSource('../src/repositoryNames.ts'))
const dashboardUrl = moduleUrl(readSource('../src/composables/useDashboard.ts'), { ...stubs, vue: import.meta.resolve('vue') })
const repositoryRefreshUrl = moduleUrl(readSource('../src/composables/useRepositoryRefresh.ts'), { ...stubs, '../repositoryNames': repositoryNamesUrl, vue: import.meta.resolve('vue') })
const sourceRefreshUrl = moduleUrl(readSource('../src/composables/useSourceRefresh.ts'), {
  ...stubs, '../repositoryNames': repositoryNamesUrl,
  '../communityRepositories': catalogueUrl, '../repositorySources': sourcesUrl, '../repositoryAssets': assetsUrl,
  vue: import.meta.resolve('vue'),
})
const appScript = readSource('../src/App.vue').match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1]
const appSource = ts.createSourceFile('App.ts', appScript, ts.ScriptTarget.Latest, true)
// createCommunity and saveRepositorySourceChanges still live in App.vue; everything
// else runs through the extracted engine composables below.
const names = new Set(['createCommunity', 'saveRepositorySourceChanges'])
const declarations = appSource.statements.flatMap((statement) => {
  if (ts.isVariableStatement(statement)) return statement.declarationList.declarations.some((node) => names.has(node.name.getText(appSource))) ? [statement.getText(appSource)] : []
  return ts.isFunctionDeclaration(statement) && names.has(statement.name?.text) ? [statement.getText(appSource)] : []
}).join('\n')
const { createAppFixture } = await import(moduleUrl(`
import { computed, nextTick, reactive, ref } from 'vue'
import { COMMUNITY_SOURCE_ID, GITEE_COMMUNITY_SOURCE_ID, GITEE_COMMUNITY_TOPIC_URL, communitySourceId, communityProviderMatchesRegion, isCommunitySource, isCommunityRepository } from '${catalogueUrl}'
import { repositoryRegionMatches, repositorySourceKey } from '${sourcesUrl}'
import { useCommunityRepositories } from '${communityUrl}'
import { useDashboard } from '${dashboardUrl}'
import { useRepositoryRefresh } from '${repositoryRefreshUrl}'
import { useSourceRefresh } from '${sourceRefreshUrl}'
export function createAppFixture(api, snapshot) {
  const window = globalThis
  let appUnmounted = false
  const tutorial = { blocksRepositoryRefresh: ref(false), visible: ref(false), previewing: ref(false), repositoryLoadingActive: ref(false) }
  const errors = []
  const successes = []
  const t = (key) => key
  const localizedRepositorySourceName = (source) => source.nickname
  const localizedRepositoryName = (repo) => repo.name
  const showError = (reason) => errors.push(reason instanceof Error ? reason.message : String(reason))
  const showSuccess = (message) => successes.push(message)
  const run = async (_label, action) => action()
  const closeRepositorySourceManager = () => undefined
  const applyRepositorySourceDraft = () => { throw new Error('Unexpected open source editor') }
  const dashboard = ref(structuredClone(snapshot))
  const locale = ref('zh-CN')
  const repositoryReleaseReady = reactive({})
  const sourceLoading = reactive({})
  const sourceRefreshingManual = reactive({})
  const repositorySourceEditorOpen = ref(false)
  const repositorySourceDraft = ref(structuredClone(snapshot.config.repository_sources))
  const config = computed(() => dashboard.value?.config)
  const {
    getUpdateQueue, refreshDashboardSerialized, refreshConfigSerialized,
  } = useDashboard({
    dashboard, run, requestConfirmation: async () => true, applyStartupZoom: async () => {},
    isBlocked: () => false, onRuntimeChanged: () => {}, currentWinhttp: () => 'missing', api,
  })
  const {
    repositoryRefreshing, repositoryRefreshBatchActive, repositoryRefreshTasks,
    refreshRepositoryBatch, refreshRepo, refreshRepositoryEntry,
  } = useRepositoryRefresh({
    dashboard, locale, repositoryReleaseReady, run, showError, showSuccess,
    isUnmounted: () => appUnmounted,
    getRepos: () => repos.value, getInstalledMods: () => [], getModReleaseRepositoryIds: () => new Set(),
    getMappedReposAvailable: () => false, api,
  })
  const repos = computed(() => (config.value?.repositories ?? []).filter((repo) => repositoryRegionMatches(repo.region, config.value?.region)
    && (!repo.source_id || activeRepositorySourceIds.value.has(repo.source_id))))
  ${declarations}
  const community = createCommunity('github')
  const giteeCommunity = createCommunity('gitee')
  const communities = [community, giteeCommunity]
  const {
    sourceDocumentsLoading, initialRepositorySourcesLoaded, activeRepositorySources, activeRepositorySourceIds,
    sourceMatchesRegion, sourceDescendantIds, refreshRootSources, refreshAllRepositories,
    startInitialRepositoryRefresh, refreshSource, getInitialProxySync, dispose,
  } = useSourceRefresh({
    dashboard, config, repos, locale, communities,
    sourceLoading, sourceRefreshingManual, repositoryRefreshing, repositoryReleaseReady, repositoryRefreshTasks,
    refreshRepositoryBatch, refreshDashboardSerialized, refreshConfigSerialized, getUpdateQueue,
    isUnmounted: () => appUnmounted,
    isRepositoryRefreshBlocked: () => tutorial.blocksRepositoryRefresh.value,
    isTutorialOverlayActive: () => tutorial.visible.value || tutorial.previewing.value,
    getTutorialRepositoryLoadingActive: () => tutorial.repositoryLoadingActive.value,
    showError, api,
  })
  return {
    dashboard, tutorial, community, giteeCommunity, communities, errors, successes, repositoryRefreshTasks, sourceRefreshingManual,
    initialRepositorySourcesLoaded, startInitialRepositoryRefresh, refreshAllRepositories,
    saveRepositorySourceChanges, refreshSource, refreshRepositoryEntry,
    snapshots: () => getUpdateQueue(),
    dispose: () => { appUnmounted = true; dispose() },
  }
}
`, { vue: import.meta.resolve('vue') }))

function deferred() {
  let resolve
  let reject
  const promise = new Promise((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

async function until(predicate, message) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (predicate()) return
    await tick()
  }
  assert.ok(predicate(), message)
}

function source(id, patch = {}) {
  return { id, nickname: id, url: `https://example.invalid/raw/main/${id}.toml`, builtin: false, ...patch }
}

function repository(id, source_id, patch = {}) {
  return { id, name: id, provider: 'github', username: 'Owner', repository: id, source_id,
    enabled: true, download_mode: 'release', channel: 'all', cached_releases: [], ...patch }
}

function fixture(options = {}) {
  const root = source('official', { builtin: true })
  const child = source('official-child', { builtin: true, parent_id: root.id })
  const roots = [root, ...(options.roots ?? [])]
  const documents = { official: { sources: [child] }, 'official-child': { repositories: [repository('official-mod', child.id)] }, ...options.documents }
  const releaseGate = deferred()
  const trace = []
  const searches = []
  const config = { region: options.region ?? 'chinese_mainland', repositories: [...(options.repositories ?? [])], repository_sources: roots,
    github_proxies: { release_proxies: [], raw_proxies: [], github_api_proxies: [], gitee_api_proxies: [] } }
  const backend = { config, game: { kitchen_running: false }, mods: [], updates: [] }
  const clone = (value) => structuredClone(value)
  function clear(ids) {
    const descendants = new Set(ids)
    let size
    do {
      size = descendants.size
      for (const item of config.repository_sources) if (descendants.has(item.parent_id)) descendants.add(item.id)
    } while (size !== descendants.size)
    config.repositories = config.repositories.filter((repo) => !descendants.has(repo.source_id))
    config.repository_sources = config.repository_sources.filter((item) => ids.includes(item.id) || !descendants.has(item.id))
  }
  const api = {
    config: async () => clone(config),
    dashboard: async () => { trace.push('dashboard'); return clone(backend) },
    saveRepositorySources: async (sources) => { config.repository_sources = clone(sources); return clone(config) },
    clearRepositorySourceCache: async (ids) => { trace.push('clear'); clear(ids); return clone(config) },
    importRepositorySource: async (id, _url, builtin) => {
      const key = builtin ? `${id}:builtin` : id
      trace.push(`raw:start:${key}`)
      clear([id])
      await options.rawGates?.[key]?.promise
      const document = documents[key] ?? {}
      if (document.parseError) throw new Error(document.parseError)
      if (document.accessError) return { access_error: document.accessError, builtin_fallback_available: !!document.fallback }
      config.repository_sources.push(...clone(document.sources ?? []))
      config.repositories.push(...clone(document.repositories ?? []))
      trace.push(`raw:done:${key}`)
      return { access_error: null, builtin_fallback_available: !!document.fallback }
    },
    refreshGithubProxies: async () => {
      trace.push('proxies:start')
      await options.proxyGate?.promise
      config.github_proxies.github_api_proxies = [{ id: 'loaded-api-proxy', enabled: true }]
      trace.push('proxies:done')
      return clone(config)
    },
    refreshRepository: async (id) => {
      trace.push(`release:start:${id}`)
      if (!id.startsWith('community-')) await releaseGate.promise
      await options.releaseGates?.[id]?.promise
      if (options.releaseFailures?.includes(id)) {
        const repo = config.repositories.find((item) => item.id === id)
        if (repo) { repo.cached_releases = []; repo.last_error = 'Release unavailable' }
        throw new Error('Release unavailable')
      }
      const releases = [{ tag_name: 'v2.0.0', version: '2.0.0', prerelease: false, draft: false, assets: [] }]
      const repo = config.repositories.find((item) => item.id === id)
      if (repo) repo.cached_releases = clone(releases)
      trace.push(`release:done:${id}`)
      return releases
    },
    refreshCommunityRepositories: async (provider = 'github') => {
      if (provider === 'gitee') {
        trace.push('gitee:start')
        await options.giteeGate?.promise
        if (options.giteeFails) throw new Error('Gitee unavailable')
        if (!config.repository_sources.some((item) => item.id === 'gitee-community')) config.repository_sources.push(source('gitee-community'))
        config.repositories = config.repositories.filter((repo) => repo.source_id !== 'gitee-community')
        const found = options.giteeRepository ? [repository('community-gitee', 'gitee-community', { provider: 'gitee' })] : []
        config.repositories.push(...found)
        config.gitee_community_discovery = { repository_ids: found.map((repo) => repo.id) }
        trace.push('gitee:done')
        return clone(config)
      }
      trace.push('search:start')
      searches.push({ repositories: config.repositories.map((repo) => repo.id),
        sources: config.repository_sources.map((item) => item.id), proxies: config.github_proxies.github_api_proxies.map((proxy) => proxy.id) })
      await options.searchGate?.promise
      if (options.searchFails) throw new Error('GitHub unavailable')
      if (!config.repository_sources.some((item) => item.id === 'github-community')) config.repository_sources.push(source('github-community'))
      config.repositories = config.repositories.filter((repo) => repo.source_id !== 'github-community')
      config.repositories.push(repository('community-mod', 'github-community'))
      config.community_discovery = { repository_ids: ['community-mod'] }
      trace.push('search:done')
      return clone(config)
    },
    cancelCommunitySearch: async (provider = 'github') => { trace.push(provider === 'gitee' ? 'gitee:cancel' : 'search:cancel') },
  }
  const scope = effectScope()
  const app = scope.run(() => createAppFixture(api, backend))
  return { ...app, trace, searches, backend, releaseGate,
    async close() {
      releaseGate.resolve()
      for (const gate of Object.values(options.rawGates ?? {})) gate.resolve()
      for (const gate of Object.values(options.releaseGates ?? {})) gate.resolve()
      options.proxyGate?.resolve()
      options.searchGate?.resolve()
      options.giteeGate?.resolve()
      await until(() => app.repositoryRefreshTasks.size === 0 && !app.communities.some((item) => item.refreshing.value)
        && !Object.values(app.sourceRefreshingManual).some(Boolean), 'All fixture requests settle during cleanup')
      await app.snapshots()
      app.dispose()
      scope.stop()
    },
  }
}

test('international refreshes never search or refresh Gitee community, including saved unregional entries', async () => {
  const saved = repository('saved-gitee', 'gitee-community', { provider: 'gitee' })
  const app = fixture({ region: 'international', roots: [source('gitee-community')], repositories: [saved] })
  try {
    app.startInitialRepositoryRefresh()
    app.releaseGate.resolve()
    await until(() => app.trace.includes('search:done') && !app.community.refreshing.value
      && !Object.values(app.sourceRefreshingManual).some(Boolean), 'International initial refresh settles')
    assert.equal(app.trace.includes('gitee:start'), false)
    assert.equal(app.trace.includes('release:start:saved-gitee'), false)
    assert.equal(app.giteeCommunity.started.value, false)
    assert.equal(app.giteeCommunity.available.value, false)
    assert.equal(await app.giteeCommunity.refresh(), false, 'Manual discovery cannot bypass the region gate')
    await app.refreshAllRepositories()
    assert.equal(app.searches.length, 2, 'GitHub refresh-all is unaffected')
    assert.equal(app.trace.includes('gitee:start'), false)
    assert.equal(app.trace.includes('release:start:saved-gitee'), false)
    assert.ok(app.dashboard.value.config.repositories.some((repo) => repo.id === saved.id), 'Saved bindings are retained')
  } finally { await app.close() }
})

for (const slow of ['github', 'gitee']) {
  test(`${slow} discovery cannot delay the other provider after the official Raw commit`, async () => {
    const gate = deferred()
    const app = fixture({ [slow === 'github' ? 'searchGate' : 'giteeGate']: gate, giteeRepository: true })
    try {
      app.startInitialRepositoryRefresh()
      await until(() => app.trace.includes('search:start') && app.trace.includes('gitee:start'), 'Both providers start with official Releases still pending')
      assert.equal(app.trace.includes('release:done:official-mod'), false)
      const fast = slow === 'github' ? 'community-gitee' : 'community-mod'
      await until(() => app.trace.includes(`release:done:${fast}`), 'The fast provider independently reaches its Release refresh')
      const pending = slow === 'github' ? app.community : app.giteeCommunity
      assert.equal(pending.searching.value, true)
      assert.equal(app.trace.includes('raw:done:official-child'), true)
    } finally { await app.close() }
  })
}

test('Gitee verification failure does not fail GitHub or clear official entries', async () => {
  const app = fixture({ giteeFails: true })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.giteeCommunity.error.value === 'Gitee unavailable', 'Gitee shows its own failure')
    await until(() => app.trace.includes('release:done:community-mod'), 'GitHub still refreshes its assets')
    assert.equal(app.community.error.value, '')
    assert.equal(app.repositoryRefreshTasks.has('official-mod'), true)
    assert.equal(app.giteeCommunity.searching.value, false)
    assert.deepEqual(app.errors, [])
  } finally { await app.close() }
})

test('GitHub failure does not prevent successful Gitee discovery and assets', async () => {
  const app = fixture({ searchFails: true, giteeRepository: true })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.trace.includes('release:done:community-gitee'), 'Gitee reaches asset discovery while GitHub fails')
    assert.equal(app.community.error.value, 'GitHub unavailable')
    assert.equal(app.giteeCommunity.error.value, '')
    assert.equal(app.repositoryRefreshTasks.has('official-mod'), true)
    assert.deepEqual(app.errors, [])
  } finally { await app.close() }
})

for (const slow of ['github', 'gitee']) {
  test(`a pending ${slow} community Release does not block manually refreshing the other discovery`, async () => {
    const pending = deferred()
    const slowId = slow === 'github' ? 'community-mod' : 'community-gitee'
    const app = fixture({ giteeRepository: true, releaseGates: { [slowId]: pending } })
    const fast = slow === 'github' ? app.giteeCommunity : app.community
    const fastStart = slow === 'github' ? 'gitee:start' : 'search:start'
    try {
      app.startInitialRepositoryRefresh()
      await until(() => app.trace.includes(`release:start:${slowId}`) && !fast.refreshing.value, 'The slow Release remains pending after the other discovery finishes')
      assert.equal(await fast.refresh(), true)
      assert.equal(app.trace.filter((event) => event === fastStart).length, 2)
      assert.equal(app.trace.includes(`release:done:${slowId}`), false)
      assert.equal(app.trace.includes('release:done:official-mod'), false)
    } finally { await app.close() }
  })
}

for (const entry of ['startInitialRepositoryRefresh', 'refreshAllRepositories', 'saveRepositorySourceChanges']) {
  test(`${entry} starts community discovery after Raw imports without waiting for official Releases`, async () => {
    const app = fixture()
    try {
      void app[entry]()
      await until(() => app.trace.includes('release:start:official-mod'), 'Official Release request starts')
      await until(() => app.trace.includes('search:start'), 'Search starts while the official Release is still pending')
      await until(() => app.trace.includes('gitee:start'), 'The same Raw completion node starts Gitee')
      assert.equal(app.trace.includes('release:done:official-mod'), false)
      assert.equal(app.repositoryRefreshTasks.has('official-mod'), true)
      assert.equal(app.initialRepositorySourcesLoaded.value, true)
      assert.equal(app.sourceRefreshingManual.official, true)
      assert.deepEqual(app.searches[0].repositories, ['official-mod'])
      assert.deepEqual(app.searches[0].sources, ['official', 'official-child'])
      assert.equal(app.trace.filter((event) => event === 'search:start').length, 1)
      await until(() => app.trace.includes('release:start:community-mod'), 'Community Releases also run before official Releases finish')
    } finally { await app.close() }
    assert.equal(app.dashboard.value.config.repositories.find((repo) => repo.id === 'official-mod').cached_releases.length, 1)
    assert.equal(app.dashboard.value.config.repositories.find((repo) => repo.id === 'community-mod').cached_releases.length, 1)
  })
}

test('all official child Raw files must finish and commit before discovery starts', async () => {
  const pendingChild = deferred()
  const slowChild = source('slow-child', { builtin: true, parent_id: 'official' })
  const app = fixture({ rawGates: { 'slow-child': pendingChild }, documents: {
    official: { sources: [source('official-child', { builtin: true, parent_id: 'official' }), slowChild] },
    'slow-child': { repositories: [repository('slow-mod', slowChild.id)] },
  } })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.trace.includes('release:start:official-mod'), 'The first child can already refresh its Release')
    assert.equal(app.trace.includes('search:start'), false)
    assert.equal(app.community.available.value, false)
    pendingChild.resolve()
    await until(() => app.trace.includes('search:start'), 'Completing the remaining Raw file releases discovery')
    assert.deepEqual(app.searches[0].repositories.toSorted(), ['official-mod', 'slow-mod'])
    assert.equal(app.trace.includes('release:done:official-mod'), false)
    assert.equal(app.trace.includes('release:done:slow-mod'), false)
  } finally { await app.close() }
})

test('discovery waits for the API proxy commit, not for official Release completion', async () => {
  const proxyGate = deferred()
  const app = fixture({ proxyGate })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.trace.includes('release:start:official-mod'), 'Official Raw files have finished')
    assert.equal(app.trace.includes('search:start'), false)
    proxyGate.resolve()
    await until(() => app.trace.includes('search:start'), 'Proxy configuration releases the pending search')
    assert.deepEqual(app.searches[0].proxies, ['loaded-api-proxy'])
    assert.equal(app.trace.includes('release:done:official-mod'), false)
  } finally { await app.close() }
})

for (const failure of ['index', 'children']) {
  test(`${failure} access failure starts discovery after the builtin tree is imported, with Releases pending`, async () => {
    const bundledChild = source('bundled-child', { builtin: true, parent_id: 'official', url: 'bundled-child.toml' })
    const app = fixture({ documents: {
      official: failure === 'index' ? { accessError: 'Index unreachable', fallback: true }
        : { sources: [source('official-child', { builtin: true, parent_id: 'official' })], fallback: true },
      'official-child': { accessError: 'Child unreachable' },
      'official:builtin': { sources: [bundledChild] },
      'bundled-child': { repositories: [repository('bundled-mod', bundledChild.id)] },
    } })
    try {
      app.startInitialRepositoryRefresh()
      await until(() => app.trace.includes('search:start'), 'Bundled source imports release community discovery')
      assert.deepEqual(app.searches[0].repositories, ['bundled-mod'])
      assert.deepEqual(app.searches[0].sources, ['official', 'bundled-child'])
      assert.equal(app.repositoryRefreshTasks.has('bundled-mod'), true)
      assert.equal(app.trace.includes('release:done:bundled-mod'), false)
      assert.equal(app.trace.filter((event) => event === 'raw:start:official:builtin').length, 1)
      assert.equal(app.trace.filter((event) => event === 'search:start').length, 1)
    } finally { await app.close() }
  })
}

test('unrelated custom Raw requests do not delay discovery after the official source tree loads', async () => {
  const customGate = deferred()
  const custom = source('custom')
  const app = fixture({ roots: [custom], rawGates: { custom: customGate }, documents: {
    custom: { repositories: [repository('custom-mod', custom.id)] },
  } })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.trace.includes('search:start'), 'The official source alone releases community discovery')
    assert.equal(app.trace.includes('raw:done:custom'), false)
    assert.equal(app.community.available.value, true)
    assert.equal(app.sourceRefreshingManual.custom, true)
  } finally { await app.close() }
  assert.ok(app.dashboard.value.config.repositories.some((repo) => repo.id === 'custom-mod'))
  assert.ok(app.dashboard.value.config.repositories.some((repo) => repo.id === 'community-mod'))
})

test('tutorial gating prevents both Raw loading and discovery until it ends', async () => {
  const app = fixture()
  try {
    app.tutorial.blocksRepositoryRefresh.value = true
    app.startInitialRepositoryRefresh()
    await tick()
    assert.deepEqual(app.trace, [])
    assert.equal(await app.community.refresh(), false)
    app.tutorial.blocksRepositoryRefresh.value = false
    app.startInitialRepositoryRefresh()
    await until(() => app.trace.includes('search:start'), 'The completed tutorial releases source loading and discovery')
    assert.equal(app.trace.includes('release:done:official-mod'), false)
  } finally { await app.close() }
})

test('community failure is isolated while official Releases continue and finish normally', async () => {
  const app = fixture({ searchFails: true })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.community.error.value === 'GitHub unavailable', 'The community search exposes its independent error')
    assert.equal(app.community.searching.value, false)
    assert.equal(app.community.refreshing.value, false)
    assert.equal(app.repositoryRefreshTasks.has('official-mod'), true)
    assert.ok(app.dashboard.value.config.repositories.some((repo) => repo.id === 'official-mod'))
  } finally { await app.close() }
  assert.equal(app.dashboard.value.config.repositories.find((repo) => repo.id === 'official-mod').cached_releases.length, 1)
  assert.deepEqual(app.errors, [])
})

test('a repeated search waits only for an older community Release and coalesces duplicate refreshes', async () => {
  const releaseGates = {}
  const app = fixture({ releaseGates })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.trace.includes('search:done') && !app.community.refreshing.value, 'The first community batch finishes independently')
    const previousRelease = deferred()
    releaseGates['community-mod'] = previousRelease
    const repo = app.dashboard.value.config.repositories.find((item) => item.id === 'community-mod')
    const previous = app.refreshRepositoryEntry(repo)
    const search = app.community.refresh()
    assert.equal(app.community.refresh(), search)
    await tick()
    assert.equal(app.searches.length, 1, 'Replacing community entries must wait for their older Release request')
    previousRelease.resolve()
    assert.equal(await previous, true)
    assert.equal(await search, true)
    assert.equal(app.searches.length, 2)
    assert.equal(app.repositoryRefreshTasks.has('official-mod'), true)
    assert.equal(app.trace.includes('release:done:official-mod'), false)
  } finally { await app.close() }
})

test('each refresh-all reloads community search and Releases again after the new official Raw commit', async () => {
  const rawGates = {}
  const releaseGates = {}
  const app = fixture({ rawGates, releaseGates, giteeRepository: true })
  const count = (event) => app.trace.filter((entry) => entry === event).length
  try {
    app.startInitialRepositoryRefresh()
    app.releaseGate.resolve()
    await until(() => app.searches.length === 1 && app.communities.every((discovery) => !discovery.refreshing.value)
      && !Object.values(app.sourceRefreshingManual).some(Boolean), 'The initial refresh finishes')

    for (const round of [2, 3]) {
      rawGates['official-child'] = deferred()
      releaseGates['official-mod'] = deferred()
      releaseGates['community-mod'] = deferred()
      const task = app.refreshAllRepositories()
      await until(() => count('raw:start:official-child') === round, 'Refresh-all reloads the official Raw tree')
      assert.equal(app.searches.length, round - 1, 'The new search must wait for this round of Raw imports')
      rawGates['official-child'].resolve()
      await until(() => count('release:start:community-mod') === round, 'Refresh-all starts a fresh community Release request')
      await until(() => count('release:start:community-gitee') === round, 'Refresh-all also starts fresh Gitee discovery and Releases')
      assert.equal(app.searches.length, round)
      assert.equal(count('gitee:start'), round)
      assert.equal(count('release:done:official-mod'), round - 1, 'Official Releases must not delay community discovery')
      assert.equal(app.community.refreshing.value, true)
      releaseGates['community-mod'].resolve()
      await until(() => !app.community.refreshing.value, 'The independent community batch completes')
      releaseGates['official-mod'].resolve()
      await task
      assert.equal(count('release:done:community-mod'), round)
      assert.equal(count('release:done:community-gitee'), round)
      assert.equal(count('release:done:official-mod'), round)
      assert.deepEqual(app.dashboard.value.config.repositories.map((repo) => repo.id).sort(), ['community-gitee', 'community-mod', 'official-mod'])
    }
    assert.deepEqual(app.errors, [])
  } finally { await app.close() }
})

test('refresh-all retries a failed community search without interrupting official refreshes', async () => {
  const options = { searchFails: true }
  const app = fixture(options)
  try {
    app.startInitialRepositoryRefresh()
    app.releaseGate.resolve()
    await until(() => app.community.error.value === 'GitHub unavailable'
      && !Object.values(app.sourceRefreshingManual).some(Boolean), 'The failed community search and successful official batch finish')
    options.searchFails = false
    await app.refreshAllRepositories()
    assert.equal(app.searches.length, 2)
    assert.equal(app.trace.filter((event) => event === 'release:done:official-mod').length, 2)
    assert.equal(app.trace.filter((event) => event === 'release:done:community-mod').length, 1)
    assert.equal(app.community.error.value, '')
    assert.equal(app.community.refreshing.value, false)
    assert.deepEqual(app.errors, [])
  } finally { await app.close() }
})

test('refresh-all retries failed Gitee verification and preserves both discoveries in the final snapshot', async () => {
  const options = { giteeFails: true, giteeRepository: true }
  const app = fixture(options)
  try {
    app.startInitialRepositoryRefresh()
    app.releaseGate.resolve()
    await until(() => app.giteeCommunity.error.value === 'Gitee unavailable' && !app.community.refreshing.value
      && !Object.values(app.sourceRefreshingManual).some(Boolean), 'The first failure is isolated')
    options.giteeFails = false
    await app.refreshAllRepositories()
    assert.equal(app.trace.filter((event) => event === 'gitee:start').length, 2)
    assert.equal(app.searches.length, 2)
    assert.equal(app.giteeCommunity.error.value, '')
    assert.equal(app.dashboard.value.config.gitee_community_discovery.repository_ids[0], 'community-gitee')
    assert.equal(app.dashboard.value.config.community_discovery.repository_ids[0], 'community-mod')
    assert.deepEqual(app.dashboard.value.config.repositories.map((repo) => repo.id).sort(), ['community-gitee', 'community-mod', 'official-mod'])
  } finally { await app.close() }
})

test('a later official child reload gates discovery only until its Raw file is committed', async () => {
  const rawGates = {}
  const releaseGates = {}
  const app = fixture({ rawGates, releaseGates })
  try {
    app.startInitialRepositoryRefresh()
    app.releaseGate.resolve()
    await until(() => app.trace.includes('search:done') && !app.community.refreshing.value
      && !Object.values(app.sourceRefreshingManual).some(Boolean), 'The first source batch finishes')
    const raw = deferred()
    rawGates['official-child'] = raw
    releaseGates['official-mod'] = deferred()
    const reload = app.refreshSource('official-child', { updateDashboard: false, notify: false })
    assert.equal(app.community.available.value, false)
    assert.equal(await app.community.refresh(), false)
    raw.resolve()
    await until(() => app.community.available.value, 'The child Raw commit releases discovery')
    assert.equal(app.sourceRefreshingManual['official-child'], true)
    assert.equal(app.repositoryRefreshTasks.has('official-mod'), true)
    assert.equal(await app.community.refresh(), true)
    releaseGates['official-mod'].resolve()
    assert.equal((await reload).ok, true)
  } finally { await app.close() }
})

test('parse and Release failures do not broaden the builtin fallback conditions', async () => {
  const app = fixture({ documents: { official: { parseError: 'Invalid TOML', fallback: true } } })
  try {
    app.startInitialRepositoryRefresh()
    await until(() => app.trace.includes('search:start'), 'A failed official import does not disable independent discovery')
    assert.equal(app.trace.includes('raw:start:official:builtin'), false)
    assert.equal(app.trace.includes('release:start:official-mod'), false)
  } finally { await app.close() }
  const releases = fixture({ releaseFailures: ['official-mod'], documents: {
    official: { sources: [source('official-child', { builtin: true, parent_id: 'official' })], fallback: true },
  } })
  try {
    releases.startInitialRepositoryRefresh()
    await until(() => releases.trace.includes('search:start'), 'Search starts before a Release failure can settle')
  } finally { await releases.close() }
  assert.equal(releases.trace.includes('raw:start:official:builtin'), false)
  assert.ok(releases.dashboard.value.config.repositories.find((repo) => repo.id === 'official-mod').last_error)
})
