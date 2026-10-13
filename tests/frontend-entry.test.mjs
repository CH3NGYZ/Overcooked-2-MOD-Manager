import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

const root = fileURLToPath(new URL('../', import.meta.url))
const generator = join(root, 'scripts/frontend-entry.mjs')

function workspace(t) {
  const base = resolve(root, 'src-tauri/target/verification/frontend-entry-tests')
  const directory = join(base, randomUUID())
  mkdirSync(directory, { recursive: true })
  t.after(() => {
    assert.ok(resolve(directory).startsWith(`${base}${sep}`))
    rmSync(directory, { recursive: true, force: true })
  })
  return directory
}

function generate(directory) {
  const result = spawnSync(process.execPath, [generator], {
    cwd: directory, encoding: 'utf8', windowsHide: true,
  })
  assert.equal(result.status, 0, result.stderr)
}

test('a clean checkout generates the existing Vite entry without a tracked HTML file', t => {
  const directory = workspace(t)
  assert.equal(existsSync(join(directory, 'index.html')), false)
  generate(directory)
  const html = readFileSync(join(directory, 'index.html'), 'utf8')
  assert.ok(html.includes('<html lang="zh-CN">'))
  assert.ok(html.includes('<title>OC2 MOD \u7ba1\u7406\u5668</title>'))
  assert.ok(html.includes('<div id="app"></div>'))
  assert.ok(html.includes('type="module" src="/src/main.ts"'))
  assert.ok(html.includes('/src/assets/bb4518aa-dc55-4cae-a432-08ccdc5f28f8.png'))
})

test('entry generation is idempotent and repairs stale generated content', t => {
  const directory = workspace(t)
  generate(directory)
  const file = join(directory, 'index.html')
  const original = readFileSync(file)
  const timestamp = statSync(file).mtimeMs
  generate(directory)
  assert.deepEqual(readFileSync(file), original)
  assert.equal(statSync(file).mtimeMs, timestamp)
  writeFileSync(file, 'stale generated entry')
  generate(directory)
  assert.deepEqual(readFileSync(file), original)
})

test('development and production build lifecycle hooks both generate the entry', () => {
  const { scripts } = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  assert.equal(scripts.predev, 'node scripts/frontend-entry.mjs')
  assert.equal(scripts.prebuild, scripts.predev)
  const tauri = JSON.parse(readFileSync(join(root, 'src-tauri/tauri.conf.json'), 'utf8'))
  assert.ok(tauri.build.beforeDevCommand.startsWith('npm run dev'))
  assert.equal(tauri.build.beforeBuildCommand, 'npm run build')
})

test('generated HTML and local BAT/log files are ignored and absent from the Git index', () => {
  const files = ['index.html', 'build.bat', 'build.log', 'build with tag.bat']
  const ignored = spawnSync('git', ['check-ignore', '--no-index', '--', ...files], {
    cwd: root, encoding: 'utf8', windowsHide: true,
  })
  assert.equal(ignored.status, 0, ignored.stderr)
  assert.deepEqual(ignored.stdout.trim().split(/\r?\n/).sort(), [...files].sort())
  const tracked = spawnSync('git', ['ls-files', '--', ...files], {
    cwd: root, encoding: 'utf8', windowsHide: true,
  })
  assert.equal(tracked.status, 0, tracked.stderr)
  assert.equal(tracked.stdout, '')
})
