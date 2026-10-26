import assert from 'node:assert/strict'
import test from 'node:test'

/** 多彩名字生成器：真实源码直接执行（Node 24 类型擦除）。 */
const { DEFAULT_SIZE, MAX_NAME_BYTES, buildShortestRichText, isNameOverLimit, toUnits, utf16LeBytes } =
  await import('../src/colorfulName.ts')

function chars(text, color = '#000000', size = DEFAULT_SIZE) {
  return [...text].map((char) => ({ char, color, size }))
}

test('全默认样式不产生任何标签', () => {
  assert.equal(buildShortestRichText(chars('厨师')), '厨师')
})

test('连续同样式合并成一个标签对', () => {
  const input = [...chars('AB', '#ff0000'), ...chars('C', '#000000')]
  assert.equal(buildShortestRichText(input), '<color=#ff0000>AB</color>C')
})

test('颜色与字号可同时嵌套，且收尾顺序正确', () => {
  const input = chars('X', '#00ff00', 50)
  const output = buildShortestRichText(input)
  // 两种嵌套顺序都合法，取较短者；这里验证标签闭合且内容完整
  assert.match(output, /^<color=#00ff00><size=50>X<\/size><\/color>$|^<size=50><color=#00ff00>X<\/color><\/size>$/)
})

test('相邻不同样式逐字着色时不会丢掉字符', () => {
  const input = [
    { char: 'A', color: '#ff0000', size: DEFAULT_SIZE },
    { char: 'B', color: '#00ff00', size: DEFAULT_SIZE },
    { char: 'C', color: '#0000ff', size: DEFAULT_SIZE },
  ]
  const output = buildShortestRichText(input)
  assert.ok(output.includes('A') && output.includes('B') && output.includes('C'))
  assert.ok(output.length > 'ABC'.length)
})

test('输出长度按 UTF-16 字节统计，238 为上限', () => {
  assert.equal(utf16LeBytes('中文'), 4)
  assert.equal(isNameOverLimit('a'.repeat(119)), false)
  assert.equal(isNameOverLimit('a'.repeat(120)), true)
  assert.equal(MAX_NAME_BYTES, 238)
})

test('样式相同的连续字符只产生一个 unit', () => {
  assert.equal(toUnits(chars('AAA')).length, 1)
  assert.equal(toUnits([...chars('A'), ...chars('B', '#ffffff')]).length, 2)
})
