import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { pathToFileURL } from 'node:url'
import test from 'node:test'
import { collectRustTags, sourceFiles } from './helpers/source-scan.mjs'
import { auditLocaleSources, readLocaleTables } from './helpers/locale-tables.mjs'

function fixture(t) {
  const dir = mkdtempSync(join(tmpdir(), 'oc2-source-scan-'))
  t.after(() => rmSync(dir, { recursive: true, force: true }))
  mkdirSync(join(dir, 'nested', 'domain'), { recursive: true })
  return dir
}

const entry = { 'zh-CN': '{name}', 'zh-TW': '{name}', en: '{name}', ko: '{name}', ja: '{name}' }

test('Rust 递归扫描保留嵌套路径并发现未登记 tag', t => {
  const dir = fixture(t)
  writeFileSync(join(dir, 'top.rs'), '"mods.dllNotFound"\r\n')
  writeFileSync(join(dir, 'nested/domain/child.rs'), '"mods.splittingMissing?name=x"\r\n')
  writeFileSync(join(dir, 'nested/ignored.txt'), '"mods.ignored"')
  const tags = collectRustTags(dir)
  assert.equal(sourceFiles(dir, ['.rs']).length, 2)
  assert.deepEqual(tags.get('mods.splittingMissing'), ['nested/domain/child.rs:1'])
  const { registered } = readLocaleTables()
  const missing = [...tags.keys()].filter(tag => !registered.has(tag))
  assert.deepEqual(missing, ['mods.splittingMissing'])
})

test('递归语言表审计发现未聚合的子域，并接受同一对象的聚合', async t => {
  const dir = fixture(t)
  const file = join(dir, 'nested/domain/messages.ts')
  writeFileSync(file, `export const nested: DomainMessages = { greeting: ${JSON.stringify(entry)} }\r\n`)
  const { nested } = await import(pathToFileURL(file).href)
  const orphan = await auditLocaleSources(dir, [])
  assert.equal(orphan.declarations, 1)
  assert.match(orphan.errors.join('\n'), /unregistered greeting/)
  assert.deepEqual((await auditLocaleSources(dir, [{ domain: 'tools', messages: { ...nested } }])).errors, [])
})

test('源审计在执行对象被覆盖前检测重复 key', async t => {
  const dir = fixture(t)
  const file = join(dir, 'nested/domain/duplicate.ts')
  writeFileSync(file, `export const duplicate: DomainMessages = { x: ${JSON.stringify(entry)}, x: ${JSON.stringify(entry)} }`)
  const { duplicate } = await import(pathToFileURL(file).href)
  const audit = await auditLocaleSources(dir, [{ domain: 'tools', messages: duplicate }])
  assert.match(audit.errors.join('\n'), /duplicate property x/)
})

test('源审计检测聚合展开覆盖，即使两个子表复用同一文案对象', async t => {
  const dir = fixture(t)
  const child = join(dir, 'nested/domain/shared.ts')
  writeFileSync(child, `const entry = ${JSON.stringify(entry)}
export const first: DomainMessages = { x: entry }
export const second: DomainMessages = { x: entry }`)
  const file = join(dir, 'index.ts')
  writeFileSync(file, `import { first, second } from './nested/domain/shared.ts'
export const merged: DomainMessages = { ...first, ...second }`)
  const { merged } = await import(pathToFileURL(file).href)
  const audit = await auditLocaleSources(dir, [{ domain: 'tools', messages: merged }])
  assert.match(audit.errors.join('\n'), /duplicate property x/)
})

test('登记检查可区分 flat 与 domain，并报告新增重复、缺语和占位符漂移', () => {
  const result = readLocaleTables([
    { domain: 'tools', messages: { greeting: entry } },
    { flat: true, messages: { 'tools.greeting': { ...entry, ja: '', en: '{other}' } } },
  ])
  assert.equal(result.registered.size, 1)
  assert.deepEqual(result.duplicates, { 'tools.greeting': 2 })
  assert.deepEqual(result.errors, ['tools.greeting: placeholders en', 'tools.greeting: missing ja'])
})
