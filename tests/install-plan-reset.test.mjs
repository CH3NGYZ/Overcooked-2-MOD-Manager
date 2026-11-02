import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 安装前检查的状态必须**每次打开都重置**，行 key 必须带身份。
 *
 * 真实事故：弹窗组件实例是常驻的（只有内层 n-modal 被销毁），而 `done` /
 * `failures` 是组件级 ref，关窗后不会清空。行 key 当时按索引生成（`install-0`），
 * 于是第二次打开弹窗时，新内容的 `install-0` 直接命中了上一次残留的「已完成」：
 *
 *   · 依赖那一行一打开就打着 ✓
 *   · 按钮从「全部处理」变成「完成」
 *   · 点「完成」→ close() emit allOk=true → 宿主放行主包
 *
 * 结果依赖被完整跳过，主包装上了却仍然「缺依赖」。这里钉住两条：
 *   1. 打开时必须重置 done / failures（共享弹窗 `InstallCheckDialog.vue`）；
 *   2. 行 key 必须带身份（仓库 id / 版本 / 卸载句柄），不能只用索引
 *      （`installCheck.ts` 生成）。
 *
 * 弹窗是共享的：MOD 关系计划与 MOD / 地图的同身份冲突都走它，所以这两条
 * 规则一漏就是三种安装一起漏。
 *
 * 组件依赖 Vue + Naive UI，Node 里不能直接执行，所以对真实源码做结构断言
 * （与 tests/install-confirm-order.test.mjs 同风格）。
 */
const DIALOG = readFileSync(
  new URL('../src/components/InstallCheckDialog.vue', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

const MODEL = readFileSync(
  new URL('../src/installCheck.ts', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

/** 取 `<script setup>` 段，避免模板里的文本干扰断言。 */
const script = DIALOG.split('<script setup lang="ts">')[1]?.split('</script>')[0] ?? ''

test('打开弹窗时重置执行状态', () => {
  // 必须监听 show 变化
  assert.match(script, /watch\(\s*\(\)\s*=>\s*props\.show/, '必须 watch props.show')

  const watcher = script.slice(script.indexOf('watch(() => props.show'))
  const body = watcher.slice(0, watcher.indexOf('\n})') + 3)
  assert.match(body, /done\.value\s*=\s*\{\}/, '打开时必须清空 done')
  assert.match(body, /failures\.value\s*=\s*\[\]/, '打开时必须清空 failures')
})

test('行 key 带身份，不能只用索引', () => {
  /*
   * 只用索引的话，不同批次的第一行会撞 key。安装行的 key 必须同时含仓库 id
   * （或 guid）与版本；卸载行必须含卸载句柄（MOD 的 dll 路径 / 地图的目录名）。
   */
  const installKeys = MODEL.match(/key:\s*`install-[^`]*`/g) ?? []
  const uninstallKeys = MODEL.match(/key:\s*`uninstall-[^`]*`/g) ?? []

  assert.equal(installKeys.length, 1, '应只有一个 install 行 key 模板')
  assert.equal(uninstallKeys.length, 2, '关系计划与同身份冲突各有一个 uninstall 行 key 模板')

  // 不能是 `install-${index}`
  for (const key of [...installKeys, ...uninstallKeys]) {
    assert.doesNotMatch(key, /-\$\{index\}/, 'key 不能只含索引')
  }

  // 必须包含稳定身份字段
  assert.match(installKeys[0], /repository\?\.id|relation\.guid/, 'install key 需含仓库身份')
  assert.match(uninstallKeys[0], /dll_file|relation\.guid/, '关系计划的卸载 key 需含文件身份')
  assert.match(uninstallKeys[1], /conflict\.handle/, '冲突的卸载 key 需含卸载句柄')
})

test('没有全部处理完时不能以「完成」放行主包', () => {
  /*
   * allOk 是放行主包的唯一依据（close() 里 emit allOk）。它必须同时要求
   * 「有行」「都做完」，否则空计划会被误判成可以继续。
   */
  assert.match(
    script,
    /const allDone = computed\(\(\)\s*=>\s*rows\.value\.length > 0 && pending\.value\.length === 0\)/,
    'allDone 必须同时要求有行且都已处理',
  )
  assert.match(
    script,
    /const allOk = computed\(\(\)\s*=>\s*[\s\S]{0,120}allDone\.value[\s\S]{0,120}failures\.value\.length === 0/,
    'allOk 必须建立在 allDone 与无失败之上',
  )
  // close() 必须把 allOk 作为放行依据，不能无条件 true
  const closeFn = script.slice(script.indexOf('async function close()'))
  const closeBody = closeFn.slice(0, closeFn.indexOf('\n}'))
  assert.match(closeBody, /emit\('finished', allOk\.value\)/, 'close 必须 emit allOk.value')
  assert.doesNotMatch(closeBody, /emit\('finished', true\)/, 'close 不能无条件放行')
})
