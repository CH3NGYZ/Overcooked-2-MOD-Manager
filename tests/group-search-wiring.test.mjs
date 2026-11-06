import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'

const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8')
test('搜索展示只读命中成员，完整包仍用于操作；地图同样先逐卡筛选再分组', () => {
  const mods = read('src/views/ModsLocalView.vue')
  assert.match(mods, /keys.add\(mod.dll_file\)/)
  assert.match(mods, /mods: ordered,\s+visibleMods: ordered.filter/)
  assert.match(mods, /filter\(pkg => pkg.visibleMods.length > 0\)/)
  assert.doesNotMatch(mods, /searchMatchedPackageKeys/)
  const body = read('src/components/ModGroupBody.vue')
  assert.match(body, /v-for="mod in pkg.visibleMods"/)
  assert.match(body, /<ModPackageBody :group-key="groupKey" :package-key="pkg.key"/)
  const members = read('src/components/ModPackageBody.vue')
  assert.match(members, /ctx.groups.find/)
  assert.match(members, /packages.find/)
  assert.match(members, /v-for="mod in pkg.visibleMods"/)
  assert.doesNotMatch(members, /v-for="mod in pkg.mods"/)
  assert.doesNotMatch(body, /v-for="mod in pkg.mods"/)
  assert.match(body, /ctx.togglePackageEnabled\(pkg, value\)/)
  const maps = read('src/views/LevelsView.vue')
  assert.match(maps, /filter\(\(set\) => matchesLocalSearch/)
  assert.match(maps, /for \(const set of visibleSets.value\)/)
  assert.match(read('src/components/LevelSetGroupBody.vue'), /ctx.groups.find/)
})

test('在线两页的源、分组与社区插槽按稳定身份响应式取节点，不捕获旧过滤结果', () => {
  const slot = read('src/components/SourceGroupSlot.vue')
  assert.match(slot, /computed\(\(\) => props.getTree\(\).find/)
  assert.match(slot, /computed\(\(\) => node.value\?\.groups.find/)
  for (const [path, count] of [['src/views/ModsOnlineView.vue', 6], ['src/views/MapsOnlineView.vue', 4]]) {
    const view = read(path)
    assert.equal((view.match(/<SourceGroupSlot :get-tree="\(\) => tree"/g) ?? []).length, count)
    assert.equal((view.match(/:group-key="group.key"/g) ?? []).length, 2)
    assert.doesNotMatch(view, /:key="search"/)
  }
})
