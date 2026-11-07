import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * Release 缓存的接线守则。
 *
 * 真实事故：用户按 F5 后日志里每条都是「跳过缓存=false」，却照样逐个请求
 * API —— 10 分钟缓存形同虚设。两层原因，都出在「整批刷新会删光仓库再重建」：
 *
 *   1. 清理仓库时把 `cached_releases` 一起删了；
 *   2. `apply_document` 逐源调用，每次结束无条件清空暂存表，于是只有第一个
 *      源能拿回缓存。
 *
 * 纯逻辑测试覆盖不到这两处（一个是调用顺序，一个是跨源的生命周期），
 * 所以这里直接对源码断言。
 */
const APPLY = readFileSync(new URL('../src-tauri/src/repository_sources/apply.rs', import.meta.url), 'utf8')
const SOURCES = readFileSync(new URL('../src-tauri/src/repository_sources.rs', import.meta.url), 'utf8')

test('清理源缓存时把 Release 缓存搬进暂存表，而不是直接丢弃', () => {
  assert.match(
    SOURCES,
    /staged_release_cache\.insert\(/,
    'clear_source_cache 必须暂存缓存，否则重建后永远命中不了',
  )
})

test('重建仓库时把暂存的缓存接回来', () => {
  assert.match(
    APPLY,
    /staged_release_cache\.remove\(previous_id\)/,
    'apply_document 必须按 repository_id 取回缓存',
  )
})

test('apply_document 结束时不无条件清空暂存表', () => {
  /*
   * 这是逐源调用的函数。早先在这里 `clear()`，第一个源应用完就把后面所有源的
   * 缓存扔了 —— 表现为「只有第一个仓库的缓存能用」。
   *
   * 允许 `remove`（认领单个条目），不允许对整个表 `clear`。
   */
  const clearCall = /config\.staged_release_cache\.clear\(\)/
  assert.ok(
    !clearCall.test(APPLY),
    'apply_document 不能清空整个暂存表（它是逐源调用的）',
  )
})

test('覆盖仓库定义时保留 Release 缓存字段', () => {
  /*
   * `import_bundle` 在 overwrite 分支会 `*existing = repository_from_export(...)`，
   * 造出来的是全新对象。如果连缓存一起覆盖，源文档每刷新一次缓存就没了。
   */
  assert.match(
    readFileSync(new URL('../src-tauri/src/config/bundle.rs', import.meta.url), 'utf8'),
    /let cached_releases = std::mem::take\(&mut existing\.cached_releases\)/,
    'overwrite 分支必须先备份再恢复缓存字段',
  )
})
