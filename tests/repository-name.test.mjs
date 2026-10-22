import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

function moduleUrl(source, imports = {}) {
  let { outputText } = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
  })
  for (const [name, target] of Object.entries(imports)) outputText = outputText.replaceAll(`from '${name}'`, `from '${target}'`)
  return `data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`
}

const nameSource = readFileSync(new URL('../src/repositoryNames.ts', import.meta.url), 'utf8')
const nameUrl = moduleUrl(nameSource)
const { repositoryName } = await import(nameUrl)
const sourceNameSource = readFileSync(new URL('../src/repositorySources.ts', import.meta.url), 'utf8')
const sourceNameUrl = moduleUrl(sourceNameSource)
const { repositorySourceName } = await import(sourceNameUrl)
const scenarioSource = readFileSync(new URL('../src/tutorial/scenarios.ts', import.meta.url), 'utf8')
const scenarioUrl = moduleUrl(scenarioSource, { '../repositoryNames': nameUrl })
const { createTutorialDashboard } = await import(scenarioUrl)

test('repository names prefer current locale, Chinese fallback, raw name, then identity', () => {
  assert.equal(repositoryName({ name: 'Raw name', name_i18n: { en: ' English ', 'zh-CN': '中文' } }, 'en'), 'English')
  assert.equal(repositoryName({ name: 'Raw name', name_i18n: { 'zh-CN': ' 中文 ' } }, 'ko'), '中文')
  assert.equal(repositoryName({ name: ' Raw name ' }, 'ko'), 'Raw name')
  assert.equal(repositoryName({ username: ' Owner ', repository: ' Mod ' }, 'ko'), 'Owner/Mod')
  assert.equal(repositoryName({ name: '  ', name_i18n: { en: ' ' }, username: 'Owner', repository: 'Mod' }, 'en'), 'Owner/Mod')
})

test('repository source names fall back to Chinese before the raw nickname', () => {
  assert.equal(repositorySourceName({ nickname: 'Raw nickname', nickname_i18n: { en: 'English', 'zh-CN': '中文' } }, 'en'), 'English')
  assert.equal(repositorySourceName({ nickname: 'Raw nickname', nickname_i18n: { 'zh-CN': ' 中文 ' } }, 'ko'), '中文')
  assert.equal(repositorySourceName({ nickname: ' Raw nickname ', nickname_i18n: {} }, 'ko'), 'Raw nickname')
})

test('tutorial dashboards accept repositories without a raw name', () => {
  const template = (id, nameI18n = {}) => ({
    id,
    name_i18n: nameI18n,
    provider: 'github',
    username: 'Owner',
    repository: id,
    source_id: 'bundled-source',
    group_i18n: {},
    asset_rules: [{ type: 'exact', value: `${id}.zip` }],
    cached_releases: [],
  })
  const dashboard = {
    config: { repository_sources: [{ id: 'bundled-source', nickname: 'Bundled', url: 'repositories.toml', builtin: true }] },
    game: {},
    mods: [],
    updates: [],
  }

  const preview = createTutorialDashboard(dashboard, 'overview', (value) => value, [
    template('host', { 'zh-CN': '主仓库' }),
    template('manager'),
  ], 'en')

  assert.ok(preview)
  assert.equal(preview.config.repositories[0].cached_releases[0].name, '主仓库 v2.4.0')
  assert.equal(preview.config.repositories[1].cached_releases[0].name, 'Owner/manager v1.0.0')
  assert.equal(preview.config.repository_sources[1].nickname, 'Owner/manager')
  assert.equal(preview.mods.find((mod) => mod.dll_file === 'HostUtilities.dll').name, '主仓库')
})
