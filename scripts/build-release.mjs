import { spawnSync } from 'node:child_process'
import { readFileSync, readdirSync, statSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { BUILD_KEYS, collectUpdateAssets, readProject, verifyUpdateSignature } from './release-lib.mjs'

export function buildUpdateKeys(args, target) {
  const keys = BUILD_KEYS[target]
  if (!keys) throw new Error(`Unsupported release build target: ${target}`)
  const index = args.indexOf('--bundles')
  if (index < 0) return keys
  const values = args.slice(index + 1)
  const end = values.findIndex(value => value.startsWith('-'))
  const bundles = values.slice(0, end < 0 ? undefined : end).flatMap(value => value.split(','))
  const supported = [...keys.map(key => key.split('-').at(-1)), ...(target.endsWith('apple-darwin') ? ['dmg'] : []), 'all']
  if (!bundles.length || bundles.some(bundle => !supported.includes(bundle))) {
    throw new Error(`Unsupported updater bundle selection for ${target}`)
  }
  const selected = keys.filter(key => bundles.includes('all') || bundles.includes(key.split('-').at(-1)))
  if (!selected.length) throw new Error('A signed release build must include an updater bundle')
  return selected
}

export function updateFiles(bundleRoot, target, version, requiredKeys = BUILD_KEYS[target]) {
  const keys = BUILD_KEYS[target]
  if (!keys || !requiredKeys?.length || requiredKeys.some(key => !keys.includes(key))) {
    throw new Error(`Unsupported release build target or updater formats: ${target}`)
  }
  const assets = []
  for (const directory of ['nsis', 'msi', 'appimage', 'deb', 'rpm', 'macos']) {
    let entries
    try { entries = readdirSync(join(bundleRoot, directory), { withFileTypes: true }) }
    catch (error) { if (error.code === 'ENOENT') continue; throw error }
    for (const entry of entries.filter(entry => entry.isFile())) {
      const path = join(bundleRoot, directory, entry.name)
      const name = target.endsWith('apple-darwin') && entry.name === 'OC2ModManager.app.tar.gz'
        ? `OC2ModManager_${target.startsWith('aarch64') ? 'aarch64' : 'x64'}.app.tar.gz` : entry.name
      assets.push({ path, name, size: statSync(path).size })
    }
  }
  return collectUpdateAssets(assets, version, requiredKeys)
}

export function runTauri(parameters, env = process.env, invoke = spawnSync) {
  const cli = resolve('node_modules/@tauri-apps/cli/tauri.js')
  const commandEnv = { ...env }
  // build accepts a key path in KEY; signer sign reserves KEY for inline contents.
  if (parameters[0] === 'build' && !env.TAURI_SIGNING_PRIVATE_KEY?.trim()
    && env.TAURI_SIGNING_PRIVATE_KEY_PATH?.trim()) {
    commandEnv.TAURI_SIGNING_PRIVATE_KEY = env.TAURI_SIGNING_PRIVATE_KEY_PATH
  }
  const result = invoke(process.execPath, [cli, ...parameters], { stdio: 'inherit', env: commandEnv })
  if (result.error || result.status !== 0) throw new Error('Tauri release build/signing failed')
}

export function main(args = process.argv.slice(2)) {
  const { version, publicKey } = readProject()
  if (!process.env.TAURI_SIGNING_PRIVATE_KEY?.trim() && !process.env.TAURI_SIGNING_PRIVATE_KEY_PATH?.trim()) {
    throw new Error('An updater signing key must be configured before a release build')
  }
  const targetIndex = args.indexOf('--target')
  const target = args[targetIndex + 1]
  if (args[0] !== 'build' || targetIndex < 0 || !BUILD_KEYS[target]
    || args.includes('--debug') || args.includes('--profile') || args.includes('--no-bundle')) {
    throw new Error('Release builds require build --target with a supported release target')
  }
  const requiredKeys = buildUpdateKeys(args, target)
  runTauri(args)
  const targetRoot = resolve(process.env.CARGO_TARGET_DIR || 'src-tauri/target')
  const artifacts = updateFiles(join(targetRoot, target, 'release/bundle'), target, version, requiredKeys)
  for (const [key, asset] of artifacts) {
    // DEB/RPM are supported by the updater, but not all CLI builds sign them.
    runTauri(['signer', 'sign', '--app-version', version, asset.path])
    const signature = readFileSync(`${asset.path}.sig`, 'utf8').trim()
    verifyUpdateSignature(readFileSync(asset.path), signature, publicKey, version)
    console.log(`Verified signed release target: ${key} (${basename(asset.path)})`)
  }
  if (target.endsWith('apple-darwin') && (args.indexOf('--bundles') < 0
    || args.slice(args.indexOf('--bundles') + 1).some(value => value.split(',').some(bundle => ['all', 'dmg'].includes(bundle))))) {
    const arch = target.startsWith('aarch64') ? 'aarch64' : 'x64'
    const dmg = join(targetRoot, target, `release/bundle/dmg/OC2ModManager_${version}_${arch}.dmg`)
    if (statSync(dmg).size <= 0) throw new Error('The macOS installer DMG is missing or empty')
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main() } catch (error) { console.error(error.message); process.exitCode = 1 }
}
