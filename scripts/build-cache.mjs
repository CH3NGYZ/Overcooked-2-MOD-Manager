import { spawnSync } from 'node:child_process'
import { appendFileSync, chmodSync, copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, utimesSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { BUILD_KEYS, RELEASE_OWNER, RELEASE_REPO } from './release-lib.mjs'

export const MAX_CACHE_BYTES = 1800 * 1024 * 1024
export const CACHE_DIRECTORY = 'src-tauri/target/ci-build-cache'
const manifestReserve = 64 * 1024
const prefix = 'oc2-build-v1-'

export function cacheKey(target) {
  if (!Object.hasOwn(BUILD_KEYS, target)) throw new Error('Unsupported build-cache target')
  return `${prefix}${target}`
}

function below(base, path) {
  const suffix = relative(base, path)
  if (!suffix || isAbsolute(suffix) || suffix === '..' || suffix.startsWith(`..${sep}`)) {
    throw new Error('Cache path escapes its allowed directory')
  }
  return path
}

function noLinks(base, path) {
  let current = resolve(path)
  if (current !== base) below(base, current)
  while (true) {
    if (lstatSync(current, { throwIfNoEntry: false })?.isSymbolicLink()) throw new Error('Cache paths must not contain links')
    if (current === base) break
    current = dirname(current)
  }
}

function entryPath(base, path) {
  if (typeof path !== 'string' || path.length > 1024 || /[\\:\x00-\x1f]/.test(path)
    || path.split('/').some(part => !part || part === '.' || part === '..')) {
    throw new Error('Invalid cache entry path')
  }
  const result = below(base, resolve(base, ...path.split('/')))
  noLinks(base, result)
  return result
}

export function cacheContext({ root = process.cwd(), target, cargoHome = process.env.CARGO_HOME || join(homedir(), '.cargo'),
  npmCache = process.env.NPM_CONFIG_CACHE || join(process.env.RUNNER_TEMP || join(root, 'src-tauri/target'), 'oc2-npm-cache'), rustc } = {}) {
  cacheKey(target)
  root = resolve(root)
  const targetRoot = join(root, 'src-tauri/target')
  const cache = join(root, CACHE_DIRECTORY)
  noLinks(root, cache)
  return {
    root, target, cache, rustc,
    scopes: {
      npm: join(resolve(npmCache), '_cacache'),
      crates: join(resolve(cargoHome), 'registry/cache'),
      index: join(resolve(cargoHome), 'registry/index'),
      sources: join(resolve(cargoHome), 'registry/src'),
      release: join(targetRoot, target, 'release'),
      hostRelease: join(targetRoot, 'release'),
      debug: join(targetRoot, 'debug'),
    },
    npmCache: resolve(npmCache),
  }
}

function accepted(scope, path) {
  if (typeof path !== 'string') return false
  if (['npm', 'crates', 'index', 'sources'].includes(scope)) return true
  const [directory, name] = path.split('/')
  return ['deps', 'build', '.fingerprint'].includes(directory) && name
    && !/^(lib)?app[-_.]/.test(name) && !path.endsWith('.lock')
}

function filesIn(base, scope, path = '') {
  const directory = path ? join(base, path) : base
  if (!existsSync(directory)) return []
  if (lstatSync(directory).isSymbolicLink()) return []
  const files = []
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const child = path ? `${path}/${entry.name}` : entry.name
    if (!path && !['npm', 'crates', 'index', 'sources'].includes(scope) && !['deps', 'build', '.fingerprint'].includes(entry.name)) continue
    if (entry.isDirectory()) files.push(...filesIn(base, scope, child))
    else if (entry.isFile()) {
      const stat = lstatSync(join(base, child))
      files.push({ path: child, size: stat.size, mtime: stat.mtimeMs, mode: stat.mode & 0o777 })
    }
  }
  return files
}

// The byte budget includes per-file tar/manifest overhead, not only payload bytes.
function entryBytes(entry) { return entry.size + 4096 + Buffer.byteLength(entry.path) * 4 }

function removeSnapshot(context) {
  noLinks(context.root, context.cache)
  below(context.root, context.cache)
  rmSync(context.cache, { recursive: true, force: true })
}

