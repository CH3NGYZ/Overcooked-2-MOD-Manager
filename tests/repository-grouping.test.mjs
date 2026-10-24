import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
const names = readFileSync(new URL('../src/repositoryNames.ts', import.meta.url), 'utf8')
const sourceRefresh = readFileSync(new URL('../src/composables/useSourceRefresh.ts', import.meta.url), 'utf8')
const repositoryEditor = readFileSync(new URL('../src/composables/useRepositoryEditor.ts', import.meta.url), 'utf8')
const dialog = readFileSync(new URL('../src/components/RepositoryDocumentDialog.vue', import.meta.url), 'utf8')
const appScript = app.match(/<script setup[^>]*>([\s\S]*?)<\/script>/)?.[1]
assert.ok(appScript, 'Read the App.vue script setup block')
const appAst = ts.createSourceFile('App.vue.ts', appScript, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
const sourceRefreshAst = ts.createSourceFile('useSourceRefresh.ts', sourceRefresh, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
const repositoryEditorAst = ts.createSourceFile('useRepositoryEditor.ts', repositoryEditor, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
function functionSource(name) {
  for (const [ast, fileName] of [[appAst, 'App.vue.ts'], [sourceRefreshAst, 'useSourceRefresh.ts'], [repositoryEditorAst, 'useRepositoryEditor.ts']]) {
    let declaration
    function visit(node) {
      if (ts.isFunctionDeclaration(node) && node.name?.text === name) declaration = node
      ts.forEachChild(node, visit)
    }
    visit(ast)
    if (declaration) return declaration.getText(ast)
  }
  assert.fail(`Use the actual ${name} function from App.vue, useSourceRefresh.ts or useRepositoryEditor.ts`)
}
const groupedRepositoriesSource = functionSource('groupedRepositories')
const localizedNameSource = functionSource('localizedRepositoryName')
const localizedNoteSource = functionSource('localizedRepositoryNote')
const editRepoSource = functionSource('editRepo')
const updateTranslationSource = functionSource('updateRepositoryTranslation')
const ensureChineseNameSource = functionSource('ensureRepositoryChineseName')
const namesUrl = `data:text/javascript;base64,${Buffer.from(ts.transpileModule(names, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText).toString('base64')}`

const appHarness = `
import { repositoryGroup, repositoryGroupKey as repositoryStableGroupKey, repositoryName, repositoryNote } from '${namesUrl}'
const locale = { value: 'en' }
const repoForm = { name_i18n: {}, group_i18n: {}, note_i18n: {} }
const showRepoEditor = { value: false }
function normalizeRepositoryRegion(region) { return region || '' }
${localizedNameSource}
${localizedNoteSource}
${groupedRepositoriesSource}
${editRepoSource}
${updateTranslationSource}
${ensureChineseNameSource}
export { groupedRepositories, localizedRepositoryName, localizedRepositoryNote, editRepo, updateRepositoryTranslation, ensureRepositoryChineseName, locale, repoForm, showRepoEditor }
`
const appUrl = `data:text/javascript;base64,${Buffer.from(ts.transpileModule(appHarness, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText).toString('base64')}`
const { groupedRepositories, localizedRepositoryName, localizedRepositoryNote, editRepo, updateRepositoryTranslation, ensureRepositoryChineseName, locale, repoForm, showRepoEditor } = await import(appUrl)
const { repositoryGroupKey: repositoryStableGroupKey } = await import(namesUrl)

function repo(id, patch = {}) {
  return { id, name: `Raw ${id}`, group: `Raw group ${id}`, note: `Raw note ${id}`, name_i18n: {}, group_i18n: {}, note_i18n: {}, ...patch }
}

function editableRepository(patch = {}) {
  return repo('editable', {
    provider: 'github', username: 'Owner', repository: 'Repo', download_mode: 'release', branch: 'main', file_path: '',
    channel: 'all', asset_rules: [], install_target: 'BepInEx/plugins', plugin_guid: '', local_product_name: '',
    local_original_filename: '', version_regex: '', max_release_count: 10, download_threads: 1, enabled: true,
    ...patch,
  })
}

test('group key stays zh-CN stable while label uses current locale fallback', () => {
  const repositories = [
    repo('english-label', { group_i18n: { 'zh-CN': '工具', en: 'Tools' } }),
    repo('chinese-only-label', { group_i18n: { 'zh-CN': '工具' } }),
    repo('other', { group_i18n: { 'zh-CN': '后端', en: 'Backend' } }),
    repo('ungrouped'),
  ]

  const englishGroups = groupedRepositories(repositories)
  assert.deepEqual(englishGroups.map(({ key, label }) => [key, label]), [
    ['工具', 'Tools'], ['后端', 'Backend'], ['', ''],
  ])
  assert.deepEqual(englishGroups[0].repositories.map(({ id }) => id), ['english-label', 'chinese-only-label'])

  locale.value = 'zh-TW'
  const traditionalChineseGroups = groupedRepositories(repositories)
  assert.deepEqual(traditionalChineseGroups.map(({ key, label }) => [key, label]), [
    ['工具', '工具'], ['后端', '后端'], ['', ''],
  ])
  assert.deepEqual(traditionalChineseGroups[0].repositories.map(({ id }) => id), ['english-label', 'chinese-only-label'])
  assert.equal(repositoryStableGroupKey(repositories[0]), repositoryStableGroupKey(repositories[1]))
})

test('Japanese locale reads ja name, group, and note while the group key stays zh-CN', () => {
  locale.value = 'ja'
  const japanese = repo('japanese', {
    name_i18n: { 'zh-CN': '中文名称', ja: '日本語の名前' },
    group_i18n: { 'zh-CN': '工具', ja: 'ツール' },
    note_i18n: { 'zh-CN': '中文备注', ja: '日本語のメモ' },
  })
  const chineseOnly = repo('chinese-only', {
    name_i18n: { 'zh-CN': '仅有中文' },
    group_i18n: { 'zh-CN': '工具' },
    note_i18n: { 'zh-CN': '中文备注' },
  })
  assert.equal(localizedRepositoryName(japanese), '日本語の名前')
  assert.equal(localizedRepositoryNote(japanese), '日本語のメモ')
  const groups = groupedRepositories([japanese, chineseOnly])
  assert.deepEqual(groups.map(({ key, label }) => [key, label]), [['工具', 'ツール']])
  assert.equal(repositoryStableGroupKey(japanese), '工具')
  assert.equal(repositoryStableGroupKey(chineseOnly), '工具')

  // Editing under the Japanese UI writes the ja translation, not another key.
  editRepo(editableRepository({
    name_i18n: { 'zh-CN': '中文名称' },
    group_i18n: { 'zh-CN': '工具' },
    note_i18n: { 'zh-CN': '' },
  }))
  updateRepositoryTranslation('name', { target: { value: '日本語名' } })
  updateRepositoryTranslation('group', { target: { value: 'ツール' } })
  updateRepositoryTranslation('note', { target: { value: '日本語のメモ' } })
  assert.equal(repoForm.name_i18n.ja, '日本語名')
  assert.equal(repoForm.group_i18n.ja, 'ツール')
  assert.equal(repoForm.note_i18n.ja, '日本語のメモ')
  assert.equal(repoForm.name_i18n['zh-CN'], '中文名称')
  assert.equal(repoForm.group_i18n['zh-CN'], '工具')
})

test('repository display helpers use current locale then zh-CN without raw fallbacks', () => {
  locale.value = 'en'
  const repository = repo('display', {
    name_i18n: { en: ' English name ', 'zh-CN': '中文名称' },
    group_i18n: { en: ' English group ', 'zh-CN': '中文分组' },
    note_i18n: { 'zh-CN': ' 中文备注 ' },
  })
  assert.equal(localizedRepositoryName(repository), 'English name')
  assert.equal(localizedRepositoryNote(repository), '中文备注')
  locale.value = 'ko'
  assert.equal(localizedRepositoryName(repository), '中文名称')
  assert.equal(localizedRepositoryNote(repository), '中文备注')
  assert.equal(localizedRepositoryName(repo('missing-i18n')), '')
  assert.equal(localizedRepositoryNote(repo('missing-i18n')), '')
})

test('editing repository populates name, group, and note from localized fields', () => {
  locale.value = 'en'
  editRepo(editableRepository({
    name_i18n: { en: 'English name', 'zh-CN': '中文名称' },
    group_i18n: { en: 'English group', 'zh-CN': '中文分组' },
    note_i18n: { 'zh-CN': '中文备注' },
  }))
  assert.equal(repoForm.name, 'English name')
  assert.equal(repoForm.group, 'English group')
  assert.equal(repoForm.note, '中文备注')
  assert.deepEqual(repoForm.name_i18n, { en: 'English name', 'zh-CN': '中文名称' })
  assert.deepEqual(repoForm.group_i18n, { en: 'English group', 'zh-CN': '中文分组' })
  assert.deepEqual(repoForm.note_i18n, { 'zh-CN': '中文备注' })
  assert.equal(showRepoEditor.value, true)

  updateRepositoryTranslation('name', { target: { value: 'Updated English name' } })
  updateRepositoryTranslation('group', { target: { value: 'Updated English group' } })
  updateRepositoryTranslation('note', { target: { value: 'Updated English note' } })
  assert.equal(repoForm.name, 'Updated English name')
  assert.equal(repoForm.name_i18n.en, 'Updated English name')
  assert.equal(repoForm.group_i18n.en, 'Updated English group')
  assert.equal(repoForm.note_i18n.en, 'Updated English note')
  assert.equal(repoForm.name_i18n['zh-CN'], '中文名称')
  assert.equal(repoForm.group_i18n['zh-CN'], '中文分组')
  assert.equal(repoForm.note_i18n['zh-CN'], '中文备注')

  locale.value = 'ko'
  editRepo(editableRepository({
    name_i18n: { 'zh-CN': '中文名称' },
    group_i18n: { 'zh-CN': '中文分组' },
    note_i18n: { 'zh-CN': '中文备注' },
  }))
  assert.equal(repoForm.name, '中文名称')
  assert.equal(repoForm.group, '中文分组')
  assert.equal(repoForm.note, '中文备注')
})

test('saveRepo ensures zh-CN name before submitting the patch', () => {
  const saveRepo = repositoryEditor.match(/async function saveRepo\(\) \{[\s\S]*?\n\}/)?.[0]
  assert.ok(saveRepo)
  assert.ok(saveRepo.indexOf('ensureRepositoryChineseName()') < saveRepo.indexOf('api.saveRepository(repoForm)'))
})

test('new repositories initialize a Chinese name from the current-language input', () => {
  locale.value = 'en'
  editRepo()
  repoForm.name = 'New English name'
  updateRepositoryTranslation('name', { target: { value: 'New English name' } })
  ensureRepositoryChineseName()
  assert.equal(repoForm.name_i18n.en, 'New English name')
  assert.equal(repoForm.name_i18n['zh-CN'], 'New English name')

  locale.value = 'zh-CN'
  editRepo()
  repoForm.name = '新的中文名称'
  updateRepositoryTranslation('name', { target: { value: '新的中文名称' } })
  ensureRepositoryChineseName()
  assert.equal(repoForm.name_i18n['zh-CN'], '新的中文名称')
})

test('note dialog uses the localized helper instead of raw note', () => {
  assert.match(dialog, /repositoryNote\(props\.target\.repository, locale\.value\)/)
  assert.doesNotMatch(dialog, /props\.target\.repository\.note\b/)
})
