import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'
import { i18n } from '../src/locale/index.ts'
import { LOCALE_CODES } from '../src/locale/types.ts'
import { error } from '../src/locale/error.ts'
import { toolsUiDownloadRace } from '../src/locale/tools/downloadRace.ts'

// Resolve the app's Vite-style locale import without replacing the real message catalog.
registerHooks({ resolve(specifier, context, next) {
  if (specifier === './locale/state' && context.parentURL?.endsWith('/src/i18n.ts')) return next(`${specifier}.ts`, context)
  return next(specifier, context)
} })
const { downloadAmount, downloadPercent, downloadRate, downloadRoute } = await import('../src/downloadPresentation.ts')
const { translateBackendMessage } = await import('../src/locale/backend.ts')

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
  assert.equal(downloadRate({ speed_bytes: 2048, phase: 'verifying' }), '')
  assert.equal(downloadRate({ speed_bytes: 0, phase: 'downloading' }), '0 KB/s')
  assert.equal(downloadRate({ speed_bytes: 1024 * 512, phase: 'downloading' }), '512 KB/s')
  assert.equal(downloadRate({ speed_bytes: 1024 * 1024 * 2.5, phase: 'downloading' }), '2.5 MB/s')
  assert.equal(downloadRoute({}), '')
  assert.equal(downloadRoute({ source: 'github.example' }), 'github.example')
})

test('unknown totals, source states and lock rejection render in all supported languages', () => {
  const original = i18n.global.locale.value
  const received = {
    'zh-CN': '已下载 {amount}', 'zh-TW': '已下載 {amount}', en: 'Downloaded {amount}', ko: '{amount} 다운로드됨', ja: '{amount} ダウンロード済み',
    de: 'Heruntergeladen: {amount}', fr: 'Téléchargé {amount}', it: 'Scaricato {amount}', 'es-ES': 'Descargado {amount}',
    ru: 'Скачано {amount}', pl: 'Pobrano {amount}', 'pt-BR': 'Baixado: {amount}',
  }
  try {
    for (const language of LOCALE_CODES) {
      i18n.global.locale.value = language
      assert.equal(downloadAmount({ downloaded: 0, total: null }), received[language].replace('{amount}', '0 B'), language)
      assert.equal(downloadAmount({ downloaded: 2 * 1024 * 1024 }), received[language].replace('{amount}', '2.0 MB'), language)
      assert.equal(downloadRoute({ route: 'proxy', source: 'mirror.example' }), `${i18n.global.t('common.proxy')} · mirror.example`)
      assert.equal(downloadRate({ speed_bytes: null, phase: 'downloading' }), toolsUiDownloadRace['downloadRace.measuring'][language])
      for (const [key, copy] of Object.entries(toolsUiDownloadRace)) {
        assert.ok(copy[language], `${key} must include ${language}`)
        const message = i18n.global.t(`tools.${key}`, { count: 5 })
        assert.equal(message, copy[language].replace('{count}', '5'))
      }
      const unavailable = error['download.raceUnavailable'][language]
      assert.ok(unavailable?.trim(), language)
      assert.notEqual(unavailable, 'download.raceUnavailable')
      assert.equal(translateBackendMessage('download.raceUnavailable', language), unavailable, language)
    }
  } finally { i18n.global.locale.value = original }
})

test('primary rate is independent of a faster ranked source and zero differs from measuring', () => {
  const progress = { speed_bytes: 1024, phase: 'downloading', race: { primary_id: 18,
    sources: [{ id: 1, speed_bytes: 3072 }, { id: 18, speed_bytes: null }],
  } }
  assert.equal(downloadRate(progress), i18n.global.t('tools.downloadRace.measuring'))
  progress.race.sources[1].speed_bytes = 0
  assert.equal(downloadRate(progress), '0 KB/s')
})
