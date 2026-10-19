import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { chmodSync, existsSync, lstatSync, mkdirSync, readFileSync, rmSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import { BuildCacheAPI, CACHE_DIRECTORY, MAX_CACHE_BYTES, cacheContext, cacheKey, cacheRefreshSource, hydrateCache, managedCache, snapshotCache } from '../scripts/build-cache.mjs'
import { BUILD_KEYS, RELEASE_OWNER, RELEASE_REPO } from '../scripts/release-lib.mjs'

const root = fileURLToPath(new URL('../', import.meta.url))
const targets = Object.keys(BUILD_KEYS)
const target = 'x86_64-unknown-linux-gnu'
const sourceSha = 'a'.repeat(40)
const trusted = { GITHUB_ACTIONS: 'true', GITHUB_REF: 'refs/heads/main', GITHUB_EVENT_NAME: 'workflow_dispatch',
  GITHUB_SHA: sourceSha, GITHUB_REPOSITORY: `${RELEASE_OWNER}/${RELEASE_REPO}`, GITHUB_TOKEN: 'unit-test-token' }

function releaseCompletion(sha = sourceSha) {
  return {
    action: 'completed', repository: { full_name: trusted.GITHUB_REPOSITORY, default_branch: 'main' },
    workflow: { id: 17, name: 'Build and Release', path: '.github/workflows/build.yml' },
    workflow_run: { id: 123, workflow_id: 17, event: 'push', status: 'completed', conclusion: 'success',
      head_branch: 'v1.2.8', head_sha: sha, head_repository: { full_name: trusted.GITHUB_REPOSITORY } },
  }
}

function workspace(t) {
  const base = join(root, 'src-tauri/target/verification/build-cache-unit')
  const directory = join(base, randomUUID())
  mkdirSync(directory, { recursive: true })
  t.after(() => {
    assert.ok(resolve(directory).startsWith(`${resolve(base)}${sep}`))
    rmSync(directory, { recursive: true, force: true })
  })
  const context = cacheContext({ root: directory, target, cargoHome: join(directory, 'cargo'),
    npmCache: join(directory, 'npm'), rustc: 'rustc test-toolchain' })
  function write(scope, path, text = path) {
    const file = join(context.scopes[scope], path)
    mkdirSync(dirname(file), { recursive: true })
    writeFileSync(file, text)
    return file
  }
  return { directory, context, write }
}

test('four fixed keys combine npm and Cargo without source, version, tag or lockfile hashes', () => {
  assert.equal(targets.length, 4)
  assert.equal(new Set(targets.map(cacheKey)).size, 4)
  for (const value of targets) assert.equal(cacheKey(value), `oc2-build-v1-${value}`)
  assert.throws(() => cacheKey('unknown'))
  assert.ok(MAX_CACHE_BYTES * 4 < 8 * 1024 ** 3)
})

test('the real snapshot round trip restores allowed downloads and compiler dependencies, not credentials or bundles', t => {
  const { directory, context, write } = workspace(t)
  const expected = [
    ['npm', 'content-v2/sha512/file'], ['crates', 'registry/serde.crate'], ['index', 'registry/.cache/config'],
    ['sources', 'registry/serde/src/lib.rs'],
    ['release', 'deps/libserde-123.rlib'], ['hostRelease', 'build/serde-123/out/output'],
    ['debug', '.fingerprint/serde-123/lib-serde'],
  ]
  for (const [scope, path] of expected) write(scope, path)
  const executable = write('release', 'build/helper-123/build-script-build')
  chmodSync(executable, 0o755)
  const originalTime = new Date('2026-01-01T01:02:03Z')
  utimesSync(executable, originalTime, originalTime)
  for (const path of ['bundle/app/installer.exe', 'incremental/private/data', 'OC2ModManager.exe',
    'deps/app-123.exe', 'deps/libapp_lib-123.rlib', '.fingerprint/app-123/lib-app', 'deps/cache.lock']) write('release', path)
  writeFileSync(join(directory, 'cargo/credentials.toml'), 'do-not-cache')
  writeFileSync(join(directory, 'npm/.npmrc'), 'do-not-cache')
  const manifest = snapshotCache(context)
  assert.equal(manifest.entries.length, expected.length + 1)
  assert.ok(!JSON.stringify(manifest).includes('credentials'))
  for (const base of Object.values(context.scopes)) rmSync(base, { recursive: true, force: true })
  assert.equal(hydrateCache(context), expected.length + 1)
  for (const [scope, path] of expected) assert.equal(readFileSync(join(context.scopes[scope], path), 'utf8'), path)
  if (process.platform !== 'win32') assert.equal(lstatSync(executable).mode & 0o777, 0o755)
  assert.equal(lstatSync(executable).mtime.toISOString(), originalTime.toISOString())
  assert.equal(existsSync(join(context.scopes.release, 'bundle')), false)
})

test('a bounded snapshot omits oversized files, keeps smaller files and never changes the source', t => {
  const { context, write } = workspace(t)
  const small = write('npm', 'small', 'cached')
  const large = write('release', 'deps/too-large.rlib', Buffer.alloc(80_000))
  const manifest = snapshotCache(context, 80_000)
  assert.ok(manifest.budgetBytes <= 80_000)
  assert.deepEqual(manifest.entries.map(entry => entry.path), ['small'])
  assert.equal(readFileSync(small, 'utf8'), 'cached')
  assert.equal(lstatSync(large).size, 80_000)
  assert.throws(() => snapshotCache(context, MAX_CACHE_BYTES + 1))
})

test('missing caches are cold starts; empty snapshots cannot replace a usable remote cache', t => {
  const { context } = workspace(t)
  assert.equal(hydrateCache(context), 0)
  assert.throws(() => snapshotCache(context), /empty snapshot/)
})

test('Rust toolchain changes restore downloads but skip old compiler objects', t => {
  const { context, write } = workspace(t)
  write('crates', 'registry/serde.crate')
  write('release', 'deps/libserde-123.rlib')
  snapshotCache(context)
  for (const base of Object.values(context.scopes)) rmSync(base, { recursive: true, force: true })
  assert.equal(hydrateCache({ ...context, rustc: 'new toolchain' }), 1)
  assert.equal(existsSync(join(context.scopes.release, 'deps/libserde-123.rlib')), false)
})

test('manifest validation rejects path traversal, unknown scopes, incomplete files, links and forged budgets before restoring', t => {
  const { context, write, directory } = workspace(t)
  write('npm', 'valid')
  const original = snapshotCache(context)
  rmSync(context.scopes.npm, { recursive: true, force: true })
  const manifestFile = join(context.cache, 'manifest.json')
  for (const mutate of [
    manifest => { manifest.target = targets.find(value => value !== target) },
    manifest => { manifest.entries[0].path = '../escape' },
    manifest => { manifest.entries[0].path = '/escape' },
    manifest => { manifest.entries[0].path = 'C:/escape' },
    manifest => { manifest.entries[0].path = 'valid\\escape' },
    manifest => { manifest.entries[0].scope = '__proto__' },
    manifest => { manifest.entries[0].size++ },
    manifest => { manifest.entries[0].mode = 0o4777 },
    manifest => { manifest.entries.push({ ...manifest.entries[0] }) },
    manifest => { manifest.budgetBytes++ },
  ]) {
    const manifest = structuredClone(original)
    mutate(manifest)
    writeFileSync(manifestFile, JSON.stringify(manifest))
    assert.throws(() => hydrateCache(context))
    assert.equal(existsSync(context.scopes.npm), false)
  }
  writeFileSync(manifestFile, JSON.stringify(original))
  const outside = join(directory, 'outside')
  mkdirSync(outside)
  symlinkSync(outside, context.scopes.npm, process.platform === 'win32' ? 'junction' : 'dir')
  assert.throws(() => hydrateCache(context), /links/)
  assert.deepEqual(readFileSync(join(context.cache, 'npm/valid'), 'utf8'), 'valid')
})

test('snapshot ignores linked source directories and refuses a linked snapshot destination', t => {
  const { context, write, directory } = workspace(t)
  write('crates', 'registry/dependency.crate')
  const outside = join(directory, 'outside')
  mkdirSync(outside)
  writeFileSync(join(outside, 'private'), 'do-not-cache')
  mkdirSync(context.scopes.npm, { recursive: true })
  symlinkSync(outside, join(context.scopes.npm, 'linked'), process.platform === 'win32' ? 'junction' : 'dir')
  assert.equal(snapshotCache(context).entries.length, 1)
  rmSync(context.cache, { recursive: true, force: true })
  symlinkSync(outside, context.cache, process.platform === 'win32' ? 'junction' : 'dir')
  assert.throws(() => snapshotCache(context), /links/)
  assert.equal(readFileSync(join(outside, 'private'), 'utf8'), 'do-not-cache')
})

test('only trusted main manual runs or validated tag completions can access the destructive cache API', () => {
  for (const env of [
    { ...trusted, GITHUB_REF: 'refs/tags/v1.2.4' }, { ...trusted, GITHUB_REF: 'refs/heads/feature' },
    { ...trusted, GITHUB_EVENT_NAME: 'push' }, { ...trusted, GITHUB_EVENT_NAME: 'pull_request_target' },
    { ...trusted, GITHUB_ACTIONS: 'false' }, { ...trusted, GITHUB_SHA: '' }, { ...trusted, GITHUB_SHA: 'main' },
    { ...trusted, GITHUB_REPOSITORY: 'somebody/other' }, { ...trusted, GITHUB_TOKEN: '' },
  ]) assert.throws(() => new BuildCacheAPI(env), /trusted main/)
})

test('successful tag completions must match the release workflow, repository and current main source', () => {
  const env = { ...trusted, GITHUB_EVENT_NAME: 'workflow_run' }
  assert.equal(cacheRefreshSource(trusted), sourceSha)
  assert.equal(cacheRefreshSource(env, releaseCompletion()), sourceSha)
  for (const [name, mutate] of [
    ['requested event', event => { event.action = 'requested' }],
    ['main push', event => { event.workflow_run.head_branch = 'main' }],
    ['feature push', event => { event.workflow_run.head_branch = 'feature/test' }],
    ['invalid version tag', event => { event.workflow_run.head_branch = 'v1.2' }],
    ['test dispatch', event => { event.workflow_run.event = 'workflow_dispatch' }],
    ['pull request', event => { event.workflow_run.event = 'pull_request' }],
    ['still running', event => { event.workflow_run.status = 'in_progress' }],
    ...['failure', 'cancelled', 'timed_out', 'skipped', null].map(conclusion => [
      `unsuccessful ${conclusion}`, event => { event.workflow_run.conclusion = conclusion },
    ]),
    ['wrong workflow path', event => { event.workflow.path = '.github/workflows/build-cache.yml' }],
    ['wrong workflow name', event => { event.workflow.name = 'Some other workflow' }],
    ['wrong workflow id', event => { event.workflow_run.workflow_id++ }],
    ['invalid workflow id', event => { event.workflow.id = event.workflow_run.workflow_id = 0 }],
    ['invalid run id', event => { event.workflow_run.id = 0 }],
    ['fork source', event => { event.workflow_run.head_repository.full_name = 'fork/other' }],
    ['missing source repository', event => { delete event.workflow_run.head_repository }],
    ['wrong event repository', event => { event.repository.full_name = 'fork/other' }],
    ['wrong default branch', event => { event.repository.default_branch = 'other' }],
    ['old tag or main advanced', event => { event.workflow_run.head_sha = 'b'.repeat(40) }],
    ['no source SHA', event => { delete event.workflow_run.head_sha }],
  ]) {
    const event = releaseCompletion()
    mutate(event)
    assert.throws(() => cacheRefreshSource(env, event), /trusted main/, name)
  }
  for (const event of [null, {}, { workflow_run: {} }]) {
    assert.throws(() => cacheRefreshSource(env, event), /trusted main/)
  }
})

test('the cache API reads and validates the actual workflow_run event file before any cache request', async t => {
  const { directory } = workspace(t)
  const eventFile = join(directory, 'event.json')
  const env = { ...trusted, GITHUB_EVENT_NAME: 'workflow_run', GITHUB_EVENT_PATH: eventFile }
  const valid = { id: 1, key: cacheKey(target), ref: 'refs/heads/main', size_in_bytes: 100 }
  let calls = 0
  const fetcher = async () => { calls++; return Response.json({ actions_caches: [valid] }) }
  for (const body of ['{invalid-json', '{}', JSON.stringify({ ...releaseCompletion(), action: 'requested' })]) {
    writeFileSync(eventFile, body)
    assert.throws(() => new BuildCacheAPI(env, fetcher), /trusted main/)
    assert.equal(calls, 0)
  }
  assert.throws(() => new BuildCacheAPI({ ...env, GITHUB_EVENT_PATH: join(directory, 'missing') }, fetcher), /trusted main/)
  writeFileSync(eventFile, JSON.stringify(releaseCompletion()))
  await new BuildCacheAPI(env, fetcher).verify(target)
  assert.equal(calls, 1)
})

test('the real authorization CLI requires both a trusted event and the pinned checkout', t => {
  const { directory } = workspace(t)
  const checkout = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: root, encoding: 'utf8', windowsHide: true })
  assert.equal(checkout.status, 0)
  const sha = checkout.stdout.trim()
  const eventFile = join(directory, 'event.json')
  writeFileSync(eventFile, JSON.stringify(releaseCompletion(sha)))
  const execute = overrides => spawnSync(process.execPath, [join(root, 'scripts/build-cache.mjs'), 'authorize'], {
    cwd: root, env: { ...process.env, ...trusted, GITHUB_SHA: sha, ...overrides }, encoding: 'utf8', windowsHide: true,
  })
  for (const overrides of [{}, { GITHUB_EVENT_NAME: 'workflow_run', GITHUB_EVENT_PATH: eventFile }]) {
    const result = execute(overrides)
    assert.equal(result.status, 0, result.stderr)
    assert.ok(result.stdout.includes(sha))
    assert.ok(!result.stdout.includes(trusted.GITHUB_TOKEN))
  }
  const mismatch = execute({ GITHUB_SHA: sourceSha === sha ? 'b'.repeat(40) : sourceSha })
  assert.equal(mismatch.status, 1)
  assert.match(mismatch.stderr, /source does not match the authorized main SHA/)
  for (const overrides of [{ GITHUB_EVENT_NAME: 'push' }, { GITHUB_EVENT_NAME: 'workflow_run', GITHUB_EVENT_PATH: eventFile, GITHUB_SHA: 'b'.repeat(40) }]) {
    const result = execute(overrides)
    assert.equal(result.status, 1)
    assert.match(result.stderr, /trusted main/)
    assert.ok(!result.stderr.includes(trusted.GITHUB_TOKEN))
  }
})