export function snapshotCache(context, maxBytes = MAX_CACHE_BYTES) {
  if (!Number.isSafeInteger(maxBytes) || maxBytes < manifestReserve || maxBytes > MAX_CACHE_BYTES) throw new Error('Invalid cache budget')
  removeSnapshot(context)
  mkdirSync(context.cache, { recursive: true })
  const entries = []
  let bytes = manifestReserve
  let skipped = 0
  for (const [scope, base] of Object.entries(context.scopes)) {
    noLinks(base, base)
    const files = filesIn(base, scope).filter(entry => accepted(scope, entry.path))
      .sort((a, b) => b.mtime - a.mtime || a.path.localeCompare(b.path))
    for (const entry of files) {
      const cost = entryBytes(entry)
      if (bytes + cost > maxBytes) { skipped++; continue }
      const source = entryPath(base, entry.path)
      const destination = entryPath(join(context.cache, scope), entry.path)
      mkdirSync(dirname(destination), { recursive: true })
      copyFileSync(source, destination)
      chmodSync(destination, entry.mode)
      entries.push({ scope, path: entry.path, size: entry.size, mode: entry.mode, mtime: entry.mtime })
      bytes += cost
    }
  }
  if (!entries.length) throw new Error('Refusing to replace a cache with an empty snapshot')
  const manifest = { format: 1, target: context.target, rustc: context.rustc, entries, budgetBytes: bytes }
  writeFileSync(join(context.cache, 'manifest.json'), JSON.stringify(manifest))
  console.log(`Build cache snapshot: ${entries.length} files, budget ${bytes}/${maxBytes} bytes, ${skipped} files omitted`)
  return manifest
}

function checkedManifest(context) {
  const file = join(context.cache, 'manifest.json')
  noLinks(context.root, file)
  if (!existsSync(file)) return undefined
  if (lstatSync(file).size > 32 * 1024 * 1024) throw new Error('Cache manifest is too large')
  const manifest = JSON.parse(readFileSync(file, 'utf8'))
  if (manifest.format !== 1 || manifest.target !== context.target || !Array.isArray(manifest.entries) || !manifest.entries.length) {
    throw new Error('Cache snapshot identity does not match')
  }
  let bytes = manifestReserve
  const seen = new Set()
  for (const entry of manifest.entries) {
    const identity = `${entry.scope}/${entry.path}`
    if (!Object.hasOwn(context.scopes, entry.scope) || !accepted(entry.scope, entry.path)
      || !Number.isSafeInteger(entry.size) || entry.size < 0 || !Number.isFinite(entry.mtime)
      || !Number.isInteger(entry.mode) || entry.mode < 0 || entry.mode > 0o777
      || seen.has(identity)) throw new Error('Invalid cache manifest entry')
    seen.add(identity)
    const source = entryPath(join(context.cache, entry.scope), entry.path)
    const stat = lstatSync(source)
    if (!stat.isFile() || stat.size !== entry.size) throw new Error('Cache snapshot file is missing or incomplete')
    entryPath(context.scopes[entry.scope], entry.path)
    bytes += entryBytes(entry)
    if (bytes > MAX_CACHE_BYTES) throw new Error('Cache snapshot exceeds its byte budget')
  }
  if (manifest.budgetBytes !== bytes) throw new Error('Cache snapshot budget does not match')
  return manifest
}

export function hydrateCache(context) {
  const manifest = checkedManifest(context)
  if (!manifest) { console.log('No shared build cache; proceeding with a cold build'); return 0 }
  let restored = 0
  for (const entry of manifest.entries) {
    if (!['npm', 'crates', 'index', 'sources'].includes(entry.scope) && manifest.rustc !== context.rustc) continue
    const source = entryPath(join(context.cache, entry.scope), entry.path)
    const destination = entryPath(context.scopes[entry.scope], entry.path)
    mkdirSync(dirname(destination), { recursive: true })
    copyFileSync(source, destination)
    chmodSync(destination, entry.mode)
    utimesSync(destination, new Date(entry.mtime), new Date(entry.mtime))
    restored++
  }
  console.log(`Restored ${restored} shared cache files; Rust toolchain ${manifest.rustc === context.rustc ? 'matches' : 'changed, compiled objects skipped'}`)
  return restored
}

function legacyNodePrefix(target) {
  if (target.endsWith('windows-msvc')) return 'node-cache-Windows-x64-npm-'
  if (target.endsWith('linux-gnu')) return 'node-cache-Linux-x64-npm-'
  return `node-cache-macOS-${target.startsWith('aarch64') ? 'arm64' : 'x64'}-npm-`
}

export function managedCache(key, target) {
  cacheKey(target)
  return typeof key === 'string' && (key === cacheKey(target) || key.startsWith(legacyNodePrefix(target))
    || new RegExp(`^v[0-9]+-rust-${target}-build-`).test(key))
}

