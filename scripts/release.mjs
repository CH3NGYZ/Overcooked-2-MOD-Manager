import { createHash } from 'node:crypto'
import { execFileSync } from 'node:child_process'
import { appendFileSync, readFileSync } from 'node:fs'
import { basename, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { GitHubRelease } from './github-release.mjs'
import {
  RELEASE_OWNER, RELEASE_REPO, assertDraftRelease, collectReleaseChangelog, collectUpdateAssets,
  publicUpdateAssetUrl, readProject, releaseNotesFromMessage, releaseTag, updateManifest,
  serializeUpdateManifest, verifyUpdateSignature,
} from './release-lib.mjs'

export function readReleaseNotes(commitish, root = process.cwd()) {
  if (!/^[a-f0-9]{40}$/i.test(commitish || '')) {
    throw new Error('The release source commit must be a full GitHub commit SHA')
  }
  let message
  try {
    message = execFileSync('git', ['-c', 'i18n.logOutputEncoding=utf-8', 'show', '--no-patch', '--format=%B', `${commitish}^{commit}`, '--'], {
      cwd: root, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
    }).replace(/[\r\n]+$/, '')
  } catch {
    throw new Error('Could not read the release source commit message')
  }
  if (!message.trim()) throw new Error('The release source commit message is empty')
  // The release body is the user-facing changelog only: drop commit trailers
  // (Co-Authored-By and friends) and local build/verification paragraphs.
  const notes = releaseNotesFromMessage(message)
  if (!notes) throw new Error('The release source commit message has no notes after removing trailers and verification text')
  return notes
}

export async function prepareRelease(github, version, tag, commitish, notes) {
  if (commitish !== undefined && !/^[a-f0-9]{40}$/i.test(commitish)) {
    throw new Error('The release source commit must be a full GitHub commit SHA')
  }
  if (typeof notes !== 'string' || !notes.trim()) throw new Error('Release notes must contain the source commit message')
  let release = await github.findRelease(tag)
  if (!release) {
    const repository = await github.request('')
    release = await github.request('/releases', {
      method: 'POST',
      body: {
        tag_name: tag,
        target_commitish: commitish || repository.default_branch,
        name: `Overcooked 2 MOD Manager ${tag}`,
        body: notes,
        draft: true,
        prerelease: false,
      },
    })
  }
  assertDraftRelease(release, version)
  if ((commitish && release.target_commitish !== commitish) || release.body !== notes) {
    release = await github.request(`/releases/${release.id}`, {
      method: 'PATCH',
      // GitHub can detach a draft from its tag when a PATCH omits tag_name.
      body: { tag_name: tag, target_commitish: commitish || release.target_commitish, body: notes, draft: true, prerelease: false },
    })
    assertDraftRelease(release, version)
  }
  // A retry must not leave an old manifest advertising a partially rebuilt draft.
  for (const asset of await github.assets(release.id)) {
    if (asset.name === 'latest.json') await github.deleteAsset(asset.id)
  }
  return release
}

export async function aggregateManifest(github, release, assets, version, publicKey) {
  assertDraftRelease(release, version)
  const selected = collectUpdateAssets(assets, version)
  const platforms = {}
  for (const [key, asset] of selected) {
    const signatures = assets.filter(item => item.name === `${asset.name}.sig`)
    if (signatures.length !== 1) throw new Error(`Exactly one signature is required for ${asset.name}`)
    const signature = (await github.download(signatures[0].id)).toString('utf8').trim()
    const bytes = await github.download(asset.id)
    if (bytes.length !== asset.size) throw new Error(`Incomplete uploaded package: ${asset.name}`)
    if (asset.digest && asset.digest !== `sha256:${createHash('sha256').update(bytes).digest('hex')}`) {
      throw new Error(`Uploaded package digest mismatch: ${asset.name}`)
    }
    verifyUpdateSignature(bytes, signature, publicKey, version)
    const url = publicUpdateAssetUrl(release, asset, version)
    platforms[key] = { signature, url }
  }
  let changelog
  try {
    changelog = collectReleaseChangelog(await github.releases(), version)
  } catch {
    console.warn('Optional updater release history unavailable; keeping target notes')
  }
  return updateManifest(version, platforms, { notes: release.body || '', changelog })
}

async function checkStoreDraft(github, id, version, commitish, tagged) {
  const tag = releaseTag(version)
  if (tagged) {
    const ref = await github.request(`/git/ref/tags/${encodeURIComponent(tag)}`)
    let target = ref.object
    if (ref.ref === `refs/tags/${tag}` && target?.type === 'tag' && /^[a-f0-9]{40}$/.test(target.sha)) {
      const annotation = await github.request(`/git/tags/${target.sha}`)
      if (annotation.sha !== target.sha) throw new Error(`Invalid annotated tag ${tag}`)
      target = annotation.object
    }
    if (ref.ref !== `refs/tags/${tag}` || target?.type !== 'commit' || target.sha !== commitish) {
      throw new Error(`Tag ${tag} does not identify this build's source commit`)
    }
  }
  const release = await github.release(id)
  assertDraftRelease(release, version)
  if (String(release.id) !== String(id) || release.draft !== true || release.prerelease !== false
    || release.target_commitish !== commitish) {
    throw new Error('MSIX upload requires the matching stable draft and source commit')
  }
}

export async function uploadStoreMsix(github, id, version, commitish, name, bytes, tagged = true) {
  if (name !== `OC2ModManager_${version}_x64.msix` || bytes.length === 0 || !/^[a-f0-9]{40}$/.test(commitish || '')) {
    throw new Error('MSIX upload requires the exact release filename, nonempty bytes and a source commit SHA')
  }
  const digest = `sha256:${createHash('sha256').update(bytes).digest('hex')}`
  const matches = async asset => {
    if (asset.name !== name || asset.state !== 'uploaded' || asset.size !== bytes.length) return false
    if (asset.digest != null) return asset.digest === digest
    return bytes.equals(await github.download(asset.id))
  }
  await checkStoreDraft(github, id, version, commitish, tagged)
  const existing = (await github.assets(id)).filter(asset => asset.name?.toLowerCase() === name.toLowerCase())
  if (existing.length > 1) throw new Error(`Multiple assets match ${name}`)
  if (existing.length === 1) {
    const [asset] = existing
    if (await matches(asset)) return false
    if (asset.name !== name || asset.state !== 'starter' || asset.size !== 0) {
      throw new Error(`Refusing to overwrite existing asset ${name}`)
    }
    await checkStoreDraft(github, id, version, commitish, tagged)
    await github.deleteAsset(asset.id)
  }
  await checkStoreDraft(github, id, version, commitish, tagged)
  const asset = await github.uploadAsset(id, name, bytes, {
    contentType: 'application/octet-stream', label: 'Microsoft Store submission (unsigned MSIX)',
  })
  if (!await matches(asset)) throw new Error(`Uploaded MSIX does not match the local package: ${name}`)
  return true
}

export async function main(args = process.argv.slice(2)) {
  const { version, publicKey } = readProject()
  const tag = releaseTag(version, process.env.GITHUB_REF_TYPE, process.env.GITHUB_REF_NAME)
  const github = new GitHubRelease(process.env.GITHUB_TOKEN)
  if (args[0] === 'prepare' && args.length === 1) {
    if (!process.env.GITHUB_OUTPUT) throw new Error('This command requires GitHub Actions outputs')
    const notes = readReleaseNotes(process.env.GITHUB_SHA)
    const release = await prepareRelease(github, version, tag, process.env.GITHUB_SHA, notes)
    appendFileSync(process.env.GITHUB_OUTPUT, `release_id=${release.id}\ntag_name=${tag}\nversion=${version}\n`)
    console.log(`Prepared draft ${tag} in ${RELEASE_OWNER}/${RELEASE_REPO}`)
    return
  }
  if (args[0] === 'msix' && args.length === 3) {
    if (process.platform !== 'win32' || process.env.GITHUB_ACTIONS !== 'true'
      || !['tag', 'branch'].includes(process.env.GITHUB_REF_TYPE)) {
      throw new Error('MSIX upload is only supported by the Windows GitHub Actions release build')
    }
    const name = basename(args[2])
    const uploaded = await uploadStoreMsix(github, args[1], version, process.env.GITHUB_SHA, name,
      readFileSync(args[2]), process.env.GITHUB_REF_TYPE === 'tag')
    console.log(uploaded ? `Uploaded ${name} to draft ${tag}` : `${name} already matches draft ${tag}`)
    return
  }
  if (!['manifest', 'check'].includes(args[0]) || args.length !== 2) {
    throw new Error('Usage: node scripts/release.mjs prepare | check <release-id> | manifest <release-id> | msix <release-id> <file>')
  }
  const release = await github.release(args[1])
  assertDraftRelease(release, version)
  if (args[0] === 'check') return
  const assets = await github.assets(release.id)
  const manifest = await aggregateManifest(github, release, assets, version, publicKey)
  const manifestBytes = serializeUpdateManifest(manifest)
  assertDraftRelease(await github.release(release.id), version)
  for (const asset of assets) {
    if (asset.name === 'latest.json') await github.deleteAsset(asset.id)
  }
  await github.uploadAsset(release.id, 'latest.json', manifestBytes)
  console.log(`Uploaded a verified ${Object.keys(manifest.platforms).length}-target latest.json to draft ${tag}; nothing was published`)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1 })
}
