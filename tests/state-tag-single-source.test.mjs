import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 可点状态标签（`.state-tag--link`）只能有一份定义。
 *
 * 状态标签大多渲染在 `LocalItemCard` 的 `status` 插槽里，插槽内容由**父页面**
 * 编译、带的是页面的 scoped 作用域，组件自己的 scoped 规则命中不到它。于是每
 * 个用到插槽的页面都在自己的 `<style scoped>` 里补一条 —— 曾经同时存在
 * DependencyTable / OnlineCard / ModGroupBody / LevelsView / LevelSetGroupBody /
 * ModsLocalView 六份，亮度在 1.15 与 1.25 之间飘，地图页甚至一份都没有：
 * 同一个标签在 MOD 页有手型和悬停描边，在地图页点了毫无反馈。
 *
 * 现在只在 `src/styles/global.css` 里定义一次，其余地方只挂类名。这条守卫
 * 靠源码文本锁住：重新抄一份 scoped 副本时，`npm run build` 和真机截图都可能
 * 看不出差别（值恰好相同就完全静默），只有文本能立刻判红。
 */
const SRC = new URL('../src/', import.meta.url)
const read = (rel) => readFileSync(new URL(rel, SRC), 'utf8')

function walk(dir) {
  return readdirSync(new URL(dir, SRC), { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? walk(`${dir}${entry.name}/`)
      : [`${dir}${entry.name}`])
}

const files = walk('').filter((rel) => rel.endsWith('.vue') || rel.endsWith('.css'))

test('global.css 是可点状态标签的唯一定义处', () => {
  const css = read('styles/global.css')
  assert.equal((css.match(/^\.state-tag--link\.n-tag\s*\{/gm) ?? []).length, 1)
  assert.equal((css.match(/^\.state-tag--link\.n-tag:hover\s*\{/gm) ?? []).length, 1)
})

test('页面与组件里不再保留 .state-tag--link 的样式副本', () => {
  const copies = []
  for (const rel of files) {
    if (rel === 'styles/global.css') continue
    read(rel).split('\n').forEach((line, index) => {
      if (/^\s*\.state-tag--link[\s{,:]/.test(line)) copies.push(`${rel}:${index + 1} ${line.trim()}`)
    })
  }
  assert.deepEqual(copies, [])
})
