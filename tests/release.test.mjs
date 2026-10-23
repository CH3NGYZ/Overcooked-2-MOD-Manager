import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { createHash, randomUUID } from 'node:crypto'
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import { buildUpdateKeys, updateFiles } from '../scripts/build-release.mjs'
import { GitHubRelease } from '../scripts/github-release.mjs'
import { aggregateManifest, prepareRelease, readReleaseNotes } from '../scripts/release.mjs'
import { signingFixture } from './fixtures/updater-signing.mjs'
import {
  BUILD_KEYS, PLATFORM_KEYS, RELEASE_OWNER, RELEASE_REPO, assertDraftRelease,
  collectUpdateAssets, readProject, releaseNotesFromMessage, releaseTag, updateAssetKey, updateManifest,
  validateProjectVersions, verifyUpdateSignature,
} from '../scripts/release-lib.mjs'

const root = fileURLToPath(new URL('../', import.meta.url))
const names = [
  'OC2ModManager_1.2.0_x64-setup.exe',
  'OC2ModManager_1.2.0_x64_en-US.msi',
  'OC2ModManager_1.2.0_amd64.AppImage',
  'OC2ModManager_1.2.0_amd64.deb',
  'OC2ModManager-1.2.0-1.x86_64.rpm',
  'OC2ModManager_aarch64.app.tar.gz',
  'OC2ModManager_x64.app.tar.gz',
]
const urlFor = name => `https://github.com/${RELEASE_OWNER}/${RELEASE_REPO}/releases/download/v1.2.0/${encodeURIComponent(name)}`
const draft = { id: 1, tag_name: 'v1.2.0', draft: true, prerelease: false, body: 'Release notes' }

function projectInputs() {
  return {
    packageJson: JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')),
    packageLock: JSON.parse(readFileSync(join(root, 'package-lock.json'), 'utf8')),
    tauri: JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8')),
    cargo: readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8'),
    cargoLock: readFileSync(join(root, 'src-tauri/Cargo.lock'), 'utf8'),
  }
}

function releaseFixture() {
  const signer = signingFixture('1.2.0')
  const files = new Map()
  const assets = []
  for (const name of names) {
    const bytes = Buffer.from(`Package fixture: ${name}`)
    const id = assets.length + 1
    assets.push({ id, name, size: bytes.length, digest: `sha256:${createHash('sha256').update(bytes).digest('hex')}`, browser_download_url: urlFor(name) })
    files.set(id, bytes)
    const signature = Buffer.from(signer.signatureFor(bytes))
    assets.push({ id: id + 1, name: `${name}.sig`, size: signature.length })
    files.set(id + 1, signature)
  }
  return { ...signer, assets, files, github: { download: async id => files.get(id) } }
}

