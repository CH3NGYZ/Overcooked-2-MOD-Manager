import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')
const cardStatus = read('src/mapCardStatus.ts')
const levelStatus = read('src/levelSetStatus.ts')
const mapsView = read('src/views/MapsOnlineView.vue')
const cardTests = read('tests/level-set-status.test.mjs')
const originTests = read('tests/map-install-origin.test.mjs')

test('在线地图卡片投影只有 mapCardStatus.ts 持有，且复用地图身份与版本规则', () => {
  assert.match(cardStatus, /export function mapCardState/)
  assert.match(cardStatus, /export function mapCardStatus/)
  assert.match(cardStatus, /export function mapCardAction/)
  assert.match(cardStatus, /from '\.\/levelSetStatus\.ts'/)
  assert.match(cardStatus, /levelSetForMap\(/)
  assert.match(cardStatus, /mapUpdateState\(/)

  assert.doesNotMatch(levelStatus, /export function mapCardState/)
  assert.doesNotMatch(levelStatus, /export function mapCardStatus/)
  assert.doesNotMatch(levelStatus, /export function mapCardAction/)
  assert.match(mapsView, /from '\.\.\/mapCardStatus'/)
  assert.match(cardTests, /mapCardStatus\.ts/)
  assert.match(originTests, /mapCardStatus\.ts/)
})
