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
const handlerBlock = lib.slice(lib.indexOf('generate_handler!'))
const registered = new Set([...handlerBlock.matchAll(/([a-z_]+)::([a-z_]+)/g)].map((m) => m[2]))

test('every frontend invoke target is registered in generate_handler', () => {
  const missing = [...invoked].filter((name) => !registered.has(name))
  assert.deepEqual(missing, [], `未注册的 IPC 命令: ${missing.join(', ')}`)
})

test('v2 调用面覆盖核心命令', () => {
  for (const required of ['get_dashboard', 'get_config', 'set_game_path', 'reset_game_path', 'detect_game',
    'install_bepinex', 'uninstall_bepinex', 'backup_bepinex', 'restore_bepinex', 'switch_winhttp',
    'toggle_local_mod', 'uninstall_local_mod', 'install_repository', 'refresh_repository',
    'check_app_update', 'install_app_update', 'save_settings']) {
    assert.ok(invoked.has(required), `services/tauri.ts 缺少 ${required}`)
  }
})

test('v2 前端不再引用教程/社区/代理命令（精简调用面）', () => {
  for (const trimmed of ['get_builtin_tutorial', 'refresh_community_repositories', 'save_github_proxies', 'parse_tutorial_config']) {
    assert.ok(!invoked.has(trimmed), `${trimmed} 不应出现在 v2 调用面`)
  }
})
