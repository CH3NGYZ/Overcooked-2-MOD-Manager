import { RELEASE_OWNER, RELEASE_REPO } from './release-lib.mjs'

const repositoryPath = `/repos/${RELEASE_OWNER}/${RELEASE_REPO}`

export class GitHubRelease {
  constructor(token, fetcher = fetch) {
    if (!token?.trim()) throw new Error('GITHUB_TOKEN must have Contents write access to the code repository')
    this.token = token
    this.fetcher = fetcher
  }

  async request(path, { method = 'GET', body, binary = false, upload = false, contentType = 'application/json' } = {}) {
    const response = await this.fetcher(`https://${upload ? 'uploads' : 'api'}.github.com${repositoryPath}${path}`, {
      method,
      headers: {
        Authorization: `Bearer ${this.token}`,
        Accept: binary ? 'application/octet-stream' : 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
        'User-Agent': 'OC2ModManager-release',
        ...(body ? { 'Content-Type': contentType } : {}),
      },
      body: body ? (upload ? body : JSON.stringify(body)) : undefined,
      signal: AbortSignal.timeout(binary || upload ? 180_000 : 30_000),
    })
    if (!response.ok) throw new Error(`GitHub Releases request failed: ${method} ${path} (HTTP ${response.status})`)
    if (binary) return Buffer.from(await response.arrayBuffer())
    return response.status === 204 ? undefined : response.json()
  }

  async releases() {
    const found = []
    for (let page = 1; ; page++) {
      const releases = await this.request(`/releases?per_page=100&page=${page}`)
      if (!Array.isArray(releases)) throw new Error('Invalid GitHub release list')
      found.push(...releases)
      if (releases.length < 100) break
      if (page >= 100) throw new Error('Too many releases to locate the draft safely')
    }
    return found
  }

  async findRelease(tag) {
    const found = (await this.releases()).filter(release => release.tag_name === tag)
    if (found.length > 1) throw new Error('Multiple releases use the requested version tag')
    return found[0]
  }

  async assets(id) {
    const assets = []
    for (let page = 1; ; page++) {
      const batch = await this.request(`/releases/${checkedId(id)}/assets?per_page=100&page=${page}`)
      if (!Array.isArray(batch)) throw new Error('Invalid GitHub release asset list')
      assets.push(...batch)
      if (batch.length < 100) return assets
      if (page >= 100) throw new Error('Too many release assets')
    }
  }

  release(id) { return this.request(`/releases/${checkedId(id)}`) }
  download(id) { return this.request(`/releases/assets/${checkedId(id)}`, { binary: true }) }
  deleteAsset(id) { return this.request(`/releases/assets/${checkedId(id)}`, { method: 'DELETE' }) }
  uploadAsset(id, name, bytes, { contentType, label } = {}) {
    const query = new URLSearchParams({ name })
    if (label) query.set('label', label)
    return this.request(`/releases/${checkedId(id)}/assets?${query}`, {
      method: 'POST', body: bytes, upload: true, contentType,
    })
  }
}

function checkedId(id) {
  if (!/^\d+$/.test(String(id)) || !Number.isSafeInteger(Number(id)) || Number(id) <= 0) {
    throw new Error('Invalid GitHub release or asset ID')
  }
  return id
}
