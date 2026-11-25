import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { selectReleaseNotes, updateNotesForLocale } from '../src/appUpdateNotes.ts'
import { LOCALE_CODES, LOCALE_LABELS } from '../src/locale/types.ts'
import { renderReadme } from '../src/markdown.ts'

test('all 12 native names and language codes select only the matching Release section', () => {
  assert.equal(LOCALE_CODES.length, 12)
  for (const heading of [code => LOCALE_LABELS[code], code => code.toUpperCase()]) {
    const source = LOCALE_CODES.map(code => `# ${heading(code)} #\n\n- Notes for ${code}`).join('\n\n')
    for (const code of LOCALE_CODES) {
      assert.equal(selectReleaseNotes(source, code).trim(), `- Notes for ${code}`, code)
    }
  }
})

test('language names can be written in Chinese or English; Spanish and Brazilian Portuguese stay distinct', () => {
  const headings = {
    en: '英語', de: 'German', fr: '法语', it: 'Italian', ko: '韩语', 'es-ES': 'Español',
    'zh-CN': 'Simplified Chinese', 'zh-TW': 'Traditional Chinese', ru: '俄语', ja: 'Japanese',
    pl: 'Polish', 'pt-BR': 'Português',
  }
  const source = Object.entries(headings).map(([code, heading]) => `# **${heading}**\r\n\r\n${code}`).join('\r\n\r\n')
  for (const code of LOCALE_CODES) assert.equal(selectReleaseNotes(source, code).trim(), code)
  const qualified = '# Spanish (Spain)\nEspaña\n\n# Portuguese (Brazil)\nBrasil'
  assert.equal(selectReleaseNotes(qualified, 'es-ES').trim(), 'España')
  assert.equal(selectReleaseNotes(qualified, 'pt-BR').trim(), 'Brasil')
  const unmatchedDialect = '# Português (Portugal)\nOnly Portugal\n\n# English\nEnglish notes'
  assert.equal(selectReleaseNotes(unmatchedDialect, 'pt-BR'), '')
  assert.equal(selectReleaseNotes(unmatchedDialect, 'en').trim(), 'English notes')
})

test('only top-level Markdown h1 titles delimit language sections', () => {
  const body = [
    '## Français', '- Bullet  ', '  with an intentional line break', '',
    '> # Italiano', '', '- # Polski', '',
    '    # Deutsch', '',
    '```markdown', '# 日本語', '```',
  ].join('\n')
  const source = `unscoped preamble\n\nEnglish\n=======\n\n${body}\n\n# Small print\n\nunscoped footer\n\n# 한국어\n\n한글 본문`
  assert.equal(selectReleaseNotes(source, 'en').trim(), body)
  assert.equal(selectReleaseNotes(source, 'ko').trim(), '한글 본문')
  for (const locale of ['de', 'fr', 'it', 'ja', 'pl']) assert.equal(selectReleaseNotes(source, locale), '')
  assert.match(renderReadme(selectReleaseNotes(source, 'en')), /<h2>Français<\/h2>/)
})

test('legacy Markdown without a recognized root language heading remains byte-for-byte unchanged', () => {
  const legacy = '\r\n# Changes\r\n\r\n## English\r\n> # 日本語\r\n\r\n```md\r\n# de\r\n```\r\n\r\n- Old note  \r\n'
  assert.equal(selectReleaseNotes(legacy, 'en'), legacy)
  assert.equal(selectReleaseNotes(legacy, 'ja'), legacy)
  assert.equal(selectReleaseNotes('# 中文\n不猜測簡體還是繁體', 'zh-TW'), '# 中文\n不猜測簡體還是繁體')
})

test('headings inside HTML comments do not delimit or create release-note sections', () => {
  const comment = '<!--\r\n# Deutsch\r\ninternal note\r\n-->'
  const legacy = `${comment}\r\n\r\nLegacy notes`
  assert.equal(selectReleaseNotes(legacy, 'en'), legacy)
  assert.equal(selectReleaseNotes(legacy, 'de'), legacy)

  const english = 'First item\n\n<!--\n# Español\nignored\n-->\n\nLast item'
  const source = `# English\n${english}\n\n# Deutsch\nBeispiel`
  assert.equal(selectReleaseNotes(source, 'en').trim(), english)
  assert.equal(selectReleaseNotes(source, 'de').trim(), 'Beispiel')
  assert.equal(selectReleaseNotes(source, 'es-ES'), '')
  assert.doesNotMatch(renderReadme(selectReleaseNotes(source, 'en')), /<!--/)
  assert.match(renderReadme(selectReleaseNotes('# English\n\n<b>Untrusted HTML</b>', 'en')), /&lt;b&gt;Untrusted HTML&lt;\/b&gt;/)
})

test('each version is isolated, with an empty or missing language shown as no notes', () => {
  const old = '- Old unsegmented notes\n\n```md\n# Deutsch'
  const update = {
    body: '# Deutsch\nZiel',
    changelog: [
      { version: '2.1.0', notes: old },
      { version: '2.2.0', notes: '# Deutsch\n \n\n# English\nAvoid fallback' },
      { version: '2.3.0', notes: '' },
      { version: '2.4.0', notes: '# English\nNot the requested language' },
      { version: '2.5.0', notes: '# Deutsch\nZiel' },
    ],
  }
  assert.deepEqual(updateNotesForLocale(update, 'de'), [
    { version: '2.1.0', notes: old },
    { version: '2.2.0', notes: '' },
    { version: '2.3.0', notes: '' },
    { version: '2.4.0', notes: '' },
    { version: '2.5.0', notes: 'Ziel' },
  ])
  assert.deepEqual(updateNotesForLocale({ body: update.body }, 'de'), [{ notes: 'Ziel' }])
  assert.deepEqual(updateNotesForLocale({ body: update.body }, 'en'), [{ notes: '' }])
  assert.deepEqual(updateNotesForLocale({ body: null }, 'en'), [{ notes: '' }])
  assert.deepEqual(updateNotesForLocale(null, 'en'), [])
  assert.equal(selectReleaseNotes('# Deutsch\n\n# de\nActual text\n\n# de\nLater duplicate', 'de').trim(), 'Actual text')
})

test('shared update notes react to the selected locale and render Markdown separately for each version', () => {
  const notes = readFileSync(new URL('../src/components/AppUpdateNotes.vue', import.meta.url), 'utf8')
  assert.match(notes, /computed\(\(\) => updateNotesForLocale\(props\.update, locale\.value\)\)/)
  assert.match(notes, /<section v-for="entry in notes"[^>]*>\s*<h3 v-if="entry.version">[^<]*<\/h3>\s*<MarkdownBody v-if="entry.notes" :source="entry.notes" \/>\s*<p v-else>\{\{ t\('dialogs.noReleaseNotesWereProvided'\) \}\}/)
})
