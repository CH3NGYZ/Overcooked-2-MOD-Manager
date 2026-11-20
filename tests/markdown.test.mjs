import assert from 'node:assert/strict'
import test from 'node:test'
import { renderReadme } from '../src/markdown.ts'

test('installation notes render native links and Markdown structure', () => {
  const html = renderReadme([
    '# 安装说明', '', '> **首次使用须先阅读教程。**', '',
    '1. 打开 `F6` 面板。', '2. 阅读[教程](https://example.com/tutorial)。', '',
    '| 选项 | 默认值 |', '| --- | --- |', '| 菜谱 | 关闭 |', '',
    '```text', 'panel=F6', '```', '', '[配置](#configuration)', '', '[设置](#/settings)',
  ].join('\n'))
  assert.match(html, /<blockquote>\s*<p><strong>首次使用须先阅读教程。<\/strong><\/p>\s*<\/blockquote>/)
  assert.match(html, /<ol>\s*<li>打开 <code>F6<\/code> 面板。<\/li>/)
  assert.match(html, /<a href="https:\/\/example\.com\/tutorial">教程<\/a>/)
  assert.match(html, /<div class="markdown-table"><table>.*<th>默认值<\/th>.*<td>关闭<\/td>.*<\/table>\s*<\/div>/s)
  assert.match(html, /<pre><code[^>]*>.*<\/code><\/pre>/s)
  assert.match(html, /<a href="#configuration">配置<\/a>/)
  assert.match(html, /<a href="#\/settings">设置<\/a>/)
  assert.doesNotMatch(html, /target=|\*\*/)
})

test('raw HTML stays text, including inside Mermaid placeholders', () => {
  const source = 'A --> B["<b>A & B</b>"]'
  const html = renderReadme(`<b>原始 HTML</b>\n\n\`\`\`mermaid\n${source}\n\`\`\``)
  assert.doesNotMatch(html, /<\/?b>/)
  assert.match(html, /&lt;b&gt;原始 HTML&lt;\/b&gt;/)
  assert.match(html, /data-mermaid-source="A --&gt; B\[&quot;&lt;b&gt;A &amp; B&lt;\/b&gt;&quot;\]/)
  assert.match(html, /<code>A --&gt; B\[&quot;&lt;b&gt;A &amp; B&lt;\/b&gt;&quot;\]/)
})
