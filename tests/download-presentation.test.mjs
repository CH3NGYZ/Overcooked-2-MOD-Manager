import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { i18n } from '../src/locale/index.ts'
import { LOCALE_CODES } from '../src/locale/types.ts'

// Resolve the app's Vite-style locale import without replacing the real message catalog.
registerHooks({ resolve(specifier, context, next) {
  if (specifier === './locale/state' && context.parentURL?.endsWith('/src/i18n.ts')) return next(`${specifier}.ts`, context)
  return next(specifier, context)
} })
const { downloadAmount, downloadPercent, downloadRace, downloadRate, downloadRoute } = await import('../src/downloadPresentation.ts')

test('byte progress distinguishes connecting, known 0%, unknown size and rounded display', () => {
  assert.equal(downloadPercent({ downloaded: 0, total: 10, phase: 'connecting' }), null)
  assert.equal(downloadPercent({ downloaded: 0, total: 10, phase: 'downloading' }), 0)
  assert.equal(downloadPercent({ downloaded: 9, phase: 'downloading' }), null)
  assert.equal(downloadPercent({ downloaded: 9, total: null, phase: 'downloading' }), null)
  assert.equal(downloadPercent({ downloaded: 1, total: 8, phase: 'downloading' }), 12.5)
  assert.equal(downloadPercent({ downloaded: 12, total: 10, phase: 'downloading' }), 100)
  assert.equal(downloadAmount({ downloaded: 0, total: 1024 }), '0 B / 1.0 KB')
  assert.equal(downloadAmount({ downloaded: 512, total: 1024 }), '512 B / 1.0 KB')
})

test('statistics carry only reported data, including a known stalled rate', () => {
  assert.equal(downloadRate({ speed_bytes: 0, phase: 'connecting' }), '')
  assert.equal(downloadRate({ speed_bytes: 0, phase: 'downloading' }), '0 KB/s')
  assert.equal(downloadRate({ speed_bytes: 1024 * 512, phase: 'downloading' }), '512 KB/s')
  assert.equal(downloadRate({ speed_bytes: 1024 * 1024 * 2.5, phase: 'downloading' }), '2.5 MB/s')
  assert.equal(downloadRoute({}), '')
  assert.equal(downloadRoute({ source: 'github.example' }), 'github.example')
  assert.equal(downloadRace({ parallel_sources: 1, attempt: 1, attempt_total: 1 }), '')
  assert.equal(downloadRace({ parallel_sources: 0, attempt_total: 8 }), '')
})

test('unknown totals, route, and race summaries render in all five languages', () => {
  const original = i18n.global.locale.value
  const unknown = ['大小未知', '大小未知', 'Unknown size', '크기 알 수 없음', 'サイズ不明']
  const racing = ['竞速中', '競速中', 'Racing', '경쟁 중', '競争中']
  const attempt = ['第 5/32 次尝试', '第 5/32 次嘗試', 'Attempt 5/32', '5/32번째 시도', '5/32 回目の試行']
  try {
    for (const [i, language] of LOCALE_CODES.entries()) {
      i18n.global.locale.value = language
      assert.equal(downloadAmount({ downloaded: 0, total: null }), `0 B / ${unknown[i]}`)
      assert.equal(downloadRoute({ route: 'proxy', source: 'mirror.example' }), `${i18n.global.t('common.proxy')} · mirror.example`)
      assert.equal(downloadRace({ parallel_sources: 3, attempt: 5, attempt_total: 32 }), `${racing[i]} · 5/32`)
      assert.equal(downloadRace({ parallel_sources: 3, attempt: 5 }), racing[i])
      assert.equal(downloadRace({ parallel_sources: 1, attempt: 5, attempt_total: 32 }), attempt[i])
    }
  } finally { i18n.global.locale.value = original }
})
