import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 两条线的接线守卫：地图管理页（集卡片 → 随包插件）与 MOD 管理页
 * （独立卡 / 包外壳 → 包内子 MOD、随包依赖）。
 *
 * 两个页面的数据源不同、详情区选择器也不同（地图页与独立卡是 `.card-detail`，
 * 包外壳是 `.package-body`），但展开 / 收起的**实现只有一份**
 * （`expandScroll.ts` + `useCardExpansion`），差异只允许出现在调用点传的选择器与键上。
 *
 * 几何单测（`expand-scroll.test.mjs`）只保证算式对；页面把调用点改坏了照样全绿 ——
 * 例如 MOD 页又拆回两套展开状态（一张独立卡与一个包能同时摊开，互斥就漏了）、
 * 包内子卡不再传包 key（掉进外层状态）、外层展开时没有先量上一张的收缩量
 * （被动收起会把下方卡片标题拽向视口顶）、点空白把包外壳一起收掉、地图页把
 * 「先认领二级卡」删掉（点插件把整张集收掉）。这几条只能靠源码文本锁住。
 */
const read = (name) => readFileSync(new URL(`../src/views/${name}`, import.meta.url), 'utf8')
const modsLocal = read('ModsLocalView.vue')
const levels = read('LevelsView.vue')

/** 抓出某个函数体（从 `function 名(` 到下一个顶格 `}`），用来判断顺序。 */
function bodyOf(source, name) {
  const start = source.indexOf(`function ${name}(`)
  assert.ok(start >= 0, `找不到函数 ${name}`)
  const end = source.indexOf('\n}\n', start)
  assert.ok(end > start, `函数 ${name} 没有结束`)
  return source.slice(start, end)
}

