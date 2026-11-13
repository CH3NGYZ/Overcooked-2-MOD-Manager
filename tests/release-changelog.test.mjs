import assert from 'node:assert/strict'
import { createHash, generateKeyPairSync, sign } from 'node:crypto'
import test from 'node:test'
import { GitHubRelease } from '../scripts/github-release.mjs'
import { aggregateManifest } from '../scripts/release.mjs'
import {
  CHANGELOG_LIMITS, MAX_UPDATE_MANIFEST_BYTES, PLATFORM_KEYS, RELEASE_OWNER, RELEASE_REPO,
  collectReleaseChangelog, serializeUpdateManifest, updateManifest,
} from '../scripts/release-lib.mjs'

const platforms = Object.fromEntries(PLATFORM_KEYS.map(key => [key, { signature: 'fixture', url: `https://example.com/${key}` }]))
const published = (tag_name, body = tag_name, overrides = {}) => ({ tag_name, body, draft: false, prerelease: false, ...overrides })
const manifest = (changelog, options = {}) => updateManifest('2.10.0', platforms, {
  notes: '目标版本正文', pubDate: '2026-11-13T00:00:00Z', changelog, ...options,
})

test('release history uses published stable tags, numeric order, first duplicate and authoritative target notes', () => {
  const history = collectReleaseChangelog([
    published('v2.10.0', 'stale target'), published('v2.9.0', '- 九'), published('2.2.0', '- 二'),
    published('v2.9.0', 'duplicate'), published('v2.11.0'), published('v2.1.0', null),
    published('v02.0.0'), published('v2.0'), published('v2.0.0-beta.1'), published('v2.0.0+meta'),
    published('v2.3.0', 'hidden', { draft: true }), published('v2.4.0', 'preview', { prerelease: true }),
    published('v2.5.0', 'unknown status', { draft: undefined }),
  ], '2.10.0')
  assert.deepEqual(manifest(history).changelog, [
    { version: '2.1.0', notes: '' }, { version: '2.2.0', notes: '- 二' },
    { version: '2.9.0', notes: '- 九' }, { version: '2.10.0', notes: '目标版本正文' },
  ])
  assert.equal(manifest(history).notes, '目标版本正文')
  assert.deepEqual(manifest([{ version: '2.10.0', notes: 'stale' }]).changelog,
    [{ version: '2.10.0', notes: '目标版本正文' }])
})

test('legacy callers keep the original manifest shape without optional history', () => {
  assert.deepEqual(manifest(undefined), {
    version: '2.10.0', notes: '目标版本正文', pub_date: '2026-11-13T00:00:00Z', platforms,
  })
  assert.deepEqual(manifest([]).changelog, [{ version: '2.10.0', notes: '目标版本正文' }])
})

for (const [label, history] of [
  ['non-array', {}], ['null entry', [null]], ['invalid version', [{ version: 'v2.0.0', notes: 'bad' }]],
  ['leading zeros', [{ version: '02.0.0', notes: 'bad' }]], ['prerelease', [{ version: '2.0.0-beta.1', notes: 'bad' }]],
  ['future version', [{ version: '2.11.0', notes: 'bad' }]], ['wrong notes type', [{ version: '2.0.0', notes: 4 }]],
]) {
  test(`invalid ${label} history only removes the optional extension`, () => {
    assert.deepEqual(manifest(history), manifest(undefined))
  })
}

test('malformed published body discards history while excluded records cannot poison the extension', () => {
  assert.deepEqual(manifest(collectReleaseChangelog([published('v2.0.0', {})], '2.10.0')), manifest(undefined))
  assert.deepEqual(manifest(collectReleaseChangelog([
    published('v9.0.0', {}), published('v2.1.0', {}, { draft: true }), published('invalid', {}),
  ], '2.10.0')).changelog, [{ version: '2.10.0', notes: '目标版本正文' }])
  assert.throws(() => collectReleaseChangelog({}, '2.10.0'), /Invalid GitHub release list/)
})

test('history entry limit includes the target and accepts the exact boundary', () => {
  const history = Array.from({ length: CHANGELOG_LIMITS.entries - 1 }, (_, index) => ({ version: `1.${index}.0`, notes: '' }))
  assert.equal(manifest(history).changelog.length, CHANGELOG_LIMITS.entries)
  assert.deepEqual(manifest([...history, { version: '1.999.0', notes: '' }]), manifest(undefined))
  assert.deepEqual(manifest(Array.from({ length: CHANGELOG_LIMITS.entries + 1 }, () => history[0])), manifest(undefined))
})