test('a ready snapshot cannot reach replacement or verification from a main push or a mismatched checkout', t => {
  const { directory, context, write } = workspace(t)
  write('npm', 'valid')
  snapshotCache(context)
  for (const command of ['replace', 'verify']) {
    for (const overrides of [{ GITHUB_EVENT_NAME: 'push' }, {}]) {
      const result = spawnSync(process.execPath, [join(root, 'scripts/build-cache.mjs'), command, target], {
        cwd: directory, env: { ...process.env, ...trusted, ...overrides }, encoding: 'utf8', windowsHide: true,
      })
      assert.equal(result.status, 1)
      assert.match(result.stderr, /trusted main|source does not match the authorized main SHA/)
      assert.ok(!result.stdout.includes('Removed'))
      assert.ok(existsSync(join(context.cache, 'manifest.json')))
    }
  }
})

test('cleanup paginates before deleting and removes only this platform caches, keeping the current entry until last', async () => {
  const calls = []
  const rows = Array.from({ length: 100 }, (_, i) => ({ id: i + 1, key: `unrelated-${i}` }))
  rows[0] = { id: 1, key: cacheKey(target), ref: 'refs/heads/main' }
  rows[1] = { id: 2, key: 'node-cache-Linux-x64-npm-123', ref: 'refs/tags/v1.2.3' }
  rows[2] = { id: 3, key: cacheKey('aarch64-apple-darwin') }
  const github = new BuildCacheAPI(trusted, async (url, options) => {
    const parsed = new URL(url)
    calls.push(`${options.method} ${parsed.pathname}${parsed.search}`)
    assert.equal(parsed.hostname, 'api.github.com')
    assert.equal(options.headers.Authorization, 'Bearer unit-test-token')
    if (options.method === 'DELETE') return new Response(null, { status: parsed.pathname.endsWith('/1') ? 204 : 404 })
    return Response.json({ actions_caches: parsed.searchParams.get('page') === '1' ? rows
      : [{ id: 101, key: `v0-rust-${target}-build-Linux-x64-key`, ref: 'refs/tags/v1.2.2' }] })
  })
  assert.equal(await github.replace(target), 3)
  assert.deepEqual(calls.map(call => call.split('/actions/caches')[1]), [
    '?per_page=100&page=1', '?per_page=100&page=2', '/2', '/101', '/1',
  ])
  for (const value of targets) {
    assert.equal(managedCache(cacheKey(value), target), value === target)
    assert.equal(managedCache(`v0-rust-${value}-build-platform-key`, target), value === target)
  }
})

