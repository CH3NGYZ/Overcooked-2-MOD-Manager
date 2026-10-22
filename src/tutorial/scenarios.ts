import type {
  Dashboard,
  InstalledMod,
  Repository,
  RepositorySource,
  TutorialScenario,
  TutorialRepositoryConfig,
} from '../types'
import { repositoryName } from '../repositoryNames'

export type TutorialTranslate = (source: string) => string

export function tutorialRepository(
  template: TutorialRepositoryConfig,
  id: string,
  sourceId: string,
  version: string,
  assetName: string,
  translate: TutorialTranslate,
  language: string,
): Repository {
  // Only release metadata is simulated. Every editable MOD setting comes from
  // the bundled source parser and is cloned before a tutorial scene uses it.
  const repository = JSON.parse(JSON.stringify(template)) as TutorialRepositoryConfig
  const tag = `v${version}`
  const host = repository.provider === 'github' ? 'github.com' : 'gitee.com'
  const homepage = `https://${host}/${encodeURIComponent(repository.username)}/${encodeURIComponent(repository.repository)}`
  return {
    ...repository,
    id,
    source_id: sourceId,
    builtin: true,
    cached_releases: [{
      tag_name: tag,
      version,
      name: `${repositoryName(repository, language)} ${tag}`,
      body: translate('教程示例 Release'),
      published_at: '2026-01-02T00:00:00Z',
      prerelease: false,
      draft: false,
      // The real asset model requires an address for file matching. Derive it
      // from the structured identity; tutorial IDs never enter real downloads.
      assets: [{ name: assetName, size: 8388608, download_url: `${homepage}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(assetName)}` }],
    }],
  }
}

export function tutorialMod(
  overrides: Partial<InstalledMod> & Pick<InstalledMod, 'name' | 'dll_file'>,
  translate: TutorialTranslate,
): InstalledMod {
  return {
    repository_id: 'tutorial-mod',
    repository_ids: ['tutorial-mod'],
    package_id: undefined,
    package_name: undefined,
    package_mod_count: undefined,
    title: undefined,
    description: translate('首次启动教程中的虚拟 MOD 数据。'),
    product_name: overrides.name,
    original_filename: overrides.dll_file,
    guid: `com.example.oc2.${overrides.name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`,
    plugin_name: overrides.name,
    company: 'OC2 MOD Manager',
    copyright: undefined,
    enabled: true,
    version: '1.1.0',
    version_source: 'BepInPlugin',
    update_available: false,
    duplicate_guid: false,
    duplicate_version: false,
    load_warning: undefined,
    note: undefined,
    status: 'bound',
    ...overrides,
  }
}

export function cloneDashboard(value: Dashboard): Dashboard {
  return JSON.parse(JSON.stringify(value)) as Dashboard
}

