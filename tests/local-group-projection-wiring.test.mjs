import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const modsView = read('src/views/ModsLocalView.vue')
const levelsView = read('src/views/LevelsView.vue')
const modsProjection = read('src/localModGroups.ts')
const levelProjection = read('src/levelSetGroups.ts')

test('both local management pages delegate group projection to pure domain modules', () => {
  assert.match(modsView, /buildLocalModGroups\(/)
  assert.match(modsView, /localModStatusUnits\(groups\.value\)/)
  assert.match(modsView, /packageKeysMatchingStatus\(/)
  assert.match(modsView, /filterLocalModGroups\(groups\.value/)
  assert.doesNotMatch(modsView, /function modCategory\(|interface StatusUnitNode|const groups = computed<ModGroupNode\[\]>\(\(\) => \{/)

  assert.match(levelsView, /buildLevelSetGroups\(/)
  assert.doesNotMatch(levelsView, /const buckets = new Map<string, \{ label: string; sets: LevelSetInfo\[\] \}>\(\)/)
  assert.match(levelProjection, /levelSetGroup\(set, maps\)/)
  assert.match(modsProjection, /mods: ordered/)
  assert.match(modsProjection, /visibleMods: ordered\.filter/)
})
