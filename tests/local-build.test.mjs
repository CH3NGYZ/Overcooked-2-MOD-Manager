import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { randomUUID } from 'node:crypto'
import { copyFileSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { dirname, join, resolve, sep } from 'node:path'
import { fileURLToPath, pathToFileURL } from 'node:url'
import test from 'node:test'
import { readProject } from '../scripts/release-lib.mjs'

const root = fileURLToPath(new URL('../', import.meta.url))
const documents = ['src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/tauri.conf.json', 'package.json', 'package-lock.json']
const versionBat = readFileSync(join(root, 'build with tag.bat'), 'utf8')
const updater = versionBat.replace(/\r\n/g, '\n').split('\n// VERSION_UPDATE_NODE\n')[1]

function workspace(t, suffix = '') {
  const base = resolve(root, 'src-tauri/target/verification/local-build-tests')
  const directory = join(base, randomUUID() + suffix)
  for (const file of [...documents, 'build.bat', 'build with tag.bat']) {
    mkdirSync(dirname(join(directory, file)), { recursive: true })
    copyFileSync(join(root, file), join(directory, file))
  }
  t.after(() => {
    assert.ok(resolve(directory).startsWith(`${base}${sep}`))
    rmSync(directory, { recursive: true, force: true })
  })
  return directory
}

function runBuildBat(t, mode, codePage, exitCode = 23) {
  const directory = workspace(t, ' build \u6d4b\u8bd5')
  const rust = join(directory, 'rust')
  mkdirSync(join(rust, '.cargo/bin'), { recursive: true })
  writeFileSync(join(rust, '.cargo/bin/cargo.exe'), '')
  const buildPath = join(directory, 'build.bat')
  writeFileSync(buildPath, readFileSync(buildPath, 'utf8').replaceAll('D:\\ProgramData\\rust', '%OC2_TEST_RUST_ROOT%'))
  mkdirSync(join(directory, 'scripts'), { recursive: true })
  const releaseBuildUrl = pathToFileURL(join(root, 'scripts/build-release.mjs')).href
  // Stop at the real bundle selector, before any compiler, signer or installer runs.
  writeFileSync(join(directory, 'scripts/build-release.mjs'), `
    import { writeFileSync } from 'node:fs';
    import { buildUpdateKeys } from ${JSON.stringify(releaseBuildUrl)};
    const args = process.argv.slice(2);
    writeFileSync('build-invocation.json', JSON.stringify({
      args,
      githubToken: Boolean(process.env.OC2_INSTALLER_GITHUB_TOKEN),
      giteeToken: Boolean(process.env.OC2_INSTALLER_GITEE_TOKEN),
      signingKey: Boolean(process.env.TAURI_SIGNING_PRIVATE_KEY),
    }));
    try {
      console.log('BUILD_KEYS=' + buildUpdateKeys(args, 'x86_64-pc-windows-msvc').join(','));
      console.log('BUILD_STDOUT_PROBE_\\u4e2d\\u6587');
      console.error('BUILD_STDERR_PROBE_\\u4e2d\\u6587');
      process.exitCode = ${exitCode};
    } catch (error) {
      console.error(error.message);
      process.exitCode = 23;
    }
  `)
  const tmp = join(directory, 'tmp')
  mkdirSync(tmp, { recursive: true })
  const result = spawnSync('cmd.exe', ['/d', '/c', `chcp ${codePage} >nul & call build.bat ${mode}`], {
    cwd: directory, input: '\r\n', encoding: 'utf8', windowsHide: true,
    windowsVerbatimArguments: true, timeout: 15_000,
    env: {
      ...process.env,
      OC2_TEST_RUST_ROOT: rust,
      CARGO_TARGET_DIR: join(directory, 'target'),
      OC2_INSTALLER_GITHUB_TOKEN: '[REDACTED_SECRET]',
      OC2_INSTALLER_GITEE_TOKEN: '[REDACTED_SECRET]',
      TAURI_SIGNING_PRIVATE_KEY: '[REDACTED_SECRET]',
      TAURI_SIGNING_PRIVATE_KEY_PATH: join(directory, 'unused-signing.key'),
      TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '[REDACTED_SECRET]',
      TEMP: tmp, TMP: tmp, TMPDIR: tmp,
    },
  })
  return { directory, result }
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

test('the build BAT uses ASCII commands and CRLF without a BOM', () => {
  const bytes = readFileSync(join(root, 'build.bat'))
  assert.ok(bytes.every(value => value < 128), 'Non-ASCII batch comments can be misparsed after CHCP changes')
  assert.ok(!bytes.toString().replace(/\r\n/g, '').includes('\n'), 'Batch commands must use CRLF')
})

test('Git checkout keeps both BAT entries CRLF without platform automatic conversion', t => {
  const directory = workspace(t)
  const checkout = join(directory, 'checkout')
  mkdirSync(checkout)
  const result = spawnSync('git', ['-c', 'core.autocrlf=false', 'checkout-index',
    `--prefix=${checkout.replaceAll('\\', '/')}/`, '--', 'build.bat', 'build with tag.bat'], {
    cwd: root, encoding: 'utf8', windowsHide: true,
  })
  assert.equal(result.status, 0, result.error?.message || result.stderr)
  for (const file of ['build.bat', 'build with tag.bat']) {
    const bytes = readFileSync(join(checkout, file))
    assert.ok(!bytes.toString().replace(/\r\n/g, '').includes('\n'), `${file} must be checked out as CRLF on every platform`)
    assert.ok(!bytes.subarray(0, 3).equals(Buffer.from([0xef, 0xbb, 0xbf])), `${file} must not contain a BOM`)
  }
})

for (const codePage of [936, 65001]) {
  for (const mode of ['', 'noenv', 'clean']) {
    test(`Windows build BAT preserves NSIS arguments and both output streams (${codePage}, ${mode || 'default'})`, {
      skip: process.platform !== 'win32',
    }, t => {
      const { directory, result } = runBuildBat(t, mode, codePage)
      const details = `${result.error?.message || ''}\n${result.stderr}\n${result.stdout}`
      assert.equal(result.status, 1, details)
      const invocation = JSON.parse(readFileSync(join(directory, 'build-invocation.json'), 'utf8'))
      assert.deepEqual(invocation.args, ['build', '--target', 'x86_64-pc-windows-msvc', '--bundles', 'nsis'], details)
      assert.equal(invocation.githubToken, !mode, details)
      assert.equal(invocation.giteeToken, !mode, details)
      assert.equal(invocation.signingKey, true, details)
      assert.equal(result.stderr, '', details)
      const log = readFileSync(join(directory, 'build.log'), 'utf8')
      for (const output of [result.stdout, log]) {
        assert.ok(output.includes('BUILD_KEYS=windows-x86_64-nsis'), output)
        assert.ok(output.includes('BUILD_STDOUT_PROBE_\u4e2d\u6587'), output)
        assert.ok(output.includes('BUILD_STDERR_PROBE_\u4e2d\u6587'), output)
        assert.ok(output.includes('BUILD FAILED.'), output)
        assert.ok(!output.includes('Installing NSIS package'), output)
        assert.ok(!output.includes('[REDACTED_SECRET]'), output)
      }
      assert.ok(!log.includes('\0'), 'Build logs must not mix UTF-8 and UTF-16')
    })
  }
}

test('Windows build BAT preserves a successful build exit and refuses a missing current-version installer', {
  skip: process.platform !== 'win32',
}, t => {
  const { directory, result } = runBuildBat(t, '', 936, 0)
  const details = `${result.error?.message || ''}\n${result.stderr}\n${result.stdout}`
  assert.equal(result.status, 1, details)
  assert.equal(result.stderr, '', details)
  for (const output of [result.stdout, readFileSync(join(directory, 'build.log'), 'utf8')]) {
    assert.ok(output.includes('BUILD SUCCEEDED.'), output)
    assert.ok(output.includes('the NSIS installer for the current version was not found'), output)
    assert.ok(!output.includes('BUILD FAILED.'), output)
    assert.ok(!output.includes('Installing NSIS package'), output)
  }
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
