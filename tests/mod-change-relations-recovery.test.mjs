import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText } from './helpers/source-scan.mjs'

test('dependency snapshot sync completes recovery under operation before reading and writing manifests', () => {
  const source = readText(resolve('src-tauri/src/commands/relations.rs'))
  assert.match(source, /pub fn sync_dependency_snapshots\([\s\S]*?sync_dependency_snapshots_in\(&state\)/)
  const body = source.match(/fn sync_dependency_snapshots_in\([\s\S]*?\n\}/)?.[0]
  assert.ok(body)
  const recovered = body.indexOf('ensure_recovered(state)?')
  assert.ok(recovered > body.indexOf('.operation'))
  for (const marker of ['load_or_default()', 'archive::read_manifests', 'archive::write_manifest', 'archive::write_map_relation_snapshots']) {
    assert.ok(recovered < body.indexOf(marker), `${marker} must follow recovery`)
  }
})
