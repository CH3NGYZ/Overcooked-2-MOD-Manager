import { readdirSync, readFileSync } from 'node:fs'
import { extname, join, relative } from 'node:path'

export function readText(path) {
  return readFileSync(path, 'utf8').replace(/\r\n/g, '\n')
}

/** 不跟随联接；只递归调用方指定的源码根，避免误扫 target/node_modules。 */
export function sourceFiles(root, extensions) {
  return readdirSync(root, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name)).flatMap(entry => {
    const path = join(root, entry.name)
    if (entry.isDirectory()) return sourceFiles(path, extensions)
    return entry.isFile() && extensions.includes(extname(entry.name)) ? [path] : []
  })
}

export function collectRustTags(root) {
  const found = new Map()
  for (const file of sourceFiles(root, ['.rs'])) {
    readText(file).split('\n').forEach((line, index) => {
      for (const match of line.matchAll(/"([A-Za-z][A-Za-z0-9]*(?:\.[A-Za-z][A-Za-z0-9]*)+)(\?[^\"]*)?"/g)) {
        const tag = match[1]
        if (/\.(toml|json|log|exe|dll|md|zip|png|jpg|lock|txt|nsh|wxs|nsi|so|dylib|cfg|sig|db|app|rs|com)$/i.test(tag)) continue
        if (!/^[a-z]/.test(tag)) continue
        if (!found.has(tag)) found.set(tag, [])
        found.get(tag).push(`${relative(root, file).replaceAll('\\', '/')}:${index + 1}`)
      }
    })
  }
  return found
}
