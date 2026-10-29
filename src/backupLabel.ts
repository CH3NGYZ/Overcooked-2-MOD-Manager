/**
 * 框架备份路径的展示格式化。
 *
 * 备份目录名形如 `BepInEx.backup-20261029143033-<uuid>` /
 * `DIYLevel.backup-20261029143033-<uuid>`，直接铺在界面里既长又难看懂。
 * 统一格式化成「时间 · 路径」：
 *
 *   `2026-10-29 14:30:33 · D:\...\backups\BepInEx.backup-20261029143033-abc`
 *
 * 纯函数、不依赖 Vue / i18n / IPC，Node 测试可直接引用。
 */

/** 解析出的备份信息。 */
export interface BackupLabel {
  /** 从目录名解析出的时间，格式 `YYYY-MM-DD HH:mm:ss`；解析失败为空串。 */
  time: string
  /** 备份目录的完整路径（原样展示，便于用户复制或定位）。 */
  path: string
}

/**
 * 从备份目录名里取出 `YYYYMMDDHHMMSS` 时间戳。
 *
 * 目录名格式：`<任意前缀>.backup-<14 位时间戳>-<uuid>`。
 * 只认 14 位纯数字，避免把 uuid 里的数字误当时间。
 */
export function parseBackupTime(backupPath: string): string {
  // 取最后一段，兼容 Windows 反斜杠与 POSIX 斜杠
  const name = backupPath.replace(/[\\/]+$/, '').split(/[\\/]/).pop() ?? ''
  const match = /backup-(\d{14})/.exec(name)
  if (!match) return ''
  const raw = match[1]
  const year = raw.slice(0, 4)
  const month = raw.slice(4, 6)
  const day = raw.slice(6, 8)
  const hour = raw.slice(8, 10)
  const minute = raw.slice(10, 12)
  const second = raw.slice(12, 14)
  // 简单范围校验：月份 01-12、日 01-31、时分秒 00-59，其余视为解析失败
  if (
    Number(month) < 1 || Number(month) > 12
    || Number(day) < 1 || Number(day) > 31
    || Number(hour) > 23 || Number(minute) > 59 || Number(second) > 59
  ) {
    return ''
  }
  return `${year}-${month}-${day} ${hour}:${minute}:${second}`
}

/**
 * 备份行的展示文本：`时间 · 路径`。
 *
 * 解析不出时间时只显示路径 —— 不编一个假时间，用户至少还能靠路径辨认。
 */
export function formatBackupLabel(backupPath: string): BackupLabel {
  return { time: parseBackupTime(backupPath), path: backupPath }
}
