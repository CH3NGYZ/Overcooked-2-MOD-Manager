import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'
import { mapPublish } from '../src/locale/mapPublish/index.ts'
const read = path => readFileSync(new URL('../' + path, import.meta.url), 'utf8')

test('发布检查使用社区地图 topic，仓库只能由用户自行创建', () => {
  const backend = read('src-tauri/src/map_publish.rs')
  assert.match(backend, /PUBLISH_TOPICS: \[&str; 3\] = \["overcooked-2", "bepinex", "map"\]/)
  assert.match(backend, /topic == "map"/)
  for (const path of ['src-tauri/src/map_publish.rs', 'src-tauri/src/lib.rs', 'src/services/tauri.ts', 'src/composables/useMapPublish.ts', 'src/components/publish/PublishSettingsPopover.vue']) {
    assert.doesNotMatch(read(path), /create_publish_repository|createPublishRepository|creatingRepository|createRepository|set_repository_topics/, path)
  }
  assert.match(backend, /pub async fn save_publish_release/)
  assert.match(backend, /pub async fn publish_repository_topics/)
  assert.match(read('src/components/publish/PublishSettingsPopover.vue'), /repository.topics.includes\('map'\)/)
  for (const language of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
    assert.ok(mapPublish.manualRepositoryHint[language])
    assert.ok(mapPublish.repositoryScopeHint[language])
    assert.doesNotMatch(mapPublish.repositoryHint[language], /map-source/)
  }
})
