import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import { cleanPackageTest, collectPackageTest, preparePackageTest, verifyPackageTests } from '../scripts/ci-packages.mjs'
import { runTauri } from '../scripts/build-release.mjs'
import { BUILD_KEYS, readProject, verifyUpdateSignature } from '../scripts/release-lib.mjs'
import { signingFixture } from './fixtures/updater-signing.mjs'

const root = fileURLToPath(new URL('../', import.meta.url))
const commit = 'a'.repeat(40)
const projectVersion = readProject(root).version
const signer = signingFixture(projectVersion)
const publicKey = signer.encodedPublicKey
const signature = signer.signatureFor(Buffer.from('OC2 signed update fixture'))

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
  const env = { ...process.env, GITHUB_ACTIONS: 'true', OC2_CI_PACKAGE_TEST: '1', OC2_CI_SIGNING_MODE: 'temporary', GITHUB_ENV: join(directory, 'github-env') }
  delete env.TAURI_SIGNING_PRIVATE_KEY
  delete env.TAURI_SIGNING_PRIVATE_KEY_PATH
  delete env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD
  return env
}

test('package CI runs source regressions before signing preparation mutates updater configuration', () => {
  const workflow = readFileSync(join(root, '.github/workflows/build.yml'), 'utf8')
  const frontendTests = workflow.indexOf('- name: Frontend and release-script regression tests')
  const rustTests = workflow.indexOf('- name: Rust regression tests')
  const prepareSigning = workflow.indexOf('- name: Prepare and verify signing for non-release package tests')
  const buildPackages = workflow.indexOf('- name: Build and verify CI-only signed packages')
  assert.ok(frontendTests >= 0 && rustTests >= 0 && prepareSigning >= 0 && buildPackages >= 0)
  assert.ok(frontendTests < prepareSigning, 'Frontend regressions must inspect the checked-in configuration')
  assert.ok(rustTests < prepareSigning, 'Rust regressions must inspect the checked-in updater endpoints')
  assert.ok(prepareSigning < buildPackages, 'Package builds must still use isolated, verified test signing')
})

test('the shared CLI boundary maps key paths for build without passing a path as signer key contents', t => {
  const directory = workspace(t)
  const key = join(directory, 'temporary-updater.key')
  const env = { ...testEnvironment(directory), TAURI_SIGNING_PRIVATE_KEY_PATH: key }
  const original = { ...env }
  const calls = []
  const invoke = (_executable, args, options) => {
    calls.push({ args, env: options.env })
    return { status: 0 }
  }
  runTauri(['build', '--target', 'x86_64-pc-windows-msvc'], env, invoke)
  runTauri(['signer', 'sign', 'payload.bin'], env, invoke)
  assert.equal(calls[0].env.TAURI_SIGNING_PRIVATE_KEY, key)
  assert.equal(calls[1].env.TAURI_SIGNING_PRIVATE_KEY, undefined)
  assert.equal(calls[1].env.TAURI_SIGNING_PRIVATE_KEY_PATH, key)
  assert.deepEqual(env, original)

  const inline = { ...testEnvironment(directory), TAURI_SIGNING_PRIVATE_KEY: '[REDACTED_SECRET]' }
  runTauri(['build'], inline, invoke)
  runTauri(['signer', 'sign', 'payload.bin'], inline, invoke)
  assert.equal(calls[2].env.TAURI_SIGNING_PRIVATE_KEY, '[REDACTED_SECRET]')
  assert.equal(calls[3].env.TAURI_SIGNING_PRIVATE_KEY, '[REDACTED_SECRET]')
})

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

