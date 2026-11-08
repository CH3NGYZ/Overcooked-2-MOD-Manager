import { strict as assert } from 'node:assert'
import { readFileSync, readdirSync } from 'node:fs'
import { test } from 'node:test'

const src = new URL('../src/', import.meta.url)
const read = (rel) => readFileSync(new URL(rel, src), 'utf8')
const views = readdirSync(new URL('views/', src)).filter((file) => file.endsWith('.vue'))

/**
 * 本地关卡集扫描的状态只有一份（`useLevelSetScan` 的模块级单例）。
 *
 * 总览页的「本地地图」卡靠它显示读取中，而读取可能由地图管理页/地图下载页
 * 发起 —— 任何页面绕过它直接调 `api.levelSets()`，那次读取就对总览不可见，
 * 切页时状态也丢。这条测试守的就是这个回归。
 */
test('本地关卡集扫描只有 useLevelSetScan 一个入口', () => {
  for (const file of views) {
    const source = read(`views/${file}`)
    assert.ok(
      !source.includes('api.levelSets()'),
      `${file} 不许直接调 api.levelSets()，改走 scanLevelSets()`,
    )
    assert.ok(
      !source.includes('watchLevelSetScan'),
      `${file} 不许自己订阅扫描进度，订阅已收进 useLevelSetScan`,
    )
  }

  const scan = read('composables/useLevelSetScan.ts')
  assert.match(scan, /export function scanLevelSets\(/)
  // 状态必须在模块作用域：挂在页面实例上就熬不过一次切页。
  assert.match(scan, /^const inFlight = ref\(0\)$/m)
  assert.match(scan, /const scanning = computed\(\(\) => inFlight\.value > 0\)/)
  assert.match(scan, /watchLevelSetScan\(/)
  // 结束时按计数归零清理，不能由先结束的那次清掉还在跑的进度。
  assert.match(scan, /progress\.value = null/)
})

/**
 * 总览：扫描期间按钮转圈并换成「读取中」，数值不让 0 冒充结论。
 * 交互与本地 MOD 卡的「检测中」一致（`:loading` + 文案切换 + 不可点）。
 */
test('总览「本地地图」卡显示读取状态', () => {
  const overview = read('views/OverviewView.vue')
  assert.match(overview, /const \{ scanning: levelSetScanning, scanLevelSets \} = useLevelSetScan\(\)/)
  assert.match(overview, /:loading="localMapsChecking"/)
  assert.match(overview, /levelSets\.value = await scanLevelSets\(\)/)
  assert.match(overview, /if \(levelSetScanning\.value\) return t\('levelSet\.scanning'\)/)
  assert.match(overview, /levelSetScanning\.value && !levelSets\.value\.length \? '—'/)
})
