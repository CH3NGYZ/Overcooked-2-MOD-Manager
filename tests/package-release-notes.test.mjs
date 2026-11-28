import assert from 'node:assert/strict'
import test from 'node:test'
import { selectReleaseNotes } from '../src/appUpdateNotes.ts'
import { LOCALE_CODES, LOCALE_LABELS } from '../src/locale/types.ts'
import { installedModReleaseTag, mapReleaseNotesReview, packageReleaseBody, packageReleaseNotes } from '../src/packageReleaseNotes.ts'

const release = (tag_name, overrides = {}) => ({
  tag_name, name: tag_name, body: `Notes for ${tag_name}`, prerelease: false, draft: false, assets: [], ...overrides,
})
const tags = notes => notes.map(({ tag }) => tag)

test('package history is oldest first, bounded by the installed and exact selected tags', () => {
  const target = release('v1.4', { body: 'Exact pinned release body' })
  const cached = Object.freeze([
    release('v1.7'), release('v1.3'), release('v1.2'), release('v1.2'), release('v1.1'), release('v1.0'),
    release('v1.4'), release('v1.4.0'), release('v1.2.1', { draft: true }), release('v1.2.2', { body: ' \r\n ' }),
  ])
  assert.deepEqual(packageReleaseNotes(cached, target, 'v1.1'), [
    { tag: 'v1.2', body: 'Notes for v1.2', prerelease: false },
    { tag: 'v1.3', body: 'Notes for v1.3', prerelease: false },
    { tag: 'v1.4', body: 'Exact pinned release body', prerelease: false },
  ])
})

test('each crossed release respects the already-filtered channel snapshot and retains its own prerelease marker', () => {
  const platformBeta = release('v1.3', { apiPrerelease: true, body: '' })
  const cached = [release('v1.1'), release('v1.2-beta.2'), release('v1.2.1', { apiPrerelease: true }), platformBeta]
  const crossed = packageReleaseNotes(cached, release('v1.4'), 'v1.0')
  assert.deepEqual(tags(crossed), ['v1.1', 'v1.2-beta.2', 'v1.2.1', 'v1.4'])
  assert.deepEqual(crossed.map(entry => entry.prerelease), [false, true, true, false])
  assert.deepEqual(packageReleaseNotes(cached, platformBeta, 'v1.0').at(-1), {
    tag: 'v1.3', body: '', prerelease: true,
  })
  assert.deepEqual(tags(packageReleaseNotes(cached, platformBeta, 'v1.0')), ['v1.1', 'v1.2-beta.2', 'v1.2.1', 'v1.3'])

  const promoted = release('v1.3.0.7', { apiPrerelease: false, prerelease: true })
  assert.deepEqual(packageReleaseNotes([], promoted), [{ tag: promoted.tag_name, body: promoted.body, prerelease: false }])
  assert.equal(packageReleaseNotes([], release('v1.3-rc.1', { apiPrerelease: false }))[0].prerelease, true)
})

test('MOD history uses the unique core and its scanned version rather than a stale manifest tag', () => {
  const repo = { id: 'test-mod', provider: 'github', username: 'author', repository: 'test', channel: 'stable' }
  const binding = { provider: 'github', owner: 'Author', repository: 'TEST', channel: 'stable', tag: 'v2.0.0' }
  const installed = { repository_id: repo.id, repository_binding: binding, version: '2.0.0', update_version: '2.0.0' }
  assert.equal(installedModReleaseTag(repo, [installed, { ...installed, release_companion: true }]), 'v2.0.0')
  assert.equal(installedModReleaseTag(repo, [{ ...installed, update_version: '2.2.0' }]), '2.2.0')
  assert.equal(installedModReleaseTag(repo, [{ ...installed, update_version: null }]), undefined)
  assert.equal(installedModReleaseTag(repo, [installed, { ...installed, version: '1.0.0' }]), undefined)
  for (const otherBinding of [{ ...binding, owner: 'DifferentAuthor' }, { ...binding, channel: 'beta' }]) {
    assert.equal(installedModReleaseTag(repo, [{ ...installed, repository_binding: otherBinding }]), undefined)
  }
})

test('map history uses the recorded package version and author order, never the info version', () => {
  const cached_releases = ['0.8', '0.7', '0.69', '0.68', '0.6'].map(tag => release(tag))
  const asset_versions = ['0.6', '0.68', '0.69', '0.7', '0.8'].map(mapset_version => ({ mapset_version, info_version: '0.6' }))
  const map = { id: 'm', cached_releases, asset_versions }
  const set = { map_id: 'm', installed_mapset_version: '0.68', version: '0.6' }
  assert.deepEqual(tags(mapReleaseNotesReview(map, cached_releases[1], [set]).releases), ['0.69', '0.7'])
  assert.deepEqual(tags(mapReleaseNotesReview(map, cached_releases[1], [{ ...set, installed_mapset_version: '' }]).releases), ['0.7'])
  assert.deepEqual(tags(mapReleaseNotesReview(map, cached_releases[1], [{ ...set, map_id: 'other' }]).releases), ['0.7'])
  assert.deepEqual(tags(mapReleaseNotesReview({ ...map, asset_versions: [] }, cached_releases[0], [{ ...set, installed_mapset_version: '0.66' }]).releases), ['0.8'])
})

test('fixed-tag map asset versions show their shared Release body only once', () => {
  const body = 'Author’s original shared notes\n## 0.7\nChange\n## 0.69\nAnother change'
  const cached_releases = ['0.7', '0.69', '0.68'].map(tag => release(tag, { body }))
  const map = { id: 'm', shared_release_notes: true, cached_releases }
  const set = { map_id: 'm', installed_mapset_version: '0.68' }
  assert.deepEqual(mapReleaseNotesReview(map, cached_releases[0], [set]), {
    installedTag: '0.68', releases: [{ tag: '0.7', body, prerelease: false }],
  })
})

