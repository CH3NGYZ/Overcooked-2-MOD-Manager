import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const modsView = read('src/views/ModsLocalView.vue')
const levelsView = read('src/views/LevelsView.vue')
const modsProjection = read('src/localModGroups.ts')
const levelProjection = read('src/levelSetGroups.ts')
const levelStatusSummary = read('src/levelSetStatusSummary.ts')

test('both local management pages delegate group projection to pure domain modules', () => {
  assert.match(modsView, /buildLocalModGroups\(/)
  assert.match(modsView, /localModStatusUnits\(groups\.value\)/)
  assert.match(modsView, /summarizeLocalModStatuses\(statusUnitNodes\.value\)/)
  assert.match(modsProjection, /summarizeLocalModStatuses/)
  assert.match(modsView, /packageKeysMatchingStatus\(/)
  assert.match(modsView, /filterLocalModGroups\(groups\.value/)
  assert.doesNotMatch(modsView, /function modCategory\(|interface StatusUnitNode|const groups = computed<ModGroupNode\[\]>\(\(\) => \{/)

  assert.match(levelsView, /buildLevelSetGroups\(/)
  assert.match(levelsView, /summarizeLevelSetTags\(sortedSets\.value, tagsOf\)/)
  assert.match(levelsView, /summarizeLevelSetToggleStatus\(sortedSets\.value\)/)
  assert.doesNotMatch(levelsView, /function tagKeyOf\(|for \(const set of sortedSets\.value\)/)
  assert.doesNotMatch(levelsView, /const buckets = new Map<string, \{ label: string; sets: LevelSetInfo\[\] \}>\(\)/)
  assert.match(levelProjection, /levelSetGroup\(set, maps\)/)
  assert.match(levelStatusSummary, /tag:\$\{tag\.kind\}:\$\{tag\.text\}/)
  assert.match(modsProjection, /mods: ordered/)
  assert.match(modsProjection, /visibleMods: ordered\.filter/)
})
