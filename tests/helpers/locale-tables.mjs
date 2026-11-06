import { pathToFileURL } from 'node:url'
import ts from 'typescript'
import { TABLES } from '../../src/locale/catalog.ts'
import { LOCALE_CODES } from '../../src/locale/types.ts'
import { readText, sourceFiles } from './source-scan.mjs'

export function placeholders(text) {
  return [...new Set([...text.matchAll(/\{([A-Za-z_][A-Za-z0-9_]*)\}/g)].map(m => m[1]))].sort()
}

/** 与应用消费同一登记表，不再用正则猜测导出名、域或对象展开。 */
export function readLocaleTables(tables = TABLES) {
  const registered = new Map()
  const occurrences = new Map()
  const byDomain = new Map()
  const flat = new Set()
  const errors = []
  for (const { domain, flat: isFlat, messages } of tables) {
    for (const [key, entry] of Object.entries(messages)) {
      const tag = isFlat ? key : `${domain}.${key}`
      occurrences.set(tag, (occurrences.get(tag) ?? 0) + 1)
      registered.set(tag, entry)
      if (isFlat) flat.add(tag)
      else {
        if (!byDomain.has(domain)) byDomain.set(domain, new Set())
        byDomain.get(domain).add(key)
      }
      for (const code of LOCALE_CODES) {
        if (typeof entry[code] !== 'string' || !entry[code].trim()) errors.push(`${tag}: missing ${code}`)
        else if (JSON.stringify(placeholders(entry[code])) !== JSON.stringify(placeholders(entry['zh-CN'] ?? ''))) errors.push(`${tag}: placeholders ${code}`)
      }
    }
  }
  return { registered, byDomain, flat, errors, duplicates: Object.fromEntries([...occurrences].filter(([, count]) => count > 1).sort(([a], [b]) => a.localeCompare(b))) }
}

/** 扫描所有子目录的表声明，检查未被聚合/登记的孤立表和对象内重复字段。 */
export async function auditLocaleSources(root, tables = TABLES) {
  const errors = []
  const registeredEntries = new Map()
  for (const { messages } of tables) {
    for (const [key, entry] of Object.entries(messages)) {
      if (!registeredEntries.has(key)) registeredEntries.set(key, new Set())
      registeredEntries.get(key).add(entry)
    }
  }
  let declarations = 0
  for (const file of sourceFiles(root, ['.ts'])) {
    const ast = ts.createSourceFile(file, readText(file), ts.ScriptTarget.Latest, true)
    const declarationsInFile = ast.statements.flatMap(statement =>
      ts.isVariableStatement(statement) && statement.modifiers?.some(m => m.kind === ts.SyntaxKind.ExportKeyword)
        ? [...statement.declarationList.declarations].filter(d => d.type?.getText(ast) === 'DomainMessages')
        : [],
    )
    if (!declarationsInFile.length) continue
    const module = await import(pathToFileURL(file).href)
    const bindings = new Map(Object.entries(module))
    for (const statement of ast.statements) {
      if (!ts.isImportDeclaration(statement) || statement.importClause?.isTypeOnly) continue
      const imports = statement.importClause?.namedBindings
      if (!imports || !ts.isNamedImports(imports)) continue
      const names = imports.elements.filter(item => !item.isTypeOnly)
      if (!names.length) continue
      const imported = await import(new URL(statement.moduleSpecifier.text, pathToFileURL(file)).href)
      for (const item of names) bindings.set(item.name.text, imported[item.propertyName?.text ?? item.name.text])
    }
    function checkObject(node) {
      if (ts.isObjectLiteralExpression(node)) {
        const names = new Set()
        for (const prop of node.properties) {
          let keys = []
          if (ts.isSpreadAssignment(prop)) {
            const value = ts.isIdentifier(prop.expression) ? bindings.get(prop.expression.text) : undefined
            if (!value || typeof value !== 'object') errors.push(`${file}: unsupported spread ${prop.expression.getText(ast)}`)
            else keys = Object.keys(value)
          } else if (prop.name && (ts.isIdentifier(prop.name) || ts.isStringLiteral(prop.name))) {
            keys = [prop.name.text]
          }
          for (const name of keys) {
            if (names.has(name)) errors.push(`${file}: duplicate property ${name}`)
            names.add(name)
          }
        }
      }
      ts.forEachChild(node, checkObject)
    }
    for (const declaration of declarationsInFile) {
      declarations += 1
      checkObject(declaration)
      const messages = module[declaration.name.getText(ast)]
      for (const [key, entry] of Object.entries(messages)) {
        if (!registeredEntries.get(key)?.has(entry)) errors.push(`${file}: unregistered ${key}`)
      }
    }
  }
  return { declarations, errors }
}
