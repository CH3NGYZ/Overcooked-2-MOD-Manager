import assert from 'node:assert/strict'
import test from 'node:test'

/** 状态 → 快捷动作：真实源码直接执行（不依赖 i18n / naive-ui）。 */
const { statusActionFor } = await import('../src/statusActions.ts')

test('可更新映射到「更新」动作', () => {
  const action = statusActionFor(['update_available'])
  assert.equal(action.kind, 'update')
  assert.equal(action.label, 'dialogs.update')
  // 与「可更新」标签同档，按钮与标签颜色一致
  assert.equal(action.tone, 'moderate')
})

test('仓库刷新失败映射到「刷新」动作（已安装 MOD 视角）', () => {
  // 后端扫描算出的标签 key
  const action = statusActionFor(['repository_unavailable'])
  assert.equal(action.kind, 'refresh')
  assert.equal(action.label, 'modsOnline.refresh')
  assert.equal(action.tone, 'moderate')
})

test('仓库刷新失败映射到「刷新」动作（仓库卡片视角）', () => {
  // MOD 安装页仓库卡片的标签 key 与上一条同义、不同名
  const action = statusActionFor(['refresh_failed'])
  assert.equal(action.kind, 'refresh')
  assert.equal(action.label, 'modsOnline.refresh')
})

test('绑定仓库缺失也给刷新按钮，且用严重档配色', () => {
  // 仓库从配置里消失：重新拉源文档可能找回它
  const action = statusActionFor(['bound_repository_missing'])
  assert.equal(action.kind, 'refresh')
  assert.equal(action.tone, 'critical')
})

test('一条 MOD 带多个标签时取第一个有动作的', () => {
  // 调用方传入的顺序按严重度排好，所以不需要在函数里再排序
  const action = statusActionFor(['duplicate', 'repository_unavailable'])
  assert.equal(action.kind, 'refresh')

  const update = statusActionFor(['update_available', 'repository_unavailable'])
  assert.equal(update.kind, 'update')
})

test('没有需要动作的状态时不产生按钮', () => {
  for (const labels of [
    ['latest'],
    ['refreshing'],
    ['unknown'],
    ['disabled'],
    ['prerelease'],
    ['duplicate'],
    ['load_warning'],
    ['dependency'],
    ['not_release_asset'],
    ['unbound'],
    [],
    undefined,
  ]) {
    assert.equal(statusActionFor(labels), undefined, JSON.stringify(labels))
  }
})

test('重复、加载警告这类需要人工处理的状态不给一键动作', () => {
  // 它们要么要用户删文件、要么要改环境，点一下解决不了，给按钮是误导
  assert.equal(statusActionFor(['duplicate', 'load_warning']), undefined)
})
