import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'
import { JSDOM } from 'jsdom'

const { window } = new JSDOM('', { url: 'http://localhost:5178' })
for (const name of ['window', 'document', 'Element', 'HTMLElement', 'MouseEvent', 'DOMParser', 'XMLSerializer', 'location']) globalThis[name] = name === 'window' ? window : window[name]

const source = readFileSync(new URL('../src/markdown.ts', import.meta.url), 'utf8')
let { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
})
for (const name of ['dompurify', 'marked', 'github-slugger']) outputText = outputText.replaceAll(`from '${name}'`, `from '${import.meta.resolve(name)}'`)
const { repositoryMarkdownBaseUrl, markdownResourceUrl, renderMarkdown, markdownLinkUrl } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`)
let enhancements = ts.transpileModule(readFileSync(new URL('../src/markdownEnhancements.ts', import.meta.url), 'utf8'), {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
}).outputText
for (const name of ['dompurify', 'highlight.js/lib/common', 'mermaid']) {
  const url = import.meta.resolve(name)
  enhancements = enhancements.replaceAll(`from '${name}'`, `from '${url}'`).replaceAll(`import('${name}')`, `import('${url}')`)
}
const { enhanceMarkdown } = await import(`data:text/javascript;base64,${Buffer.from(enhancements).toString('base64')}`)

function documentFrom(source) {
  const root = document.createElement('div')
  root.className = 'markdown-body'
  root.innerHTML = renderMarkdown(source, 'https://github.com/Owner/Mod/raw/refs/heads/develop/README.md')
  return root
}

test('Markdown bases use known repository branches and never guess main or master', () => {
  const repo = { provider: 'github', username: 'Owner', repository: 'Mod', branch: 'develop' }
  assert.equal(repositoryMarkdownBaseUrl(repo), 'https://github.com/Owner/Mod/raw/refs/heads/develop/README.md')
  assert.equal(repositoryMarkdownBaseUrl({ ...repo, provider: 'gitee' }), 'https://gitee.com/Owner/Mod/raw/develop/README.md')
  assert.equal(repositoryMarkdownBaseUrl({ ...repo, branch: '' }), 'https://github.com/Owner/Mod/')
  assert.equal(repositoryMarkdownBaseUrl({ ...repo, branch: '../wrong' }), 'https://github.com/Owner/Mod/')
  assert.equal(repositoryMarkdownBaseUrl({ ...repo, branch: 'feature/assets' }), 'https://github.com/Owner/Mod/raw/refs/heads/feature/assets/README.md')
})

test('Markdown images resolve against the real Raw document branch on both providers', () => {
  for (const base of ['https://github.com/Owner/Mod/raw/refs/heads/develop/', 'https://gitee.com/Owner/Mod/raw/develop/']) {
    for (const path of ['./images/screen.svg', 'images/screen.svg', '/images/screen.svg']) {
      assert.equal(markdownResourceUrl(path, `${base}README.md`, true), `${base}images/screen.svg`)
    }
    assert.equal(markdownResourceUrl('docs/guide.md', `${base}README.md`), `${base}docs/guide.md`)
  }
  assert.equal(markdownResourceUrl('https://github.com/Owner/Mod/blob/develop/screen.png', undefined, true), 'https://github.com/Owner/Mod/raw/refs/heads/develop/screen.png')
  assert.equal(markdownResourceUrl('https://gitee.com/Owner/Mod/blob/develop/screen.png', undefined, true), 'https://gitee.com/Owner/Mod/raw/develop/screen.png')
  assert.equal(markdownResourceUrl('//img.shields.io/badge/build-passing.svg', undefined, true), 'https://img.shields.io/badge/build-passing.svg')
})

test('Markdown resources reject scripts, embedded data, local files, credentials and unsafe bases', () => {
  for (const url of ['javascript:alert(1)', 'data:image/svg+xml,<svg></svg>', 'file:///C:/secret.png', 'tauri://localhost/config',
    'blob:https://example.com/token', 'ftp://example.com/image.png', 'https://user:password@example.com/image.png', 'java\nscript:alert(1)']) {
    assert.equal(markdownResourceUrl(url, 'https://github.com/Owner/Mod/README.md', true), undefined, url)
    assert.equal(markdownResourceUrl(url), undefined, url)
  }
  for (const base of ['file:///C:/README.md', 'tauri://localhost/', 'https://user:password@example.com/']) {
    assert.equal(markdownResourceUrl('./image.png', base, true), undefined, base)
  }
  assert.equal(markdownResourceUrl('https://example.com/safe.png'), 'https://example.com/safe.png')
  assert.equal(markdownResourceUrl('http://example.com/docs'), 'http://example.com/docs')
  assert.equal(markdownResourceUrl('./image.png', undefined, true), undefined, 'Unresolved paths must not load from the application origin')
})

test('desktop CSP permits remote Markdown images but not remote scripts, frames or arbitrary API requests', () => {
  const security = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8')).app.security
  for (const policy of [security.csp, security.devCsp]) {
    const directives = new Map(policy.split(';').map((line) => {
      const [directive, ...values] = line.trim().split(/\s+/)
      return [directive, values]
    }))
    assert.ok(directives.get('img-src').includes('https:'))
    for (const directive of ['default-src', 'script-src', 'connect-src']) {
      assert.ok(!directives.get(directive).includes('https:'))
      assert.ok(!directives.get(directive).includes('*'))
    }
  }
})

test('actual renderer supports GFM headings, lists, quotes, code, tables, details and GitHub line breaks', () => {
  const root = documentFrom('# Title\n\n## Subtitle\n\n**bold** *italic* ~~deleted~~ `inline`\n\n- one\n- two\n\n3. third\n\n> quote\n\n```rust\nlet x = "<safe>";\n```\n\n| Left | Center | Right |\n| :--- | :---: | ---: |\n| a | b | c |\n\n<details><summary>Expand</summary>More</details>\n\nfirst\nsecond\n\nhttps://example.com')
  for (const selector of ['h1', 'h2', 'strong', 'em', 'del', 'code', 'ul', 'ol[start="3"]', 'blockquote', 'pre > code.language-rust', 'details > summary', '.markdown-table-scroll > table']) assert.ok(root.querySelector(selector), selector)
  assert.deepEqual([...root.querySelectorAll('th')].map(cell => cell.className), ['markdown-align-left', 'markdown-align-center', 'markdown-align-right'])
  assert.equal(root.querySelector('pre code').textContent, 'let x = "<safe>";\n')
  assert.equal(root.querySelectorAll('br').length, 0, 'GitHub soft line breaks stay soft')
  assert.equal(root.querySelector('a[href="https://example.com/"]').textContent, 'https://example.com')
})

test('task lists are read-only and cannot smuggle other form controls or source classes', () => {
  const root = documentFrom('- [x] done\n- [ ] pending\n\n<input type="text" value="unsafe"><input type="checkbox"><form><button>submit</button></form><div class="modal-backdrop markdown-diagram-staging" id="app">visible</div>')
  const inputs = [...root.querySelectorAll('input')]
  assert.equal(inputs.length, 2)
  assert.ok(inputs.every(input => input.disabled && input.type === 'checkbox' && input.parentElement.classList.contains('markdown-task-item')))
  assert.deepEqual(inputs.map(input => input.checked), [true, false])
  assert.equal(root.querySelectorAll('form,button,.modal-backdrop,.markdown-diagram-staging,[id="app"]').length, 0)
})

test('loose and nested GFM tasks retain their disabled checkbox inside paragraph wrappers', () => {
  const root = documentFrom('- [x] done\n\n  Details paragraph\n\n  - [ ] nested\n\n- [ ] pending\n\n  More details')
  const inputs = [...root.querySelectorAll('input')]
  assert.equal(inputs.length, 3)
  assert.ok(inputs.every(input => input.disabled && input.closest('li').classList.contains('markdown-task-item')))
  assert.equal(inputs[0].parentElement.tagName, 'P')
  assert.deepEqual(inputs.map(input => input.checked), [true, false, false])
})

test('headings have GitHub slugs and duplicate fragments navigate only within the clicked document', () => {
  const first = documentFrom('# Hello World!\n\n# Hello World!\n\n## 中文标题\n\n[Jump](#hello-world-1)')
  const second = first.cloneNode(true)
  document.body.append(first, second)
  try {
    assert.deepEqual([...first.querySelectorAll('h1,h2')].map(heading => heading.id), ['markdown-hello-world', 'markdown-hello-world-1', 'markdown-中文标题'])
    let scrolled = false
    first.querySelector('[id="markdown-hello-world-1"]').scrollIntoView = () => { throw new Error('Wrong document') }
    second.querySelector('[id="markdown-hello-world-1"]').scrollIntoView = () => { scrolled = true }
    const anchor = second.querySelector('p a')
    anchor.addEventListener('click', event => assert.equal(markdownLinkUrl(event), undefined))
    const event = new MouseEvent('click', { cancelable: true })
    anchor.dispatchEvent(event)
    assert.ok(scrolled)
    assert.ok(event.defaultPrevented)
    assert.equal(location.hash, '')
  } finally { first.remove(); second.remove() }
})

test('actual DOM sanitizer rejects scripts, styles, SVG, frames, unsafe URLs and dangerous attributes', () => {
  const root = documentFrom('<script>alert(1)</script><style>*{display:none}</style><svg onload="alert(1)"><circle /></svg><iframe src="https://example.com"></iframe><img src="javascript:alert(1)" onerror="alert(1)" style="position:fixed" srcset="https://example.com/other 2x"><a href="file:///secret">file</a><span class="hljs-keyword" id="config">text</span>')
  assert.equal(root.querySelectorAll('script,style,svg,iframe,[onload],[onerror],[style],[srcset],[id],.hljs-keyword').length, 0)
  assert.equal(root.querySelector('img').getAttribute('src'), null)
  assert.equal(root.querySelector('a').getAttribute('href'), null)
})

test('real lazy highlighter preserves source, highlights known languages and leaves unknown ones alone', async () => {
  const root = documentFrom('```js\nconst x = "<script>";\n```\n\n```unknown-language\n<still-safe>\n```')
  document.body.append(root)
  const controller = new AbortController()
  try {
    await enhanceMarkdown(root, { signal: controller.signal, diagramLabel: 'Diagram', diagramError: 'Error', onLayout() {} })
    assert.ok(root.querySelector('code.language-js.hljs .hljs-keyword'))
    assert.equal(root.querySelector('code.language-js').textContent, 'const x = "<script>";\n')
    assert.equal(root.querySelector('code.language-unknown-language').textContent, '<still-safe>\n')
    assert.equal(root.querySelectorAll('script').length, 0)
  } finally { controller.abort(); root.remove() }
})

test('aborted enhancements never mutate a stale document', async () => {
  const root = documentFrom('```js\nconst x = 1\n```\n\n```mermaid\nflowchart TD\nA-->B\n```')
  document.body.append(root)
  const controller = new AbortController()
  controller.abort()
  const html = root.innerHTML
  await enhanceMarkdown(root, { signal: controller.signal, diagramLabel: 'Diagram', diagramError: 'Error', onLayout() { throw new Error('Late layout') } })
  assert.equal(root.innerHTML, html)
  root.remove()
})

test('Mermaid configuration directives and oversized sources fail safely without loading or mounting SVG', async () => {
  for (const source of ['%%{init: {"securityLevel":"loose"}}%%\nflowchart TD\nA-->B', '---\nconfig:\n  securityLevel: loose\n---\nflowchart TD\nA-->B', 'flowchart TD\nA@{ img: "https://example.com/image.png" }', 'flowchart TD\nclassDef remote fill:url(https://example.com/image.svg);', `flowchart TD\n${'A'.repeat(50_001)}`]) {
    const root = documentFrom(`\`\`\`mermaid\n${source}\n\`\`\``)
    document.body.append(root)
    const controller = new AbortController()
    try {
      assert.equal(root.querySelectorAll('pre > code').length, 1, 'Diagram source is present')
      assert.ok(root.isConnected)
      await enhanceMarkdown(root, { signal: controller.signal, diagramLabel: 'Diagram', diagramError: 'Safe fallback', onLayout() {} })
      assert.equal(root.querySelector('.markdown-diagram-error')?.textContent, 'Safe fallback', root.innerHTML.slice(0, 500))
      assert.equal(root.querySelector('pre code').textContent, `${source}\n`)
      assert.equal(root.querySelectorAll('svg,img,iframe').length, 0)
      assert.equal(document.querySelectorAll('.markdown-diagram-staging').length, 0)
    } finally { controller.abort(); root.remove() }
  }
})
