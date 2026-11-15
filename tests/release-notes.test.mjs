import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { mkdtempSync, realpathSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import test from 'node:test'
import { releaseNotesFromMessage } from '../scripts/release-lib.mjs'
import { readReleaseNotes } from '../scripts/release.mjs'

const start = '<!-- release-notes:start -->'
const end = '<!-- release-notes:end -->'
const concise = '- 改善地图安装与更新体验\n- 增加反馈渠道'
const explicitMessage = ['2.2.0', '- 详细实现与内部变更', '- 本地验证细节', '',
  start, concise, end, '', '内部交付记录：不公开'].join('\n')

function gitFixture(t) {
  const temporary = realpathSync(tmpdir())
  const directory = mkdtempSync(join(temporary, 'oc2-release-notes-'))
  t.after(() => {
    assert.equal(dirname(directory), temporary)
    rmSync(directory, { recursive: true, force: true })
  })
  const git = (args, input) => execFileSync('git', args, {
    cwd: directory, input, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'], windowsHide: true, timeout: 10_000,
  }).trim()
  git(['init', '--bare', '--quiet', '--object-format=sha1', '.'])
  const tree = git(['hash-object', '-t', 'tree', '-w', '--stdin'], '')
  // Unreferenced objects exercise the real reader without commits or hooks in the workspace.
  const commit = message => git(['hash-object', '-t', 'commit', '-w', '--stdin'], [
    `tree ${tree}`, 'author Release Test <release-test@example.invalid> 1700000000 +0000',
    'committer Release Test <release-test@example.invalid> 1700000000 +0000', '', message, '',
  ].join('\n'))
  return { directory, commit }
}

test('explicit release notes omit the version title and all detailed text outside the block', () => {
  assert.equal(releaseNotesFromMessage(explicitMessage), concise)
  assert.equal(releaseNotesFromMessage(explicitMessage.replaceAll('\n', '\r\n')), concise)
  assert.equal(releaseNotesFromMessage(`2.2.0\n  ${start}\n${concise}\n\t${end}\t`), concise)
})

test('explicit release notes retain the existing trailer and verification filtering', () => {
  const message = ['2.2.0', start, concise, 'Reviewed-by: fixture', '',
    '校验：npm test 通过', '', '未做真实网络验证', '', 'cargo check --locked', end].join('\n')
  assert.equal(releaseNotesFromMessage(message), concise)
})

test('unmarked messages keep the complete legacy changelog and existing filtering', () => {
  const message = ['2.1.1', '', '- 原有用户日志', 'Signed-off-by: fixture', '',
    '详细改动：\n- 原有详细条目', '', '校验：npm test 通过', '', '未做真实网络验证', '',
    'cargo fmt -- --check', '', 'Reviewed-by: fixture'].join('\r\n')
  assert.equal(releaseNotesFromMessage(message), '2.1.1\n\n- 原有用户日志\n\n详细改动：\n- 原有详细条目')
  assert.equal(releaseNotesFromMessage(''), '')
  assert.equal(releaseNotesFromMessage(undefined), '')
})

const invalidBlocks = [
  ['missing end', `${start}\npublic`], ['missing start', `public\n${end}`],
  ['reversed markers', `${end}\npublic\n${start}`],
  ['duplicate start', `${start}\n${start}\npublic\n${end}`],
  ['duplicate end', `${start}\npublic\n${end}\n${end}`],
  ['multiple blocks', `${start}\none\n${end}\n${start}\ntwo\n${end}`],
  ['empty block', `${start}\n${end}`], ['whitespace block', `${start}\n \n\t\n${end}`],
  ['filtered block', `${start}\n校验：npm test 通过\n\nReviewed-by: fixture\n${end}`],
  ['inline start', `prefix ${start}\npublic\n${end}`],
  ['inline end', `${start}\npublic\n${end} suffix`],
  ['markers on one line', `${start} public ${end}`],
  ['malformed delimiters', '<!-- release-notes:start-->\npublic\n<!-- release-notes:end-->'],
]

for (const [label, block] of invalidBlocks) {
  test(`${label} fails instead of exposing detailed commit text`, () => {
    assert.throws(() => releaseNotesFromMessage(`2.2.0\n- Detailed private text\n\n${block}`), /release notes/i)
  })
}

test('readReleaseNotes reads the explicit block from an actual Git commit object', t => {
  const { directory, commit } = gitFixture(t)
  assert.equal(readReleaseNotes(commit(explicitMessage), directory), concise)
})

test('readReleaseNotes preserves unmarked commits and rejects malformed or empty explicit blocks', t => {
  const { directory, commit } = gitFixture(t)
  assert.equal(readReleaseNotes(commit('2.1.1\n\n- Original notes\n\n校验：npm test 通过'), directory),
    '2.1.1\n\n- Original notes')
  for (const [, block] of invalidBlocks) {
    assert.throws(() => readReleaseNotes(commit(`2.2.0\n- Private details\n\n${block}`), directory), /release notes/i)
  }
})