export function createTutorialDashboard(
  dashboard: Dashboard | null,
  scenario: TutorialScenario,
  translate: TutorialTranslate,
  builtinRepositories: TutorialRepositoryConfig[],
  language: string,
): Dashboard | null {
  if (!dashboard || scenario === 'live') return dashboard

  const value = cloneDashboard(dashboard)
  const sourceId = 'tutorial-source'
  const childSourceId = 'tutorial-source-child'
  const hostTemplate = builtinRepositories[0]
  const managerTemplate = builtinRepositories[1]
  const hostUtilities = hostTemplate ? tutorialRepository(hostTemplate, 'tutorial-hostutilities', sourceId,
    '2.4.0', hostTemplate.asset_rules.find((rule) => rule.type === 'exact')?.value || 'HostUtilities.zip', translate, language) : undefined
  const configManager = managerTemplate ? tutorialRepository(managerTemplate, 'tutorial-config-manager', childSourceId,
    '1.0.0', 'BepInEx.ConfigurationManager_BepInEx5_v1.0.zip', translate, language) : undefined
  const bundledSource = dashboard.config.repository_sources.find((source) => source.id === hostTemplate?.source_id)
  const source: RepositorySource = {
    ...bundledSource,
    id: sourceId,
    nickname: bundledSource?.nickname || (hostUtilities ? repositoryName(hostUtilities, language) : ''),
    url: 'repositories.toml',
    parent_id: undefined,
    builtin: true,
  }
  const childSource: RepositorySource = {
    id: childSourceId,
    nickname: configManager?.group || (configManager ? repositoryName(configManager, language) : ''),
    nickname_i18n: configManager?.group_i18n,
    url: bundledSource?.url || 'repositories-azhe-hostutilities.toml',
    parent_id: sourceId,
    builtin: true,
  }
  const repositories = [hostUtilities, configManager].filter((repository): repository is Repository => !!repository)
  const repositorySources = repositories.length ? [source, childSource] : []

  value.config = {
    ...value.config,
    repositories: [],
    repository_sources: [],
  }
  value.mods = []
  value.updates = []
  value.game = {
    ...value.game,
    game_exists: true,
    kitchen_running: false,
    path: 'C:\\Games\\Overcooked2',
  }

  if (scenario === 'framework-missing' || scenario === 'framework-channel' || scenario === 'framework-installing') {
    value.game.bepinex = { installed: false, complete: false, has_config: false, version: undefined, version_source: undefined, backups: [] }
    value.game.winhttp_state = 'missing'
  } else if (scenario === 'framework-ready' || scenario === 'framework-backup') {
    value.game.bepinex = {
      installed: true,
      complete: true,
      has_config: true,
      version: '5.4.23.5',
      version_source: 'BepInEx core metadata',
      backups: scenario === 'framework-backup'
        ? ['BepInEx-backup-2026-01-01', 'BepInEx-backup-2025-12-18']
        : ['BepInEx-backup-2026-01-01'],
    }
    value.game.winhttp_state = 'modenabled'
  } else {
    value.config = {
      ...value.config,
      repositories,
      repository_sources: repositorySources,
    }
    value.game.bepinex = {
      installed: true,
      complete: true,
      version: '5.4.23.5',
      version_source: 'BepInEx core metadata',
      backups: [],
      has_config: true,
    }
    value.game.winhttp_state = 'modenabled'
  }

  // Keep overview totals stable while the framework card changes scenarios.
  if (scenario === 'framework-missing') {
    value.config = {
      ...value.config,
      repositories,
      repository_sources: repositorySources,
    }
  }

  if (hostUtilities && configManager && (scenario === 'framework-missing' || scenario === 'mod-management' || scenario === 'repository-ready' || scenario === 'overview')) {
    value.mods = [
      tutorialMod({ name: 'Chef Rotation', dll_file: 'OC2ChefRotation.dll', version: '1.0.0', enabled: true, repository_id: undefined, repository_ids: [] }, translate),
      tutorialMod({ name: 'DLC Unlocker', dll_file: 'OC2Unlocker.dll', version: '1.1.0', enabled: false, update_available: true, repository_id: undefined, repository_ids: [] }, translate),
      tutorialMod({ name: repositoryName(hostUtilities, language), guid: hostUtilities.plugin_guid, product_name: hostUtilities.local_product_name, original_filename: hostUtilities.local_original_filename, dll_file: 'HostUtilities.dll', version: '2.4.0', enabled: true, package_id: 'arcade-mod-package', package_name: repositoryName(hostUtilities, language), package_mod_count: 2, repository_id: hostUtilities.id, repository_ids: [hostUtilities.id] }, translate),
      tutorialMod({ name: repositoryName(configManager, language), guid: configManager.plugin_guid, has_config: true, product_name: configManager.local_product_name, original_filename: configManager.local_original_filename, dll_file: 'ConfigurationManager.dll', version: '1.0.0', enabled: true, package_id: 'arcade-mod-package', package_name: repositoryName(hostUtilities, language), package_mod_count: 2, repository_id: configManager.id, repository_ids: [configManager.id] }, translate),
    ]
  }

  return value
}
