import assert from 'node:assert/strict'
import test from 'node:test'

/** 通知时长估算：真实源码直接执行（纯函数，不依赖 Vue / Naive）。 */
const { messageDuration, messageNeedsCloseButton, messageReadingUnits } = await import(
  '../src/notifyDuration.ts'
)

test('短提示保持基础时长', () => {
  // 「已保存」3 个汉字，远低于免费额度
  assert.equal(messageDuration('已保存'), 3000)
})

test('长提示按时长递增', () => {
  const short = messageDuration('已保存到备份目录')
  const long = messageDuration(
    'BepInEx 安装失败: 管理器安装目录与备份目录重叠，已停止备份以避免递归复制。请把管理器安装到游戏目录外。',
  )
  assert.ok(long > short, `长文本应更久：${long} vs ${short}`)
})

test('中文比等长拉丁文本更占时间', () => {
  // 中文按 1 单位、拉丁按 0.5 单位：同样的字符数，中文阅读量翻倍
  const chinese = messageReadingUnits('这是一个比较长的中文提示文本内容')
  const latin = messageReadingUnits('this is a fairly long latin message')
  assert.ok(chinese > latin, `中文 ${chinese} 应大于拉丁 ${latin}`)
})

test('空白不计数', () => {
  assert.equal(messageReadingUnits('   '), 0)
  assert.equal(messageReadingUnits('已保存'), messageReadingUnits(' 已 保 存 '))
})

test('超长文本有上限，不会永久停留', () => {
  const huge = '错'.repeat(5000)
  assert.equal(messageDuration(huge), 15000)
})

test('非字符串内容回退到 provider 默认值', () => {
  // VNode 渲染函数无法估阅读量，返回 undefined 让 Naive 用自己的 duration
  assert.equal(messageDuration(() => null), undefined)
  assert.equal(messageDuration(undefined), undefined)
  assert.equal(messageDuration({}), undefined)
})

test('只有长文本才加关闭按钮', () => {
  assert.equal(messageNeedsCloseButton('已保存'), false)
  assert.equal(messageNeedsCloseButton('错'.repeat(31)), true)
})

test('按码位计数，不因代理对被算成两个字符', () => {
  // 扩展区汉字（U+20000 起）在 JS 里 length 为 2，但阅读量仍是 1 个字
  assert.equal(messageReadingUnits('𠀀'), 1)
})
