import { createHash } from 'node:crypto'
import { appendFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { GitHubRelease } from './github-release.mjs'
import {
  RELEASE_OWNER, RELEASE_REPO, assertDraftRelease, collectUpdateAssets,
  publicUpdateAssetUrl, readProject, releaseTag, updateManifest, verifyUpdateSignature,
} from './release-lib.mjs'

export async function prepareRelease(github, version, tag, commitish) {
  if (commitish !== undefined && !/^[a-f0-9]{40}$/i.test(commitish)) {
    throw new Error('The release source commit must be a full GitHub commit SHA')
  }
  let release = await github.findRelease(tag)
  if (!release) {
    const repository = await github.request('')
    release = await github.request('/releases', {
      method: 'POST',
      body: {
        tag_name: tag,
        target_commitish: commitish || repository.default_branch,
        name: `Overcooked 2 MOD Manager ${tag}`,
        body: 'Download the installer for your system. Publish only after all four builds and the updater manifest succeed.',
        draft: true,
        prerelease: false,
      },
    })
  }
  assertDraftRelease(release, version)
  if (commitish && release.target_commitish !== commitish) {
    release = await github.request(`/releases/${release.id}`, {
      method: 'PATCH', body: { target_commitish: commitish },
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
  return updateManifest(version, platforms, { notes: release.body || '' })
}

export async function main(args = process.argv.slice(2)) {
  const { version, publicKey } = readProject()
  const tag = releaseTag(version, process.env.GITHUB_REF_TYPE, process.env.GITHUB_REF_NAME)
  const github = new GitHubRelease(process.env.GITHUB_TOKEN)
  if (args[0] === 'prepare' && args.length === 1) {
    const release = await prepareRelease(github, version, tag, process.env.GITHUB_SHA)
    if (!process.env.GITHUB_OUTPUT) throw new Error('This command requires GitHub Actions outputs')
    appendFileSync(process.env.GITHUB_OUTPUT, `release_id=${release.id}\ntag_name=${tag}\nversion=${version}\n`)
    console.log(`Prepared draft ${tag} in ${RELEASE_OWNER}/${RELEASE_REPO}`)
    return
  }
  if (!['manifest', 'check'].includes(args[0]) || args.length !== 2) {
    throw new Error('Usage: node scripts/release.mjs prepare | check <release-id> | manifest <release-id>')
  }
  const release = await github.release(args[1])
  assertDraftRelease(release, version)
  if (args[0] === 'check') return
  const assets = await github.assets(release.id)
  const manifest = await aggregateManifest(github, release, assets, version, publicKey)
  assertDraftRelease(await github.release(release.id), version)
  for (const asset of assets) {
    if (asset.name === 'latest.json') await github.deleteAsset(asset.id)
  }
  await github.uploadManifest(release.id, Buffer.from(JSON.stringify(manifest, null, 2) + '\n'))
  console.log(`Uploaded a verified seven-target latest.json to draft ${tag}; nothing was published`)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch(error => { console.error(error.message); process.exitCode = 1 })
}
