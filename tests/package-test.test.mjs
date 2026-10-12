import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import { cleanPackageTest, collectPackageTest, preparePackageTest, verifyPackageTests } from '../scripts/ci-packages.mjs'
import { BUILD_KEYS, readProject, verifyUpdateSignature } from '../scripts/release-lib.mjs'

const root = fileURLToPath(new URL('../', import.meta.url))
const commit = 'a'.repeat(40)
const rust = readFileSync(join(root, 'src-tauri/src/app_update.rs'), 'utf8')
const fixture = name => rust.match(new RegExp(`const ${name}: &str = "([^"]+)"`))[1]
const publicKey = fixture('TEST_KEY')
const signature = fixture('TEST_SIGNATURE')

function workspace(t) {
  const base = resolve(root, 'src-tauri/target/verification/package-test-unit')
  const directory = join(base, randomUUID())
  mkdirSync(directory, { recursive: true })
  for (const file of ['package.json', 'package-lock.json', 'src-tauri/tauri.conf.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock']) {
    mkdirSync(dirname(join(directory, file)), { recursive: true })
    copyFileSync(join(root, file), join(directory, file))
  }
  t.after(() => {
    assert.ok(resolve(directory).startsWith(`${base}${sep}`))
    rmSync(directory, { recursive: true, force: true })
  })
  return directory
}

function testEnvironment(directory) {
  const env = { ...process.env, GITHUB_ACTIONS: 'true', OC2_CI_PACKAGE_TEST: '1', GITHUB_ENV: join(directory, 'github-env') }
  delete env.TAURI_SIGNING_PRIVATE_KEY
  delete env.TAURI_SIGNING_PRIVATE_KEY_PATH
  return env
}

test('CI package-test signing refuses ordinary or production-key environments and captures key output', t => {
  const directory = workspace(t)
  const env = testEnvironment(directory)
  assert.throws(() => preparePackageTest(directory, { ...env, GITHUB_ACTIONS: '' }))
  assert.throws(() => preparePackageTest(directory, { ...env, OC2_CI_PACKAGE_TEST: '' }))
  assert.throws(() => preparePackageTest(directory, { ...env, TAURI_SIGNING_PRIVATE_KEY: '[REDACTED_SECRET]' }))
  const generate = (_executable, args) => {
    const path = args.at(-1)
    writeFileSync(path, '[REDACTED_SECRET]')
    writeFileSync(`${path}.pub`, publicKey)
    return { status: 0, stdout: '[REDACTED_SECRET]', stderr: '' }
  }
  assert.deepEqual(preparePackageTest(directory, env, generate), { publicKey })
  const config = JSON.parse(readFileSync(join(directory, 'src-tauri/tauri.conf.json'), 'utf8'))
  assert.equal(config.plugins.updater.pubkey, publicKey)
  assert.deepEqual(config.plugins.updater.endpoints, ['https://ci-package-test.invalid/latest.json'])
  const environmentFile = readFileSync(env.GITHUB_ENV, 'utf8')
  assert.ok(environmentFile.includes('temporary-updater.key'))
  assert.ok(!environmentFile.includes('[REDACTED_SECRET]'))
  cleanPackageTest(directory, env)
  assert.ok(!existsSync(join(directory, 'src-tauri/target/verification/package-test/temporary-updater.key')))
})

