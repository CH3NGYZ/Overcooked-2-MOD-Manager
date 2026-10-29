import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 左下角通知必须让出侧栏宽度。
 *
 * 真实事故：原先只写 `body > .n-message-container { left: calc(...) }`，
 * 特指度 0,1,1；而 Naive 自己的定位规则是
 * `.n-message-container.n-message-container--bottom-left { left: 12px }`，
 * 特指度 0,2,0 —— 更高，于是我们的规则被静默忽略，通知一直压在侧栏下方，
 * 而 computed style 看上去「有写 left」但值仍是 12px。
 *
 * 这条测试钉住两点：选择器必须用双类（≥0,2,0）+ body 前缀，且必须覆盖
 * 所有靠左的位置变体。
 */
const css = readFileSync(
  new URL('../src/styles/global.css', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

/**
 * 去掉注释后的样式表。
 *
 * 说明性注释里会引用 Naive 的原始规则（`left: 12px`），直接在原文上匹配
 * 会把注释当成生效代码。剥掉再判断才反映真实规则。
 */
const code = css.replace(/\/\*[\s\S]*?\*\//g, '')

/** 取出所有针对 message container 的规则块。 */
function messageContainerRules() {
  const blocks = []
  const re = /([^{}]*\.n-message-container[^{}]*)\{([^}]*)\}/g
  let match
  while ((match = re.exec(code))) {
    blocks.push({ selectors: match[1].trim(), body: match[2].trim() })
  }
  return blocks
}

test('通知容器的定位规则覆盖侧栏宽度', () => {
  const blocks = messageContainerRules()
  assert.ok(blocks.length > 0, '应当有针对 .n-message-container 的规则')

  const leftRules = blocks.filter((block) => /(^|;)\s*left\s*:/.test(block.body))
  assert.ok(leftRules.length > 0, '应当有设置 left 的规则')

  for (const rule of leftRules) {
    assert.match(
      rule.body,
      /var\(--oc2-rail-width\)/,
      `left 必须基于侧栏宽度：${rule.selectors}`,
    )
    // 双类：压不过 Naive 的 `.n-message-container.n-message-container--*`
    const classCount = (rule.selectors.match(/\.n-message-container/g) ?? []).length
    assert.ok(
      classCount >= 2,
      `选择器特指度不足，压不过 Naive 的双类规则：${rule.selectors}`,
    )
  }

  // 所有靠左的位置变体都要覆盖，否则该位置的提示仍压在侧栏下
  for (const variant of ['--bottom-left', '--top-left', '--bottom', '--top']) {
    assert.ok(
      leftRules.some((rule) => rule.selectors.includes(variant)),
      `必须覆盖 ${variant} 变体`,
    )
  }
})

test('不存在单类选择器的 left 规则（那正是失效的写法）', () => {
  for (const rule of messageContainerRules()) {
    if (!/(^|;)\s*left\s*:/.test(rule.body)) continue
    const classCount = (rule.selectors.match(/\.n-message-container/g) ?? []).length
    assert.ok(
      classCount >= 2,
      `单类选择器 + left 会被 Naive 覆盖，属于已知失效写法：${rule.selectors}`,
    )
  }
})
