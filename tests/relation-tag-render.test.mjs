import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 卡头关系标签的渲染接线。
 *
 * `dependencyTagFor` 的纯逻辑由 `mod-relations.test.mjs` 覆盖（含未安装场景）。
 * 这里额外守住每处共享卡片的 prop 接线，实际点击路径有独立测试。
 *
 * 卡片模板已经下沉到 `OnlineCard.vue`（MOD 安装页与地图下载页共用），
 * 所以这条测试的落点也跟着分成两半：
 *
 *   · 视图侧：把物理已装对象的关系交给 `dependencyTagFor` 汇总；
 *   · 组件侧：模板只按传入的 `dependency` 渲染，不额外门控。
 *
 * 任何一半失效，规则都会被在渲染层悄悄推翻，而后端/纯函数测试仍然全绿。
 */
const VIEW = readFileSync(new URL('../src/views/ModsOnlineView.vue', import.meta.url), 'utf8')
const CARD = readFileSync(new URL('../src/components/OnlineCard.vue', import.meta.url), 'utf8')

test('安装页按物理 DLL 读取已装评估，并把汇总交给共享判定', () => {
  assert.match(
    VIEW,
    /dependencyTagFor\(installed\.flatMap\(mod\s*=>\s*relationStatusesForSubject\(\{ dll_file: mod\.dll_file \}, localMods\)\), installed\.length > 0\)/,
    '卡头标签必须走 dependencyTagFor，否则四个页面的判定口径会分叉',
  )
})

test('卡片组件只按传入的 dependency 渲染，不额外按「已安装」门控', () => {
  /*
   * 组件拿到的 `dependency` 已经是判定结果（`cardDependency` 里由
   * `dependencyTagFor` 产出）。组件没有必要再按自己的安装状态重复推导。
   */
  const tagDirectives = [...CARD.matchAll(/v-if="[^"]*dependency[^"]*"/g)].map((m) => m[0])
  assert.ok(tagDirectives.length > 0, '组件里没找到依赖标签的 v-if，选择器可能失效')
  for (const directive of tagDirectives) {
    assert.ok(
      !/installed|hasLocalInstall/.test(directive),
      `组件不应再按「已安装」门控：${directive}`,
    )
  }
})

test('安装页把关系判定结果喂给共享卡片，而不是让组件自己算', () => {
  /*
   * 组件不知道 `ModRepository` 是什么，判定必须留在视图层。
   * 少了这个接线，卡片上的关系提醒会消失，而纯函数测试仍然通过。
   *
   * 断言的是「每一张 OnlineCard 都传了 dependency」：页面上有源码树卡片和
   * 社区卡片两处，只查一次出现会让漏改的那一半静默通过。
   */
  const cards = [...VIEW.matchAll(/<OnlineCard\b[\s\S]*?\/>/g)].map((m) => m[0])
  assert.ok(cards.length >= 2, `应至少有两处 OnlineCard，实际 ${cards.length}`)
  for (const [index, card] of cards.entries()) {
    assert.match(
      card,
      /:dependency="cardDependency\(repo\)"/,
      `第 ${index + 1} 张卡片没有接到 cardDependency`,
    )
  }
  assert.match(VIEW, /function cardDependency\(/, '视图应定义 cardDependency 适配函数')
})
