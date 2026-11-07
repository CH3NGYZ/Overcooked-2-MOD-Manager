import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 启动更新检查的契约。
 *
 * 「启动时检查程序更新」开关已按用户要求删除 —— 启动恒检测；省请求交给后端：
 * `check_app_update` 按地区缓存成功结果 5 分钟，前端只有用户手动检测时才传
 * `force: true` 绕过缓存。
 *
 * 两件事都在真实源码上钉住：谁把开关读回来、谁让手动检测也吃缓存，这里先红。
 */
const app = readFileSync(new URL('../src/App.vue', import.meta.url), 'utf8')
const settings = readFileSync(new URL('../src/views/SettingsView.vue', import.meta.url), 'utf8')
const services = readFileSync(new URL('../src/services/tauri.ts', import.meta.url), 'utf8')
const commands = readFileSync(new URL('../src-tauri/src/app_update/cache.rs', import.meta.url), 'utf8')

test('启动更新检查恒开，不再有开关门控', () => {
  const mountBlock = app.slice(app.indexOf('onMounted('), app.indexOf('</script>'))
  assert.equal(app.includes('check_updates_on_start'), false, 'App.vue 仍在读已删除的开关')
  assert.match(mountBlock, /api\.config\(\)/, '启动路径要先读配置（教程进度也用它）')
  assert.match(mountBlock, /api\.checkAppUpdate\('zh-CN'\)/, '启动路径必须无条件发起一次检查')
})

test('手动检测绕过 5 分钟缓存', () => {
  assert.match(
    services,
    /checkAppUpdate: \(locale: [^)]*force = false\)/,
    '服务层 checkAppUpdate 必须带 force 参数（默认走缓存）',
  )
  assert.match(
    settings,
    /api\.checkAppUpdate\('zh-CN', true\)/,
    '设置页「检查更新」必须传 force: true，否则会拿缓存当结论',
  )
})

test('后端按地区缓存更新检查结果 5 分钟', () => {
  assert.match(commands, /const APP_UPDATE_CACHE_TTL_MS: i64 = 5 \* 60 \* 1000;/, '缺少 5 分钟缓存常量')
  assert.match(commands, /fn app_update_cache_get\(/, '缺少缓存读取实现')
  assert.match(commands, /fn app_update_cache_store\(/, '缺少缓存写入实现')
})
