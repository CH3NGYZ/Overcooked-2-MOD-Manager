import assert from 'node:assert/strict'
import { relative, resolve } from 'node:path'
import test from 'node:test'
import ts from 'typescript'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const rustRoot = resolve('src-tauri/src')
const frontendRoot = resolve('src')
const pathOf = (root, file) => relative(root, file).replaceAll('\\', '/')

// Keep offsets/newlines while hiding comments and Rust literals. In particular,
// a channel's send(value), a test server, or a quoted example is not an HTTP send.
function rustCode(source) {
  const code = [...source]
  const hide = (start, end) => {
    for (let at = start; at < end; at++) if (code[at] !== '\n') code[at] = ' '
  }
  for (let at = 0; at < source.length;) {
    const start = at
    if (source.startsWith('//', at)) {
      at = source.indexOf('\n', at)
      if (at < 0) at = source.length
    } else if (source.startsWith('/*', at)) {
      at += 2
      let depth = 1
      while (at < source.length && depth) {
        if (source.startsWith('/*', at)) { depth++; at += 2 }
        else if (source.startsWith('*/', at)) { depth--; at += 2 }
        else at++
      }
    } else {
      const raw = source.slice(at).match(/^(?:br|r)(#*)"/)
      const char = source.slice(at).match(/^'(?:\\[^\n]|[^'\\\n])'/u)
      if (raw) {
        const end = source.indexOf(`"${raw[1]}`, at + raw[0].length)
        at = end < 0 ? source.length : end + raw[1].length + 1
      } else if (source[at] === '"') {
        at++
        while (at < source.length) {
          if (source[at] === '\\') at += 2
          else if (source[at++] === '"') break
        }
      } else if (char) at += char[0].length
      else { at++; continue }
    }
    hide(start, at)
  }
  return code.join('')
}

function productionRust(source) {
  const code = rustCode(source)
  const output = code.split('')
  for (const match of code.matchAll(/#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]/g)) {
    let at = match.index + match[0].length
    let nesting = 0
    for (; at < code.length; at++) {
      if ('(['.includes(code[at])) nesting++
      else if (')]'.includes(code[at])) nesting--
      else if (!nesting && code[at] === ';') { at++; break }
      else if (!nesting && code[at] === '{') {
        let depth = 1
        at++
        while (at < code.length && depth) {
          if (code[at] === '{') depth++
          else if (code[at] === '}') depth--
          at++
        }
        break
      }
    }
    for (let index = match.index; index < at; index++) if (output[index] !== '\n') output[index] = ' '
  }
  return output.join('')
}

const rust = new Map(sourceFiles(rustRoot, ['.rs']).flatMap(file => {
  const path = pathOf(rustRoot, file)
  if (/(?:^|\/)(?:tests?|test_support)(?:\/|\.rs$)|(?:^|\/)[^/]*_tests\.rs$/.test(path)) return []
  return [[path, productionRust(readText(file))]]
}))
const rawSend = /\.\s*send\s*\(\s*\)/g
const execute = /\.\s*execute\s*\(/g

test('HTTP source scan excludes literals and test-only items while retaining channel sends and later production', () => {
  const code = productionRust(String.raw`
    // client.get(url).send();
    const SAMPLE: &str = r##"client.execute(request); request.send()"##;
    /* outer /* request.send() */ comment */
    #[cfg(test)] mod tests { fn fixture() { client.get(url).send(); } }
    fn production() { sender.send(value); client.get(url).send(); }
  `)
  assert.equal([...code.matchAll(rawSend)].length, 1)
  assert.equal([...code.matchAll(execute)].length, 0)
  assert.match(code, /sender\.send\(value\)/)
  assert.match(code, /fn production\(/)
})

test('the shared diagnostics module owns request execution and is registered in the Rust crate', () => {
  const shared = rust.get('http_diagnostics.rs')
  assert.ok(shared, 'missing src-tauri/src/http_diagnostics.rs')
  assert.match(rust.get('lib.rs'), /\bmod\s+http_diagnostics\s*;/)
  assert.match(shared, /\bfn\s+execute\b/)
  assert.match(shared, /\.\s*execute\s*\(\s*request\s*\)/)
  assert.match(shared, /\bfn\s+send_logged\b/)
})

test('production HTTP cannot bypass diagnostics with RequestBuilder.send or Client.execute', () => {
  const violations = [...rust].flatMap(([path, code]) => {
    if (path === 'http_diagnostics.rs') return []
    return [...code.matchAll(/\.\s*(?:send\s*\(\s*\)|execute\s*\()/g)].map(match =>
      `${path}:${code.slice(0, match.index).split('\n').length}: ${match[0]}`)
  })
  assert.deepEqual(violations, [], 'only http_diagnostics.rs may send or execute raw HTTP; send(value) channels are excluded')
})

test('API scheduler retains rate-limit admission before delegating to shared diagnostics', () => {
  const scheduler = rust.get('api_scheduler.rs')
  assert.match(scheduler, /impl\s+ApiRequestExt\s+for\s+reqwest::RequestBuilder/)
  const admission = scheduler.indexOf('.acquire(')
  const dispatch = scheduler.search(/http_diagnostics::execute\s*\(/)
  assert.ok(admission >= 0 && dispatch > admission, 'send_api must admit the request before the shared execute call')
})

const apiFiles = [
  'community/github.rs', 'community/gitcode.rs', 'releases/transport.rs',
  'map_publish/assets.rs', 'map_publish/credentials.rs', 'map_publish/http.rs',
  'map_publish/releases.rs', 'map_publish/repositories.rs', 'map_publish/session.rs',
]

test('production API request owners keep send_api rather than bypassing provider scheduling', () => {
  for (const path of apiFiles) {
    const code = rust.get(path)
    assert.ok(code, path)
    assert.match(code, /\bApiRequestExt\b/, path)
    assert.match(code, /\.\s*send_api\s*\(/, path)
  }
  for (const [path, code] of rust) {
    if (!/\bApiRequestExt\b/.test(code) || path === 'api_scheduler.rs') continue
    assert.match(code, /\.\s*send_api\s*\(/, path)
    // Signed asset PUTs share this owner with scheduled API requests.
    if (path !== 'map_publish/assets.rs') assert.doesNotMatch(code, /\.\s*send_logged\s*\(/, path)
  }
})

const nonApiFiles = [
  'archive/transfer.rs', 'repository_sources/transport.rs', 'repository_sources/verified.rs',
  'repository_readme.rs', 'avatars.rs', 'app_update/manifest.rs', 'app_update/download.rs',
  'direct_mod.rs', 'commands/colorful_name.rs', 'map_publish/assets.rs',
]

test('all ten non-API HTTP owners use the shared logged request trait', () => {
  for (const path of nonApiFiles) {
    const code = rust.get(path)
    assert.ok(code, path)
    assert.match(code, /\buse\s+crate::http_diagnostics::/, path)
    assert.match(code, /\.\s*send_logged\s*\(\s*\)/, path)
  }
})

test('frontend business scripts do not introduce direct fetch, axios, XHR or Tauri HTTP clients', () => {
  const violations = []
  const forbiddenModule = /^(?:axios(?:\/|$)|@tauri-apps\/(?:plugin-http(?:\/|$)|api\/http(?:\/|$)))/
  const globals = new Set(['fetch', 'axios', 'XMLHttpRequest'])
  for (const file of sourceFiles(frontendRoot, ['.ts', '.vue'])) {
    const source = readText(file)
    // Remote Markdown images and browser-managed resources are a separate boundary.
    const scripts = file.endsWith('.vue')
      ? [...source.matchAll(/<script\b[^>]*>([\s\S]*?)<\/script>/g)].map(match => match[1])
      : [source]
    for (const script of scripts) {
      const ast = ts.createSourceFile(file, script, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS)
      const report = node => violations.push(`${pathOf(frontendRoot, file)}: ${node.getText(ast)}`)
      function visit(node) {
        if (ts.isImportDeclaration(node) && ts.isStringLiteral(node.moduleSpecifier) && forbiddenModule.test(node.moduleSpecifier.text)) report(node)
        if (ts.isCallExpression(node) || ts.isNewExpression(node)) {
          const expression = node.expression
          if (ts.isIdentifier(expression) && globals.has(expression.text)) report(node)
          if (ts.isPropertyAccessExpression(expression)) {
            const owner = expression.expression
            if (ts.isIdentifier(owner) && (
              (['window', 'globalThis', 'self'].includes(owner.text) && globals.has(expression.name.text))
              || owner.text === 'axios'
            )) report(node)
          }
          if (ts.isCallExpression(node) && (expression.kind === ts.SyntaxKind.ImportKeyword || (ts.isIdentifier(expression) && expression.text === 'require'))) {
            const module = node.arguments[0]
            if (module && ts.isStringLiteral(module) && forbiddenModule.test(module.text)) report(node)
          }
        }
        ts.forEachChild(node, visit)
      }
      visit(ast)
    }
  }
  assert.deepEqual(violations, [], 'business requests must use the Rust IPC services; spec.fetch callbacks are not browser fetch')
})
