import assert from 'node:assert/strict'
import { readFileSync, readdirSync } from 'node:fs'
import { join } from 'node:path'
import test from 'node:test'

/** 源码根目录：用于扫描「绕过桥接层」的写法。 */
const SRC = new URL('../src/', import.meta.url)

/**
 * 后端错误 tag → 前端文案的翻译链路。
 *
 * 这是本轮 i18n 改造的核心：后端不再返回中文（`Err("请打开游戏并确认…")`），
 * 只返回 `error.<域>.<原因>` 形式的 tag（可带 `?k=v` 内联参数），
 * 由前端按当前语言翻译。
 *
 * 直接引真实模块：`src/locale/backend.ts` 只依赖 vue-i18n 与语言表，
 * 不碰 Vue 组件与 IPC，Node 里可直接执行。
 */
const { translateBackendMessage, isErrorTag, backendMessageOf } = await import(
  '../src/locale/backend.ts'
)
const { i18n } = await import('../src/locale/index.ts')

/** 切到某语言并翻译一个 tag。 */
function inLocale(locale, tag) {
  i18n.global.locale.value = locale
  return translateBackendMessage(tag)
}

test('同一 tag 在五种语言下各自翻译', () => {
  const tag = 'error.diyLevel.notRunning'
  assert.equal(inLocale('zh-CN', tag), '请打开游戏并确认 街机MOD 正在运行')
  assert.equal(inLocale('zh-TW', tag), '請開啟遊戲並確認主機實用工具正在執行')
  assert.match(inLocale('en', tag), /HostUtilities is running/)
  assert.match(inLocale('ko', tag), /호스트 유틸리티/)
  assert.match(inLocale('ja', tag), /ホストユーティリティ/)
})

test('内联参数被解析并插值', () => {
  // 后端产出形态：error.rename.nameTooLong?bytes=240&limit=238
  const text = inLocale('en', 'error.rename.nameTooLong?bytes=240&limit=238')
  assert.match(text, /240/, `参数未插值：${text}`)
  assert.match(text, /238/, `参数未插值：${text}`)
  // 中文下同样插值，且不是裸 tag
  const zh = inLocale('zh-CN', 'error.rename.nameTooLong?bytes=240&limit=238')
  assert.match(zh, /240.*238/, `中文参数未插值：${zh}`)
  assert.ok(!zh.includes('error.'), `不应显示裸 tag：${zh}`)
})

test('带路径参数的错误不会破坏插值（含反斜杠与冒号）', () => {
  const tag = 'error.backup.recycleFailed?reason=busy&path=D%3A%5Cbackups%5CDIYLevel.backup-1'
  const text = inLocale('en', tag)
  assert.match(text, /busy/)
  assert.match(text, /D:\\backups\\DIYLevel\.backup-1/, `路径未正确插值：${text}`)
})

test('未登记的 tag 原样返回，便于发现漏翻译', () => {
  const text = inLocale('en', 'error.notRegistered.anywhere')
  assert.equal(text, 'error.notRegistered.anywhere')
})

test('非 error.* 的中文串原样返回（迁移期兼容）', () => {
  // 尚未改造的命令仍返回中文整句，不能被当成 tag 硬翻
  const legacy = '备份完成: D:\\backups\\x'
  assert.equal(translateBackendMessage(legacy), legacy)
  assert.equal(isErrorTag(legacy), false)
})

test('backendMessageOf 收口 Error / 字符串 / 对象三种形态', () => {
  i18n.global.locale.value = 'en'
  assert.match(backendMessageOf(new Error('error.diyLevel.notRunning')), /HostUtilities/)
  assert.match(backendMessageOf('error.diyLevel.notRunning'), /HostUtilities/)
  assert.match(backendMessageOf({ message: 'error.diyLevel.notRunning' }), /HostUtilities/)
})

test('空值不产生文案', () => {
  assert.equal(translateBackendMessage(''), '')
  assert.equal(translateBackendMessage(null), '')
  assert.equal(translateBackendMessage(undefined), '')
})

/* ===== MOD 卸载与回收站域 ===== */

