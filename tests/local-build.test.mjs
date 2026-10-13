import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'
import { readProject } from '../scripts/release-lib.mjs'

const root = fileURLToPath(new URL('../', import.meta.url))
const documents = ['src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/tauri.conf.json', 'package.json', 'package-lock.json']
const versionBat = readFileSync(join(root, 'build with tag.bat'), 'utf8')
const updater = versionBat.replace(/\r\n/g, '\n').split('\n// VERSION_UPDATE_NODE\n')[1]

function workspace(t) {
  const base = resolve(root, 'src-tauri/target/verification/local-build-tests')
  const directory = join(base, randomUUID())
  for (const file of [...documents, 'build with tag.bat']) {
    mkdirSync(dirname(join(directory, file)), { recursive: true })
    copyFileSync(join(root, file), join(directory, file))
  }
  t.after(() => {
    assert.ok(resolve(directory).startsWith(`${base}${sep}`))
    rmSync(directory, { recursive: true, force: true })
  })
  return directory
}

function snapshot(directory) {
  return new Map(documents.map(file => [file, readFileSync(join(directory, file))]))
}

function updateVersion(directory, version, prefix = '') {
  assert.ok(updater)
  return spawnSync(process.execPath, ['-e', prefix + updater], {
    cwd: directory, encoding: 'utf8', windowsHide: true,
    env: { ...process.env, OC2_BUILD_VERSION: version, OC2_VERSION_BUILD_SCRIPT: join(directory, 'build with tag.bat') },
  })
}

test('both BAT entries share Actions signing, an explicit x64 target and an exact current-version installer', () => {
  const build = readFileSync(join(root, 'build.bat'), 'utf8')
  assert.ok(build.includes('node scripts/build-release.mjs build --target %BUILD_TARGET% --bundles nsis'))
  assert.ok(build.includes('set "BUILD_TARGET=x86_64-pc-windows-msvc"'))
  assert.ok(build.includes('%CARGO_TARGET_DIR%\\%BUILD_TARGET%\\release\\bundle\\nsis'))
  assert.ok(build.includes('OC2ModManager_%BUILD_VERSION%_x64-setup.exe'))
  assert.ok(build.includes('if not exist "%NSIS_INSTALLER%.sig"'))
  assert.ok(build.includes('TAURI_SIGNING_PRIVATE_KEY_PATH'))
  assert.ok(!build.includes('npx tauri build'))
  assert.ok(!build.includes('OC2ModManager_*_x64-setup.exe'))
  assert.ok(!build.includes('createUpdaterArtifacts'))
  assert.ok(versionBat.includes('call "%~dp0build.bat" %*'))
})

test('the actual BAT version updater synchronizes Cargo.lock and leaves dependency versions unchanged', t => {
  const directory = workspace(t)
  const before = snapshot(directory)
  const version = '9.8.7'
  const result = updateVersion(directory, ` ${version} `)
  assert.equal(result.status, 0, result.stderr)
  assert.equal(readProject(directory).version, version)
  const originalVersion = readProject(root).version
  const cargoLock = readFileSync(join(directory, 'src-tauri/Cargo.lock'), 'utf8')
  assert.equal(cargoLock, before.get('src-tauri/Cargo.lock').toString().replace(/(name = "app"\r?\nversion = ")[^"]+/, `$1${version}`))
  const cargo = readFileSync(join(directory, 'src-tauri/Cargo.toml'), 'utf8')
  assert.equal(cargo, before.get('src-tauri/Cargo.toml').toString().replace(`version = "${originalVersion}"`, `version = "${version}"`))
  for (const file of ['package.json', 'package-lock.json']) {
    const expected = JSON.parse(before.get(file))
    expected.version = version
    if (file === 'package-lock.json') expected.packages[''].version = version
    assert.deepEqual(JSON.parse(readFileSync(join(directory, file), 'utf8')), expected)
  }
})

test('invalid input and a malformed app lock entry fail before changing any project version', t => {
  const directory = workspace(t)
  const original = snapshot(directory)
  for (const version of ['v1.2.0', '01.2.0', '1.2.0-beta', '1.2.0 & echo invalid']) {
    assert.notEqual(updateVersion(directory, version).status, 0)
    assert.deepEqual(snapshot(directory), original)
  }
  const lockPath = join(directory, 'src-tauri/Cargo.lock')
  writeFileSync(lockPath, original.get('src-tauri/Cargo.lock').toString().replace('name = "app"', 'name = "not-app"'))
  const malformed = snapshot(directory)
  assert.notEqual(updateVersion(directory, '9.8.7').status, 0)
  assert.deepEqual(snapshot(directory), malformed)
})

test('a version file write failure restores every previously written project document', t => {
  const directory = workspace(t)
  const original = snapshot(directory)
  const fault = `
    const fsFault = require('node:fs');
    const originalWrite = fsFault.writeFileSync;
    let failed = false;
    fsFault.writeFileSync = function(file, ...args) {
      if (!failed && file === require('node:path').join(process.cwd(), 'package-lock.json')) {
        failed = true;
        throw new Error('Controlled version write failure');
      }
      return originalWrite.call(this, file, ...args);
    };
  `
  const result = updateVersion(directory, '9.8.7', fault)
  assert.notEqual(result.status, 0)
  assert.ok(result.stderr.includes('Controlled version write failure'))
  assert.deepEqual(snapshot(directory), original)
})

test('Windows executes the real version BAT and forwards noenv/clean to a harmless build stub', { skip: process.platform !== 'win32' }, t => {
  const directory = workspace(t)
  writeFileSync(join(directory, 'build.bat'), '@echo off\r\necho FORWARDED_ARGS=%*\r\nexit /b 0\r\n')
  for (const argument of ['noenv', 'clean']) {
    const result = spawnSync('cmd.exe', ['/d', '/c', `call "build with tag.bat" ${argument}`], {
      cwd: directory, input: '9.8.7\r\n', encoding: 'utf8', windowsHide: true,
      windowsVerbatimArguments: true, timeout: 15_000,
    })
    assert.equal(result.status, 0, `${result.error?.message || ''}\n${result.stderr}\n${result.stdout.slice(-2000)}`)
    assert.ok(result.stdout.includes(`FORWARDED_ARGS=${argument}`), result.stdout)
    assert.equal(readProject(directory).version, '9.8.7')
  }
})
