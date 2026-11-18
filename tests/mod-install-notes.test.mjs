import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return nextResolve(`${specifier}.ts`, context) } catch {}
      try { return nextResolve(`${specifier}/index.ts`, context) } catch {}
    }
    return nextResolve(specifier, context)
  },
})

const { localizedInstallNotes, installNoteKey } = await import('../src/modChanges/planNotes.ts')
const { locale } = await import('../src/i18n.ts')
const note = (id, overrides = {}) => ({
  id, name: id, name_i18n: {}, note: `# ${id}`, note_i18n: {}, ...overrides,
})

test('首次安装说明按当前语言显示标题和正文，按共享规则回退并跳过空白正文', () => {
  const previous = locale.value
  locale.value = 'ja'
  try {
    assert.deepEqual(localizedInstallNotes([
      note('selected', { name_i18n: { ja: '日本語名' }, note_i18n: { ja: ' **日本語** ', en: 'English' } }),
      note('dependency', { note_i18n: { ja: ' ', en: 'English' } }),
      note('blank', { note: '  ', note_i18n: { ja: ' ' } }),
      note('plain'),
    ]), [
      { id: 'selected', name: '日本語名', text: '**日本語**' },
      { id: 'dependency', name: 'dependency', text: 'English' },
      { id: 'plain', name: 'plain', text: '# plain' },
    ])
  } finally { locale.value = previous }
})

test('已读凭据只看渠道和实际正文，改名不会重弹，同文的另一渠道仍需确认', () => {
  const [selected] = localizedInstallNotes([note('selected')])
  assert.equal(installNoteKey({ ...selected, name: 'Renamed' }), installNoteKey(selected))
  assert.notEqual(installNoteKey({ ...selected, id: 'other' }), installNoteKey(selected))
  assert.notEqual(installNoteKey({ ...selected, text: 'Changed' }), installNoteKey(selected))
})
