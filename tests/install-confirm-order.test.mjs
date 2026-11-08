import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 安装入口的确认顺序：**预发布确认 → 关系检查（依赖/冲突/兼容）→ 下载**。
 *
 * 为什么顺序不能反：关系检查（`prepareInstall`）会真的卸载冲突 MOD、
 * 装依赖，是有副作用的一步。如果把预发布确认排在它后面，用户点了一个
 * 测试版、管理器先把冲突的 MOD 卸了，才问「这是预览版，确定要装吗」——
 * 用户一取消，环境已经被改动过。
 *
 * `useModRepositoryPreview.ts` 依赖 Vue 与 Naive UI，无法在 Node 里直接执行，
 * 所以这里对真实源码做结构性断言（与 tests/install-language.test.mjs 同风格）。
 * 运行时行为由 tools/verify-install-plan.mjs 的弹窗链路覆盖。
 *
 * 读取的真实文件在 Windows 上是 CRLF，先归一化再按位置比较 ——
 * 否则 indexOf 找到的位置区间可能被 `\r` 干扰。
 */
const preview = readFileSync(
  new URL('../src/composables/useModRepositoryPreview.ts', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

/** 取 `previewModRepository` 的函数体（到函数结束的顶层 `}` 为止）。 */
function body() {
  const start = preview.indexOf('async function previewModRepository')
  assert.ok(start >= 0, '找不到 previewModRepository')
  const rest = preview.slice(start)
  const end = rest.indexOf('\n  }\n')
  assert.ok(end > 0, '函数体没有正常结束')
  return rest.slice(0, end)
}

test('预发布确认排在关系检查之前', () => {
  const source = body()
  const prereleaseAt = source.indexOf('if (prerelease)')
  const prepareAt = source.indexOf('options.prepareInstall')

  assert.ok(prereleaseAt >= 0, '应当有预发布确认分支')
  assert.ok(prepareAt >= 0, '应当有关系检查调用')
  assert.ok(
    prereleaseAt < prepareAt,
    `预发布确认必须在关系检查之前（prerelease@${prereleaseAt}, prepare@${prepareAt}）`,
  )
})

test('关系检查排在下载之前', () => {
  const source = body()
  const prepareAt = source.indexOf('options.prepareInstall')
  const downloadAt = source.indexOf('api.installModRepository')

  assert.ok(prepareAt >= 0 && downloadAt >= 0)
  assert.ok(
    prepareAt < downloadAt,
    `关系检查必须在下载之前（prepare@${prepareAt}, download@${downloadAt}）`,
  )
})

test('用户在预发布确认里取消时直接返回，不进入关系检查', () => {
  const source = body()
  const prereleaseBlock = source.slice(
    source.indexOf('if (prerelease)'),
    source.indexOf('options.prepareInstall'),
  )
  // 取消路径必须 return：否则会继续往下走，等于「确认了」
  assert.ok(
    /if \(!confirmed\) return/.test(prereleaseBlock),
    '预发布确认取消后必须 return',
  )
})

test('关系检查未通过时直接返回，不进入下载', () => {
  const source = body()
  const prepareBlock = source.slice(
    source.indexOf('if (options.prepareInstall)'),
    source.indexOf('api.installModRepository'),
  )
  assert.ok(
    /if \(!ready\) return/.test(prepareBlock),
    '关系检查未通过时不能让主包继续安装',
  )
})
