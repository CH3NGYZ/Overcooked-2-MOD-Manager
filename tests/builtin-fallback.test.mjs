import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 内置源回退：索引自身没有仓库时必须继续展开它声明的子源。
 *
 * 用户报的 bug：官方源读取失败、提示回退之后，界面上一片空白。
 *
 * 根因：内置索引 `src-tauri/resources/repositories.toml` **只声明子源**，
 * 它自己的 `repositories` 是空的（仓库在 `repositories-*.toml` 里）。
 * 原来的 `fallbackToBuiltinIfOffline` 只调了一次
 * `importRepositorySource(officialRoot, ..., true)` 就返回 —— 那是「加载索引」，
 * 不是「加载内置仓库」，于是回退「成功」但一个仓库都没有。
 *
 * 这个测试锁的是**调度形状**：回退分支必须复用 `refreshSourceChain`
 * 展开子源，与在线流程共用同一套递归。
 */

/*
 * 读到的是磁盘原文：Windows 上带 CRLF，所以所有匹配都先归一化。
 * 之前这里用 `\n}\n` 找函数结尾，在 CRLF 文件上永远找不到 ——
 * 测试会「失败得看起来像代码没修」，非常容易误判。
 */
const refresh = readFileSync(
  new URL('../src/composables/useRepositoryRefresh.ts', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')
const resources = readFileSync(
  new URL('../src-tauri/resources/repositories.toml', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

/** 取出 `fallbackToBuiltinIfOffline` 的函数体（到下一个顶层 `}` 为止）。 */
function fallbackBody() {
  const start = refresh.indexOf('async function fallbackToBuiltinIfOffline')
  assert.ok(start >= 0, '找不到 fallbackToBuiltinIfOffline')
  const rest = refresh.slice(start)
  const end = rest.indexOf('\n}\n')
  assert.ok(end > 0, '函数体没有正常结束')
  return rest.slice(0, end)
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

/*
 * 核心回归：回退必须递归子源。
 *
 * 允许两种实现：直接调用 `refreshSourceChain`，或对子源逐个
 * `importRepositorySource`。共同点是「必须对子源再发请求」——
 * 只加载索引的实现不满足任何一条。
 */
test('回退分支会继续展开子源，而不是只加载索引', () => {
  const body = fallbackBody()
  assert.ok(
    body.includes('refreshSourceChain') || body.includes('parent_id'),
    '回退后必须继续处理索引声明的子源',
  )
  assert.ok(
    body.includes('reloadConfig'),
    '索引应用后要重读配置，子源才会出现在列表里',
  )
})

/* 子源展开必须发生在索引成功之后：索引失败时没有子源可展开。 */
test('索引读取失败时提前返回，不继续展开子源', () => {
  const body = fallbackBody()
  const errorBranch = body.slice(
    body.indexOf('if (result.access_error)'),
    body.indexOf('},', body.indexOf('if (result.access_error)')),
  )
  assert.ok(
    errorBranch.includes('return'),
    '索引失败分支必须先返回',
  )
})

/* 回退也要报告子源失败，不能只在索引成功时清空错误。 */
test('子源失败会并入回退错误列表', () => {
  const body = fallbackBody()
  assert.ok(
    /errors\.push\(\s*\.\.\.|fallbackErrors: errors/.test(body),
    '子源的失败原因要合并进返回的错误列表',
  )
})
