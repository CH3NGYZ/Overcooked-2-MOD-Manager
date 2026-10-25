import { createServer } from 'node:net'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const configPath = join(root, 'src-tauri', 'tauri.conf.json')
const outPath = process.argv[2]
const preferredPort = Number(process.argv[3] || 5173)

if (!outPath) {
  console.error('usage: node scripts/start-test-config.mjs <output.json> [port]')
  process.exit(2)
}

function portFree(port) {
  return new Promise((resolve) => {
    const server = createServer()
    server.once('error', () => resolve(false))
    server.listen(port, '127.0.0.1', () => server.close(() => resolve(true)))
  })
}

async function pickPort(start) {
  for (let port = start; port <= start + 49; port += 1) {
    if (await portFree(port)) return port
  }
  return null
}

const port = await pickPort(preferredPort)
if (!port) {
  console.error(`[start test] no free port in ${preferredPort}..${preferredPort + 49}`)
  process.exit(1)
}

const base = JSON.parse(readFileSync(configPath, 'utf8'))

/*
 * Tauri merges this override into tauri.conf.json for the dev run. devCsp is
 * hardcoded to localhost:5173 there, so any other port would have its scripts
 * and styles blocked and the window would render blank. Rebuild devCsp for the
 * port actually in use instead of assuming the default.
 */
const origin = `http://127.0.0.1:${port}`
const wsOrigin = `ws://127.0.0.1:${port}`
const baseCsp = base.app?.security?.devCsp
  ?? "default-src 'self'; connect-src 'self' ipc: http://ipc.localhost; img-src 'self' data: blob: asset: http://asset.localhost https: http:; style-src 'self' 'unsafe-inline'; script-src 'self' 'unsafe-inline'"

const csp = baseCsp
  .replaceAll('http://localhost:5173', origin)
  .replaceAll('ws://localhost:5173', wsOrigin)
  .replaceAll('https://localhost:5173', `https://127.0.0.1:${port}`)

const override = {
  build: {
    devUrl: origin,
    beforeDevCommand: '',
  },
  app: {
    security: { devCsp: csp },
  },
}

writeFileSync(outPath, JSON.stringify(override, null, 2), 'utf8')
console.log(port)
