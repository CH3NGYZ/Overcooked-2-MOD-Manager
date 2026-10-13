import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import ts from 'typescript'

const source = readFileSync(new URL('../src/markdown.ts', import.meta.url), 'utf8')
let { outputText } = ts.transpileModule(source, {
  compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
})
for (const name of ['dompurify', 'marked']) outputText = outputText.replaceAll(`from '${name}'`, `from '${import.meta.resolve(name)}'`)
const { repositoryMarkdownBaseUrl, markdownResourceUrl } = await import(`data:text/javascript;base64,${Buffer.from(outputText).toString('base64')}`)

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
