/**
 * 框架备份目录名的展示格式化。
 *
 * 备份目录名形如 `BepInEx.backup-20261029143033-<uuid>` /
 * `DIYLevel.backup-20261029143033-<uuid>`。
 *
 * ## 时间戳是 UTC
 *
 * 目录名由后端 `Utc::now().format("%Y%m%d%H%M%S")` 生成，**刻意用 UTC**：
 * 目录名要求与机器时区无关（同一台机器换时区后目录仍能正确排序、
 * 也不受夏令时影响）。所以展示时必须把它当 UTC 解析，再转成当前时区。
 *
 * 早期版本把它当裸时间直接显示，东八区用户会看到比实际早 8 小时的时间。
 *
 * ## 时区来源
 *
 * 用 `Date` 的本地时区：Windows 的 WebView2 与 macOS 的 WKWebView 都直接
 * 读系统时区设置，无需额外配置。纯函数、不依赖 Vue / i18n / IPC，
 * Node 测试可直接引用（测试里显式传 timeZone，不依赖运行机器）。
 */

/** 解析出的备份信息。 */
export interface BackupLabel {
  /** 备份时间，已按目标时区格式化；解析失败为空串。 */
  time: string
  /** 备份目录的完整路径（原样展示，便于用户复制或定位）。 */
  path: string
}

/**
 * 从备份目录名里取出 `YYYYMMDDHHMMSS`（UTC）时间戳。
 *
 * 目录名格式：`<任意前缀>.backup-<14 位时间戳>-<uuid>`。
 * 只认 14 位纯数字，避免把 uuid 里的数字误当时间。
 */
export function parseBackupTimestamp(backupPath: string): Date | null {
  // 取最后一段，兼容 Windows 反斜杠与 POSIX 斜杠
  const name = backupPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? ''
  const match = /backup-(\d{14})/.exec(name)
  if (!match) return null
  const raw = match[1]
  const year = Number(raw.slice(0, 4))
  const month = Number(raw.slice(4, 6))
  const day = Number(raw.slice(6, 8))
  const hour = Number(raw.slice(8, 10))
  const minute = Number(raw.slice(10, 12))
  const second = Number(raw.slice(12, 14))
  // 范围校验：月份 01-12、日 01-31、时分秒 00-59，其余视为解析失败
  if (
    month < 1 || month > 12
    || day < 1 || day > 31
    || hour > 23 || minute > 59 || second > 59
  ) {
    return null
  }
  // 目录名是 UTC：用 Date.UTC 构造，避免被本地时区二次偏移
  const date = new Date(Date.UTC(year, month - 1, day, hour, minute, second))
  // Date.UTC 会把 2 月 31 日之类溢出到下个月，回读校验拦掉
  if (date.getUTCFullYear() !== year || date.getUTCMonth() !== month - 1 || date.getUTCDate() !== day) {
    return null
  }
  return date
}

/**
 * 把 UTC 时间按指定时区格式化。
 *
 * `timeZone` 传 `undefined` 时用运行环境（浏览器/Node）的本地时区 ——
 * Windows WebView2 与 macOS WKWebView 都读系统时区。测试里显式传值，
 * 结果与测试机器时区无关。
 *
 * 输出固定 `YYYY-MM-DD HH:mm:ss`，不跟随界面语言（时间格式跨语言一致
 * 更不容易误读）。
 */
export function formatTimestampInZone(date: Date, timeZone?: string): string {
  const parts = new Intl.DateTimeFormat('en-CA', {
    timeZone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  }).formatToParts(date)
  const pick = (type: string) => parts.find((part) => part.type === type)?.value ?? ''
  // en-CA + hour12:false 在部分环境会把午夜给成 24，统一折成 00
  const hour = pick('hour') === '24' ? '00' : pick('hour')
  return `${pick('year')}-${pick('month')}-${pick('day')} ${hour}:${pick('minute')}:${pick('second')}`
}

/**
 * 备份行的展示文本：`时间 · 路径`（时间按本地时区显示）。
 *
 * 解析不出时间时只显示路径 —— 不编一个假时间，用户至少还能靠路径辨认。
 */
export function formatBackupLabel(backupPath: string, timeZone?: string): BackupLabel {
  const timestamp = parseBackupTimestamp(backupPath)
  return {
    time: timestamp ? formatTimestampInZone(timestamp, timeZone) : '',
    path: backupPath,
  }
}
