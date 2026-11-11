import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const read = path => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const view = read('src/views/ToolsView.vue')

test('chef cache maintenance is the final cache row and reuses the shared clearing operation and toast', () => {
  const group = view.match(/<SettingsGroup :title="t\('tools.cache'\)"[\s\S]*?<\/SettingsGroup>/)?.[0]
  assert.ok(group)
  assert.deepEqual([...group.matchAll(/:title="t\('(tools\.[^']+)'\)"/g)].map(match => match[1]), [
    'tools.cache', 'tools.clearAvatarCache', 'tools.clearLevelCache', 'tools.clearChefCache',
  ])
  assert.match(group, /data-clear-chef-cache[\s\S]*?@click="clearChefs"/)
  assert.match(group, /@keydown\.enter\.prevent="clearChefs"/)
  assert.match(group, /@keydown\.space\.prevent="clearChefs"/)
  const handler = view.slice(view.indexOf('async function clearChefs'), view.indexOf('</script>'))
  assert.match(handler, /await clearChefThumbnailCache\(\)/)
  assert.match(handler, /message\.success\(t\('tools\.clearedChefCache', \{ count: removed \}\)\)/)
  assert.match(handler, /message\.error\(backendMessageOf\(reason\)\)/)
})

test('the clearing IPC accepts no filesystem coordinates and runs the owned operation off the IPC thread', () => {
  const service = read('src/services/tauri.ts')
  assert.match(service, /clearDiyChefCache: \(\) => invoke<number>\('clear_diychef_cache'\)/)
  const command = read('src-tauri/src/commands/diychef_preview.rs')
  assert.match(command, /pub async fn clear_diychef_cache\(\) -> Result<usize, String>/)
  assert.match(command, /spawn_blocking\(crate::diychef::preview::clear_cache\)/)
  assert.match(read('src-tauri/src/lib.rs'), /commands::diychef_preview::clear_diychef_cache,/)
})
