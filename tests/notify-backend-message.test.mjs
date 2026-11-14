import assert from 'node:assert/strict'
import { readdirSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import test from 'node:test'

// Run the real notify/locale chain; only the UI provider and game gate are services.
registerHooks({
  resolve(specifier, context, next) {
    if (specifier === 'naive-ui') return { url: 'fixture:notify-ui', shortCircuit: true }
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try { return next(`${specifier}.ts`, context) } catch { /* original below */ }
    }
    return next(specifier, context)
  },
  load(url, context, next) {
    if (url === 'fixture:notify-ui') return { format: 'module', shortCircuit: true, source: `
      export const useMessage = () => Object.fromEntries(['create','info','success','warning','error','loading','destroyAll']
        .map(kind => [kind, (content, options) => {
          const call = {kind, content, options, destroy(){}}
          globalThis.__notifyMessages.push(call)
          return call
        }]))` }
    if (url.endsWith('/src/composables/useGameOperationGate.ts')) return {
      format: 'module', shortCircuit: true, source: 'export const gameOperationNotice = {value: false}',
    }
    return next(url, context)
  },
})

const { useNotify } = await import('../src/useNotify.ts')
const { i18n } = await import('../src/locale/index.ts')
const { backendMessageOf } = await import('../src/locale/backend.ts')
const { messageDuration, messageNeedsCloseButton } = await import('../src/notifyDuration.ts')
const { gameOperationNotice } = await import('../src/composables/useGameOperationGate.ts')

test('产品通知只能通过useNotify使用Naive UI消息服务', () => {
  const root = fileURLToPath(new URL('../src/', import.meta.url))
  const bypasses = readdirSync(root, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile() && /\.(?:vue|ts)$/.test(entry.name))
    .map(entry => join(entry.parentPath, entry.name).replaceAll('\\', '/'))
    .filter(path => !path.endsWith('/src/useNotify.ts'))
    .filter(path => /\buseMessage\b/.test(readFileSync(path, 'utf8')))
  assert.deepEqual(bypasses, [])
  const dialog = readFileSync(new URL('../src/components/levelPreview/LevelPreviewDialog.vue', import.meta.url), 'utf8')
  assert.match(dialog, /const message = useNotify\(\)/)
})

test('每个通知入口使用同一真实翻译器，按最终五语文案估算时长且可重复桥接', () => {
  const reason = '另一个程序正在使用此文件，进程无法访问。 (os error 32): C:\\配置\\config.toml?x=a=b&rate=100%+'
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    globalThis.__notifyMessages = []
    const notify = useNotify()
    const text = `MOD源: ${i18n.global.t('config.writeFailedReason', { reason })}`
    for (const kind of ['create', 'info', 'success', 'warning', 'error', 'loading']) {
      for (const content of [
        `MOD源: config.writeFailedReason: ${reason}`,
        `MOD源: config.writeFailedReason?reason=${encodeURIComponent(reason)}`,
        backendMessageOf(new Error(`MOD源: config.writeFailedReason: ${reason}`)),
      ]) {
        const result = notify[kind](content)
        assert.equal(result, globalThis.__notifyMessages.at(-1))
        assert.equal(result.content, text)
        assert.equal(result.options.duration, kind === 'loading' ? 0 : messageDuration(text))
        if (kind !== 'loading') {
          assert.equal(result.options.closable, messageNeedsCloseButton(text))
          assert.equal(result.options.keepAliveOnHover, true)
        }
      }
    }
  }
})

test('通知保留技术详情和render函数，尊重选项及game running公共拦截', () => {
  globalThis.__notifyMessages = []
  i18n.global.locale.value = 'en'
  const notify = useNotify()
  const render = () => null
  assert.equal(notify.warning(render, { duration: 99, closable: false }).content, render)
  assert.equal(globalThis.__notifyMessages.at(-1).options.duration, 99)
  assert.equal(globalThis.__notifyMessages.at(-1).options.closable, false)
  const technical = 'File: config.toml?mode=a=b&x=c — https://source.addressInvalid/a'
  assert.equal(notify.error(technical).content, technical)
  gameOperationNotice.value = false
  notify.error('MOD源: game.running')
  assert.equal(gameOperationNotice.value, true)
  assert.equal(globalThis.__notifyMessages.at(-1).content, `MOD源: ${i18n.global.t('game.running')}`)
  const result = notify.error('config.futureWriteFailure?reason=sharing%20violation%20(os%20error%2032)')
  assert.ok(!result.content.includes('config.futureWriteFailure'))
  assert.match(result.content, /sharing violation \(os error 32\)/)
  notify.destroyAll()
  assert.equal(globalThis.__notifyMessages.at(-1).kind, 'destroyAll')
})

test('源错误的opaque参数重复桥接不误翻或误开游戏运行拦截', () => {
  globalThis.__notifyMessages = []
  const notify = useNotify()
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    for (const [tag, parameter] of [['source.idDuplicateValue', 'id'], ['source.builtinFileMissing', 'path'], ['config.repositoryLabeled', 'name']]) {
      for (const value of ['game.running', 'proxy.futureId']) {
        gameOperationNotice.value = false
        const text = `MOD源: ${backendMessageOf(`${tag}?${parameter}=${encodeURIComponent(value)}`)}`
        const expected = `MOD源: ${i18n.global.t(tag, { [parameter]: value })}`
        assert.equal(text, expected)
        assert.equal(notify.error(text).content, expected)
        assert.equal(notify.error(backendMessageOf(text)).content, expected)
        assert.equal(gameOperationNotice.value, false)
      }
    }
  }
})
