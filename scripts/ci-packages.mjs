import { spawnSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { appendFileSync, chmodSync, copyFileSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs'
import { basename, join, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { updateFiles } from './build-release.mjs'
import { BUILD_KEYS, PLATFORM_KEYS, collectUpdateAssets, readProject, verifyUpdateSignature } from './release-lib.mjs'

const workDirectory = 'src-tauri/target/verification/package-test'
const reportName = 'package-test-report.json'

function checkedWorkspacePath(root, relative) {
  const workspace = resolve(root)
  const path = resolve(workspace, relative)
  if (!path.startsWith(`${workspace}${sep}`)) throw new Error('Package-test files must remain inside the workspace')
  return path
}

function requireTestEnvironment(env) {
  if (env.GITHUB_ACTIONS !== 'true' || env.OC2_CI_PACKAGE_TEST !== '1') {
    throw new Error('Temporary signing is restricted to the explicit GitHub Actions package-test mode')
  }
}

export function preparePackageTest(root = process.cwd(), env = process.env, invoke = spawnSync) {
  requireTestEnvironment(env)
  if (env.TAURI_SIGNING_PRIVATE_KEY?.trim() || env.TAURI_SIGNING_PRIVATE_KEY_PATH?.trim()) {
    throw new Error('Package tests must not receive a production signing key')
  }
  if (!env.GITHUB_ENV) throw new Error('The GitHub Actions environment file is required')
  readProject(root)
  const directory = checkedWorkspacePath(root, workDirectory)
  rmSync(directory, { recursive: true, force: true })
  mkdirSync(directory, { recursive: true })
  const privateKey = join(directory, 'temporary-updater.key')
  const result = invoke(process.execPath, [
    resolve(root, 'node_modules/@tauri-apps/cli/tauri.js'),
    'signer', 'generate', '--ci', '-p', '', '-w', privateKey,
  ], { encoding: 'utf8', env, windowsHide: true })
  // Key generation prints the private key; capture all output, never forward it.
  if (result.error || result.status !== 0) throw new Error('Temporary CI updater-key generation failed')
  chmodSync(privateKey, 0o600)
  const publicKey = readFileSync(`${privateKey}.pub`, 'utf8').trim()
  const configPath = join(root, 'src-tauri/tauri.conf.json')
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  config.plugins.updater.pubkey = publicKey
  config.plugins.updater.endpoints = ['https://ci-package-test.invalid/latest.json']
  writeFileSync(configPath, `${JSON.stringify(config, null, 2)}\n`)
  if (/[\r\n]/.test(privateKey)) throw new Error('Invalid temporary key path')
  appendFileSync(env.GITHUB_ENV, `TAURI_SIGNING_PRIVATE_KEY_PATH=${privateKey}\nTAURI_SIGNING_PRIVATE_KEY_PASSWORD=\n`)
  return { publicKey }
}

function checkedCommit(value) {
  if (!/^[a-f0-9]{40}$/.test(value || '')) throw new Error('The tested Git commit is required')
  return value
}

function checkedFileName(name) {
  if (typeof name !== 'string' || !name || name !== basename(name) || /[/\\]/.test(name)) {
    throw new Error('Invalid package-test artifact name')
  }
  return name
}

function macInstaller(target, version) {
  return target.endsWith('apple-darwin')
    ? `OC2ModManager_${version}_${target.startsWith('aarch64') ? 'aarch64' : 'x64'}.dmg`
    : undefined
}

function fileRecord(path, name) {
  const bytes = readFileSync(path)
  if (!bytes.length) throw new Error(`Empty package-test artifact: ${name}`)
  return { name, size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') }
}

export function collectPackageTest(target, root = process.cwd(), commit = process.env.GITHUB_SHA) {
  const { version, publicKey } = readProject(root)
  checkedCommit(commit)
  if (!BUILD_KEYS[target]) throw new Error('Unsupported package-test build target')
  const targetRoot = resolve(root, process.env.CARGO_TARGET_DIR || 'src-tauri/target')
  const bundle = join(targetRoot, target, 'release/bundle')
  const staging = checkedWorkspacePath(root, `${workDirectory}/artifacts`)
  rmSync(staging, { recursive: true, force: true })
  mkdirSync(staging, { recursive: true })
  const packages = []
  for (const [key, asset] of updateFiles(bundle, target, version)) {
    const signature = readFileSync(`${asset.path}.sig`, 'utf8').trim()
    verifyUpdateSignature(readFileSync(asset.path), signature, publicKey, version)
    copyFileSync(asset.path, join(staging, asset.name))
    copyFileSync(`${asset.path}.sig`, join(staging, `${asset.name}.sig`))
    packages.push({ ...fileRecord(asset.path, asset.name), key, signature })
  }
  const dmgName = macInstaller(target, version)
  if (dmgName) {
    const path = join(bundle, 'dmg', dmgName)
    const record = fileRecord(path, dmgName)
    copyFileSync(path, join(staging, dmgName))
    packages.push(record)
  }
  const report = { schemaVersion: 1, testOnly: true, version, commit, target, publicKey, packages }
  writeFileSync(join(staging, reportName), `${JSON.stringify(report, null, 2)}\n`)
  return report
}

export function verifyPackageTests(downloads, root = process.cwd(), commit = process.env.GITHUB_SHA) {
  const { version } = readProject(root)
  checkedCommit(commit)
  const reports = []
  const seenTargets = new Set()
  const seenKeys = new Set()
  for (const directory of readdirSync(downloads, { withFileTypes: true }).filter(entry => entry.isDirectory())) {
    const path = join(downloads, directory.name)
    const report = JSON.parse(readFileSync(join(path, reportName), 'utf8'))
    if (report.schemaVersion !== 1 || report.testOnly !== true || report.version !== version
      || report.commit !== commit || !BUILD_KEYS[report.target] || seenTargets.has(report.target)) {
      throw new Error('Package-test reports must cover the same version/commit and four distinct targets')
    }
    seenTargets.add(report.target)
    const updates = collectUpdateAssets(report.packages, version, BUILD_KEYS[report.target])
    const expectedDmg = macInstaller(report.target, version)
    if (report.packages.length !== updates.size + Number(Boolean(expectedDmg))
      || (expectedDmg && !report.packages.some(asset => asset.name === expectedDmg && !asset.key))) {
      throw new Error('The installer and updater package counts are inconsistent')
    }
    const expectedFiles = new Set([reportName])
    for (const asset of report.packages) {
      checkedFileName(asset.name)
      const record = fileRecord(join(path, asset.name), asset.name)
      if (record.size !== asset.size || record.sha256 !== asset.sha256) throw new Error(`Corrupted package-test artifact: ${asset.name}`)
      expectedFiles.add(asset.name)
      if (asset.key) {
        if (updates.get(asset.key) !== asset || seenKeys.has(asset.key)) throw new Error('Duplicate or mismatched updater target')
        const signatureName = `${asset.name}.sig`
        const signature = readFileSync(join(path, signatureName), 'utf8').trim()
        if (signature !== asset.signature) throw new Error('Package-test signatures differ from the report')
        verifyUpdateSignature(readFileSync(join(path, asset.name)), signature, report.publicKey, version)
        expectedFiles.add(signatureName)
        seenKeys.add(asset.key)
      }
    }
    const files = readdirSync(path)
    if (files.length !== expectedFiles.size || files.some(name => !expectedFiles.has(name))) {
      throw new Error('Unexpected package-test artifact; private keys must never be uploaded')
    }
    reports.push(report)
  }
  if (seenTargets.size !== Object.keys(BUILD_KEYS).length || seenKeys.size !== PLATFORM_KEYS.length) {
    throw new Error('Four build targets and all seven signed updater formats are required')
  }
  return { testOnly: true, version, commit, buildTargets: seenTargets.size, updaterFormats: seenKeys.size, reports }
}

export function cleanPackageTest(root = process.cwd(), env = process.env) {
  requireTestEnvironment(env)
  const directory = checkedWorkspacePath(root, workDirectory)
  for (const file of ['temporary-updater.key', 'temporary-updater.key.pub']) {
    rmSync(join(directory, file), { force: true })
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const [command, value] = process.argv.slice(2)
    if (command === 'prepare') {
      preparePackageTest()
      console.log('Prepared a temporary CI-only updater key; production endpoints are disabled')
    } else if (command === 'collect') {
      const report = collectPackageTest(value)
      console.log(`Verified ${report.packages.length} package-test files for ${report.target}`)
    } else if (command === 'verify') {
      const summary = verifyPackageTests(resolve(value))
      const directory = checkedWorkspacePath(process.cwd(), `${workDirectory}/summary`)
      mkdirSync(directory, { recursive: true })
      writeFileSync(join(directory, 'summary.json'), `${JSON.stringify(summary, null, 2)}\n`)
      console.log(`Verified ${summary.buildTargets} builds and ${summary.updaterFormats} signed updater formats`)
    } else if (command === 'clean') {
      cleanPackageTest()
    } else throw new Error('Expected prepare, collect, verify or clean')
  } catch (error) { console.error(error.message); process.exitCode = 1 }
}