test('卸载与回收站的 tag 在五语下都有翻译', () => {
  const tags = [
    'error.mods.packageConfirmation',
    'error.mods.unmanagedConfirmation',
    'error.mods.unknownFiles',
    'error.mods.dllNotInPackage',
    'error.mods.dllNotFound',
    'error.mods.notInPlugins',
    'error.recycle.recordMissing',
    'error.recycle.fileSharedByOtherPackage',
    'error.recycle.insideModsDirectory',
    'error.recycle.partialFailure',
    'error.recycle.packageUpdated',
    'error.recycle.wrongGameDirectory',
    'message.mods.movedToRecycleBin',
    'message.mods.deleted',
    'message.mods.packageMovedToRecycleBin',
    'message.mods.packageDeleted',
  ]
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    for (const tag of tags) {
      const text = translateBackendMessage(tag)
      assert.notEqual(text, tag, `${locale}: ${tag} 未登记`)
      assert.ok(text.length > 0, `${locale}: ${tag} 翻译为空`)
    }
  }
})

test('回收站命令不再返回中文文案（源码级确认）', () => {
  const rust = readFileSync(
    new URL('../src-tauri/src/recycle_bin.rs', import.meta.url),
    'utf8',
  )
  for (const tag of [
    'error.recycle.recordMissing',
    'error.recycle.fileSharedByOtherPackage',
    'error.recycle.insideModsDirectory',
    'error.recycle.partialFailure',
    'error.recycle.packageUpdated',
    'error.recycle.wrongGameDirectory',
  ]) {
    assert.ok(rust.includes(tag), `recycle_bin.rs 应包含 tag：${tag}`)
  }
  for (const legacy of [
    '回收站记录不存在',
    '该文件仍由其他 MOD 包使用，不能移入回收站',
    '操作失败，部分文件保留在回收站，请查看日志',
  ]) {
    assert.ok(!rust.includes(legacy), `recycle_bin.rs 不应再含中文文案：${legacy}`)
  }
})

/* ===== 仓库源域 ===== */

test('仓库源导入的 tag 在五语下都有翻译', () => {
  const tags = [
    'error.source.addressEmpty',
    'error.source.addressInvalid',
    'error.source.addressHasCredentials',
    'error.source.builtinPathUnsafe',
    'error.source.builtinNotToml',
    'error.source.builtinFileMissing',
    'error.source.notBuiltin',
    'error.source.idOrNicknameEmpty',
    'error.source.proxyNotHttps',
    'error.source.proxyAddressInvalid',
    'error.source.tooLarge',
    'error.source.tomlInvalid',
  ]
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    for (const tag of tags) {
      const text = translateBackendMessage(tag)
      assert.notEqual(text, tag, `${locale}: ${tag} 未登记`)
      assert.ok(text.length > 0, `${locale}: ${tag} 翻译为空`)
    }
  }
})

/* ===== 前端不再直接显示后端 message（全局护栏）===== */

test('前端不直接显示后端 message（必须经过桥接层）', () => {
  /*
   * 这是「弹窗通知没多语言」的根因：只要有一处直接
   * `message.error(result.message)`，那条提示就永远是后端语言。
   * 桥接层存在后仍可能有人写回旧写法，所以扫源码守住。
   */
  const offenders = []
  const files = []
  const walk = (dir) => {
    for (const name of readdirSync(dir, { withFileTypes: true })) {
      const full = join(dir, name.name)
      if (name.isDirectory()) {
        if (name.name !== 'locale') walk(full)
      } else if (/\.(vue|ts)$/.test(name.name)) {
        files.push(full)
      }
    }
  }
  walk(SRC.pathname.replace(/^\/([A-Za-z]:)/, '$1'))

  for (const file of files) {
    const text = readFileSync(file, 'utf8').replace(/\r\n/g, '\n')
    for (const [index, line] of text.split('\n').entries()) {
      const trimmed = line.trimStart()
      if (trimmed.startsWith('//') || trimmed.startsWith('*')) continue
      // 裸显示后端 message：没有包在 translateBackendMessage / backendMessageOf 里
      if (/message\.(error|success|warning)\((result|outcome)\.message\)/.test(line)) {
        offenders.push(`${file.replace(/.*[\\/]src[\\/]/, 'src/')}:${index + 1}`)
      }
      if (/message\.(error|warning)\(reason instanceof Error \? reason\.message/.test(line)) {
        offenders.push(`${file.replace(/.*[\\/]src[\\/]/, 'src/')}:${index + 1}`)
      }
    }
  }
  assert.deepEqual(offenders, [], `这些位置绕过了翻译桥接：\n  ${offenders.join('\n  ')}`)
})

test('下载与归档的 tag 在五语下都有翻译', () => {
  const tags = [
    'error.download.cancelled',
    'error.download.empty',
    'error.download.incomplete',
    'error.download.httpStatus',
    'error.download.checksumFailed',
    'error.download.checksumMismatch',
    'error.download.chunkTooLong',
    'error.download.chunkLengthMismatch',
    'error.archive.invalidZipName',
    'error.archive.unsafePath',
    'error.archive.pathEscape',
    'error.archive.noInstallableContent',
    'error.archive.sourceEqualsTarget',
  ]
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    for (const tag of tags) {
      const text = translateBackendMessage(tag)
      assert.notEqual(text, tag, `${locale}: ${tag} 未登记`)
      assert.ok(text.length > 0, `${locale}: ${tag} 翻译为空`)
    }
  }
})

