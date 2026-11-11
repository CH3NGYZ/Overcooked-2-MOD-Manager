import assert from 'node:assert/strict'
import { resolve } from 'node:path'
import test from 'node:test'
import { readText, sourceFiles } from './helpers/source-scan.mjs'

const root = resolve('src-tauri/src/models')
const read = file => readText(resolve(root, file))
const owners = {
  app: ['AppUpdateChannelEvent', 'AppUpdateChannelStatus', 'AppUpdateCheckResult', 'AppUpdateInfo', 'AppUpdatePhaseEvent', 'DashboardData', 'RustLog'],
  config: ['AppConfig', 'AuthConfig', 'LaunchConfig', 'PublicConfig', 'SettingsPatch', 'TokenPatch'],
  chef_preview: ['ChefPreviewBounds', 'ChefPreviewMesh', 'ChefPreviewScene'],
  diychef: ['LocalModSnapshot', 'ChefTarget', 'ChefSource', 'ChefItem', 'ChefHat', 'ChefReference', 'HatReference', 'ChefEntry', 'ChefSelection', 'ChefDocumentRow', 'ChefDiagnostic', 'ChefChange', 'ChefValidation', 'ChefDraft', 'ChefEditorSnapshot', 'ChefEditorOpen'],
  game: ['BepInExInfo', 'DiyLevelInfo', 'GameConfig', 'GameInfo', 'GameRuntime', 'WinhttpState'],
  levels: ['LevelSetFlattenCandidate', 'LevelSetFlattenReport', 'LevelSetImportResult', 'LevelSetInfo', 'LevelSetLayout', 'LevelSetPlugin', 'LevelSetScanProgress', 'LevelSetZipProgress', 'MapInstallOrigin', 'StagedMapSource', 'ZipLevelSet', 'ZipLevelSetPlugin'],
  maps: ['AssetVersionEntry', 'MapInfo', 'MapRelationSnapshot', 'MapRelationSnapshots', 'MapRepositoryConfig', 'RequiredFramework'],
  mod_repositories: ['ModDownloadMode', 'ModRepositoryBinding', 'ModRepositoryConfig', 'ModRepositoryExport', 'ModRepositoryBundle', 'ModRepositoryPatch'],
  mods: ['DllPluginInfo', 'DllRole', 'InstalledMod', 'InstalledModStatusLabel', 'ModNote', 'ModRecycleEntry', 'UninstallLocalModResult'],
  network: ['GitHubProxy', 'GitHubProxyConfig', 'GitHubProxyManifest', 'NetworkConfig', 'ProxyMode'],
  network_plan: ['NetworkPlanSelection', 'PlanOverride', 'PlanReference', 'PlanStep', 'RepositoryKind', 'RouteOverride'],
  operations: ['BepInExInstallResult', 'InstallConflict', 'OperationResult'],
  publish: ['PublishProvider', 'PublishCredentialConfig', 'MapPublishMetadata', 'PublishAsset', 'PublishAuthMode', 'PublishAuthPoll', 'PublishAuthState', 'PublishAuthorization', 'PublishDeleteReleaseRequest', 'PublishRelease', 'PublishRepository', 'PublishRepositoryList', 'PublishSaveRequest', 'PublishSessionCheck', 'PublishSessionOutcome', 'PublishTokens', 'PublishUploadProgress', 'PublishUploadRequest', 'PublishZipInspection'],
  relations: ['DependencySyncResult', 'RelationKind', 'ReleaseRelationOverride', 'RepositoryRelation'],
  releases: ['ReleaseAsset', 'ReleaseInfo', 'UpdateStatus'],
  repositories: ['AssetRule', 'AssetRuleType', 'AvatarRequest', 'Channel', 'CommunityDiscovery', 'CommunityMapRepository', 'ImportPreview', 'Provider', 'RepositorySource', 'RepositorySourceImportResult', 'StagedReleaseCache'],
}

const files = sourceFiles(root, ['.rs']).map(path => [path, readText(path)])

