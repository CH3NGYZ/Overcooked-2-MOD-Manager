import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import ts from 'typescript'

function declarations(source, names) {
  const file = ts.createSourceFile('fixture.ts', source, ts.ScriptTarget.Latest, true)
  return file.statements.flatMap((statement) => {
    if (ts.isVariableStatement(statement)) return statement.declarationList.declarations.some((node) => names.includes(node.name.getText(file))) ? [statement.getText(file)] : []
    return ts.isFunctionDeclaration(statement) && names.includes(statement.name?.text) ? [statement.getText(file)] : []
  }).join('\n')
}

async function importTypescript(source) {
  const code = ts.transpileModule(source, { compilerOptions: { target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext } }).outputText
  return import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`)
}

const app = await readFile(new URL('../src/App.vue', import.meta.url), 'utf8')
const script = app.match(/<script setup lang="ts">([\s\S]*?)<\/script>/)[1]
const runtime = await importTypescript(`
const t = (key) => key
${declarations(script, ['initialUpdateCheck', 'updateChannelName', 'updateChannelStatus'])}
export { initialUpdateCheck, updateChannelName, updateChannelStatus }
`)
const i18n = await readFile(new URL('../src/i18n.ts', import.meta.url), 'utf8')
const translations = await importTypescript(`
${declarations(i18n, ['entries', 'zhCnEntries', 'translateFor'])}
export { translateFor }
`)

test('update UI starts with no fictitious API/token routes and names bundled proxies distinctly', () => {
  assert.deepEqual(runtime.initialUpdateCheck().channels, [])
  assert.equal(runtime.initialUpdateCheck().region, 'detecting')
  assert.equal(runtime.updateChannelName('gitee-direct'), 'Gitee 更新清单')
  assert.equal(runtime.updateChannelName('github-direct'), 'GitHub 更新清单')
  assert.equal(runtime.updateChannelName('github-proxy:gh-ddlc'), 'GitHub 内置代理 (gh-ddlc)')
  assert.equal(runtime.updateChannelName('github-proxy:oc2gh-release'), 'GitHub 内置代理 (oc2gh-release)')
  assert.equal(runtime.updateChannelStatus('cancelled'), '已取消')
  assert.match(app, /startupUpdatePhase === 'manifest'/)
  assert.doesNotMatch(app, /gitee-token|github-token|Github API [123]|Gitee API [123]/)
})

test('manifest errors and route labels have all supported UI translations', () => {
  assert.equal(translations.translateFor('已取消', 'en'), 'Cancelled')
  assert.notEqual(translations.translateFor('已取消', 'ko'), '已取消')
  for (const locale of ['zh-TW', 'en', 'ko']) {
    for (const key of ['Gitee 更新清单', 'GitHub 更新清单', 'GitHub 内置代理', '更新清单请求失败', '更新清单格式无效', '更新清单不是稳定版本', '更新清单读取失败', '更新清单过大']) {
      assert.notEqual(translations.translateFor(key, locale), key, `${locale}: ${key}`)
    }
    assert.notEqual(translations.translateFor('更新清单 HTTP 429', locale), '更新清单 HTTP 429')
    assert.match(translations.translateFor('更新清单 HTTP 429', locale), /429/)
  }
})
