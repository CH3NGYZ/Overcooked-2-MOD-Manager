import { createHash, createPublicKey, verify } from 'node:crypto'
import { readFileSync } from 'node:fs'
import { join } from 'node:path'

export const RELEASE_OWNER = 'CH3NGYZ'
export const RELEASE_REPO = 'Overcooked-2-MOD-Manager-Releases'
export const BUILD_KEYS = {
  'x86_64-pc-windows-msvc': ['windows-x86_64-nsis', 'windows-x86_64-msi'],
  'x86_64-unknown-linux-gnu': ['linux-x86_64-appimage', 'linux-x86_64-deb', 'linux-x86_64-rpm'],
  'aarch64-apple-darwin': ['darwin-aarch64-app'],
  'x86_64-apple-darwin': ['darwin-x86_64-app'],
}
export const PLATFORM_KEYS = Object.values(BUILD_KEYS).flat()

function checkedVersion(value) {
  if (typeof value !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(value)) {
    throw new Error('The release version must be three numeric components, without a v prefix')
  }
  return value
}

export function validateProjectVersions({ packageJson, packageLock, tauri, cargo, cargoLock }) {
  const version = checkedVersion(packageJson.version)
  const packageSection = cargo.match(/^\[package\]\s*\r?\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1]
  const cargoVersion = packageSection?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1]
  const appPackages = cargoLock.split(/^\[\[package\]\]\s*$/m)
    .filter(section => /^name\s*=\s*"app"\s*$/m.test(section))
  const lockVersion = appPackages.length === 1
    ? appPackages[0].match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1] : undefined
  for (const [file, actual] of [
    ['package-lock.json', packageLock.version],
    ['package-lock.json root package', packageLock.packages?.['']?.version],
    ['tauri.conf.json', tauri.version],
    ['Cargo.toml', cargoVersion],
    ['Cargo.lock app package', lockVersion],
  ]) {
    if (actual !== version) throw new Error(`${file} version does not match package.json (${version})`)
  }
  if (tauri.productName !== 'OC2ModManager' || tauri.bundle?.createUpdaterArtifacts !== true
    || tauri.plugins?.updater?.requireSignedVersion !== true || !tauri.plugins.updater.pubkey) {
    throw new Error('Updater artifacts, the public key and signed versions must be enabled')
  }
  return version
}

export function readProject(root = process.cwd()) {
  const json = path => JSON.parse(readFileSync(join(root, path), 'utf8'))
  const tauri = json('src-tauri/tauri.conf.json')
  const version = validateProjectVersions({
    packageJson: json('package.json'),
    packageLock: json('package-lock.json'),
    tauri,
    cargo: readFileSync(join(root, 'src-tauri/Cargo.toml'), 'utf8'),
    cargoLock: readFileSync(join(root, 'src-tauri/Cargo.lock'), 'utf8'),
  })
  return { version, publicKey: tauri.plugins.updater.pubkey }
}

export function releaseTag(version, refType, refName) {
  const tag = `v${checkedVersion(version)}`
  if (refType === 'tag' && refName !== tag) throw new Error(`The pushed tag must be ${tag}`)
  return tag
}

export function updateAssetKey(name, version) {
  checkedVersion(version)
  if (typeof name !== 'string' || /[/\\]/.test(name)) return undefined
  const lower = name.toLowerCase()
  const prefix = `oc2modmanager_${version}_`
  const x64 = ['x64', 'x86_64', 'amd64']
  if (x64.some(arch => lower === `${prefix}${arch}-setup.exe`)) return 'windows-x86_64-nsis'
  if (x64.some(arch => {
    const msiPrefix = `${prefix}${arch}_`
    return lower.startsWith(msiPrefix) && lower.endsWith('.msi')
      && /^[a-z]{2}-[a-z]{2}$/.test(lower.slice(msiPrefix.length, -4))
  })) return 'windows-x86_64-msi'
  for (const kind of ['appimage', 'deb']) {
    if (x64.some(arch => lower === `${prefix}${arch}.${kind}`)) return `linux-x86_64-${kind}`
  }
  const rpmPrefix = `oc2modmanager-${version}-`
  const rpmSuffix = '.x86_64.rpm'
  if (lower.startsWith(rpmPrefix) && lower.endsWith(rpmSuffix)
    && /^[a-z0-9._+~]+$/.test(lower.slice(rpmPrefix.length, -rpmSuffix.length))) {
    return 'linux-x86_64-rpm'
  }
  for (const [arch, aliases] of [['x86_64', x64], ['aarch64', ['aarch64', 'arm64']]]) {
    if (aliases.some(alias => lower === `oc2modmanager_${alias}.app.tar.gz`
      || lower === `${prefix}${alias}.app.tar.gz`)) return `darwin-${arch}-app`
  }
  return undefined
}