test('configured CI signing retains the checked-in public key, verifies it before build and never generates keys', t => {
  const directory = workspace(t)
  const path = join(directory, 'src-tauri/tauri.conf.json')
  const config = JSON.parse(readFileSync(path, 'utf8'))
  config.plugins.updater.pubkey = publicKey
  writeFileSync(path, JSON.stringify(config))
  const env = { ...testEnvironment(directory), OC2_CI_SIGNING_MODE: 'configured', TAURI_SIGNING_PRIVATE_KEY: '[REDACTED_SECRET]' }
  let calls = 0
  const sign = (_executable, args, options) => {
    calls++
    assert.deepEqual(args.slice(1, 3), ['signer', 'sign'])
    assert.deepEqual(args.slice(3, 5), ['--app-version', projectVersion])
    assert.equal(options.env.TAURI_SIGNING_PRIVATE_KEY, '[REDACTED_SECRET]')
    assert.equal(options.stdio, 'pipe')
    assert.equal(readFileSync(args.at(-1), 'utf8'), 'OC2 signed update fixture')
    writeFileSync(`${args.at(-1)}.sig`, signature)
    return { status: 0, stdout: '[REDACTED_SECRET]', stderr: '' }
  }
  assert.deepEqual(preparePackageTest(directory, env, sign), { publicKey })
  assert.equal(calls, 1)
  assert.equal(JSON.parse(readFileSync(path, 'utf8')).plugins.updater.pubkey, publicKey)
  const environmentFile = readFileSync(env.GITHUB_ENV, 'utf8')
  assert.ok(environmentFile.includes('OC2_CI_SIGNING_MODE=configured'))
  assert.ok(!environmentFile.includes('TAURI_SIGNING_PRIVATE_KEY'))
  assert.ok(!environmentFile.includes('[REDACTED_SECRET]'))
  assert.ok(!existsSync(join(directory, 'src-tauri/target/verification/package-test/temporary-updater.key')))
  assert.throws(() => preparePackageTest(directory, { ...env, TAURI_SIGNING_PRIVATE_KEY: '' }, sign))
  assert.throws(() => preparePackageTest(directory, env, () => ({ status: 1, stdout: '[REDACTED_SECRET]' })), /configured|Configured/)
  assert.throws(() => preparePackageTest(directory, env, (_executable, args) => {
    writeFileSync(`${args.at(-1)}.sig`, 'invalid signature')
    return { status: 0 }
  }))
  assert.throws(() => preparePackageTest(directory, env, (_executable, args) => {
    writeFileSync(`${args.at(-1)}.sig`, signer.signatureFor(readFileSync(args.at(-1)), '0.0.0'))
    return { status: 0 }
  }), /not bound to this release version/)
})

function packageFixture(t, signingMode = 'temporary') {
  const directory = workspace(t)
  const configPath = join(directory, 'src-tauri/tauri.conf.json')
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  config.plugins.updater.pubkey = publicKey
  writeFileSync(configPath, JSON.stringify(config))
  const downloads = join(directory, 'downloads')
  mkdirSync(downloads)
  const names = {
    'x86_64-pc-windows-msvc': [`nsis/OC2ModManager_${projectVersion}_x64-setup.exe`, `msi/OC2ModManager_${projectVersion}_x64_en-US.msi`],
    'x86_64-unknown-linux-gnu': [`appimage/OC2ModManager_${projectVersion}_amd64.AppImage`, `deb/OC2ModManager_${projectVersion}_amd64.deb`, `rpm/OC2ModManager-${projectVersion}-1.x86_64.rpm`],
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
      writeFileSync(join(bundle, 'dmg', `OC2ModManager_${projectVersion}_${target.startsWith('aarch64') ? 'aarch64' : 'x64'}.dmg`), 'DMG fixture')
    }
    collectPackageTest(target, directory, commit, signingMode)
    cpSync(join(directory, 'src-tauri/target/verification/package-test/artifacts'), join(downloads, target), { recursive: true })
  }
  return { directory, downloads }
}

test('package-test aggregation verifies four targets, both DMGs and all seven actual signatures', t => {
  const { directory, downloads } = packageFixture(t)
  const summary = verifyPackageTests(downloads, directory, commit, 'temporary')
  assert.equal(summary.testOnly, true)
  assert.equal(summary.buildTargets, 4)
  assert.equal(summary.updaterFormats, 7)
  assert.equal(summary.reports.reduce((count, report) => count + report.packages.length, 0), 9)
  assert.deepEqual(new Set(summary.reports.map(report => report.target)), new Set(Object.keys(BUILD_KEYS)))
})