export class BuildCacheAPI {
  constructor(env = process.env, fetcher = fetch) {
    if (env.GITHUB_ACTIONS !== 'true' || env.GITHUB_REF !== 'refs/heads/main'
      || !['push', 'workflow_dispatch'].includes(env.GITHUB_EVENT_NAME)
      || env.GITHUB_REPOSITORY !== `${RELEASE_OWNER}/${RELEASE_REPO}` || !env.GITHUB_TOKEN?.trim()) {
      throw new Error('Only the trusted main workflow may replace build caches')
    }
    this.token = env.GITHUB_TOKEN
    this.fetcher = fetcher
  }

  async request(path, method = 'GET') {
    const response = await this.fetcher(`https://api.github.com/repos/${RELEASE_OWNER}/${RELEASE_REPO}/actions/caches${path}`, {
      method,
      headers: { Authorization: `Bearer ${this.token}`, Accept: 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28', 'User-Agent': 'OC2ModManager-build-cache' },
      signal: AbortSignal.timeout(30_000),
    })
    if (method === 'DELETE' && response.status === 404) return undefined
    if (!response.ok) throw new Error(`Build cache ${method} failed (HTTP ${response.status})`)
    return response.status === 204 ? undefined : response.json()
  }

  async list() {
    const entries = []
    for (let page = 1; page <= 100; page++) {
      const result = await this.request(`?per_page=100&page=${page}`)
      if (!Array.isArray(result.actions_caches) || result.actions_caches.length > 100) throw new Error('Invalid cache list response')
      entries.push(...result.actions_caches)
      if (result.actions_caches.length < 100) return entries
    }
    throw new Error('Too many cache pages; no caches were deleted')
  }

  async replace(target) {
    const entries = (await this.list()).filter(entry => managedCache(entry.key, target))
      .sort((a, b) => Number(a.key === cacheKey(target)) - Number(b.key === cacheKey(target)))
    if (entries.some(entry => !Number.isSafeInteger(entry.id) || entry.id <= 0)) throw new Error('Invalid cache ID; no caches were deleted')
    for (const entry of entries) await this.request(`/${entry.id}`, 'DELETE')
    console.log(`Removed ${entries.length} previous caches for ${target}; other targets and unrelated caches were preserved`)
    return entries.length
  }

  async verify(target, { attempts = 1, pause = () => new Promise(resolve => setTimeout(resolve, 2000)) } = {}) {
    for (let attempt = 1; attempt <= attempts; attempt++) {
      const entries = (await this.list()).filter(entry => managedCache(entry.key, target))
      if (entries.length === 1 && entries[0].key === cacheKey(target) && entries[0].ref === 'refs/heads/main'
        && Number.isSafeInteger(entries[0].size_in_bytes) && entries[0].size_in_bytes > 0 && entries[0].size_in_bytes <= MAX_CACHE_BYTES) {
        console.log(`Verified one main cache for ${target}: ${entries[0].size_in_bytes} bytes`)
        return
      }
      if (attempt < attempts) await pause()
    }
    throw new Error('Expected exactly one bounded main cache for this target')
  }
}

async function main() {
  const [command, target] = process.argv.slice(2)
  cacheKey(target)
  if (command === 'verify') { await new BuildCacheAPI().verify(target, { attempts: 3 }); return }
  const compiler = spawnSync('rustc', ['--version', '--verbose'], { encoding: 'utf8', windowsHide: true })
  if (compiler.status !== 0) throw new Error('Cannot identify the build-cache Rust toolchain')
  const context = cacheContext({ target, rustc: compiler.stdout.trim() })
  if (command === 'init') {
    appendFileSync(process.env.GITHUB_OUTPUT, `key=${cacheKey(target)}\npath=${CACHE_DIRECTORY}\n`)
    appendFileSync(process.env.GITHUB_ENV, `NPM_CONFIG_CACHE=${context.npmCache}\n`)
  } else if (command === 'restore') {
    try { hydrateCache(context) } catch {
      console.warn('Shared cache could not be restored; proceeding with a cold build')
    }
    // Do not let node --test discover third-party tests in the staging snapshot.
    removeSnapshot(context)
  } else if (command === 'snapshot') {
    snapshotCache(context)
  } else if (command === 'replace') {
    // Validate the complete new local snapshot before deleting any remote cache.
    if (!checkedManifest(context)) throw new Error('A complete new snapshot is required before replacement')
    await new BuildCacheAPI().replace(target)
  } else throw new Error('Unknown build-cache command')
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1 })
}
