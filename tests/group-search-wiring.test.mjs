import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'

const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8')
test('搜索展示只读命中成员，完整包仍用于操作；地图同样先逐卡筛选再分组', () => {
  const mods = read('src/views/ModsLocalView.vue')
  assert.match(mods, /searchMatchedDllFiles/)
  const modProjection = read('src/localModGroups.ts')
  assert.match(modProjection, /mods: ordered,\s+visibleMods: ordered.filter/)
  assert.match(modProjection, /filter\(\(pkg\) => pkg.visibleMods.length > 0\)/)
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
  assert.match(maps, /buildLevelSetGroups\(/)
  assert.match(read('src/levelSetGroups.ts'), /for \(const set of sets\)/)
  assert.match(read('src/components/LevelSetGroupBody.vue'), /ctx.groups.find/)
})

test('在线两页的源、分组与社区插槽按稳定身份响应式取节点，不捕获旧过滤结果', () => {
  const directory = read('src/components/SourceDirectory.vue')
  assert.match(directory, /<SourceGroupSlot :get-tree="\(\) => nodes" :source-id="entry.id"/)
  assert.match(directory, /<SourceDirectory :nodes="nodes"/)
  const slot = read('src/components/SourceGroupSlot.vue')
  assert.match(slot, /computed\(\(\) => props.getTree\(\).find/)
  assert.match(slot, /computed\(\(\) => node.value\?\.groups.find/)
  for (const [path, count] of [['src/views/ModsOnlineView.vue', 5], ['src/views/MapsOnlineView.vue', 3]]) {
    const view = read(path)
    assert.equal((view.match(/<SourceGroupSlot :get-tree="\(\) => tree"/g) ?? []).length, count)
    assert.equal((view.match(/:group-key="group.key"/g) ?? []).length, 2)
    assert.doesNotMatch(view, /:key="search"/)
  }
})