function packageFixture(t) {
  const directory = workspace(t)
  const configPath = join(directory, 'src-tauri/tauri.conf.json')
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  config.plugins.updater.pubkey = publicKey
  writeFileSync(configPath, JSON.stringify(config))
  const downloads = join(directory, 'downloads')
  mkdirSync(downloads)
  const names = {
    'x86_64-pc-windows-msvc': ['nsis/OC2ModManager_1.2.0_x64-setup.exe', 'msi/OC2ModManager_1.2.0_x64_en-US.msi'],
    'x86_64-unknown-linux-gnu': ['appimage/OC2ModManager_1.2.0_amd64.AppImage', 'deb/OC2ModManager_1.2.0_amd64.deb', 'rpm/OC2ModManager-1.2.0-1.x86_64.rpm'],
    'aarch64-apple-darwin': ['macos/OC2ModManager.app.tar.gz'],
    'x86_64-apple-darwin': ['macos/OC2ModManager.app.tar.gz'],
  }
  for (const [target, paths] of Object.entries(names)) {
    const bundle = join(directory, 'src-tauri/target', target, 'release/bundle')
    for (const path of paths) {
      mkdirSync(dirname(join(bundle, path)), { recursive: true })
      writeFileSync(join(bundle, path), 'OC2 signed update fixture')
      writeFileSync(join(bundle, `${path}.sig`), signature)
    }
    if (target.endsWith('apple-darwin')) {
      mkdirSync(join(bundle, 'dmg'))
      writeFileSync(join(bundle, 'dmg', `OC2ModManager_1.2.0_${target.startsWith('aarch64') ? 'aarch64' : 'x64'}.dmg`), 'DMG fixture')
    }
    collectPackageTest(target, directory, commit)
    cpSync(join(directory, 'src-tauri/target/verification/package-test/artifacts'), join(downloads, target), { recursive: true })
  }
  return { directory, downloads }
}

test('package-test aggregation verifies four targets, both DMGs and all seven actual signatures', t => {
  const { directory, downloads } = packageFixture(t)
  const summary = verifyPackageTests(downloads, directory, commit)
  assert.equal(summary.testOnly, true)
  assert.equal(summary.buildTargets, 4)
  assert.equal(summary.updaterFormats, 7)
  assert.equal(summary.reports.reduce((count, report) => count + report.packages.length, 0), 9)
  assert.deepEqual(new Set(summary.reports.map(report => report.target)), new Set(Object.keys(BUILD_KEYS)))
})

test('package-test aggregation rejects corruption, different commits and unexpected key files', t => {
  const { directory, downloads } = packageFixture(t)
  assert.throws(() => verifyPackageTests(downloads, directory, 'b'.repeat(40)))
  const windows = join(downloads, 'x86_64-pc-windows-msvc')
  const installer = join(windows, 'OC2ModManager_1.2.0_x64-setup.exe')
  const original = readFileSync(installer)
  writeFileSync(installer, 'corrupted')
  assert.throws(() => verifyPackageTests(downloads, directory, commit))
  writeFileSync(installer, original)
  const signaturePath = `${installer}.sig`
  writeFileSync(signaturePath, 'corrupted signature')
  assert.throws(() => verifyPackageTests(downloads, directory, commit))
  writeFileSync(signaturePath, signature)
  writeFileSync(join(windows, 'temporary-updater.key'), '[REDACTED_SECRET]')
  assert.throws(() => verifyPackageTests(downloads, directory, commit))
})

test('the installed Tauri CLI can generate a temporary key and sign a version-bound payload without logging keys', t => {
  const directory = workspace(t)
  const env = testEnvironment(directory)
  const cli = resolve(root, 'node_modules/@tauri-apps/cli/tauri.js')
  const invoke = (executable, args, options) => spawnSync(executable, [cli, ...args.slice(1)], options)
  const { publicKey: generatedPublicKey } = preparePackageTest(directory, env, invoke)
  const key = join(directory, 'src-tauri/target/verification/package-test/temporary-updater.key')
  const payload = join(directory, 'payload.bin')
  const { version } = readProject(directory)
  writeFileSync(payload, 'Real Tauri signer smoke test')
  const result = spawnSync(process.execPath, [cli, 'signer', 'sign', '--app-version', version, payload], {
    encoding: 'utf8', windowsHide: true, env: { ...env, TAURI_SIGNING_PRIVATE_KEY_PATH: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' },
  })
  assert.equal(result.status, 0, 'The real CLI signer must accept the generated private-key path')
  verifyUpdateSignature(readFileSync(payload), readFileSync(`${payload}.sig`, 'utf8').trim(), generatedPublicKey, version)
  cleanPackageTest(directory, env)
})