test('all project versions are consistent and a dispatch uses a version tag, never main', () => {
  const inputs = projectInputs()
  const { version } = readProject(root)
  assert.equal(validateProjectVersions(inputs), version)
  assert.equal(releaseTag(version, 'branch', 'main'), `v${version}`)
  assert.equal(releaseTag(version, 'tag', `v${version}`), `v${version}`)
  assert.throws(() => releaseTag(version, 'tag', 'v0.0.0'))
  for (const change of [
    input => { input.packageLock.version = '0.0.0' },
    input => { input.packageLock.packages[''].version = '0.0.0' },
    input => { input.tauri.version = '0.0.0' },
    input => { input.cargo = input.cargo.replace(/^version = "[^"]+"/m, 'version = "0.0.0"') },
    input => { input.cargoLock = input.cargoLock.replace(/(name = "app"\r?\nversion = ")[^"]+/, '$10.0.0') },
    input => { input.tauri.bundle.createUpdaterArtifacts = false },
    input => { input.tauri.plugins.updater.requireSignedVersion = false },
    input => { input.tauri.plugins.updater.pubkey = '' },
  ]) {
    const changed = projectInputs()
    change(changed)
    assert.throws(() => validateProjectVersions(changed))
  }
})

test('the exact supplied package names map to seven disjoint system/architecture/format targets', () => {
  assert.deepEqual(names.map(name => updateAssetKey(name, '1.2.0')), PLATFORM_KEYS)
  assert.equal(updateAssetKey('OC2ModManager-1.2.0-2.x86_64.rpm', '1.2.0'), 'linux-x86_64-rpm')
  assert.equal(updateAssetKey('OC2ModManager_1.2.0_x86_64.app.tar.gz', '1.2.0'), 'darwin-x86_64-app')
  for (const name of [
    'OC2ModManager_1.2.0_aarch64.dmg', 'OC2ModManager.app.tar.gz',
    'OC2ModManager_1.2.0_x64_msi.msi', 'OC2ModManager-1.2.0-1.aarch64.rpm',
    'OC2ModManager-1.2.0-.x86_64.rpm', '../OC2ModManager_x64.app.tar.gz',
    'OC2ModManager_1.2.1_amd64.deb', 'OC2ModManager_1.2.0_amd64.deb.sig',
  ]) assert.equal(updateAssetKey(name, '1.2.0'), undefined, name)
})

test('aggregation rejects missing, duplicated, wrong-version or empty updater packages', () => {
  const assets = names.map(name => ({ name, size: 1 }))
  assert.equal(collectUpdateAssets(assets, '1.2.0').size, 7)
  assert.throws(() => collectUpdateAssets(assets.slice(1), '1.2.0'))
  assert.throws(() => collectUpdateAssets([...assets, assets[0]], '1.2.0'))
  assert.throws(() => collectUpdateAssets(assets.map(asset => ({ ...asset, size: 0 })), '1.2.0'))
  assert.throws(() => collectUpdateAssets(assets, '1.2.1'))
})

test('Node verifies the same genuine minisign fixture as the Rust updater', () => {
  const rust = readFileSync(join(root, 'src-tauri/src/app_update.rs'), 'utf8')
  const constant = name => rust.match(new RegExp(`const ${name}: &str = "([^"]+)"`))[1]
  const signature = constant('TEST_SIGNATURE')
  const key = constant('TEST_KEY')
  verifyUpdateSignature(Buffer.from('OC2 signed update fixture'), signature, key, '1.2.0')
  assert.throws(() => verifyUpdateSignature(Buffer.from('tampered'), signature, key, '1.2.0'))
  assert.throws(() => verifyUpdateSignature(Buffer.from('OC2 signed update fixture'), signature, key, '9.0.0'))
  const comment = Buffer.from(signature, 'base64').toString().replace('version:1.2.0', 'version:9.0.0')
  assert.throws(() => verifyUpdateSignature(Buffer.from('OC2 signed update fixture'), Buffer.from(comment).toString('base64'), key, '9.0.0'))
})

test('signature verification covers prehashed and legacy payloads and rejects unbound versions/other keys', () => {
  const signer = signingFixture()
  const bytes = Buffer.from('Package fixture')
  for (const algorithm of ['ED', 'Ed']) {
    const signature = signer.signatureFor(bytes, '1.2.0', algorithm)
    verifyUpdateSignature(bytes, signature, signer.encodedPublicKey, '1.2.0')
    assert.throws(() => verifyUpdateSignature(bytes, signature, signingFixture().encodedPublicKey, '1.2.0'))
  }
  for (const version of ['2.0.0', '', '1.2.0\tversion:1.2.0']) {
    assert.throws(() => verifyUpdateSignature(bytes, signer.signatureFor(bytes, version), signer.encodedPublicKey, '1.2.0'))
  }
  for (const signature of ['', 'not base64!', Buffer.from('broken').toString('base64')]) {
    assert.throws(() => verifyUpdateSignature(bytes, signature, signer.encodedPublicKey, '1.2.0'))
  }
})

test('the manifest uses real browser-download URLs and verified signatures for all seven targets', async () => {
  const fixture = releaseFixture()
  const manifest = await aggregateManifest(fixture.github, draft, fixture.assets, '1.2.0', fixture.encodedPublicKey)
  assert.equal(manifest.version, '1.2.0')
  assert.equal(manifest.notes, draft.body)
  assert.deepEqual(Object.keys(manifest.platforms), PLATFORM_KEYS)
  for (const [index, name] of names.entries()) {
    assert.equal(manifest.platforms[PLATFORM_KEYS[index]].url, urlFor(name))
    assert.ok(manifest.platforms[PLATFORM_KEYS[index]].signature)
  }
  assert.throws(() => updateManifest('1.2.0', {}))
})

test('draft metadata with temporary asset URLs produces stable version URLs for publication', async () => {
  const fixture = releaseFixture()
  const temporaryTag = 'untagged-da619d69bdda4873d077'
  const temporaryDraft = {
    ...draft,
    html_url: `https://github.com/${RELEASE_OWNER}/${RELEASE_REPO}/releases/tag/${temporaryTag}`,
  }
  for (const asset of fixture.assets) {
    if (asset.browser_download_url) {
      asset.browser_download_url = asset.browser_download_url.replace('/v1.2.0/', `/${temporaryTag}/`)
    }
  }
  const manifest = await aggregateManifest(fixture.github, temporaryDraft, fixture.assets, '1.2.0', fixture.encodedPublicKey)
  for (const [index, name] of names.entries()) {
    assert.equal(manifest.platforms[PLATFORM_KEYS[index]].url, urlFor(name))
  }
})

test('draft URL normalization rejects another draft, repository, host, asset or unauthenticated version', async () => {
  const temporaryTag = 'untagged-da619d69bdda4873d077'
  const htmlUrl = `https://github.com/${RELEASE_OWNER}/${RELEASE_REPO}/releases/tag/${temporaryTag}`
  for (const change of [
    (release, asset) => { asset.browser_download_url = asset.browser_download_url.replace(temporaryTag, 'untagged-deadbeef') },
    (release, asset) => { asset.browser_download_url = asset.browser_download_url.replace(RELEASE_REPO, 'Other-Repository') },
    (release, asset) => { asset.browser_download_url = asset.browser_download_url.replace('github.com', 'example.com') },
    (release, asset) => { asset.browser_download_url += '?download=1' },
    (release, asset) => { asset.browser_download_url += '.sig' },
    release => { release.html_url = htmlUrl.replace(temporaryTag, 'v0.0.0') },
    release => { release.html_url = htmlUrl.replace(RELEASE_REPO, 'Other-Repository') },
    release => { release.html_url = undefined },
    release => { release.tag_name = 'v0.0.0' },
    release => { release.draft = false },
  ]) {
    const fixture = releaseFixture()
    const temporaryDraft = { ...draft, html_url: htmlUrl }
    for (const asset of fixture.assets) {
      if (asset.browser_download_url) {
        asset.browser_download_url = asset.browser_download_url.replace('/v1.2.0/', `/${temporaryTag}/`)
      }
    }
    change(temporaryDraft, fixture.assets[0])
    await assert.rejects(() => aggregateManifest(fixture.github, temporaryDraft, fixture.assets, '1.2.0', fixture.encodedPublicKey))
  }
})

test('manifest aggregation stops on corrupted uploads, missing signatures, wrong URLs or published releases', async () => {
  for (const change of [
    fixture => { fixture.files.set(1, Buffer.from('truncated')) },
    fixture => { fixture.assets[0].digest = 'sha256:incorrect' },
    fixture => { fixture.assets.splice(1, 1) },
    fixture => { fixture.assets[0].browser_download_url = 'https://api.github.com/assets/1' },
  ]) {
    const fixture = releaseFixture()
    change(fixture)
    await assert.rejects(() => aggregateManifest(fixture.github, draft, fixture.assets, '1.2.0', fixture.encodedPublicKey))
  }
  const fixture = releaseFixture()
  await assert.rejects(() => aggregateManifest(fixture.github, { ...draft, draft: false }, fixture.assets, '1.2.0', fixture.encodedPublicKey))
  assert.throws(() => assertDraftRelease({ ...draft, tag_name: 'main' }, '1.2.0'))
})

test('prepare only reuses a matching draft and removes its stale manifest, never installer assets', async () => {
  const deleted = []
  const github = {
    findRelease: async () => draft,
    assets: async () => [{ id: 50, name: 'latest.json' }, { id: 51, name: names[0] }],
    deleteAsset: async id => deleted.push(id),
  }
  assert.equal((await prepareRelease(github, '1.2.0', 'v1.2.0', undefined, draft.body)).id, draft.id)
  assert.deepEqual(deleted, [50])
  deleted.length = 0
  github.findRelease = async () => ({ ...draft, draft: false })
  await assert.rejects(() => prepareRelease(github, '1.2.0', 'v1.2.0', undefined, draft.body))
  assert.deepEqual(deleted, [])
})

test('release notes read the exact tagged commit, preserving multiline Unicode, not later HEAD or tag annotations', t => {
  const base = resolve(root, 'src-tauri/target/verification/release-notes-unit')
  const workspace = join(base, randomUUID())
  mkdirSync(workspace, { recursive: true })
  t.after(() => {
    assert.ok(resolve(workspace).startsWith(`${base}${sep}`))
    rmSync(workspace, { recursive: true, force: true })
  })
  const git = (args, input) => execFileSync('git', ['-c', 'commit.gpgsign=false', '-c', 'tag.gpgsign=false',
    '-c', `core.hooksPath=${join(workspace, 'disabled-hooks')}`, ...args], {
    cwd: workspace, input, encoding: 'utf8', windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    env: { ...process.env, GIT_AUTHOR_NAME: 'Release fixture', GIT_AUTHOR_EMAIL: 'release@example.invalid',
      GIT_COMMITTER_NAME: 'Release fixture', GIT_COMMITTER_EMAIL: 'release@example.invalid' },
  }).trimEnd()
  git(['init', '--quiet'])
  const message = '\u4fee\u590d\u66f4\u65b0\u8bf4\u660e\n\n- GitHub & Gitee\n- Keep "quoted" text\n\n\u6821\u9a8c\uff1anpm run build \u901a\u8fc7\uff1bcargo test --lib 348 \u901a\u8fc7\u3002\n\nCo-Authored-By: Example <example@example.invalid>'
  const expected = '\u4fee\u590d\u66f4\u65b0\u8bf4\u660e\n\n- GitHub & Gitee\n- Keep "quoted" text'
  git(['commit', '--allow-empty', '--quiet', '-F', '-'], message)
  const commit = git(['rev-parse', 'HEAD'])
  git(['tag', '-a', 'v1.2.0', '-m', 'A separate tag annotation'])
  const tagObject = git(['rev-parse', 'refs/tags/v1.2.0'])
  git(['commit', '--allow-empty', '--quiet', '-m', 'A later unrelated commit'])
  assert.equal(readReleaseNotes(commit, workspace), expected)
  assert.equal(readReleaseNotes(tagObject, workspace), expected)
  git(['commit', '--allow-empty', '--quiet', '-F', '-'],
    '\u6821\u9a8c\uff1anpm test \u901a\u8fc7\n\nCo-Authored-By: Example <example@example.invalid>')
  assert.throws(() => readReleaseNotes(git(['rev-parse', 'HEAD']), workspace), /no notes after removing trailers/)
  git(['commit', '--allow-empty', '--allow-empty-message', '--quiet', '-F', '-'], '')
  assert.throws(() => readReleaseNotes(git(['rev-parse', 'HEAD']), workspace), /message is empty/)
  assert.throws(() => readReleaseNotes('f'.repeat(40), workspace), /Could not read/)
  for (const invalid of [undefined, '', 'HEAD', '--help', '../invalid']) {
    assert.throws(() => readReleaseNotes(invalid, workspace), /full GitHub commit SHA/)
  }
})

test('release notes drop commit trailers and local verification, keeping the changelog', () => {
  const message = [
    '发布 1.3.4',
    '',
    '版本号升级至 1.3.4。本版新增：侧边栏折叠、窄窗口布局修复。',
    '',
    '校验：npm run build 通过；npm test 255 通过、14 跳过；git diff --check 通过。',
    '',
    '未做真实窗口目视验证。',
    '',
    'Signed-off-by: Example <example@example.invalid>',
    'Co-Authored-By: Example <example@example.invalid>',
  ].join('\n')
  assert.equal(releaseNotesFromMessage(message), '发布 1.3.4\n\n版本号升级至 1.3.4。本版新增：侧边栏折叠、窄窗口布局修复。')
  assert.equal(releaseNotesFromMessage('修复：\r\n\r\n- 一\r\n- 二\r\n\r\ncargo fmt --check 通过'), '修复：\n\n- 一\n- 二')
  assert.equal(releaseNotesFromMessage('仅有一行说明'), '仅有一行说明')
  assert.equal(releaseNotesFromMessage(''), '')
  assert.equal(releaseNotesFromMessage(undefined), '')
  assert.equal(releaseNotesFromMessage('Co-Authored-By: Example <example@example.invalid>'), '')
  assert.equal(releaseNotesFromMessage('Verification: cargo test --lib passed'), '')
})

test('new drafts bind to the actual source commit and its message, with a branch fallback', async () => {
  const requests = []
  const github = {
    findRelease: async () => undefined,
    assets: async () => [],
    request: async (path, options) => {
      requests.push([path, options])
      return path === '' ? { default_branch: 'main' } : { ...draft, ...options.body }
    },
  }
  await prepareRelease(github, '1.2.0', 'v1.2.0', undefined, draft.body)
  assert.equal(requests[1][1].body.target_commitish, 'main')
  assert.equal(requests[1][1].body.draft, true)
  assert.equal(requests[1][1].body.body, draft.body)
  requests.length = 0
  const commit = 'a'.repeat(40)
  await prepareRelease(github, '1.2.0', 'v1.2.0', commit, draft.body)
  assert.equal(requests[1][1].body.target_commitish, commit)
  assert.equal(requests[1][1].body.body, draft.body)
  assert.equal(requests.length, 2)
  await assert.rejects(() => prepareRelease(github, '1.2.0', 'v1.2.0', '../invalid', draft.body))
})

test('prepare requires a source commit message before any GitHub mutation or request', async () => {
  const github = { findRelease: async () => assert.fail('Invalid notes must not reach GitHub') }
  for (const notes of [undefined, null, '', ' \n ', 1]) {
    await assert.rejects(() => prepareRelease(github, '1.2.0', 'v1.2.0', 'a'.repeat(40), notes), /source commit message/)
  }
})

test('same-commit retries replace old boilerplate notes and the verified manifest inherits the exact message', async () => {
  const commit = 'c'.repeat(40)
  const notes = '\u4fee\u590d\u591a\u5e73\u53f0\u66f4\u65b0\n\n- Preserve signatures\n- Keep draft only'
  let release = { ...draft, target_commitish: commit, body: 'Old boilerplate release instructions' }
  const requests = []
  const fixture = releaseFixture()
  const github = new GitHubRelease('fixture-token', async (url, options) => {
    const path = new URL(url).pathname
    if (options.method === 'PATCH') {
      const body = JSON.parse(options.body)
      requests.push(body)
      release = { ...release, ...body }
      return Response.json(release)
    }
    if (path.endsWith('/assets')) return Response.json([])
    if (path.endsWith('/releases')) return Response.json([release])
    throw new Error(`Unexpected fixture request: ${options.method} ${path}`)
  })
  const updated = await prepareRelease(github, '1.2.0', 'v1.2.0', commit, notes)
  assert.equal(updated.body, notes)
  assert.deepEqual(requests, [{ tag_name: draft.tag_name, target_commitish: commit, body: notes, draft: true, prerelease: false }])
  const manifest = await aggregateManifest(fixture.github, updated, fixture.assets, '1.2.0', fixture.encodedPublicKey)
  assert.equal(manifest.notes, notes)
  await prepareRelease(github, '1.2.0', 'v1.2.0', commit, notes)
  assert.equal(requests.length, 1)
  release.draft = false
  await assert.rejects(() => prepareRelease(github, '1.2.0', 'v1.2.0', commit, 'New message'))
  assert.equal(requests.length, 1)
})

test('a matching draft retry refreshes its source commit without publishing or removing installers', async () => {
  const commit = 'b'.repeat(40)
  let release = { ...draft, target_commitish: 'a'.repeat(40) }
  const requests = []
  const deleted = []
  const github = {
    findRelease: async () => release,
    request: async (path, options) => {
      requests.push([path, options])
      return { ...release, ...options.body }
    },
    assets: async () => [{ id: 50, name: 'latest.json' }, { id: 51, name: names[0] }],
    deleteAsset: async id => deleted.push(id),
  }
  const updated = await prepareRelease(github, '1.2.0', 'v1.2.0', commit, draft.body)
  assert.equal(updated.target_commitish, commit)
  assert.deepEqual(requests, [[`/releases/${draft.id}`, {
    method: 'PATCH', body: { tag_name: 'v1.2.0', target_commitish: commit, body: draft.body, draft: true, prerelease: false },
  }]])
  assert.deepEqual(deleted, [50])
  release = updated
  requests.length = 0
  await prepareRelease(github, '1.2.0', 'v1.2.0', commit, draft.body)
  assert.deepEqual(requests, [])
  for (const changed of [
    { ...release, draft: false },
    { ...release, tag_name: 'v0.0.0' },
    { ...release, prerelease: true },
  ]) {
    release = changed
    deleted.length = 0
    await assert.rejects(() => prepareRelease(github, '1.2.0', 'v1.2.0', commit, draft.body))
    assert.deepEqual(requests, [])
    assert.deepEqual(deleted, [])
  }
  release = { ...draft, target_commitish: 'a'.repeat(40) }
  for (const changed of [
    { draft: false },
    { tag_name: 'v0.0.0' },
    { prerelease: true },
  ]) {
    github.request = async () => ({ ...release, ...changed })
    await assert.rejects(() => prepareRelease(github, '1.2.0', 'v1.2.0', commit, draft.body))
    assert.deepEqual(deleted, [])
  }
})

test('draft retries explicitly retain the version tag when GitHub normalizes an incomplete PATCH', async () => {
  const commit = 'b'.repeat(40)
  const temporaryTag = 'untagged-da619d69bdda4873d077'
  let release = { ...draft, target_commitish: 'a'.repeat(40) }
  const requests = []
  const github = new GitHubRelease('fixture-token', async (url, options) => {
    const path = new URL(url).pathname
    if (options.method === 'PATCH') {
      const body = JSON.parse(options.body)
      requests.push(body)
      release = { ...release, ...body, tag_name: body.tag_name || temporaryTag }
      return Response.json(release)
    }
    if (path.endsWith('/assets')) return Response.json([])
    if (path.endsWith('/releases')) return Response.json([release])
    throw new Error(`Unexpected fixture request: ${options.method} ${path}`)
  })
  const updated = await prepareRelease(github, '1.2.0', 'v1.2.0', commit, draft.body)
  assert.equal(updated.tag_name, 'v1.2.0')
  assert.equal(updated.target_commitish, commit)
  assert.equal(updated.draft, true)
  assert.equal(updated.prerelease, false)
  assert.deepEqual(requests, [{ tag_name: 'v1.2.0', target_commitish: commit, body: draft.body, draft: true, prerelease: false }])
})

test('Actions writes drafts and updater manifests to this code repository with its scoped GITHUB_TOKEN', () => {
  const workflow = readFileSync(join(root, '.github/workflows/build.yml'), 'utf8')
  assert.equal(RELEASE_REPO, 'Overcooked-2-MOD-Manager')
  assert.ok(workflow.includes(`repo: ${RELEASE_REPO}`))
  assert.ok(!workflow.includes('RELEASE_REPOSITORY_TOKEN'))
  assert.ok(workflow.includes("if: inputs.mode != 'test' && !github.event.deleted"))
  assert.equal((workflow.match(/GITHUB_TOKEN: \$\{\{ secrets\.GITHUB_TOKEN \}\}/g) || []).length, 4)
  assert.equal((workflow.match(/permissions:\r?\n +contents: write/g) || []).length, 3)
})

test('local NSIS-only builds select the same signed updater format without requiring an MSI', () => {
  for (const [target, keys] of Object.entries(BUILD_KEYS)) {
    assert.deepEqual(buildUpdateKeys(['build', '--target', target], target), keys)
    assert.deepEqual(buildUpdateKeys(['build', '--target', target, '--bundles', 'all'], target), keys)
  }
  const target = 'x86_64-pc-windows-msvc'
  assert.deepEqual(buildUpdateKeys(['build', '--target', target, '--bundles', 'nsis'], target), ['windows-x86_64-nsis'])
  assert.deepEqual(buildUpdateKeys(['build', '--target', target, '--bundles', 'nsis,msi'], target), BUILD_KEYS[target])
  assert.deepEqual(buildUpdateKeys(['build', '--target', target, '--bundles', 'nsis', 'msi', '--verbose'], target), BUILD_KEYS[target])
  for (const bundles of [[], ['deb'], ['android'], ['nsis', 'invalid']]) {
    assert.throws(() => buildUpdateKeys(['build', '--bundles', ...bundles], target))
  }
  assert.throws(() => buildUpdateKeys(['build'], 'unsupported-target'))
  assert.throws(() => buildUpdateKeys(['build', '--bundles', 'dmg'], 'aarch64-apple-darwin'))
})

test('artifact discovery signs DEB/RPM and names both Mac updater archives as tauri-action uploads them', () => {
  const verificationRoot = resolve(root, 'src-tauri/target/verification/release-script-tests')
  const workspace = join(verificationRoot, randomUUID())
  const write = (path, value = 'package fixture') => {
    mkdirSync(dirname(path), { recursive: true })
    writeFileSync(path, value)
  }
  try {
    for (const [target, keys] of Object.entries(BUILD_KEYS)) {
      const bundle = join(workspace, target)
      for (const key of keys) {
        const name = names[PLATFORM_KEYS.indexOf(key)]
        const directory = key.startsWith('darwin') ? 'macos' : key.split('-').at(-1)
        write(join(bundle, directory, key.startsWith('darwin') ? 'OC2ModManager.app.tar.gz' : name))
      }
      assert.deepEqual([...updateFiles(bundle, target, '1.2.0').keys()], keys)
    }
    const nsisOnly = join(workspace, 'local-nsis')
    write(join(nsisOnly, 'nsis', names[0]))
    const nsisKeys = buildUpdateKeys(['build', '--bundles', 'nsis'], 'x86_64-pc-windows-msvc')
    const selected = updateFiles(nsisOnly, 'x86_64-pc-windows-msvc', '1.2.0', nsisKeys)
    assert.deepEqual([...selected.keys()], ['windows-x86_64-nsis'])
    assert.equal(selected.get('windows-x86_64-nsis').path, join(nsisOnly, 'nsis', names[0]))
    assert.throws(() => updateFiles(nsisOnly, 'x86_64-pc-windows-msvc', '1.2.0'))
    assert.throws(() => updateFiles(nsisOnly, 'x86_64-pc-windows-msvc', '1.2.1', nsisKeys))
    assert.throws(() => updateFiles(nsisOnly, 'x86_64-pc-windows-msvc', '1.2.0', ['linux-x86_64-deb']))
    assert.throws(() => updateFiles(workspace, 'unsupported-target', '1.2.0'))
    assert.throws(() => updateFiles(workspace, 'x86_64-unknown-linux-gnu', '1.2.0'))
  } finally {
    assert.ok(resolve(workspace).startsWith(`${verificationRoot}${sep}`))
    rmSync(workspace, { recursive: true, force: true })
  }
})

test('the release API requires authentication and reports only safe HTTP status errors', async () => {
  assert.throws(() => new GitHubRelease(''))
  const requests = []
  const github = new GitHubRelease('[REDACTED_SECRET]', async (url, options) => {
    requests.push([url, options])
    return new Response('private server details', { status: 403 })
  })
  await assert.rejects(() => github.release(1), error => !error.message.includes('private server details') && error.message.includes('403'))
  assert.ok(requests[0][0].startsWith(`https://api.github.com/repos/${RELEASE_OWNER}/${RELEASE_REPO}/`))
  assert.throws(() => github.download('../outside'))
})
