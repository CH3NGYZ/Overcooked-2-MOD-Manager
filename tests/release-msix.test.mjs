import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import test from 'node:test'
import { GitHubRelease } from '../scripts/github-release.mjs'
import { RELEASE_OWNER, RELEASE_REPO } from '../scripts/release-lib.mjs'
import { uploadStoreMsix } from '../scripts/release.mjs'

const version = '2.4.5'
const tag = `v${version}`
const sha = 'a'.repeat(40)
const release = { id: 17, tag_name: tag, draft: true, prerelease: false, target_commitish: sha }
const name = 'OC2ModManager_2.4.5_x64.msix'
const bytes = Buffer.from('packaging fixture')
const digest = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
const uploaded = { id: 23, state: 'uploaded', name, digest, size: bytes.length }

function fixture({ drafts = [release], files = [], tagSha = sha, annotated = false, tagged = true } = {}) {
  const calls = []
  let reads = 0
  const github = new GitHubRelease('test-fixture', async (url, options) => {
    const parsed = new URL(url)
    const prefix = `/repos/${RELEASE_OWNER}/${RELEASE_REPO}`
    assert.ok(parsed.pathname.startsWith(`${prefix}/`))
    const endpoint = parsed.pathname.slice(prefix.length)
    calls.push({ endpoint, parsed, ...options })
    if (options.method === 'POST') return Response.json(uploaded, { status: 201 })
    if (endpoint === `/git/ref/tags/${tag}`) return Response.json({ ref: `refs/tags/${tag}`, object: annotated
      ? { type: 'tag', sha: 'b'.repeat(40) } : { type: 'commit', sha: tagSha } })
    if (endpoint === `/git/tags/${'b'.repeat(40)}`) return Response.json({ sha: 'b'.repeat(40), object: { type: 'commit', sha: tagSha } })
    if (endpoint === `/releases/${release.id}`) return Response.json(drafts[Math.min(reads++, drafts.length - 1)])
    if (endpoint === `/releases/${release.id}/assets`) return Response.json(files)
    if (options.method === 'DELETE' && endpoint === `/releases/assets/${uploaded.id}`) return new Response(null, { status: 204 })
    if (endpoint === `/releases/assets/${uploaded.id}`) return new Response(bytes)
    assert.fail(`Unexpected GitHub request ${options.method} ${endpoint}`)
  })
  return { calls, upload: () => uploadStoreMsix(github, release.id, version, sha, name, bytes, tagged) }
}

test('uploads one unsigned Store MSIX as a binary attachment to the same draft', async () => {
  const { upload, calls } = fixture({ annotated: true })
  await upload()
  const writes = calls.filter(call => call.method !== 'GET')
  assert.equal(writes.length, 1)
  const [write] = writes
  assert.equal(write.method, 'POST')
  assert.equal(write.endpoint, `/releases/${release.id}/assets`)
  assert.equal(write.parsed.origin, 'https://uploads.github.com')
  assert.equal(write.parsed.searchParams.get('name'), name)
  assert.equal(write.parsed.searchParams.get('label'), 'Microsoft Store submission (unsigned MSIX)')
  assert.equal(write.headers['Content-Type'], 'application/octet-stream')
  assert.deepEqual(write.body, bytes)
  assert.equal(calls.at(-2).endpoint, `/releases/${release.id}`)
})

test('manual stable draft uses the recorded source SHA without requiring an existing tag', async () => {
  const { upload, calls } = fixture({ tagged: false })
  assert.equal(await upload(), true)
  assert.ok(calls.every(call => !call.endpoint.startsWith('/git/')))
})

for (const [label, options, error] of [
  ['already published', { drafts: [{ ...release, draft: false }] }, /stable draft/],
  ['published during preparation', { drafts: [release, { ...release, draft: false }] }, /stable draft/],
  ['prerelease', { drafts: [{ ...release, prerelease: true }] }, /stable draft/],
  ['wrong draft tag', { drafts: [{ ...release, tag_name: 'v2.4.4' }] }, /stable draft/],
  ['wrong draft source', { drafts: [{ ...release, target_commitish: 'main' }] }, /source commit/],
  ['moved tag', { tagSha: 'c'.repeat(40), annotated: true }, /source commit/],
  ['different binary', { files: [{ ...uploaded, digest: `sha256:${'0'.repeat(64)}` }] }, /Refusing to overwrite/],
]) {
  test(`no GitHub writes when MSIX upload encounters ${label}`, async () => {
    const { upload, calls } = fixture(options)
    await assert.rejects(upload(), error)
    assert.ok(calls.every(call => call.method === 'GET'))
  })
}

test('same bytes are a no-op, even if GitHub has not generated an asset digest', async () => {
  for (const expectedDigest of [digest, null]) {
    const { upload, calls } = fixture({ files: [{ ...uploaded, digest: expectedDigest }] })
    assert.equal(await upload(), false)
    assert.ok(calls.every(call => call.method === 'GET'))
    assert.equal(calls.some(call => call.endpoint === `/releases/assets/${uploaded.id}`), expectedDigest === null)
  }
})

test('only an exact empty starter left by a failed upload is removed before retry', async () => {
  const { upload, calls } = fixture({ files: [{ ...uploaded, size: 0, state: 'starter' }] })
  assert.equal(await upload(), true)
  assert.deepEqual(calls.filter(call => call.method !== 'GET').map(call => call.method), ['DELETE', 'POST'])
  for (const [index, call] of calls.entries()) {
    if (call.method !== 'GET') assert.equal(calls[index - 1].endpoint, `/releases/${release.id}`)
  }
})
