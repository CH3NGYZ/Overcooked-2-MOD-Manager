import assert from 'node:assert/strict'
import test from 'node:test'

/** 备份行格式化：真实源码直接执行（纯函数，无 IPC / Vue 依赖）。 */
const { parseBackupTimestamp, formatTimestampInZone, formatBackupLabel } = await import(
  '../src/backupLabel.ts'
)

/*
 * 备份目录名里的 14 位时间戳是 **UTC**（后端 `Utc::now()` 生成，
 * 刻意与时区无关以保证排序稳定）。展示时必须按 UTC 解析再转当前时区，
 * 否则东八区用户会看到早 8 小时的时间。
 *
 * 断言一律显式传 timeZone，结果与运行测试的机器时区无关。
 */

test('时间戳按 UTC 解析，再按目标时区展示', () => {
  const path = 'D:\\backups\\BepInEx.backup-20261029143033-abc'
  const date = parseBackupTimestamp(path)
  assert.ok(date instanceof Date)
  // 目录名里的 14:30:33 是 UTC
  assert.equal(date.toISOString(), '2026-10-29T14:30:33.000Z')

  // 东八区：+8 小时
  assert.equal(formatTimestampInZone(date, 'Asia/Shanghai'), '2026-10-29 22:30:33')
  // UTC 本身
  assert.equal(formatTimestampInZone(date, 'UTC'), '2026-10-29 14:30:33')
  // 纽约（10 月底仍在 EDT，UTC-4）
  assert.equal(formatTimestampInZone(date, 'America/New_York'), '2026-10-29 10:30:33')
})

test('跨日与跨年边界随目标时区正确滚动', () => {
  // UTC 23:30 → 东八区次日 07:30
  const lateNight = parseBackupTimestamp('/b/DIYLevel.backup-20261231233000-x')
  assert.equal(formatTimestampInZone(lateNight, 'Asia/Shanghai'), '2027-01-01 07:30:00')
  assert.equal(formatTimestampInZone(lateNight, 'UTC'), '2026-12-31 23:30:00')

  // UTC 00:30 → 纽约前一天 20:30
  const morning = parseBackupTimestamp('/b/BepInEx.backup-20260101003000-x')
  assert.equal(formatTimestampInZone(morning, 'America/New_York'), '2025-12-31 19:30:00')
})

test('本地时区缺省时也能格式化（WebView 读系统时区）', () => {
  const date = parseBackupTimestamp('/b/BepInEx.backup-20260615120000-x')
  const text = formatTimestampInZone(date)
  // 只断言形状：具体值取决于运行机器时区
  assert.match(text, /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)
})

test('无法解析时返回空串，不编造时间', () => {
  for (const path of [
    'D:\\backups\\SomeFolder',
    'D:\\backups\\BepInEx.backup-abc-123',
    // 13 位数字不是合法长度
    'D:\\backups\\BepInEx.backup-2026102914303-x',
    // 数值越界
    'D:\\backups\\BepInEx.backup-20261329143033-x',
    'D:\\backups\\BepInEx.backup-20261029253033-x',
    // 2 月 31 日：Date.UTC 会溢出到 3 月，必须拒绝
    'D:\\backups\\BepInEx.backup-20260231143033-x',
    '',
  ]) {
    assert.equal(parseBackupTimestamp(path), null, path)
  }
})

test('展示标签保留完整路径，时间单独给出', () => {
  const path = 'D:\\OC2ModManager\\backups\\DIYLevel.backup-20261029143033-abc'
  assert.deepEqual(formatBackupLabel(path, 'Asia/Shanghai'), {
    time: '2026-10-29 22:30:33',
    path,
  })
  // 解析不出时间时只给路径
  assert.deepEqual(formatBackupLabel('D:\\backups\\SomeFolder', 'Asia/Shanghai'), {
    time: '',
    path: 'D:\\backups\\SomeFolder',
  })
})
