import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'
import { effectScope, nextTick, ref } from 'vue'

function moduleUrl(path, imports = {}) {
  const source = readFileSync(new URL(path, import.meta.url), 'utf8')
  let { outputText } = ts.transpileModule(source, { compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 } })
  for (const [name, target] of Object.entries(imports)) outputText = outputText.replaceAll(`from '${name}'`, `from '${target}'`)
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`
}

const catalogueUrl = moduleUrl('../src/communityRepositories.ts')
const catalogue = await import(catalogueUrl)
const updates = await import(moduleUrl('../src/modUpdates.ts'))
const { useCommunityRepositories } = await import(moduleUrl('../src/composables/useCommunityRepositories.ts', {
  vue: import.meta.resolve('vue'), '../communityRepositories': catalogueUrl,
}))
const appSource = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
const assetMatcherSource = appSource.match(/function assetMatchesUi\([\s\S]*?\n\}/)?.[0]
assert.ok(assetMatcherSource, 'Load the actual asset matcher used by the install dialog')
const { outputText: assetMatcherModule } = ts.transpileModule(`export ${assetMatcherSource}`, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
})
const { assetMatchesUi } = await import(`data:text/javascript;base64,${Buffer.from(assetMatcherModule).toString('base64')}`)

function repository(id, patch = {}) {
  return { id, name: id, provider: 'github', username: 'Owner', repository: id, download_mode: 'release',
    channel: 'all', source_id: catalogue.COMMUNITY_SOURCE_ID, enabled: true, cached_releases: [], ...patch }
}

function release(tag, prerelease = false) {
  return { tag_name: tag, version: tag.replace(/^v/, ''), prerelease, draft: false, assets: [] }
}

function installed(tag, patch = {}) {
  return { version: '100.0.0', repository_binding: tag === undefined ? undefined
    : { provider: 'github', owner: 'owner', repository: 'mod', tag, channel: 'all' }, ...patch }
}

test('community pages contain ten rows, follow search order, and retain installed off-search entries', () => {
  const found = Array.from({ length: 21 }, (_, index) => repository(`mod-${index}`))
  const retained = repository('retained')
  const official = repository('official', { source_id: 'official' })
  const config = { repositories: [retained, official, ...found.toReversed()],
    community_discovery: { repository_ids: found.map((repo) => repo.id) } }
  const ordered = catalogue.orderedCommunityRepositories(config)
  assert.deepEqual(ordered.map((repo) => repo.id), [...found.map((repo) => repo.id), 'retained'])
  assert.equal(catalogue.communityPageCount(ordered), 3)
  assert.equal(catalogue.communityPage(ordered, 1).length, 10)
  assert.equal(catalogue.communityPage(ordered, 2)[0].id, 'mod-10')
  assert.deepEqual(catalogue.communityPage(ordered, 3).map((repo) => repo.id), ['mod-20', 'retained'])
  assert.deepEqual(catalogue.communityPage(ordered, -1), catalogue.communityPage(ordered, 1))
  assert.deepEqual(catalogue.communityPage(ordered, 99), catalogue.communityPage(ordered, 3))
  assert.equal(catalogue.communityPageCount([]), 1)
})

test('official coordinates hide community duplicates without hiding custom or other-provider repositories', () => {
  const official = repository('official', { repository: 'AlreadyListed', source_id: 'official-child', builtin: true, enabled: false })
  const duplicate = repository('duplicate', { username: 'owner', repository: 'alreadylisted' })
  const retainedDuplicate = repository('retained-duplicate', { username: 'OWNER', repository: 'AlreadyListed' })
  const custom = repository('custom', { repository: 'CustomMod', source_id: 'custom', builtin: false })
  const customMatch = repository('custom-match', { repository: 'CustomMod' })
  const gitee = repository('gitee', { repository: 'OtherProvider', provider: 'gitee', source_id: 'official-child', builtin: true })
  const githubMatch = repository('github-match', { repository: 'OtherProvider' })
  const unique = repository('unique')
  const config = { repositories: [official, custom, gitee, retainedDuplicate, duplicate, customMatch, githubMatch, unique],
    community_discovery: { repository_ids: [duplicate.id, customMatch.id, githubMatch.id, unique.id] } }
  assert.deepEqual(catalogue.orderedCommunityRepositories(config).map((repo) => repo.id), ['custom-match', 'github-match', 'unique'])
  assert.equal(config.repositories.length, 8, 'Filtering must not erase installed bindings or official configuration')
  assert.deepEqual([...catalogue.communityNavigationTargets(config.repositories, new Set(['duplicate', 'custom-match', 'github-match']))],
    ['duplicate', 'custom-match', 'github-match', 'official'], 'Hidden installed duplicates navigate to the official card without cross-provider substitutions')
})

test('generated community asset rules work in the actual frontend install matcher', () => {
  const backend = readFileSync(new URL('../src-tauri/src/community.rs', import.meta.url), 'utf8')
  const encodedRules = backend.match(/"asset_rules":\s*(\[\{[^\n]+\}\])/)
  assert.ok(encodedRules, 'Read the rules generated by the real search response converter')
  const rules = JSON.parse(encodedRules[1])
  for (const [name, expected] of [['Mod.dll', true], ['Mod.DLL', true], ['Mod.zip', true], ['Mod.ZiP', true], ['README.md', false], ['Mod.zip.sig', false], ['Mod.tar.gz', false]]) {
    assert.equal(rules.some((rule) => assetMatchesUi(name, rule)), expected, name)
  }
})

test('Tag comparisons share the Rust SemVer normalization fixtures', () => {
  const cases = JSON.parse(readFileSync(new URL('./fixtures/repository-tags.json', import.meta.url), 'utf8'))
  for (const [remote, local, expected] of cases) assert.equal(updates.compareReleaseTags(remote, local), expected, `${remote} vs ${local}`)
})

test('official, custom and community repositories all prefer the installed Tag over DLL versions', () => {
  for (const source_id of ['official', undefined, catalogue.COMMUNITY_SOURCE_ID]) {
    const repo = repository('mod', { source_id, cached_releases: [release('v2.0.0')] })
    assert.equal(updates.modRepositoryReleaseState(repo, installed('v1.0.0')), 'update_available')
    assert.equal(updates.modRepositoryReleaseState(repo, installed('v2.0.0')), 'latest')
    assert.equal(updates.modRepositoryReleaseState(repo, installed('v3.0.0')), 'latest')
    assert.equal(updates.modRepositoryReleaseState(repo, installed(undefined)), 'latest')
    assert.equal(updates.modRepositoryReleaseState(repo, installed(undefined, { version: '1.0.0' })), 'update_available')
    assert.equal(updates.modRepositoryReleaseState(repo, installed(undefined, { version: undefined })), 'unknown')
    assert.equal(updates.hasLocalUpdateVersion(installed('nightly', { version: undefined })), true)
    repo.cached_releases = [release('new-nightly')]
    assert.equal(updates.modRepositoryReleaseState(repo, installed('old-nightly')), 'update_available')
    assert.equal(updates.modRepositoryReleaseState(repo, installed('new-nightly')), 'latest')
    repo.last_error = 'failure'
    assert.equal(updates.modRepositoryReleaseState(repo, installed('new-nightly')), 'repository_unavailable')
  }
})

test('repository identity, provider, channel, drafts, and Raw mode cannot cross-bind updates', () => {
  const local = installed('v1.0.0')
  local.repository_binding.channel = 'stable'
  const repo = repository('mod', { cached_releases: [release('v9.0.0', true), { ...release('v8.0.0'), draft: true }, release('v2.0.0')] })
  assert.equal(updates.modRemoteRelease(repo, local).tag_name, 'v2.0.0')
  assert.equal(updates.modRepositoryReleaseState(repo, local), 'update_available')
  for (const patch of [{ provider: 'gitee' }, { username: 'Other' }, { repository: 'Other' }, { channel: 'beta' }, { download_mode: 'repository_file' }]) {
    assert.equal(updates.modRepositoryReleaseState({ ...repo, ...patch }, local), 'unknown')
  }
})

function hostFixture(patch = {}) {
  const calls = []
  const config = ref({ repositories: [repository('official', { source_id: 'official' })] })
  const allowed = ref(false)
  const host = { config, canRefresh: () => allowed.value, beforeSearch: async () => { calls.push('before') },
    search: async () => { calls.push('search'); config.value.repositories.push(...Array.from({ length: 21 }, (_, index) => repository(`found-${index}`))) },
    cancel: async () => { calls.push('cancel') }, refreshConfig: async () => { calls.push('config') },
    refreshReleases: async (repos) => { calls.push(`releases:${repos.length}`); return 1 },
    refreshDashboard: async () => { calls.push('dashboard') }, ...patch }
  const scope = effectScope()
  const controller = scope.run(() => useCommunityRepositories(host))
  return { calls, config, allowed, controller, scope }
}

test('discovery is gated until official loading settles, and all Releases start only after search', async () => {
  const fixture = hostFixture()
  const { calls, allowed, controller, scope } = fixture
  try {
    assert.equal(await controller.refresh(), false)
    assert.equal(controller.started.value, false)
    assert.deepEqual(calls, [])
    allowed.value = true
    const task = controller.refresh()
    assert.equal(controller.searching.value, true)
    assert.equal(controller.refresh(), task)
    assert.equal(await task, true)
    assert.deepEqual(calls, ['before', 'search', 'config', 'releases:21', 'dashboard'])
    assert.equal(controller.visibleRepositories.value.length, 10)
    assert.equal(controller.searching.value, false)
    assert.equal(controller.refreshing.value, false)
    assert.equal(controller.releaseFailures.value, 1)
    controller.setPage(2)
    assert.equal(controller.visibleRepositories.value[0].id, 'found-10')
    controller.reveal(new Set(['found-20']))
    assert.equal(controller.page.value, 3)
    assert.equal(calls.filter((call) => call === 'search').length, 1)
    controller.setPage(Number.NaN)
    assert.equal(controller.page.value, 3)
    fixture.config.value.repositories = fixture.config.value.repositories.slice(0, 2)
    await nextTick()
    assert.equal(controller.page.value, 1)
  } finally { scope.stop() }
})

test('hidden installed community duplicates still refresh Releases without changing channel-aware updates', async () => {
  const hidden = repository('hidden', { repository: 'mod', cached_releases: [release('v1.0.0')] })
  const visible = repository('visible')
  const official = repository('official', { repository: 'MOD', builtin: true, source_id: 'official', channel: 'stable' })
  const refreshed = []
  const fixture = hostFixture({
    search: async () => {
      fixture.config.value.repositories = [official, hidden, visible]
      fixture.config.value.community_discovery = { repository_ids: [hidden.id, visible.id] }
    },
    refreshReleases: async (repositories) => {
      refreshed.push(...repositories.map((repo) => repo.id))
      for (const repo of repositories) repo.cached_releases = [release('v2.0.0')]
      return 0
    },
  })
  try {
    fixture.allowed.value = true
    assert.equal(await fixture.controller.refresh(), true)
    assert.deepEqual(fixture.controller.repositories.value.map((repo) => repo.id), ['visible'])
    assert.deepEqual(refreshed, ['hidden', 'visible'])
    assert.equal(fixture.controller.discoveredCount.value, 1)
    assert.equal(hidden.channel, 'all')
    assert.equal(updates.modRepositoryReleaseState(fixture.config.value.repositories.find((repo) => repo.id === hidden.id), installed('v1.0.0')), 'update_available')
    assert.equal(updates.modRepositoryReleaseState(official, installed('v1.0.0')), 'unknown')
  } finally { fixture.scope.stop() }
})

test('a search failure leaves official repositories untouched and resets independent busy states', async () => {
  const fixture = hostFixture({ search: async () => { throw new Error('GitHub unavailable') } })
  try {
    fixture.allowed.value = true
    assert.equal(await fixture.controller.refresh(), false)
    assert.equal(fixture.controller.error.value, 'GitHub unavailable')
    assert.equal(fixture.controller.searching.value, false)
    assert.equal(fixture.controller.refreshing.value, false)
    assert.deepEqual(fixture.config.value.repositories.map((repo) => repo.id), ['official'])
    assert.deepEqual(fixture.calls, ['before'])
  } finally { fixture.scope.stop() }
})

test('disposal cancels only the community request and ignores a late completion', async () => {
  let finish
  const wait = new Promise((resolve) => { finish = resolve })
  const fixture = hostFixture({ search: () => wait })
  fixture.allowed.value = true
  const task = fixture.controller.refresh()
  await nextTick()
  fixture.scope.stop()
  finish()
  assert.equal(await task, false)
  assert.deepEqual(fixture.calls, ['before', 'cancel'])
  assert.equal(fixture.controller.refreshing.value, false)
})
