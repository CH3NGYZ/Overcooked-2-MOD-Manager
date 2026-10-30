import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 关卡集状态判定（`src/levelSetStatus.ts`）。
 *
 * 这一层是从「MOD 状态」里照搬结构、但**换了身份来源**的部分：
 * MOD 靠 GUID，关卡集靠 `levelSetName + author`。所以它必须独立测 ——
 * 直接复用 modStatus 的测试会漏掉「uid 不能当身份」这类专属陷阱。
 */
const {
  isSameLevelSet,
  levelSetState,
  levelSetDisplayName,
  mapVersionForLevelSet,
} = await import('../src/levelSetStatus.ts')

function set(overrides = {}) {
  return {
    dir_name: 'RW_LevelSet',
    enabled: true,
    name: 'RW_LevelSet',
    name_zh: 'RW_关卡合集',
    author: 'RW',
    version: '0.73',
    uid: 'e34b5584',
    level_count: 10,
    ...overrides,
  }
}

test('身份比较用 author + levelSetName，忽略大小写与空白', () => {
  assert.ok(isSameLevelSet({ name: 'RW_LevelSet', author: 'RW' }, { name: 'rw_levelset ', author: ' rw' }))
  assert.ok(!isSameLevelSet({ name: 'RW_LevelSet', author: 'RW' }, { name: 'RW_LevelSet', author: 'Someone' }))
  assert.ok(!isSameLevelSet({ name: 'A', author: 'RW' }, { name: 'B', author: 'RW' }))
})

test('uid 随版本变化，不能当身份', () => {
  // 同一地图的两个版本：uid 不同，但必须认成同一个
  const v1 = { name: 'RW_LevelSet', author: 'RW', uid: 'aaa' }
  const v2 = { name: 'RW_LevelSet', author: 'RW', uid: 'bbb' }
  assert.ok(isSameLevelSet(v1, v2), 'uid 不同也必须认成同一地图')
})

test('停用优先于一切 —— 关掉的地图不该催用户更新', () => {
  assert.equal(levelSetState(set({ enabled: false, version: '0.1' }), {
    tag: 'v9.9.9', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'disabled')
})

test('解析失败优先于版本比较', () => {
  assert.equal(levelSetState(set({ error: 'levelSet.infoMissing' }), null), 'unreadable')
})

test('没有绑定仓库时是未绑定，不猜版本', () => {
  assert.equal(levelSetState(set(), undefined), 'unbound')
  assert.equal(levelSetState(set(), null), 'unbound')
})

test('本地版本低于 Release tag → 可更新', () => {
  assert.equal(levelSetState(set({ version: '0.73' }), {
    tag: 'v0.74', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'update_available')
})

test('版本一致 → 最新版（tag 的 v 前缀要能剥掉）', () => {
  assert.equal(levelSetState(set({ version: '0.73' }), {
    tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'latest')
  assert.equal(levelSetState(set({ version: 'v0.73' }), {
    tag: '0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'latest')
})

test('本地版本反而更高 → ahead，不显示成待更新', () => {
  // 作者的预发布版本，或仓库 tag 回退
  assert.equal(levelSetState(set({ version: '0.80' }), {
    tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'ahead')
})

test('任一侧版本缺失 → unknown，不误判为最新', () => {
  assert.equal(levelSetState(set({ version: '' }), {
    tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW',
  }), 'unknown')
})

test('按身份找仓库版本，找不到就是未绑定', () => {
  const refs = [
    { tag: 'v1.0', levelSetName: 'Other', author: 'RW' },
    { tag: 'v0.73', levelSetName: 'RW_LevelSet', author: 'RW' },
  ]
  assert.equal(mapVersionForLevelSet(set(), refs)?.tag, 'v0.73')
  assert.equal(mapVersionForLevelSet(set({ name: 'Missing' }), refs), undefined)
  // 集名为空（解析失败）时不做身份匹配，避免把所有空名条目串在一起
  assert.equal(mapVersionForLevelSet(set({ name: '' }), refs), undefined)
})

test('展示名按语言三级回退', () => {
  assert.equal(levelSetDisplayName(set(), 'zh-CN'), 'RW_关卡合集')
  assert.equal(levelSetDisplayName(set(), 'en'), 'RW_LevelSet')
  // 没有中文名时英文优先
  assert.equal(levelSetDisplayName(set({ name_zh: null }), 'zh-CN'), 'RW_LevelSet')
  // 两者都空时退回目录名
  assert.equal(levelSetDisplayName(set({ name: '', name_zh: null }), 'zh-CN'), 'RW_LevelSet')
})