test('list failures, invalid IDs and HTTP failures cannot silently proceed to save', async () => {
  for (const scenario of ['list', 'id', 'delete']) {
    const deleted = []
    const github = new BuildCacheAPI(trusted, async (_url, options) => {
      if (options.method === 'DELETE') { deleted.push(true); return new Response('private error', { status: 403 }) }
      if (scenario === 'list') return new Response('private error', { status: 500 })
      return Response.json({ actions_caches: [{ id: scenario === 'id' ? '../1' : 1, key: cacheKey(target) }] })
    })
    await assert.rejects(github.replace(target), error => !error.message.includes('private error') && !error.message.includes(trusted.GITHUB_TOKEN))
    assert.equal(deleted.length, scenario === 'delete' ? 1 : 0)
  }
})

test('verification requires exactly one correctly scoped entry under the hard size cap', async () => {
  const valid = { id: 1, key: cacheKey(target), ref: 'refs/heads/main', size_in_bytes: 100 }
  for (const entries of [[], [valid, valid], [{ ...valid, ref: 'refs/tags/v1.2.4' }],
    [{ ...valid, size_in_bytes: MAX_CACHE_BYTES + 1 }], [{ ...valid, size_in_bytes: 0 }]]) {
    const github = new BuildCacheAPI(trusted, async () => Response.json({ actions_caches: entries }))
    await assert.rejects(github.verify(target), /exactly one bounded/)
  }
  await new BuildCacheAPI(trusted, async () => Response.json({ actions_caches: [valid] })).verify(target)
})

