import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 安装成功彩带的接线（`src/celebration.ts`）。
 *
 * 为什么用源码扫描而不是跑真浏览器：彩带只在「安装成功」那一刻触发，
 * 且依赖 canvas 与系统动效偏好，Node 里跑不了；而这条链路里有两处**静默失效**
 * 的风险，都只能从源码形态上守住：
 *
 * 1. 不能用 `canvas-confetti` 的默认导出。默认实例固定 `useWorker: true`，
 *    走 Web Worker + `transferControlToOffscreen()`；库只对 `new Worker(blob)`
 *    做了 try/catch，没兜住 transferControlToOffscreen 抛错 —— WKWebView（macOS）
 *    上正好会在这里抛，表现就是「装完没有窗花」，而控制台一个字都不留。
 * 2. 失败与跳过必须写进 `logs/frontend.log`：否则用户报告「macOS 没有效果」时，
 *    分不清是系统关了动效、库没加载，还是播放抛错。
 */
const SOURCE = readFileSync(new URL('../src/celebration.ts', import.meta.url), 'utf8')

test('彩带不用库的默认导出：自己建关掉 worker 的实例', () => {
  assert.match(
    SOURCE,
    /confetti\.create\([^)]*useWorker:\s*false/,
    '必须 confetti.create(…, { useWorker: false })：默认导出的 worker 路径在 WKWebView 上会抛错且被吞',
  )
  assert.ok(
    !/void confetti\(\{/.test(SOURCE) && !/await confetti\(\{/.test(SOURCE),
    '不要直接调用默认导出 confetti(...)，它固定走 OffscreenCanvas + Worker',
  )
})

test('跳过与失败都写前端日志，便于线上定位「没有窗花」', () => {
  assert.match(SOURCE, /writeFrontendLog/, '寂静失败会让 macOS 的问题无法定位')
  assert.match(SOURCE, /prefersReducedMotion\(\)/, '系统「减少动态效果」必须优先')
  // 跳过时也要留痕：否则「没效果」和「被偏好跳过」在日志里无法区分
  assert.match(SOURCE, /已跳过/, '被 prefers-reduced-motion 跳过时要写日志')
})
