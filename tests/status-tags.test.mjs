import assert from 'node:assert/strict'
import test from 'node:test'

/** 共享状态标签：真实源码直接执行（Node 24 类型擦除）。 */
const { STATUS_META, installedModTags, repositoryStatus } = await import('../src/statusTags.ts')

function repo(overrides = {}) {
  return {
    id: 'repo-1', name: 'Repo', provider: 'github', username: 'Owner', repository: 'Repo',
    download_mode: 'release', branch: 'main', file_path: '', channel: 'all', asset_rules: [],
    install_target: '', note: '', plugin_guid: '', local_product_name: '', local_original_filename: '',
    version_regex: '', max_release_count: 10, download_threads: 4, enabled: true, builtin: false,
    cached_releases: [{ tag_name: 'v2.0.0', version: '2.0.0', name: '', body: '', prerelease: false, draft: false, assets: [] }],
    ...overrides,
  }
}

function mod(overrides = {}) {
  return {
    name: 'Mod', dll_file: 'Mod.dll', enabled: true, version_source: 'plugin',
    update_available: false, duplicate_guid: false, status: 'installed', labels: [], ...overrides,
  }
}

test('未安装仓库显示未安装，未刷新仓库显示未刷新', () => {
  assert.equal(repositoryStatus(repo(), []).text, '未安装')
  assert.equal(repositoryStatus(repo({ cached_releases: [] }), []).text, '未刷新')
})

test('RAW 仓库只区分未安装 / 已安装，未安装为灰色', () => {
  const raw = repo({ download_mode: 'repository_file' })
  const notInstalled = repositoryStatus(raw, [])
  assert.equal(notInstalled.text, '未安装')
  assert.equal(notInstalled.tone, 'muted')

  const installed = mod({ repository_id: 'repo-1' })
  const done = repositoryStatus(raw, [installed])
  assert.equal(done.text, '已安装')
  // 已安装与最新版同档，使用主题色
  assert.equal(done.tone, 'accent')
})

test('RAW 仓库即使被禁用或刷新失败，也只按安装状态显示', () => {
  const installed = mod({ repository_id: 'repo-1' })
  assert.equal(repositoryStatus(repo({ download_mode: 'repository_file', enabled: false }), [installed]).text, '已安装')
  assert.equal(repositoryStatus(repo({ download_mode: 'repository_file', last_error: 'x' }), []).text, '未安装')
})

test('所有未安装类标签都是灰色', () => {
  for (const key of ['not_installed', 'not_installed_raw', 'not_refreshed']) {
    assert.equal(STATUS_META[key].text.startsWith('未'), true, key)
    assert.equal(STATUS_META[key].tone, 'muted', key)
  }
})

test('Release 仓库禁用或刷新失败时优先提示状态', () => {
  assert.equal(repositoryStatus(repo({ enabled: false }), []).text, '未启用')
  assert.equal(repositoryStatus(repo({ last_error: 'x' }), []).text, '刷新失败')
})

test('后端已给出 latest 时直接采信；版本落后时判为可更新', () => {
  const installed = mod({
    repository_id: 'repo-1', labels: ['latest'], version: '2.0.0',
    repository_binding: { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v2.0.0', channel: 'all' },
  })
  assert.equal(repositoryStatus(repo(), [installed]).text, '最新版')

  const outdated = { ...installed, labels: ['update_available'], update_available: true }
  assert.equal(repositoryStatus(repo(), [outdated]).text, '可更新')
})

test('后端未绑定该仓库时回退到版本比较', () => {
  const installed = mod({
    repository_binding: { provider: 'github', owner: 'Owner', repository: 'Repo', tag: 'v1.0.0', channel: 'all' },
  })
  const status = repositoryStatus(repo(), [installed])
  assert.equal(status.text, '可更新')
  // 可更新属于「中度」
  assert.equal(status.tone, 'moderate')
})

test('每个状态标签都有文案与合法色调', () => {
  // 五档严重度 + 中性灰，外加为兼容旧快照保留的别名
  const tones = new Set([
    'accent', 'mild', 'moderate', 'severe', 'critical', 'muted',
    'ok', 'idle', 'warn', 'danger', 'violet',
  ])
  for (const [key, meta] of Object.entries(STATUS_META)) {
    assert.ok(meta.text, `${key} 缺少文案`)
    assert.ok(tones.has(meta.tone), `${key} 色调非法: ${meta.tone}`)
  }
})

test('installedModTags 保留后端顺序，labels 为空时退回 status', () => {
  const tags = installedModTags(mod({ labels: ['duplicate_lower_version', 'duplicate'] }))
  assert.deepEqual(tags.map((tag) => tag.text), ['低版本', '重复'])
  // 重复属重度，低版本属轻微
  assert.deepEqual(tags.map((tag) => tag.tone), ['mild', 'severe'])
  assert.equal(installedModTags(mod({ labels: [], status: 'unbound' }))[0].text, '未绑定')
})

test('预发布版本在最前面插入「预发布」标签，与可更新同档', () => {
  const tags = installedModTags(mod({ labels: ['latest'], prerelease: true }))
  assert.deepEqual(tags.map((tag) => tag.text), ['预发布', '最新版'])
  assert.equal(tags[0].tone, 'moderate')
  // 最新版用主题色
  assert.equal(tags[1].tone, 'accent')
})

test('非预发布版本不出现「预发布」标签', () => {
  const tags = installedModTags(mod({ labels: ['latest'], prerelease: false }))
  assert.deepEqual(tags.map((tag) => tag.text), ['最新版'])
})

test('后端已带 prerelease 标签时不重复插入', () => {
  const tags = installedModTags(mod({ labels: ['prerelease', 'latest'], prerelease: true }))
  assert.equal(tags.filter((tag) => tag.key === 'prerelease').length, 1)
  assert.equal(tags[0].key, 'prerelease')
})