test('post-upload verification allows bounded API visibility delay, never an unbounded wait', async () => {
  let calls = 0
  let pauses = 0
  const github = new BuildCacheAPI(trusted, async () => Response.json({ actions_caches: ++calls < 3 ? []
    : [{ id: 1, key: cacheKey(target), ref: 'refs/heads/main', size_in_bytes: 100 }] }))
  await github.verify(target, { attempts: 3, pause: async () => { pauses++ } })
  assert.equal(calls, 3)
  assert.equal(pauses, 2)
  const absent = new BuildCacheAPI(trusted, async () => { calls++; return Response.json({ actions_caches: [] }) })
  calls = 0
  await assert.rejects(absent.verify(target, { attempts: 3, pause: async () => {} }), /exactly one bounded/)
  assert.equal(calls, 3)
})

test('replacement CLI validates a ready local snapshot before touching GitHub', t => {
  const { directory } = workspace(t)
  const result = spawnSync(process.execPath, [join(root, 'scripts/build-cache.mjs'), 'replace', target], {
    cwd: directory, env: { ...process.env, ...trusted }, encoding: 'utf8', windowsHide: true,
  })
  assert.equal(result.status, 1)
  assert.match(result.stderr, /complete new snapshot/)
})

test('real restore CLI removes its staging snapshot before npm test can discover cached third-party tests', t => {
  const { directory, context, write } = workspace(t)
  write('npm', 'content-v2/content')
  write('sources', 'registry/dependency/tests/unrelated.test.js')
  snapshotCache(context)
  for (const base of Object.values(context.scopes)) rmSync(base, { recursive: true, force: true })
  const result = spawnSync(process.execPath, [join(root, 'scripts/build-cache.mjs'), 'restore', target], {
    cwd: directory, env: { ...process.env, CARGO_HOME: join(directory, 'cargo'), NPM_CONFIG_CACHE: join(directory, 'npm') },
    encoding: 'utf8', windowsHide: true,
  })
  assert.equal(result.status, 0, result.stderr)
  assert.equal(existsSync(context.cache), false)
  assert.equal(readFileSync(join(context.scopes.npm, 'content-v2/content'), 'utf8'), 'content-v2/content')
})