/** 页面里的展开状态实例个数：两层卡片只许有一份状态。 */
function expansionInstances(source) {
  return (source.match(/useCardExpansion\(/g) ?? []).length
}

test('两条线都复用同一份展开滚动实现，没有第二份 scrollIntoView', () => {
  for (const source of [modsLocal, levels]) {
    assert.match(source, /from '\.\.\/expandScroll'/)
    assert.match(source, /from '\.\.\/composables\/useCardExpansion'/)
    assert.doesNotMatch(source, /scrollIntoView/)
    assert.doesNotMatch(source, /requestAnimationFrame/, '页里不许自己写缓动')
  }
})

test('MOD 页：展开状态只有一套，外层单开（独立卡与包外壳算同一级）', () => {
  assert.equal(expansionInstances(modsLocal), 1,
    '两套展开状态时一张独立卡与一个包能同时摊开，互斥就漏了')
  assert.match(modsLocal, /const expansion = useCardExpansion\(\{ singleOuter: true \}\)/)
  assert.doesNotMatch(modsLocal, /cardExpansion|packageExpansion/)
})

test('MOD 页：独立卡与包外壳共用一套外层键，卡片带滚动定位锚点', () => {
  assert.match(modsLocal, /const MOD_CARD_PREFIX = 'mod:'/)
  assert.match(modsLocal, /function modCardKey\(dllFile: string\)/)
  // 外层键必须落在 DOM 上：被动收起时要按它找回上一张卡量收缩量。
  assert.match(modsLocal, /:data-expand-key="modCardKey\(mod\.dll_file\)"/)
  assert.match(modsLocal, /:data-expand-key="pkg\.key"/)
  assert.match(modsLocal, /:expanded="expansion\.isOuterOpen\(modCardKey\(mod\.dll_file\)\)"/)
  assert.match(modsLocal, /expanded: expansion\.isOuterOpen\(pkg\.key\)/)
})

test('MOD 页：外层展开统一走 expandWithScroll，先量收缩量再改状态、滚动只滚一次', () => {
  const body = bodyOf(modsLocal, 'expandWithScroll')
  assert.match(body, /naturalContentHeight/)
  assert.match(body, /collapsing:/)
  assert.match(body, /scrollExpandedCardIntoView/)
  assert.ok(body.indexOf('naturalContentHeight') < body.indexOf('  open()'),
    'shrink 必须在改状态之前量：外层收起连带清内层，改完再量会算少（实测 247px 量级）')
  // 展开后的滚动都等 nextTick：折叠时不渲染插槽，提前查是 null。
  assert.ok(body.indexOf('nextTick') < body.indexOf('scrollExpandedCardIntoView'))

  // 三条展开路径（独立卡点击、包外壳点击、跨页 focus）都走同一个入口。
  assert.match(modsLocal, /expandWithScroll\(card,\s*key,\s*'\.card-row',\s*'\.card-detail'/)
  assert.match(modsLocal, /expandWithScroll\(card,\s*node\.key,\s*'\.package-head',\s*'\.package-body'/)
  assert.match(modsLocal, /expandWithScroll\(null,\s*hitPackage\.key,/)
  assert.match(modsLocal, /expandWithScroll\(null,\s*key,\s*'\.card-row'/)
})

test('MOD 页：按键找卡片复用 expandScroll 的 cardByExpandKey（不自己写一份选择器）', () => {
  assert.match(modsLocal, /cardByExpandKey/)
  assert.ok(!modsLocal.includes('`[data-expand-key='),
    '按键找卡片只许用 expandScroll.cardByExpandKey，页面里再写一遍就是第二份实现')
})

test('MOD 页：包内子卡与随包依赖带包 key，落进内层状态而不是外层', () => {
  // 模板：包体里的每一张子卡（含 is_dependency 的依赖行）都把 pkg.key 传给点击处理。
  assert.match(modsLocal, /@click="onCardClick\(\$event, mod, pkg\.key\)"/)
  assert.match(modsLocal, /:expanded="expansion\.isInnerOpen\(pkg\.key, mod\.dll_file\)"/)
  assert.match(modsLocal, /expansion\.openInner\(packageKey, mod\.dll_file\)/)
  assert.match(modsLocal, /expansion\.closeInner\(packageKey, mod\.dll_file\)/)
  // 依赖行与子 MOD 同一张卡片、同一条路径，只是关掉开关与标签文案。
  assert.match(modsLocal, /:switchable="!mod\.is_dependency"/)
})

test('MOD 页：收起外壳连带清内层，点空白只收独立卡与内层、不许收盘外壳', () => {
  const toggle = bodyOf(modsLocal, 'togglePackage')
  // closeOuter 在 useCardExpansion 里连带清前缀内层键，页面只许调它、不许自己删键。
  assert.match(toggle, /collapseWithScroll\(card,\s*'\.package-body',\s*\(\)\s*=>\s*expansion\.closeOuter\(node\.key\)\)/)
  assert.doesNotMatch(modsLocal, /innerKeys\.value\s*=\s*\[/)

  const doc = bodyOf(modsLocal, 'onDocumentClick')
  assert.match(doc, /expansion\.closeOuterWhere\(/)
  assert.match(doc, /startsWith\(MOD_CARD_PREFIX\)/)
  assert.match(doc, /expansion\.closeAllInner\(\)/)
  assert.doesNotMatch(doc, /closeAll\(\)/, '点空白不许把包外壳一起收掉')
})

test('地图页：外层同样单开，且与 MOD 页一样只有一个展开状态实例', () => {
  assert.equal(expansionInstances(levels), 1)
  assert.match(levels, /useCardExpansion\(\{ singleOuter: true \}\)/)
})

test('地图页：点空白先认领二级卡，命中只收这一张、不动外层的集', () => {
  const doc = bodyOf(levels, 'onDocumentClick')
  assert.ok(doc.indexOf('nestedPluginCard(target)') < doc.indexOf('levelExpansion.closeOuter(dirName)'),
    '必须先在点空白里认领插件卡，否则点插件详情会把整张集收掉')
  assert.match(levels, /:data-plugin-of="set\.dir_name"/)
  assert.match(levels, /:data-plugin-file="plugin\.file_name"/)
  const collapse = bodyOf(levels, 'collapsePluginCard')
  assert.match(collapse, /levelExpansion\.closeInner\(dirName, fileName\)/)
  assert.doesNotMatch(collapse, /closeOuter/)
})
