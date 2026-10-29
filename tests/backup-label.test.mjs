import assert from 'node:assert/strict'
import test from 'node:test'

/** 备份行格式化：真实源码直接执行（纯函数，无 IPC / Vue 依赖）。 */
const { parseBackupTime, formatBackupLabel } = await import('../src/backupLabel.ts')

test('从备份目录名解析出时间（两种前缀、两种路径分隔符）', () => {
  // BepInEx 备份
  assert.equal(
    parseBackupTime('D:\\OC2ModManager\\backups\\BepInEx.backup-20261029143033-abc123'),
    '2026-10-29 14:30:33',
  )
  // DIYLevel 备份 + POSIX 路径
  assert.equal(
    parseBackupTime('/home/me/backups/DIYLevel.backup-20260101000000-deadbeef'),
    '2026-01-01 00:00:00',
  )
  // 结尾带分隔符也要能解析
  assert.equal(
    parseBackupTime('E:/b/DIYLevel.backup-20261231235959-x/'),
    '2026-12-31 23:59:59',
  )
})

test('无法解析时返回空串，不编造时间', () => {
  assert.equal(parseBackupTime('D:\\backups\\SomeFolder'), '')
  assert.equal(parseBackupTime('D:\\backups\\BepInEx.backup-abc-123'), '')
  // 13 位数字不是合法时间戳长度
  assert.equal(parseBackupTime('D:\\backups\\BepInEx.backup-2026102914303-x'), '')
  // 数值越界
  assert.equal(parseBackupTime('D:\\backups\\BepInEx.backup-20261329143033-x'), '')
  assert.equal(parseBackupTime('D:\\backups\\BepInEx.backup-20261029253033-x'), '')
  assert.equal(parseBackupTime(''), '')
})

test('展示标签保留完整路径，时间单独给出', () => {
  const path = 'D:\\OC2ModManager\\backups\\DIYLevel.backup-20261029143033-abc'
  assert.deepEqual(formatBackupLabel(path), { time: '2026-10-29 14:30:33', path })
})