test('real restore CLI rejects a corrupt snapshot without blocking a cold release build', t => {
  const { directory, context } = workspace(t)
  mkdirSync(context.cache, { recursive: true })
  writeFileSync(join(context.cache, 'manifest.json'), '{invalid-json')
  const result = spawnSync(process.execPath, [join(root, 'scripts/build-cache.mjs'), 'restore', target], {
    cwd: directory, env: { ...process.env, CARGO_HOME: join(directory, 'cargo'), NPM_CONFIG_CACHE: join(directory, 'npm') },
    encoding: 'utf8', windowsHide: true,
  })
  assert.equal(result.status, 0, result.stderr)
  assert.match(result.stderr, /cold build/)
  assert.equal(existsSync(context.cache), false)
  assert.equal(existsSync(context.scopes.npm), false)
})

test('release is restore-only and main is the only serialized, successful-build writer', () => {
  const release = readFileSync(join(root, '.github/workflows/build.yml'), 'utf8')
  const warmup = readFileSync(join(root, '.github/workflows/build-cache.yml'), 'utf8')
  const composite = readFileSync(join(root, '.github/actions/build-cache/action.yml'), 'utf8')
  for (const workflow of [release, warmup]) {
    assert.ok(workflow.includes('package-manager-cache: false'))
    assert.equal((workflow.match(/uses: actions\/setup-node@v5/g) || []).length,
      (workflow.match(/package-manager-cache: false/g) || []).length)
    assert.ok(workflow.includes('uses: ./.github/actions/build-cache'))
    assert.ok(!workflow.includes('swatinem/rust-cache'))
    assert.ok(!workflow.includes("cache: 'npm'"))
    for (const value of targets) assert.ok(workflow.includes(`target: '${value}'`))
  }
  assert.ok(!release.includes('actions/cache/save'))
  assert.ok(!release.includes('actions: write'))
  assert.ok(composite.includes('actions/cache/restore@v4'))
  assert.ok(!composite.includes('actions/cache/save'))
  assert.ok(!/^  push:/m.test(warmup))
  assert.match(warmup, /workflow_dispatch:[\s\S]*workflow_run:[\s\S]*workflows: \['Build and Release'\][\s\S]*types: \[completed\]/)
  for (const condition of ["github.ref == 'refs/heads/main'", "github.event_name == 'workflow_dispatch'",
    "github.event_name == 'workflow_run'", "github.event.workflow_run.event == 'push'",
    "github.event.workflow_run.conclusion == 'success'", "startsWith(github.event.workflow_run.head_branch, 'v')",
    'github.event.workflow_run.head_repository.full_name == github.repository',
    'github.event.workflow_run.head_sha == github.sha']) assert.ok(warmup.includes(condition))
  assert.ok(warmup.includes('ref: ${{ github.sha }}'))
  assert.ok(warmup.includes('run: node scripts/build-cache.mjs authorize'))
  assert.ok(warmup.indexOf('build-cache.mjs authorize') < warmup.indexOf('name: Restore shared platform cache'))
  assert.ok(warmup.includes('group: oc2-main-build-cache-${{ matrix.target }}'))
  assert.ok(warmup.includes('cancel-in-progress: false'))
  assert.ok(warmup.includes('actions: write'))
  assert.ok(!warmup.includes('contents: write'))
  assert.ok(!warmup.includes('TAURI_SIGNING_PRIVATE_KEY'))
  assert.ok(!warmup.includes('always()'))
  assert.ok(warmup.indexOf('cargo build ') < warmup.indexOf('build-cache.mjs snapshot'))
  assert.ok(warmup.indexOf('build-cache.mjs snapshot') < warmup.indexOf('build-cache.mjs replace'))
  assert.ok(warmup.indexOf('build-cache.mjs replace') < warmup.indexOf('uses: actions/cache/save'))
  assert.equal(CACHE_DIRECTORY, 'src-tauri/target/ci-build-cache')
})