test('下载的 HTTP 状态与校验失败参数在五语下都插值', () => {
  const cases = [
    ['error.download.httpStatus?status=404', /404/],
    ['error.download.checksumMismatch?expected=abc&actual=def', /abc/],
    ['error.archive.unsafePath?entry=..%2Fevil', /\.\./],
  ]
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    for (const [tag, pattern] of cases) {
      const text = translateBackendMessage(tag)
      assert.match(text, pattern, `${locale}: ${tag} 参数未插值 → ${text}`)
    }
  }
})

/* ===== 框架管理域（BepInEx / DIYLevel）===== */

test('框架备份与卸载的 tag 在五语下都有翻译', () => {
  const tags = [
    'error.framework.bepinexMissing',
    'error.framework.bepinexOnlyResidue',
    'error.framework.alreadyInstalled',
    'error.framework.diyLevelMissing',
    'error.backup.notFound',
    'error.backup.outsideRoot',
    'error.backup.overlap',
    'error.backup.recycleFailed',
    'error.backup.noReclaimableFiles',
    'message.backup.created',
    'message.backup.bepInExRestored',
    'message.backup.diyLevelRestored',
    'message.backup.deleted',
    'message.framework.movedToRecycleBin',
    'message.framework.diyLevelMovedToRecycleBin',
    'message.framework.diyLevelDeleted',
  ]
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    for (const tag of tags) {
      const text = translateBackendMessage(tag)
      assert.notEqual(text, tag, `${locale}: ${tag} 未登记`)
      assert.ok(text.length > 0, `${locale}: ${tag} 翻译为空`)
    }
  }
})

test('框架 tag 的参数在每种语言下都插值', () => {
  const tag = 'error.backup.recycleFailed?reason=busy&path=D%3A%5Cbackups%5Cx'
  for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    i18n.global.locale.value = locale
    const text = translateBackendMessage(tag)
    assert.match(text, /busy/, `${locale}: reason 未插值`)
    assert.match(text, /backups/, `${locale}: path 未插值`)
  }
})

test('后端不再返回中文整句的框架错误（源码级确认）', () => {
  /*
   * 反向断言：这几个命令的错误路径必须已经是 tag，不能漏回中文。
   * 后端返回中文时前端怎么翻都翻不了 —— 这条守住「不要再写中文」。
   */
  const rust = readFileSync(
    new URL('../src-tauri/src/commands.rs', import.meta.url),
    'utf8',
  )
  for (const tag of [
    'error.framework.alreadyInstalled',
    'error.framework.bepinexMissing',
    'error.framework.bepinexOnlyResidue',
    'error.framework.diyLevelMissing',
    'error.backup.recycleFailed',
  ]) {
    assert.ok(rust.includes(tag), `commands.rs 应包含 tag：${tag}`)
  }
  // 已迁移的这几条中文不该再残留
  for (const legacy of [
    'BepInEx 已安装，如需覆盖请使用重新安装',
    'BepInEx 框架未安装（仅存在 MOD 目录），无需卸载',
  ]) {
    assert.ok(!rust.includes(legacy), `commands.rs 不应再含中文文案：${legacy}`)
  }
})
