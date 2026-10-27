import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 启动更新检查的开关契约。
 *
 * 用户反馈：在设置里关闭「启动时检查程序更新」后按 F5（webview 重载，
 * onMounted 重跑）仍会弹出更新提示。原因是 App.vue 的 onMounted 无条件
 * 调用 check_app_update，从未读取 config.network.check_updates_on_start。
 *
 * 这里直接对真实源码断言顺序：必须先读配置并判断该字段，才允许发起检查。
 */
const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')

test('启动更新检查先读配置开关，再决定是否请求', () => {
  const gate = app.indexOf('check_updates_on_start')
  const call = app.indexOf("checkAppUpdate('zh-CN')")
  assert.ok(gate !== -1, 'App.vue 未读取 check_updates_on_start，启动检查会绕过设置开关')
  assert.ok(call !== -1, 'App.vue 未包含启动更新检查调用')
  assert.ok(gate < call, '必须先在启动路径上判断开关，再调用 checkAppUpdate')
})

test('开关读取来自 api.config()，且关闭时不发请求', () => {
  const mountBlock = app.slice(app.indexOf('onMounted('), app.indexOf('</script>'))
  assert.match(mountBlock, /api\.config\(\)/, '启动检查必须从 api.config() 取真实配置')
  assert.match(
    mountBlock,
    /if \(!config\.network\.check_updates_on_start\) return/,
    '开关关闭时必须提前返回，不能继续调用 checkAppUpdate',
  )
})
