import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

const view = readFileSync(new URL('../src/components/InstalledModsView.vue', import.meta.url), 'utf8')
const expression = view.match(/class="mod-version mono">\{\{\s*(.*?)\s*\}\}/)?.[1]
assert.ok(expression, 'Use the actual installed MOD version display expression')
const displayVersion = new Function('mod', 't', `return ${expression}`)
const source = ts.createSourceFile('i18n.ts', readFileSync(new URL('../src/i18n.ts', import.meta.url), 'utf8'), ts.ScriptTarget.Latest, true)
const names = new Set(['entries', 'zhCnEntries', 'translateFor'])
const declarations = source.statements.filter((statement) => (
  ts.isVariableStatement(statement) && statement.declarationList.declarations.some((node) => names.has(node.name.getText(source)))
) || (ts.isFunctionDeclaration(statement) && names.has(statement.name?.text))).map((statement) => statement.getText(source))
const { outputText } = ts.transpileModule(`${declarations.join('\n')}\nexport { translateFor }`, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
})
const { translateFor } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`)

test('local MODs display only their scanned version or localized unknown version', () => {
  for (const [locale, unknown] of [
    ['zh-CN', '\u7248\u672c\u672a\u77e5'], ['zh-TW', '\u7248\u672c\u672a\u77e5'],
    ['en', 'Unknown version'], ['ko', '\uc54c \uc218 \uc5c6\ub294 \ubc84\uc804'],
  ]) {
    const t = (key) => translateFor(key, locale)
    for (const version of [undefined, null, '']) assert.equal(displayVersion({ version }, t), unknown)
    for (const version of ['0.10.0', '0.0.0']) assert.equal(displayVersion({ version }, t), version)
  }
})
