import assert from 'node:assert/strict'
import test from 'node:test'
import { runTauri } from '../scripts/build-release.mjs'

const sign = ['signer', 'sign', '--app-version', '1.2.3', 'fixture.exe']
const keyPath = 'ci fixture/temporary-updater.key'

function childEnvironment(parameters, env) {
  let childEnv
  runTauri(parameters, env, (_command, args, options) => {
    assert.deepEqual(args.slice(1), parameters)
    childEnv = options.env
    return { status: 0 }
  })
  assert.ok(childEnv)
  return childEnv
}

test('temporary CI key path masks the empty inline option only for signer sign', () => {
  const env = Object.freeze({
    TAURI_SIGNING_PRIVATE_KEY: '',
    TAURI_SIGNING_PRIVATE_KEY_PATH: keyPath,
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '',
  })
  const build = childEnvironment(['build', '--target', 'x86_64-pc-windows-msvc'], env)
  assert.deepEqual(build, { ...env, TAURI_SIGNING_PRIVATE_KEY: keyPath })

  const signer = childEnvironment(sign, env)
  assert.deepEqual(signer, {
    TAURI_SIGNING_PRIVATE_KEY_PATH: keyPath,
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '',
  })
  assert.equal(env.TAURI_SIGNING_PRIVATE_KEY, '')
  assert.deepEqual(childEnvironment(['signer', 'generate'], env), env)
  const withoutPath = { TAURI_SIGNING_PRIVATE_KEY: '' }
  assert.deepEqual(childEnvironment(sign, withoutPath), withoutPath)
})

test('configured inline signing preserves its environment and does not silently pick between two keys', () => {
  const env = {
    TAURI_SIGNING_PRIVATE_KEY: 'inline-fixture-placeholder',
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: 'fixture-password',
  }
  assert.deepEqual(childEnvironment(sign, env), env)
  assert.deepEqual(childEnvironment(['build'], env), env)

  const ambiguous = { ...env, TAURI_SIGNING_PRIVATE_KEY_PATH: keyPath }
  assert.deepEqual(childEnvironment(sign, ambiguous), ambiguous)
})