test('per-entry limits measure UTF-8 bytes and preserve original target notes on overflow', () => {
  const exact = '中'.repeat(Math.floor(CHANGELOG_LIMITS.entryBytes / 3)) + 'ab'
  assert.equal(Buffer.byteLength(exact, 'utf8'), CHANGELOG_LIMITS.entryBytes)
  assert.equal(manifest([{ version: '2.0.0', notes: exact }]).changelog[0].notes, exact)
  assert.deepEqual(manifest([{ version: '2.0.0', notes: exact + 'c' }]), manifest(undefined))
  const target = 'x'.repeat(CHANGELOG_LIMITS.entryBytes + 1)
  const result = manifest([], { notes: target })
  assert.equal(result.notes, target)
  assert.equal(Object.hasOwn(result, 'changelog'), false)
})

test('oversized raw duplicate and target records fail before deduplication or target replacement', () => {
  const oversized = 'x'.repeat(CHANGELOG_LIMITS.entryBytes + 1)
  assert.deepEqual(manifest([
    { version: '2.0.0', notes: 'first' }, { version: '2.0.0', notes: oversized },
  ]), manifest(undefined))
  assert.deepEqual(manifest([{ version: '2.10.0', notes: oversized }]), manifest(undefined))
})

test('combined history text limit includes the target and accepts the exact boundary', () => {
  const target = 'target'
  const history = Array.from({ length: 6 }, (_, index) => ({
    version: `1.${index}.0`, notes: 'x'.repeat(CHANGELOG_LIMITS.entryBytes - (index === 0 ? target.length : 0)),
  }))
  assert.equal(manifest(history, { notes: target }).changelog.length, 7)
  history[0].notes += 'x'
  assert.deepEqual(manifest(history, { notes: target }), manifest(undefined, { notes: target }))
})

test('pretty JSON escaping, indentation and newline participate in the 1 MiB upload budget', () => {
  const history = Array.from({ length: 3 }, (_, index) => ({ version: `1.${index}.0`, notes: '\u0000'.repeat(100_000) }))
  assert.ok(history.reduce((sum, entry) => sum + Buffer.byteLength(entry.notes), 0) < CHANGELOG_LIMITS.totalBytes)
  assert.deepEqual(manifest(history), manifest(undefined))
  const baseline = manifest(undefined, { notes: '' })
  const reserve = serializeUpdateManifest(baseline).length
  const exact = { ...baseline, notes: 'x'.repeat(MAX_UPDATE_MANIFEST_BYTES - reserve) }
  const bytes = serializeUpdateManifest(exact)
  assert.equal(bytes.length, MAX_UPDATE_MANIFEST_BYTES)
  assert.equal(bytes.at(-1), 10)
  assert.throws(() => serializeUpdateManifest({ ...exact, notes: exact.notes + 'x' }), /1 MiB/)
})

test('shared GitHub pagination finds older tags and rejects duplicates across pages', async () => {
  const requests = []
  const first = Array.from({ length: 100 }, (_, index) => published(`v1.${index}.0`))
  let second = [published('v2.0.0', 'second page')]
  const github = new GitHubRelease('test-fixture', async (input) => {
    const url = new URL(input)
    requests.push(url.pathname + url.search)
    assert.equal(url.origin, 'https://api.github.com')
    return Response.json(url.searchParams.get('page') === '1' ? first : second)
  })
  assert.equal((await github.releases()).length, 101)
  assert.equal((await github.findRelease('v2.0.0')).body, 'second page')
  assert.equal(requests.length, 4)
  assert.ok(requests.every(path => path.startsWith(`/repos/${RELEASE_OWNER}/${RELEASE_REPO}/releases?per_page=100&page=`)))
  second = [published('v1.0.0')]
  await assert.rejects(github.findRelease('v1.0.0'), /Multiple releases/)
})

test('release pagination rejects malformed responses and stops at the existing safety cap', async () => {
  const invalid = new GitHubRelease('test-fixture', async () => Response.json({ error: 'unexpected' }))
  await assert.rejects(invalid.releases(), /Invalid GitHub release list/)
  let pages = 0
  const endless = new GitHubRelease('test-fixture', async () => {
    pages++
    return Response.json(Array.from({ length: 100 }, () => published('v1.0.0')))
  })
  await assert.rejects(endless.releases(), /Too many releases/)
  assert.equal(pages, 100)
})