test('Shared models have one domain owner and keep the existing root interface', () => {
  const entry = read('../models.rs')
  assert.doesNotMatch(entry, /\b(?:fn|struct|enum|impl)\s|#\[serde/)
  const exports = Object.fromEntries([...entry.matchAll(/^pub use (\w+)::\{([^}]+)\};/gm)].map(([, owner, names]) => [
    owner, names.split(',').map(name => name.trim()).filter(Boolean).sort(),
  ]))
  for (const [, owner, name] of entry.matchAll(/^pub use (\w+)::(\w+);/gm)) {
    if (exports[owner] && !exports[owner].includes(name)) exports[owner].push(name)
  }
  for (const names of Object.values(exports)) names.sort()
  assert.deepEqual(Object.keys(exports).sort(), Object.keys(owners).sort())
  for (const [owner, names] of Object.entries(owners)) {
    assert.match(entry, new RegExp(`^(?:pub\\(crate\\) )?mod ${owner};$`, 'm'))
    assert.deepEqual(exports[owner], [...names].sort(), owner)
    const definitions = [...read(`${owner}.rs`).matchAll(/^pub (?:struct|enum) (\w+)/gm)].map(match => match[1]).sort()
    assert.deepEqual(definitions, [...names].sort(), owner)
    for (const name of names) {
      const found = files.filter(([, text]) => new RegExp(`^pub (?:struct|enum) ${name}\\b`, 'm').test(text)).map(([path]) => path)
      assert.deepEqual(found, [resolve(root, `${owner}.rs`)], name)
    }
  }
  assert.match(entry, /^pub\(crate\) use maps::default_snapshot_schema_version;$/m)
  for (const [path, text] of files) {
    assert.ok(text.trimEnd().split('\n').length < 1000, path)
    assert.doesNotMatch(text, /crate::commands::|#\[tauri::command\]|^[ \t]*(?:pub(?:\([^)]*\))?\s+)?static\s+\w+|\b(?:Mutex|RwLock|OnceLock)\s*</m, path)
  }
})

test('Model serde consumers share relation readers and defaults without changing wire shapes', () => {
  const hooks = {
    deserialize_relations: 'relations',
    relation_version: 'relations',
    default_true: 'config',
    default_max_release_count: 'repositories',
    default_download_threads: 'repositories',
    default_proxy_manifest_version: 'network',
    default_map_provider: 'maps',
    default_snapshot_schema_version: 'maps',
  }
  for (const [name, owner] of Object.entries(hooks)) {
    const definition = new RegExp(`\\bfn\\s+${name}\\s*(?:<[^;{}]*>)?\\s*\\(`, 'g')
    const found = files.flatMap(([path, text]) => [...text.matchAll(definition)].map(() => path))
    assert.deepEqual(found, [resolve(root, `${owner}.rs`)], name)
  }
  const relations = read('relations.rs')
  assert.match(relations, /pub\(super\) fn deserialize_relations/)
  assert.match(relations, /fn deserialize<D>\(deserializer: D\)/)
  assert.match(relations, /kind: RelationKind/)
  assert.match(relations, /RelationKind::from_token\(&entry\.kind\)/)
  assert.match(relations, /REPOSITORY_FIELD_NAMES\.contains\(&key\.as_str\(\)\)/)
  assert.match(relations, /crate::version_range::fold_bounds\(min_version, max_version\)/)
  assert.match(relations, /alias = "requires",\s*deserialize_with = "deserialize_relations"/)
  for (const [owner, count] of [['mod_repositories', 2], ['maps', 1]]) {
    const source = read(`${owner}.rs`)
    assert.match(source, /use super::relations::\{[^}]*\bdeserialize_relations\b[^}]*\};/)
    assert.equal([...source.matchAll(/deserialize_with = "deserialize_relations"/g)].length, count, owner)
    assert.match(source, /use super::config::default_true;/)
  }
  assert.match(read('network.rs'), /use super::config::default_true;/)
  assert.match(read('maps.rs'), /use super::repositories::\{[^}]*default_download_threads[^}]*default_max_release_count[^}]*\};/)
  const config = read('config.rs')
  assert.match(config, /serde\(skip, default = "crate::github_proxies::bundled_defaults"\)/)
  assert.match(config, /serde\(default = "crate::config::default_ui_zoom"\)/)
  assert.match(config, /crate::metadata_read::DEFAULT_MAP_INFO_CONCURRENCY/)
  assert.match(config, /pub tutorial_watched_steps: Option<Vec<String>>/)
  assert.match(read('mod_repositories.rs'), /pub relations: Option<Vec<RepositoryRelation>>/)
  assert.match(read('mods.rs'), /#\[serde\(rename_all = "lowercase"\)\]\s*pub enum DllRole/)
  assert.match(read('levels.rs'), /#\[serde\(flatten\)\]\s*pub info: DllPluginInfo/)
  assert.match(read('publish.rs'), /#\[serde\(tag = "status", rename_all = "snake_case"\)\]/)
  assert.match(read('game.rs'), /#\[default\]\s*Native/)
  for (const [owner, scope] of [['game', 'game_runtime_tests'], ['config', 'tutorial_progress_tests'], ['mods', 'dll_role_tests']]) {
    assert.match(read(`${owner}.rs`), new RegExp(`#\\[cfg\\(test\\)\\]\\s*mod ${scope};`))
    assert.match(read(`${owner}/${scope}.rs`), /#\[test\]/)
  }
})
