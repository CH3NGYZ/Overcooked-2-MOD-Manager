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
 * 只返回 tag（可带 `?k=v` 内联参数），由前端按当前语言翻译。
 *
 * tag 形态以**第一段是域**为准（`mods.dllNotFound`、`game.notFound`）。
 * 早期批次写成 `error.<域>.<原因>`（`error` 本身就是域），两种都支持 ——
 * 判定统一走查表，见 `src/locale/backend.ts` 的 `isErrorTag`。
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
  const tag = 'diyLevel.notRunning'
  assert.equal(inLocale('zh-CN', tag), '请打开游戏并确认 街机MOD 正在运行')
  assert.equal(inLocale('zh-TW', tag), '請開啟遊戲並確認主機實用工具正在執行')
  assert.match(inLocale('en', tag), /HostUtilities is running/)
  assert.match(inLocale('ko', tag), /호스트 유틸리티/)
  assert.match(inLocale('ja', tag), /ホストユーティリティ/)
})

test('裸域 tag 直接可翻译', () => {
  // 后端统一发「第一段是域」的裸 tag，不需要任何前缀转换
  assert.match(inLocale('en', 'mods.higherVersionPresent'), /newer version/i)
  assert.match(inLocale('en', 'diyLevel.notRunning'), /HostUtilities/)
  assert.match(inLocale('en', 'framework.cleanupFailed'), /clean up|BepInEx/i)
})

test('内联参数被解析并插值', () => {
  // 后端产出形态：rename.nameTooLong?bytes=240&limit=238
  const text = inLocale('en', 'rename.nameTooLong?bytes=240&limit=238')
  assert.match(text, /240/, `参数未插值：${text}`)
  assert.match(text, /238/, `参数未插值：${text}`)
  // 中文下同样插值，且不是裸 tag
  const zh = inLocale('zh-CN', 'rename.nameTooLong?bytes=240&limit=238')
  assert.match(zh, /240.*238/, `中文参数未插值：${zh}`)
  assert.ok(!zh.includes('rename.'), `不应显示裸 tag：${zh}`)
})

test('带路径参数的错误不会破坏插值（含反斜杠与冒号）', () => {
  const tag = 'backup.recycleFailed?reason=busy&path=D%3A%5Cbackups%5CDIYLevel.backup-1'
  const text = inLocale('en', tag)
  assert.match(text, /busy/)
  assert.match(text, /D:\\backups\\DIYLevel\.backup-1/, `路径未正确插值：${text}`)
})

test('未登记的 tag 原样返回，便于发现漏翻译', () => {
  const text = inLocale('en', 'nothing.notRegistered')
  assert.equal(text, 'nothing.notRegistered')
})

test('非 tag 的中文串原样返回（迁移期兼容）', () => {
  // 尚未改造的命令仍返回中文整句，不能被当成 tag 硬翻
  const legacy = '备份完成: D:\\backups\\x'
  assert.equal(translateBackendMessage(legacy), legacy)
  assert.equal(isErrorTag(legacy), false)
})

test('backendMessageOf 收口 Error / 字符串 / 对象三种形态', () => {
  i18n.global.locale.value = 'en'
  assert.match(backendMessageOf(new Error('diyLevel.notRunning')), /HostUtilities/)
  assert.match(backendMessageOf('diyLevel.notRunning'), /HostUtilities/)
  assert.match(backendMessageOf({ message: 'diyLevel.notRunning' }), /HostUtilities/)
})

test('空值不产生文案', () => {
  assert.equal(translateBackendMessage(''), '')
  assert.equal(translateBackendMessage(null), '')
  assert.equal(translateBackendMessage(undefined), '')
})

/* ===== MOD 卸载与回收站域 ===== */