export function collectUpdateAssets(assets, version, requiredKeys = PLATFORM_KEYS) {
  const selected = new Map()
  for (const asset of assets) {
    const key = updateAssetKey(asset.name, version)
    if (!requiredKeys.includes(key)) continue
    if (selected.has(key)) throw new Error(`Multiple update packages found for ${key}`)
    if (!Number.isSafeInteger(asset.size) || asset.size <= 0) throw new Error(`Empty update package: ${asset.name}`)
    selected.set(key, asset)
  }
  for (const key of requiredKeys) {
    if (!selected.has(key)) throw new Error(`Missing update package for ${key}`)
  }
  return selected
}

function decodeBase64(value) {
  const text = typeof value === 'string' ? value.trim() : ''
  if (!text || text.length % 4 !== 0 || !/^[A-Za-z0-9+/]+={0,2}$/.test(text)) {
    throw new Error('Invalid updater signature encoding')
  }
  const bytes = Buffer.from(text, 'base64')
  if (bytes.toString('base64') !== text) throw new Error('Invalid updater signature encoding')
  return bytes
}

export function verifyUpdateSignature(bytes, encodedSignature, encodedPublicKey, version) {
  checkedVersion(version)
  const decodeText = value => new TextDecoder('utf-8', { fatal: true }).decode(decodeBase64(value))
  const keyLines = decodeText(encodedPublicKey).trimEnd().split(/\r?\n/)
  const signatureLines = decodeText(encodedSignature).replace(/\r?\n$/, '').split(/\r?\n/)
  const key = decodeBase64(keyLines[1])
  const signature = decodeBase64(signatureLines[1])
  const globalSignature = decodeBase64(signatureLines[3])
  if (key.length !== 42 || !['Ed', 'ED'].includes(key.subarray(0, 2).toString())
    || signatureLines.length !== 4 || signature.length !== 74 || globalSignature.length !== 64
    || !['Ed', 'ED'].includes(signature.subarray(0, 2).toString())
    || !key.subarray(2, 10).equals(signature.subarray(2, 10))
    || !signatureLines[2].startsWith('trusted comment: ')) {
    throw new Error('Updater signature does not match the configured public key')
  }
  const publicKey = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), key.subarray(10)]),
    format: 'der', type: 'spki',
  })
  const rawSignature = signature.subarray(10)
  const trustedComment = signatureLines[2].slice('trusted comment: '.length)
  // Minisign signs both the payload (or its BLAKE2b hash) and the trusted comment.
  const message = signature.subarray(0, 2).toString() === 'ED'
    ? createHash('blake2b512').update(bytes).digest() : bytes
  if (!verify(null, message, publicKey, rawSignature)
    || !verify(null, Buffer.concat([rawSignature, Buffer.from(trustedComment)]), publicKey, globalSignature)) {
    throw new Error('Updater signature verification failed')
  }
  const signedVersions = trustedComment.split('\t').filter(field => field.startsWith('version:'))
  if (signedVersions.length !== 1 || signedVersions[0].slice(8).replace(/^v/, '') !== version) {
    throw new Error('Updater signature is not bound to this release version')
  }
}

export function assertDraftRelease(release, version) {
  if (!release.draft || release.tag_name !== `v${checkedVersion(version)}` || release.prerelease) {
    throw new Error('Only the matching stable draft release may be modified; published releases are immutable')
  }
}

export function updateManifest(version, platforms, { notes = '', pubDate = new Date().toISOString() } = {}) {
  checkedVersion(version)
  if (Object.keys(platforms).length !== PLATFORM_KEYS.length
    || PLATFORM_KEYS.some(key => !platforms[key]?.signature || !platforms[key]?.url)) {
    throw new Error('The updater manifest must include all seven signed targets')
  }
  return { version, notes, pub_date: pubDate, platforms }
}