test('first install, downgrade, reinstall and uncomparable tags keep only the exact release', () => {
  const target = release('v1.4')
  const cached = [release('v1.2'), release('v1.3'), release('v1.4'), release('v1.5')]
  for (const installedTag of [undefined, null, '', 'v2', 'v1.4.0', 'v1.4', 'spring']) {
    assert.deepEqual(tags(packageReleaseNotes(cached, target, installedTag)), ['v1.4'], installedTag)
  }
  assert.deepEqual(tags(packageReleaseNotes(cached, release('winter'), 'v1.0')), ['winter'])
  assert.deepEqual(packageReleaseNotes(cached, release('v1.4', { draft: true }), 'v1.0'), [])
})

test('custom package tags use only the provided ordering, never lexical order', () => {
  const ranked = ['spring', 'winter', 'autumn', 'summer']
  const compare = (left, right) => ranked.includes(left) && ranked.includes(right)
    ? ranked.indexOf(left) - ranked.indexOf(right) : null
  const target = release('autumn')
  const cached = ['summer', 'later maybe', 'autumn', 'winter'].map(tag => release(tag))
  assert.deepEqual(tags(packageReleaseNotes(cached, target, 'spring', compare)), ['winter', 'autumn'])
  assert.deepEqual(tags(packageReleaseNotes(cached, target, 'later maybe', compare)), ['autumn'])
  assert.deepEqual(tags(packageReleaseNotes(cached, target, 'spring')), ['autumn'])

  const winterAndSummer = (left, right) => left === 'winter' && right === 'summer' || left === 'summer' && right === 'winter'
  const comparePartlyKnown = (left, right) => winterAndSummer(left, right) ? null : compare(left, right)
  assert.deepEqual(tags(packageReleaseNotes(cached, release('summer'), 'spring', comparePartlyKnown)),
    ['autumn', 'summer'])
  const middle = ['summer', 'winter'].map(tag => release(tag))
  const compareAmbiguous = (left, right) => left === 'autumn' ? 1 : right === 'autumn' ? -1 : comparePartlyKnown(left, right)
  assert.deepEqual(tags(packageReleaseNotes(middle, target, 'spring', compareAmbiguous)), ['autumn'])
})

test('each package version selects its own translated heading or falls back to its untouched source', () => {
  const missing = '\r\n# English\r\nOnly English  \r\n'
  const empty = '# Français\r\n  \r\n# English\r\nOriginal  \r\n'
  const current = '# Français\nChangements\n\n# English\nChanges'
  const versions = packageReleaseNotes([release('v1.2', { body: missing }), release('v1.3', { body: empty })],
    release('v1.4', { body: current }), 'v1.1')
  assert.deepEqual(versions.map(({ body }) => packageReleaseBody(body, 'fr')), [missing, empty, 'Changements\n'])
  assert.equal(packageReleaseBody(' \r\n ', 'en'), ' \r\n ')
  assert.equal(selectReleaseNotes(missing, 'fr'), '')
})

test('package notes prefer all 12 language names to matching codes regardless of order', () => {
  const names = LOCALE_CODES.map(locale => `# **${LOCALE_LABELS[locale]}**\nName ${locale}`)
  const codes = LOCALE_CODES.map(locale => `# ${locale.toUpperCase()}\nCode ${locale}`)
  for (const headings of [[...codes, ...names], [...names, ...codes]]) {
    const source = headings.join('\n\n')
    for (const locale of LOCALE_CODES) assert.equal(packageReleaseBody(source, locale).trim(), `Name ${locale}`)
  }
  assert.equal(selectReleaseNotes([...codes, ...names].join('\n\n'), 'ja').trim(), 'Code ja')
})

test('package notes use a code when the named section is blank, and keep the original if neither is usable', () => {
  const codeOnly = '# Release notes\r\n\r\n## **Português (Brasil)**\r\n \r\n## pt-BR\r\nCorreção  \r\n'
  assert.equal(packageReleaseBody(codeOnly, 'pt-BR'), 'Correção  \n')
  const missing = '\r\n    Original example\r\n\r\n# Português (Brasil)\r\n \r\n# pt-BR\r\n\r\n# English\r\nOnly English  \r\n'
  assert.equal(packageReleaseBody(missing, 'pt-BR'), missing)
  const secondLevel = '# Changes\n\n## zh-Hant\nCode\n\n## Traditional Chinese\nName'
  assert.equal(packageReleaseBody(secondLevel, 'zh-TW'), 'Name')
  assert.equal(packageReleaseBody('## zh-Hant\nCode', 'zh-TW'), 'Code')
})

test('package level-two headings use the same Markdown structure without changing manager notes', () => {
  const source = '# Changes\n\n## English\nHello\n\n> ## Deutsch\n\n```md\n## Deutsch\n```\n\n## Français\nBonjour\n\n# Footer\nnot in French'
  assert.equal(packageReleaseBody(source, 'fr').trim(), 'Bonjour')
  assert.match(packageReleaseBody(source, 'en'), /^Hello\n/)
  assert.equal(packageReleaseBody(source, 'de'), source)
  assert.equal(selectReleaseNotes(source, 'fr'), source)

  const firstLevel = '# English\nStart\n\n## Français\nA section inside English'
  assert.equal(packageReleaseBody(firstLevel, 'fr'), firstLevel)
  assert.equal(packageReleaseBody(firstLevel, 'en'), 'Start\n\n## Français\nA section inside English')
})
