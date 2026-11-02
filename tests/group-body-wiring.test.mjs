import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 折叠容器的渲染边界守卫。
 *
 * `n-collapse` 的动态 prop 只有 `expanded-names`：同一组里删掉一张卡时组集合
 * 没变、`groupsExpanded` computed 命中缓存返回同一个数组引用，
 * `shouldUpdateComponent` 判定 props 没变，就把整棵子树跳过 —— 插槽闭包不
 * 重跑、组内 `v-for` 停在旧数组上。用户报的「移除关卡集后列表不刷新」就是
 * 这条边界；删空一个组时组键变了才会更新，所以只有那一种情况看起来正常。
 *
 * 修法是结构性的：组标题与组内容各住一个组件，自己 inject 上下文、自己算
 * 本组数据，父级被跳过也照样重渲。这条只能靠源码文本锁住 —— 把卡片 `v-for`
 * 写回页面插槽，`npm run build` 与几何单测都还是绿的，只有真机点删除才看得出。
 */
const read = (rel) => readFileSync(new URL(`../src/${rel}`, import.meta.url), 'utf8')

const levels = read('views/LevelsView.vue')
const modsLocal = read('views/ModsLocalView.vue')
const levelBody = read('components/LevelSetGroupBody.vue')
const modBody = read('components/ModGroupBody.vue')
const levelHeader = read('components/LevelSetGroupHeader.vue')
const modHeader = read('components/ModGroupHeader.vue')
const itemCard = read('components/LocalItemCard.vue')

const LINES = [
  {
    label: '地图管理页',
    page: levels,
    body: levelBody,
    header: levelHeader,
    bodyTag: 'LevelSetGroupBody',
    headerTag: 'LevelSetGroupHeader',
    ctxKey: 'LEVEL_SET_GROUP_CTX',
    ctxType: 'LevelSetGroupContext',
    dataKey: 'sets',
    oldLoop: 'v-for="set in node.sets"',
    newLoop: 'v-for="set in sets"',
  },
  {
    label: 'MOD 管理页',
    page: modsLocal,
    body: modBody,
    header: modHeader,
    bodyTag: 'ModGroupBody',
    headerTag: 'ModGroupHeader',
    ctxKey: 'MOD_GROUP_CTX',
    ctxType: 'ModGroupContext',
    dataKey: 'packages',
    oldLoop: 'v-for="pkg in node.packages"',
    newLoop: 'v-for="pkg in packages"',
  },
]

/** 递归收集 `src/` 下的源码，用来证明绕行模块没有被任何文件再 import。 */
function walk(dir) {
  const out = []
  for (const entry of readdirSync(new URL(`../${dir}/`, import.meta.url), { withFileTypes: true })) {
    const rel = `${dir}/${entry.name}`
    if (entry.isDirectory()) out.push(...walk(rel))
    else if (/\.(vue|ts|mts)$/.test(entry.name)) out.push(rel)
  }
  return out
}

test('折叠插槽只挂组件：卡片 v-for 必须在组内容组件自己的模板里', () => {
  for (const line of LINES) {
    assert.ok(
      !line.page.includes(line.oldLoop),
      `${line.label} 的折叠槽里又直接写了卡片 v-for：n-collapse 跳过子树时它不会更新`,
    )
    assert.match(line.page, new RegExp(`<${line.bodyTag} :group-key="node\\.key" />`))
    assert.match(line.page, new RegExp(`<${line.headerTag} :group-key="node\\.key" />`))
    // 标题也必须独立成组件：`#header` 与内容槽在同一棵被跳过的子树里。
    const headerSlot = line.page.indexOf('<template #header>')
    assert.ok(headerSlot > 0, `${line.label} 的折叠项少了 #header 插槽`)
    assert.ok(
      line.page.slice(headerSlot, headerSlot + 200).includes(`<${line.headerTag}`),
      `${line.label} 的组标题必须渲染成组件，不能直接写在 #header 插槽里`,
    )
    assert.ok(line.body.includes(line.newLoop), `${line.label} 的组内容组件里没有卡片 v-for`)
    assert.match(
      line.body,
      new RegExp(`const ${line.dataKey} = computed\\(\\s*\\(\\) => ctx\\.groups\\.find`),
      '组内容必须按 groupKey 从上下文现算，不能靠父级把数组当 props 送下来',
    )
  }
})

test('组标题与组内容都经 provide/inject 拿上下文', () => {
  for (const line of LINES) {
    assert.match(line.page, new RegExp(`provide\\(\\s*\\n\\s*${line.ctxKey}`))
    assert.match(line.body, new RegExp(`inject\\(${line.ctxKey}\\)`))
    assert.match(line.header, new RegExp(`inject\\(${line.ctxKey}\\)`))
    assert.match(line.header, /ctx\.groups\.find/, '组名要随数据/语言即时变')
  }
})

test('上下文里的跨渲染值都是 ref/computed，页面用 reactive 下发', () => {
  const source = read('components/levelSetGroupContext.ts')
  assert.match(source, /groups: ComputedRef</)
  assert.match(source, /toolsMinWidth: ComputedRef</)
  assert.match(source, /InjectionKey</)
  for (const line of LINES) {
    // 页面把 ref/computed 装进 reactive() 再 provide，组件模板里才能直接读 ctx.xxx。
    assert.ok(
      line.page.includes(`reactive<${line.ctxType}>(`),
      `${line.label} 必须用 reactive() 下发上下文，否则模板里读到的是 ref 对象`,
    )
  }
})

test('批量启停的开关快照绕行已整体删除', () => {
  const files = walk('src')
  assert.ok(!files.includes('src/cardSwitchState.ts'), 'cardSwitchState.ts 应已删除')
  for (const file of files) {
    const text = read(file.slice('src/'.length))
    assert.ok(!text.includes('cardSwitchState'), `${file} 还在引用 cardSwitchState`)
    assert.ok(!text.includes('cardSwitchOn'), `${file} 还在用 cardSwitchOn 包裹开关值`)
    assert.ok(!text.includes('switch-key'), `${file} 还在传 switch-key`)
    assert.ok(!/switchKey/.test(text), `${file} 还留着 switchKey`)
  }
  // 开关值直接就是数据本身，Naive 的 n-switch 靠值变化播过渡。
  assert.match(itemCard, /<n-switch v-if="switchable !== false"[^>]*:value="enabled"/)
})