test('卸载与回收站的 tag 在五语下都有翻译', () => {
  const tags = [
    'mods.packageConfirmation',
    'mods.unmanagedConfirmation',
    'mods.unknownFiles',
    'mods.dllNotInPackage',
    'mods.dllNotFound',
    'mods.notInPlugins',
    'recycle.recordMissing',
    'recycle.fileSharedByOtherPackage',
    'recycle.insideModsDirectory',
    'recycle.partialFailure',
    'recycle.packageUpdated',
    'recycle.wrongGameDirectory',
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
    'recycle.recordMissing',
    'recycle.fileSharedByOtherPackage',
    'recycle.insideModsDirectory',
    'recycle.partialFailure',
    'recycle.packageUpdated',
    'recycle.wrongGameDirectory',
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
    'source.addressEmpty',
    'source.addressInvalid',
    'source.addressHasCredentials',
    'source.builtinPathUnsafe',
    'source.builtinNotToml',
    'source.builtinFileMissing',
    'source.notBuiltin',
    'source.idOrNicknameEmpty',
    'source.proxyNotHttps',
    'source.proxyAddressInvalid',
    'source.tooLarge',
    'source.tomlInvalid',
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
   * 这是「弹窗通知没多语言」的根因，也是用户实际报过的两个症状：
   *
   *   1. `message.success(result.message || t('兜底'))` —— 点了「推送到
   *      街机MOD」，toast 直接显示裸 tag `rename.pushed`；
   *   2. `error.value = reason instanceof Error ? reason.message : ...`
   *      —— 页面上的错误条显示后端 tag。
   *
   * 两种形态的共性是：后端字符串被**存进**某个会渲染的位置，中间没经过
   * 桥接。所以扫描要盯「存进展示位置」这个动作，而不只是 toast 调用。
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

  /*
   * 三种合法写法，避免误报：
   *
   *   1. 行内桥接：语句里出现 translateBackendMessage / backendMessageOf；
   *   2. 收口桥接：值交给**内部已桥接**的函数（如 markFailed）；
   *   3. 前端日志：`writeFrontendLog` / `console.*` 不是界面文案，不翻译。
   *
   * 跨行调用（`message.success(result.message\n ? translate…`）也要认，
   * 所以判断的是「当前行 + 后几行」而不是单行。
   */
  const BRIDGED = /\b(translateBackendMessage|backendMessageOf)\b/
  const SINK_FUNCTIONS = ['markFailed']
  const NOT_UI = /writeFrontendLog|console\.(log|warn|error|info|debug)/

  /** 会渲染到界面的接收位置。 */
  const DISPLAY_SINKS = [
    /message\.(error|success|warning|info)\(/,
    /\bphaseText\b/,
    /\bmarkFailed\(/,
    /failures\.value/,
    /\berror\.value\s*=/,
    /\blastErrors\.value\b/,
  ]
  /** 后端字符串的来源。 */
  const BACKEND_VALUES = /(result|outcome|confirmed)\.message|reason instanceof Error \? reason\.message|String\(reason\)|String\(error\)/
  /** 逐个接收点扫描用：每次用完重置 lastIndex，避免跨行/跨文件漂移。 */
  const SINK_SCAN = DISPLAY_SINKS.map((re) => new RegExp(re.source, 'g'))

  for (const file of files) {
    const text = readFileSync(file, 'utf8').replace(/\r\n/g, '\n')
    const lines = text.split('\n')
    for (let i = 0; i < lines.length; i += 1) {
      const line = lines[i].trimStart()
      if (line.startsWith('//') || line.startsWith('*') || line.startsWith('/*')) continue
      if (NOT_UI.test(lines[i])) continue
      if (!DISPLAY_SINKS.some((re) => re.test(lines[i]))) continue

      /*
       * 逐个接收点判断，并且**从接收点本身**开始取语句，不先看整行。
       *
       * 旧写法是「本行出现过桥接就整行放行」，于是同一行两臂的三元
       * `result.ok ? message.success(result.message || …) : message.error(translate…)`
       * 里，裸用法被另一臂的桥接洗白 —— 护栏报绿，界面却弹出原始 tag
       * `mods.disabled`。现在每个接收点各自取语句：谁裸谁上榜。
       */
      let reported = false
      for (const sink of SINK_SCAN) {
        sink.lastIndex = 0
        let match
        while (!reported && (match = sink.exec(lines[i])) !== null) {
          // 接收点本身就在桥接函数里（markFailed 内部已翻译）
          if (SINK_FUNCTIONS.some((fn) => match[0].startsWith(fn))) continue
          const statement = collectStatement(lines, i, match.index)
          if (BRIDGED.test(statement)) continue
          if (!BACKEND_VALUES.test(statement)) continue
          offenders.push(`${file.replace(/.*[\\/]src[\\/]/, 'src/')}:${i + 1}  ${lines[i].trim().slice(0, 90)}`)
          reported = true
        }
      }
    }
  }
  assert.deepEqual(offenders, [], `这些位置绕过了翻译桥接：\n  ${offenders.join('\n  ')}`)
})

/**
 * 从第 `start` 行起，取出属于同一个表达式的文本。
 *
 * 规则：一直读到**未闭合的括号归零**且遇到语句终止符（`;` `,` 或三元分支
 * 的 `?` `:`）为止。这样 `cond ? a : b` 的两臂各自独立，互相不会把对方的
 * 桥接借过来。
 */
function collectStatement(lines, start, column = 0) {
  let depth = 0
  let out = ''
  for (let i = start; i < lines.length; i += 1) {
    const line = lines[i]
    // 首行从接收点所在列开始：同一行另一臂的桥接不能被读进当前语句
    const from = i === start ? column : 0
    for (let c = from; c < line.length; c += 1) {
      const ch = line[c]
      if (ch === '(') depth += 1
      else if (ch === ')') depth -= 1
      // 深度归零后遇到分支/语句分隔符，就到此为止
      if (depth <= 0 && (ch === '?' || ch === ':' || ch === ';' || ch === ',')) {
        return out
      }
      out += ch
    }
    out += '\n'
    // 上一行是完整语句且不以运算符结尾时结束
    if (depth <= 0 && !/[?+:.,&|(]$/.test(line.trimEnd())) return out
  }
  return out
}

test('下载与归档的 tag 在五语下都有翻译', () => {
  const tags = [
    'download.cancelled',
    'download.empty',
    'download.incomplete',
    'download.httpStatus',
    'download.checksumFailed',
    'download.checksumMismatch',
    'download.chunkTooLong',
    'download.chunkLengthMismatch',
    'archive.invalidZipName',
    'archive.unsafePath',
    'archive.pathEscape',
    'archive.noInstallableContent',
    'archive.sourceEqualsTarget',
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
    ['download.httpStatus?status=404', /404/],
    ['download.checksumMismatch?expected=abc&actual=def', /abc/],
    ['archive.unsafePath?entry=..%2Fevil', /\.\./],
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
    'framework.bepinexMissing',
    'framework.bepinexOnlyResidue',
    'framework.alreadyInstalled',
    'framework.diyLevelMissing',
    'backup.notFound',
    'backup.outsideRoot',
    'backup.overlap',
    'backup.recycleFailed',
    'backup.noReclaimableFiles',
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
  const tag = 'backup.recycleFailed?reason=busy&path=D%3A%5Cbackups%5Cx'
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
    'framework.alreadyInstalled',
    'framework.bepinexMissing',
    'framework.bepinexOnlyResidue',
    'framework.diyLevelMissing',
    'backup.recycleFailed',
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