test('configured-key package aggregation enforces the repository public key and generates all seven latest.json entries', t => {
  const { directory, downloads } = packageFixture(t, 'configured')
  const summary = verifyPackageTests(downloads, directory, commit, 'configured')
  assert.equal(summary.signingMode, 'configured')
  assert.equal(summary.publicKey, publicKey)
  assert.equal(summary.manifest.version, projectVersion)
  assert.equal(Object.keys(summary.manifest.platforms).length, 7)
  for (const report of summary.reports) {
    for (const asset of report.packages.filter(asset => asset.key)) {
      const entry = summary.manifest.platforms[asset.key]
      assert.equal(entry.signature, asset.signature)
      assert.ok(entry.url.endsWith(`/v${projectVersion}/${encodeURIComponent(asset.name)}`))
    }
  }
  assert.throws(() => verifyPackageTests(downloads, directory, commit, 'temporary'))
  const path = join(downloads, 'x86_64-pc-windows-msvc', 'package-test-report.json')
  const original = readFileSync(path, 'utf8')
  const report = JSON.parse(original)
  report.publicKey = 'wrong-key'
  writeFileSync(path, JSON.stringify(report))
  assert.throws(() => verifyPackageTests(downloads, directory, commit, 'configured'))
  writeFileSync(path, original)
  report.publicKey = publicKey
  report.signingMode = 'temporary'
  writeFileSync(path, JSON.stringify(report))
  assert.throws(() => verifyPackageTests(downloads, directory, commit, 'configured'))
})

test('package-test aggregation rejects corruption, different commits and unexpected key files', t => {
  const { directory, downloads } = packageFixture(t)
  assert.throws(() => verifyPackageTests(downloads, directory, 'b'.repeat(40), 'temporary'))
  const windows = join(downloads, 'x86_64-pc-windows-msvc')
  const installer = join(windows, `OC2ModManager_${projectVersion}_x64-setup.exe`)
  const original = readFileSync(installer)
  writeFileSync(installer, 'corrupted')
  assert.throws(() => verifyPackageTests(downloads, directory, commit, 'temporary'))
  writeFileSync(installer, original)
  const signaturePath = `${installer}.sig`
  writeFileSync(signaturePath, 'corrupted signature')
  assert.throws(() => verifyPackageTests(downloads, directory, commit, 'temporary'))
  writeFileSync(signaturePath, signature)
  writeFileSync(join(windows, 'temporary-updater.key'), '[REDACTED_SECRET]')
  assert.throws(() => verifyPackageTests(downloads, directory, commit, 'temporary'))
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
  runTauri(['signer', 'sign', '--app-version', version, payload], {
    ...env, TAURI_SIGNING_PRIVATE_KEY_PATH: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '',
  }, (executable, args, options) => spawnSync(executable, args, {
    ...options, stdio: 'pipe', encoding: 'utf8', windowsHide: true,
  }))
  verifyUpdateSignature(readFileSync(payload), readFileSync(`${payload}.sig`, 'utf8').trim(), generatedPublicKey, version)
  cleanPackageTest(directory, env)
})

test('the installed Tauri CLI verifies configured inline key contents against the existing public key', t => {
  const directory = workspace(t)
  const env = testEnvironment(directory)
  const cli = resolve(root, 'node_modules/@tauri-apps/cli/tauri.js')
  const invoke = (executable, args, options) => spawnSync(executable, [cli, ...args.slice(1)], options)
  const { publicKey: generatedPublicKey } = preparePackageTest(directory, env, invoke)
  const key = readFileSync(join(directory, 'src-tauri/target/verification/package-test/temporary-updater.key'), 'utf8')
  const configured = { ...env, OC2_CI_SIGNING_MODE: 'configured', TAURI_SIGNING_PRIVATE_KEY: key, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '' }
  assert.deepEqual(preparePackageTest(directory, configured, invoke), { publicKey: generatedPublicKey })
  assert.equal(readProject(directory).publicKey, generatedPublicKey)
  assert.ok(!existsSync(join(directory, 'src-tauri/target/verification/package-test/temporary-updater.key')))
  const configPath = join(directory, 'src-tauri/tauri.conf.json')
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  config.plugins.updater.pubkey = publicKey
  writeFileSync(configPath, JSON.stringify(config))
  assert.throws(() => preparePackageTest(directory, configured, invoke), /public key/)
})
