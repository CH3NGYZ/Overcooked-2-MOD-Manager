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

const notes = await import('../src/modInstallNotes.ts')
const { locale } = await import('../src/i18n.ts')
const repo = (id, overrides = {}) => ({
  id, name: id, provider: 'github', username: 'owner', repository: id,
  note: `# ${id}`, plugin_guid: `guid.${id}`, cached_releases: [], ...overrides,
})

test('首次安装说明按当前语言选取，去重并跳过空说明、有清单的仓库或 GUID 已安装项', () => {
  locale.value = 'ja'
  const a = repo('a', { note_i18n: { ja: '**日本語**', en: 'English' } })
  const result = notes.firstInstallNotes([
    a, a, repo('empty', { note: '  ' }), repo('bound'), repo('guid'),
  ], [{ repository_id: 'bound', package_id: 'bound' },
    { guid: ' GUID.GUID ', enabled: false, package_id: 'manual-local' }])
  assert.deepEqual(result, [{ id: 'a', name: 'a', text: '**日本語**' }])
  locale.value = 'zh-CN'
})

test('手动放入 plugins 后绑定仓库或 GUID 仍需说明；安装清单才可跳过', () => {
  const target = repo('manual')
  for (const binding of [
    { repository_id: target.id },
    { repository_ids: [target.id] },
    { repository_binding: { provider: 'github', owner: 'OWNER', repository: target.repository } },
    { guid: ' GUID.MANUAL ', enabled: false },
  ]) {
    for (const package_id of [undefined, '', '  ']) {
      assert.equal(notes.firstInstallNotes([target], [{ ...binding, package_id }]).length, 1)
    }
    for (const package_id of ['online-install', 'manual-local-file']) {
      assert.deepEqual(notes.firstInstallNotes([target], [{ ...binding, package_id }]), [])
    }
  }
  assert.equal(notes.firstInstallNotes([target], [{ package_id: 'other', guid: 'other' }]).length, 1)
})
