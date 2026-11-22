import assert from 'node:assert/strict'
import test from 'node:test'
import { invoke, recordFileDrop } from '../src/services/invoke.ts'

test('IPC 诊断保留原回执，只向 Rust 报告命令、错误键及耗时', async () => {
  const previous = globalThis.window
  const rejection = new Error('modChanges.installTargetClaimed?path=private&token=secret')
  const unknown = 'Authorization: Bearer secret'
  const declined = { ok: false, message: 'game.running' }
  const success = { ok: true, config: 'private' }
  const diagnostics = []
  globalThis.window = {
    __TAURI_INTERNALS__: {
      async invoke(command, args) {
        if (command === 'record_ipc_failure') {
          diagnostics.push(args)
          throw new Error('diagnostic unavailable')
        }
        if (command === 'plan_mod_changes') throw rejection
        if (command === 'save_tokens') throw unknown
        return command === 'execute_mod_changes' ? declined : success
      },
    },
  }
  try {
    await assert.rejects(invoke('plan_mod_changes', { content: 'private' }), error => error === rejection)
    await assert.rejects(invoke('save_tokens', { token: 'secret' }), error => error === unknown)
    assert.equal(await invoke('execute_mod_changes'), declined)
    assert.equal(await invoke('get_config'), success)
    assert.deepEqual(diagnostics.map(({ elapsedMs, ...detail }) => {
      assert.ok(Number.isInteger(elapsedMs) && elapsedMs >= 0)
      return detail
    }), [
      { command: 'plan_mod_changes', rejected: true, tag: 'modChanges.installTargetClaimed' },
      { command: 'save_tokens', rejected: true, tag: undefined },
      { command: 'execute_mod_changes', rejected: false, tag: 'game.running' },
    ])
  } finally {
    if (previous === undefined) delete globalThis.window
    else globalThis.window = previous
  }
})

test('更新门拦截的拖放仅报告状态、类型和个数，不发送本地路径', async () => {
  const previous = globalThis.window
  const calls = []
  globalThis.window = { __TAURI_INTERNALS__: { invoke: async (...args) => { calls.push(args) } } }
  try {
    recordFileDrop('update_gate', 'ready', ['E:\\private\\map.ZIP', 'C:\\private\\other.dll'])
    await Promise.resolve()
    assert.deepEqual(calls, [['record_file_drop', {
      result: 'update_gate', updateStage: 'ready', fileType: 'zip', count: 2,
    }, undefined]])
  } finally {
    if (previous === undefined) delete globalThis.window
    else globalThis.window = previous
  }
})