function signedFixture({ failHistory = false, corruptPackage = false, corruptSignature = false,
  signedVersion, history = [published('v2.0.9', '- Previous changes')] } = {}) {
  const version = '2.10.0'
  const release = { id: 7, tag_name: `v${version}`, body: '- Target changes', draft: true, prerelease: false }
  const { privateKey, publicKey } = generateKeyPairSync('ed25519')
  const keyId = Buffer.from('0102030405060708', 'hex')
  const keyBytes = Buffer.concat([Buffer.from('Ed'), keyId, publicKey.export({ format: 'der', type: 'spki' }).subarray(-32)])
  const encodedKey = Buffer.from(`untrusted comment: fixture public key\n${keyBytes.toString('base64')}\n`).toString('base64')
  const assets = []
  const downloads = new Map()
  for (const [index, name] of [
    `OC2ModManager_${version}_x64-setup.exe`, `OC2ModManager_${version}_x64_en-US.msi`,
    'OC2ModManager_aarch64.app.tar.gz', 'OC2ModManager_x64.app.tar.gz',
  ].entries()) {
    const id = index * 2 + 1
    const bytes = Buffer.from(`signed package ${index}`)
    const rawSignature = sign(null, bytes, privateKey)
    const signatureBytes = Buffer.concat([Buffer.from('Ed'), keyId, rawSignature])
    if (corruptSignature && index === 0) signatureBytes[signatureBytes.length - 1] ^= 1
    const comment = `timestamp:1\tversion:${signedVersion || version}`
    const globalSignature = sign(null, Buffer.concat([rawSignature, Buffer.from(comment)]), privateKey)
    const encodedSignature = Buffer.from([
      'untrusted comment: fixture signature', signatureBytes.toString('base64'),
      `trusted comment: ${comment}`, globalSignature.toString('base64'), '',
    ].join('\n')).toString('base64')
    assets.push({ id, name, size: bytes.length, digest: `sha256:${createHash('sha256').update(bytes).digest('hex')}`,
      browser_download_url: `https://github.com/${RELEASE_OWNER}/${RELEASE_REPO}/releases/download/v${version}/${name}` })
    assets.push({ id: id + 1, name: `${name}.sig` })
    downloads.set(id, corruptPackage && index === 0 ? Buffer.from('corrupted same sz') : bytes)
    downloads.set(id + 1, Buffer.from(encodedSignature))
  }
  const requests = []
  const github = new GitHubRelease('test-fixture', async (input) => {
    const url = new URL(input)
    requests.push(url.pathname + url.search)
    if (url.pathname.endsWith('/releases')) {
      if (failHistory) return new Response('unavailable', { status: 503 })
      return Response.json(history)
    }
    const id = Number(url.pathname.split('/').at(-1))
    assert.ok(downloads.has(id), `Unexpected request: ${url.pathname}`)
    return new Response(downloads.get(id))
  })
  return { github, release, assets, version, publicKey: encodedKey, requests }
}

test('real manifest aggregation verifies all signed packages before adding Release history', async () => {
  const fixture = signedFixture()
  const result = await aggregateManifest(fixture.github, fixture.release, fixture.assets, fixture.version, fixture.publicKey)
  assert.deepEqual(Object.keys(result.platforms).sort(), [...PLATFORM_KEYS].sort())
  assert.equal(result.notes, fixture.release.body)
  assert.deepEqual(result.changelog, [
    { version: '2.0.9', notes: '- Previous changes' }, { version: '2.10.0', notes: '- Target changes' },
  ])
  assert.equal(fixture.requests.length, PLATFORM_KEYS.length * 2 + 1)
  assert.ok(fixture.requests.at(-1).endsWith('/releases?per_page=100&page=1'))
  assert.ok(serializeUpdateManifest(result).length <= MAX_UPDATE_MANIFEST_BYTES)
})

for (const [label, options] of [
  ['HTTP history failure', { failHistory: true }], ['malformed history list', { history: {} }],
  ['malformed published body', { history: [published('v2.0.9', {})] }],
]) {
  test(`manifest aggregation retains signed targets and latest notes after ${label}`, async () => {
    const fixture = signedFixture(options)
    const result = await aggregateManifest(fixture.github, fixture.release, fixture.assets, fixture.version, fixture.publicKey)
    assert.deepEqual(Object.keys(result.platforms).sort(), [...PLATFORM_KEYS].sort())
    assert.equal(result.notes, fixture.release.body)
    assert.equal(Object.hasOwn(result, 'changelog'), false)
  })
}

for (const [label, options, error] of [
  ['corrupt package', { corruptPackage: true }, /Incomplete uploaded package|digest mismatch/],
  ['corrupt signature', { corruptSignature: true }, /signature verification failed/],
  ['signature bound to another version', { signedVersion: '2.9.0' }, /not bound to this release version/],
]) {
  test(`optional history cannot hide ${label} failures`, async () => {
    const fixture = signedFixture({ failHistory: true, ...options })
    await assert.rejects(aggregateManifest(fixture.github, fixture.release, fixture.assets, fixture.version, fixture.publicKey), error)
    assert.equal(fixture.requests.some(path => path.includes('?per_page=')), false)
  })
}
