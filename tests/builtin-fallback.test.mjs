import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/** 源失败只显示错误；内置开关控制离线树，子源仍递归展开。 */

/*
 * 读到的是磁盘原文：Windows 上带 CRLF，所以所有匹配都先归一化。
 * 之前这里用 `\n}\n` 找函数结尾，在 CRLF 文件上永远找不到 ——
 * 测试会「失败得看起来像代码没修」，非常容易误判。
 */
const refresh = readFileSync(
  new URL('../src/composables/useModRepositoryRefresh.ts', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')
const resources = readFileSync(
  new URL('../src-tauri/resources/mods.toml', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

/** 当前策略：失败保留节点并清空卡片，只有显式选择内置开关才用内置源。 */
function sourceBody() {
  const start = refresh.indexOf('async function refreshSourceChain')
  return refresh.slice(start, refresh.indexOf('async function refreshSourceDocuments', start))
}

/*
 * 前提：内置索引确实不含仓库。
 * 这条断言看起来像在测数据，但它保证下面的回归测试有意义 ——
 * 如果哪天索引里直接写了仓库，这条测试会提醒我们重新评估结论。
 */
test('内置索引只声明子源、自身没有仓库', () => {
  assert.ok(
    resources.includes('[[repository_sources]]'),
    '内置索引应当声明子源',
  )
  assert.ok(
    !/^\[\[repositories\]\]/m.test(resources),
    '内置索引不应直接包含仓库（仓库在子源文件里）',
  )
})

test('源刷新复用递归加载，先发布父源再读取子源', () => {
  const body = sourceBody()
  assert.ok(body.indexOf('await reloadConfig()') < body.indexOf('refreshSourceChain(child.id'))
  assert.match(body, /sourceTree\.declare\(/)
  assert.match(body, /sourceTree\.settle\(sourceId\)/)
})

test('文档失败保留错误节点并停止递归，不恢复旧卡', () => {
  const body = sourceBody()
  assert.match(body, /if \(importErrors\.length\) \{ sourceTree\.settle\(sourceId, importErrors\[0\]\); return importErrors \}/)
  assert.ok(body.indexOf('const removed = sourceTree.begin') < body.indexOf('api.importRepositorySource'))
  assert.doesNotMatch(refresh, /fallbackToBuiltinIfOffline/)
})

test('子源失败合并上报，成功分支不被回退覆盖', () => {
  assert.match(sourceBody(), /return childErrors/)
  assert.doesNotMatch(refresh, /builtinFallback: true/)
})
