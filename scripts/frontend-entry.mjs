import { existsSync, readFileSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const html = `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/png" href="/src/assets/bb4518aa-dc55-4cae-a432-08ccdc5f28f8.png" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>OC2 MOD \u7ba1\u7406\u5668</title>
  </head>
  <body>
    <div id="app"></div>
    <script type="module" src="/src/main.ts"></script>
  </body>
</html>
`

export function generateFrontendEntry(root = process.cwd()) {
  const file = join(root, 'index.html')
  if (!existsSync(file) || readFileSync(file, 'utf8') !== html) {
    writeFileSync(file, html, 'utf8')
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  generateFrontendEntry()
}
