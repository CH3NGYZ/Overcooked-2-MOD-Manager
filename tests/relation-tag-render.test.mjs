import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 卡头「缺依赖」标签的渲染接线。
 *
 * `relationCardTag` 的纯逻辑由 `mod-relations.test.mjs` 覆盖（含未安装场景）。
 * 但逻辑正确不等于**页面会用到它** —— 视图里可能是 `v-if="installed && tag"`，
 * 或者根本没调用。这类接线只能扫模板。
 */
const VIEW = readFileSync(new URL('../src/views/ModsOnlineView.vue', import.meta.url), 'utf8')

test('安装页的卡头标签调用纯函数，而不是自己再判一次「装没装」', () => {
  assert.match(
    VIEW,
    /relationCardTag\(\s*unmetRelation\(repo\)\s*,\s*hasLocalInstall\(repo\)\s*\)/,
    '卡头标签必须走 relationCardTag，否则两处判定口径会分叉',
  )
})

test('模板里的标签没有额外的「已安装」门控', () => {
  /*
   * v-if 只允许判 `relationTag(repo)` 是否有值。再加 `&& hasLocalInstall(repo)`
   * 就等于把「未安装也显示缺依赖」这条规则在模板层推翻 —— 后端逻辑改了也没用。
   */
  const tagDirectives = [...VIEW.matchAll(/v-if="[^"]*relationTag\(repo\)[^"]*"/g)].map((m) => m[0])
  assert.ok(tagDirectives.length > 0, '没找到标签的 v-if，选择器可能失效')
  for (const directive of tagDirectives) {
    assert.ok(
      !/hasLocalInstall/.test(directive),
      `模板不应再按「已安装」门控：${directive}`,
    )
  }
})
