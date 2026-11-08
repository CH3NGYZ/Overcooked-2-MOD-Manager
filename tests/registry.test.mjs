import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * IPC 注册表一致性：services/tauri.ts 里 invoke 的每个命令必须已在
 * src-tauri/src/lib.rs 的 generate_handler! 注册，反之未封装的命令允许存在。
 */
const service = readFileSync(new URL('../src/services/tauri.ts', import.meta.url), 'utf8')
const lib = readFileSync(new URL('../src-tauri/src/lib.rs', import.meta.url), 'utf8')

const invoked = new Set([...service.matchAll(/invoke<[^>]*>\(\s*'([a-z_]+)'/g)].map((m) => m[1]))
function registeredPaths(source) {
  const block = source.match(/generate_handler!\s*\[([\s\S]*?)\]/)?.[1]
  assert.ok(block, '缺少 generate_handler 注册块')
  return [...block.matchAll(/\b[a-z_][a-z_0-9]*(?:::[a-z_][a-z_0-9]*)+\b/g)].map(m => m[0])
}
const registered = new Set(registeredPaths(lib).map(path => path.split('::').at(-1)))

test('注册解析只取完整路径末段，支持嵌套域且不扫描 handler 之外的代码', () => {
  const paths = registeredPaths('generate_handler![community::refresh, commands::settings::get_config] other::not_registered')
  assert.deepEqual(paths, ['community::refresh', 'commands::settings::get_config'])
  assert.deepEqual(paths.map(path => path.split('::').at(-1)), ['refresh', 'get_config'])
})

test('every frontend invoke target is registered in generate_handler', () => {
  const missing = [...invoked].filter((name) => !registered.has(name))
  assert.deepEqual(missing, [], `未注册的 IPC 命令: ${missing.join(', ')}`)
})

test('v2 调用面覆盖核心命令', () => {
  for (const required of ['get_dashboard', 'get_local_mods', 'get_mod_updates', 'get_config', 'set_game_path', 'reset_game_path', 'detect_game',
    'install_bepinex', 'uninstall_bepinex', 'backup_bepinex', 'restore_bepinex', 'switch_winhttp',
    'toggle_local_mod', 'uninstall_local_mod', 'install_mod_repository', 'refresh_mod_repository',
    'check_app_update', 'install_app_update', 'save_settings']) {
    assert.ok(invoked.has(required), `services/tauri.ts 缺少 ${required}`)
  }
})

test('v2 前端不再引用教程/代理命令（精简调用面）', () => {
  // 社区搜索已重新接入（启动刷新与「刷新全部」都会触发），不再属于「不引用」名单
  for (const trimmed of ['get_builtin_tutorial', 'save_github_proxies', 'parse_tutorial_config']) {
    assert.ok(!invoked.has(trimmed), `${trimmed} 不应出现在 v2 调用面`)
  }
})

test('社区仓库搜索已接入调用面', () => {
  assert.ok(invoked.has('refresh_community_repositories'), 'services/tauri.ts 缺少社区搜索封装')
})
