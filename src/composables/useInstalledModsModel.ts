import { computed, reactive, type ComputedRef, type Ref } from 'vue'
import type { Dashboard, InstalledMod, PublicConfig, Repository } from '../types'
import type { Locale } from '../i18n'
import { t } from '../i18n'
import { hasLocalUpdateVersion, liveModReleaseLabel, modRepositoryReleaseState, modUpdateCheckPhase } from '../modUpdates'
import { communityNavigationTargets } from '../communityRepositories'
import { repositoryName } from '../repositoryNames'
import { sortInstalledModGroups } from '../installedModOrdering'
import { MOD_STATUS_ORDER, MOD_STATUS_TEXT, installedModLabelActionable, type ModStatusCode } from '../installedModStatus'
import type { SourceGroup } from './useSourceRefresh'
import type { useCommunityRepositories } from './useCommunityRepositories'
import type { View } from '../tutorial/steps'

type CommunityDiscovery = ReturnType<typeof useCommunityRepositories>

/**
 * Installed-MOD display model: package grouping and ordering, per-MOD status
 * labels (backend labels merged with live repository refresh state), and the
 * cross-page navigation that expands and highlights bound repositories/MODs.
 */
export function useInstalledModsModel(options: {
  displayDashboard: Ref<Dashboard | null>
  config: ComputedRef<PublicConfig | undefined>
  repos: ComputedRef<Repository[]>
  locale: Ref<Locale>
  repositoryReleaseReady: Record<string, boolean>
  repositoryRefreshing: Record<string, boolean>
  modUpdatesCheckActive: Ref<boolean>
  modRepositoryRefreshActive: ComputedRef<boolean>
  modDetailsExpanded: Record<string, boolean>
  modPackageCollapsed: Record<string, boolean>
  sourceGroups: ComputedRef<SourceGroup[]>
  sourceCollapsed: Record<string, boolean>
  repositoryGroupCollapsed: Record<string, boolean>
  repositoryGroupKey: (sourceId: string, group: string) => string
  communities: CommunityDiscovery[]
  repositoryFocusIds: Ref<string[]>
  modFocusFiles: Ref<string[]>
  activeView: Ref<View>
  isTutorialPreviewing: () => boolean
  getTutorialScenario: () => string
}) {
  const {
    displayDashboard, config, repos, locale, repositoryReleaseReady, repositoryRefreshing,
    modUpdatesCheckActive, modRepositoryRefreshActive, modDetailsExpanded, modPackageCollapsed,
    sourceGroups, sourceCollapsed, repositoryGroupCollapsed, repositoryGroupKey, communities,
    repositoryFocusIds, modFocusFiles, activeView, isTutorialPreviewing, getTutorialScenario,
  } = options
  const modTogglePending = reactive<Record<string, boolean>>({})
  const modPackageTogglePending = reactive<Record<string, boolean>>({})

  const modNameCollator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' })
  const allInstalledMods = computed(() => [...(displayDashboard.value?.mods ?? [])].sort((left, right) => {
    const leftName = left.name.trim()
    const rightName = right.name.trim()
    return modNameCollator.compare(leftName, rightName)
      || modNameCollator.compare(left.dll_file, right.dll_file)
  }))
  const visibleInstalledMods = computed(() => allInstalledMods.value.filter((mod) => !config.value?.hide_unidentified_mods || !!mod.guid?.trim()))
  const installedPackageCounts = computed(() => {
    const counts = new Map<string, number>()
    for (const mod of allInstalledMods.value) {
      if (mod.package_id) counts.set(mod.package_id, (counts.get(mod.package_id) || 0) + 1)
    }
    return counts
  })
  function packageIdForMod(mod: InstalledMod) {
    const packageId = mod.package_id
    const installedCount = packageId ? installedPackageCounts.value.get(packageId) || 0 : 0
    return packageId && installedCount > 1 ? packageId : undefined
  }
  function packageNameForMod(mod: InstalledMod) {
    if (mod.package_name) return mod.package_name
    const repository = mod.package_id
      ? config.value?.repositories.find((item) => item.id === mod.package_id)
      : undefined
    return repository ? repositoryName(repository, locale.value) : mod.package_name || t('MOD 包')
  }
  function packageModCountForMod(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    return packageId ? visibleInstalledMods.value.filter((item) => item.package_id === packageId).length : 0
  }
  const installedMods = computed(() => sortInstalledModGroups(
    visibleInstalledMods.value,
    packageIdForMod,
    (mod) => installedModOrderingStatus(mod),
    (mod, packageId) => packageId ? packageNameForMod(mod) : mod.name,
    (mod) => mod.name,
    (mod) => mod.dll_file,
    (left, right) => modNameCollator.compare(left, right),
  ))
  function packageModsForMod(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    return packageId ? allInstalledMods.value.filter((item) => item.package_id === packageId) : []
  }
  function hasOtherEnabledSameGuid(mod: InstalledMod) {
    const guid = mod.guid?.trim()
    if (!guid) return false
    const normalized = guid.toLocaleLowerCase()
    return allInstalledMods.value.some((item) => item.dll_file !== mod.dll_file
      && item.enabled
      && item.guid?.trim().toLocaleLowerCase() === normalized)
  }
  function isFirstModInPackage(mod: InstalledMod, index: number) {
    const packageId = packageIdForMod(mod)
    return !!packageId && (index === 0 || packageIdForMod(installedMods.value[index - 1]) !== packageId)
  }
  function isLastModInPackage(mod: InstalledMod, index: number) {
    const packageId = packageIdForMod(mod)
    return !!packageId && (index === installedMods.value.length - 1 || packageIdForMod(installedMods.value[index + 1]) !== packageId)
  }
  function modDisplayIndex(mod: InstalledMod, index: number) {
    const packageId = packageIdForMod(mod)
    if (packageId && !isFirstModInPackage(mod, index)) return packageChildIndex(mod)
    return installedMods.value
      .slice(0, index + 1)
      .filter((item, itemIndex) => !packageIdForMod(item) || isFirstModInPackage(item, itemIndex))
      .length
  }
  function packageChildIndex(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    if (!packageId) return 1
    return installedMods.value.filter((item) => item.package_id === packageId)
      .findIndex((item) => item.dll_file === mod.dll_file) + 1
  }
  function packageIsCollapsed(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    return !!packageId && modPackageCollapsed[packageId] !== false
  }
  function toggleModPackageDetails(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    if (packageId) modPackageCollapsed[packageId] = !(modPackageCollapsed[packageId] ?? true)
  }
  function resetInstalledModExpansion() {
    for (const key of Object.keys(modPackageCollapsed)) delete modPackageCollapsed[key]
    for (const key of Object.keys(modDetailsExpanded)) delete modDetailsExpanded[key]
    for (const mod of installedMods.value) modDetailsExpanded[mod.dll_file] = false
  }
  function packageEnabledForMod(mod: InstalledMod) {
    const mods = packageModsForMod(mod)
    return mods.length > 0 && mods.every((item) => modEnabledForView(item))
  }
  function packageMixedForMod(mod: InstalledMod) {
    const mods = packageModsForMod(mod)
    return !packageEnabledForMod(mod) && mods.some((item) => modEnabledForView(item))
  }
  function packageTogglePendingForMod(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    return packageId ? modPackageTogglePending[packageId] : undefined
  }
  function modRepositoryIds(mod: InstalledMod) {
    const ids = mod.repository_ids?.length ? mod.repository_ids : mod.repository_id ? [mod.repository_id] : []
    return new Set(ids)
  }
  function modReleaseRepositoryIds(mod: InstalledMod) {
    if (mod.status === 'not_release_asset') return new Set<string>()
    const ids = modRepositoryIds(mod)
    return new Set(repos.value.filter((repo) => repo.download_mode === 'release' && ids.has(repo.id)).map((repo) => repo.id))
  }
  const mappedReposAvailable = computed(() => repos.value.some((repo) => repo.enabled && repo.download_mode === 'release'
    && installedMods.value.some((mod) => modReleaseRepositoryIds(mod).has(repo.id))))
  const updateAvailableModCount = computed(() => installedMods.value.filter((mod) => modReleaseUpdateStatus(mod) === '有更新').length)

  function repositoryInstallStatus(repo: Repository) {
    if (repo.download_mode === 'repository_file') return ''
    const boundMods = installedMods.value.filter((mod) => modReleaseRepositoryIds(mod).has(repo.id))
    if (!boundMods.length) return ''
    if (repositoryRefreshing[repo.id]) return '检查中'
    if (!repositoryReleaseReady[repo.id] && !(isTutorialPreviewing() && getTutorialScenario() !== 'live')) return '待检查'
    const states = boundMods.map((mod) => modRepositoryReleaseState(repo, mod))
    if (states.includes('update_available')) return '有更新'
    return states.every((state) => state === 'latest') ? '最新版' : '已安装'
  }
  function openRepositoryMods(repo: Repository) {
    if (!['有更新', '最新版'].includes(repositoryInstallStatus(repo))) return
    const boundMods = installedMods.value.filter((mod) => modReleaseRepositoryIds(mod).has(repo.id))
    if (!boundMods.length) return
    for (const mod of boundMods) {
      const packageId = packageIdForMod(mod)
      if (packageId) modPackageCollapsed[packageId] = false
    }
    modFocusFiles.value = boundMods.map((mod) => mod.dll_file)
    activeView.value = 'installed'
  }
  function modReleaseRepositories(mod: InstalledMod) {
    const ids = modReleaseRepositoryIds(mod)
    return repos.value.filter((repo) => repo.enabled && ids.has(repo.id))
  }
  function modRepositoryVersionReady(repo: Repository, mod: InstalledMod) {
    return repositoryReleaseReady[repo.id] && !repo.last_error
      && ['latest', 'update_available'].includes(modRepositoryReleaseState(repo, mod))
  }
  function modHasRemoteUpdate(mod: InstalledMod) {
    return modReleaseRepositories(mod).some((repo) => modRepositoryVersionReady(repo, mod)
      && modRepositoryReleaseState(repo, mod) === 'update_available')
  }
  function legacyModStatusCode(mod: InstalledMod): ModStatusCode[] {
    const code = ({ update_available: 'update_available', latest: 'latest', not_release_asset: 'not_release_asset', unbound: 'unbound', bound_repository_missing: 'bound_repository_missing', repository_unavailable: 'repository_unavailable', unknown: 'unknown', duplicate_lower_version: 'duplicate_lower_version' } as Record<string, ModStatusCode>)[mod.status]
    const labels = code ? [code] : []
    if (mod.duplicate_version || mod.duplicate_guid) labels.push('duplicate')
    if (mod.load_warning) labels.push(mod.load_warning === '存在更高版本，建议卸载' ? 'duplicate_lower_version' : 'load_warning')
    if (mod.update_available && !labels.includes('update_available')) labels.unshift('update_available')
    if (!mod.enabled) labels.push('disabled')
    return labels
  }
  function backendModStatusCodes(mod: InstalledMod): ModStatusCode[] {
    const labels = (Array.isArray(mod.labels) ? mod.labels : legacyModStatusCode(mod))
      .filter((label): label is ModStatusCode => label in MOD_STATUS_TEXT)
    const unique = [...new Set(labels)]
    if (!modEnabledForView(mod) && !unique.includes('disabled')) unique.push('disabled')
    return unique
  }
  function modBoundRepositoriesSettled(mod: InstalledMod) {
    return modReleaseRepositories(mod).every((repo) => repositoryReleaseReady[repo.id] || !!repo.last_error)
  }
  function installedModStatusCodes(mod: InstalledMod): ModStatusCode[] {
    const backend = backendModStatusCodes(mod)
    const candidates = modReleaseRepositories(mod)
    const phase = modUpdateCheckPhase({
      active: modUpdatesCheckActive.value || modRepositoryRefreshActive.value,
      refreshing: isModRepositoryRefreshing(mod),
      hasBoundRepositories: candidates.length > 0,
      settled: modBoundRepositoriesSettled(mod),
    })
    if (phase === 'idle' || phase === 'settled') return backend
    const offline = backend.filter((label) => ['disabled', 'duplicate', 'duplicate_lower_version', 'load_warning'].includes(label))
    return [...offline, phase]
  }
  function installedModLabels(mod: InstalledMod) {
    const hasBackendLabels = Array.isArray(mod.labels)
    let labels = installedModStatusCodes(mod)
    const transient = labels.includes('checking') || labels.includes('pending')
    const candidates = modReleaseRepositories(mod)
    const liveCheck = !transient && candidates.length > 0
      && (modUpdatesCheckActive.value || modRepositoryRefreshActive.value || !hasBackendLabels)
    if (liveCheck) {
      labels = labels.filter((code) => !['update_available', 'latest', 'repository_unavailable', 'unknown'].includes(code))
      labels.push(liveModReleaseLabel({
        releaseStatus: modReleaseUpdateStatus(mod),
        hasRepositoryError: candidates.some((repo) => !!repo.last_error),
        settled: modBoundRepositoriesSettled(mod),
      }))
    }
    return MOD_STATUS_ORDER.filter((code) => labels.includes(code))
  }
  function installedModOrderingStatus(mod: InstalledMod) { return installedModLabels(mod) }
  function modReleaseUpdateStatus(mod: InstalledMod): '有更新' | '最新版' | '' {
    if (['unbound', 'not_release_asset', 'duplicate_lower_version'].includes(mod.status)
      || !hasLocalUpdateVersion(mod) || mod.duplicate_version || mod.load_warning || isModRepositoryRefreshing(mod)) return ''
    const candidates = modReleaseRepositories(mod)
    if (modHasRemoteUpdate(mod)) return '有更新'
    return candidates.length > 0 && candidates.every((repo) => modRepositoryVersionReady(repo, mod)) ? '最新版' : ''
  }
  function openModRepository(mod: InstalledMod, requestedLabel?: string) {
    const label = (requestedLabel ?? installedModLabels(mod).find(installedModLabelActionable)) as ModStatusCode | undefined
    if (!label || !installedModLabelActionable(label)) return
    const status = MOD_STATUS_TEXT[label]
    const children = (requestedLabel && mod.package_id
      ? visibleInstalledMods.value.filter((child) => child.package_id === mod.package_id)
      : [mod]).filter((child) => installedModLabels(child).includes(label) && modReleaseUpdateStatus(child) === status)
    const bound = children.flatMap(modReleaseRepositories).filter((repo) => children.some((child) =>
      modReleaseRepositoryIds(child).has(repo.id) && modRepositoryVersionReady(repo, child)
      && modRepositoryReleaseState(repo, child) === (label === 'update_available' ? 'update_available' : 'latest')))
    const targets = communityNavigationTargets(repos.value, new Set(repos.value.filter((repo) => repo.download_mode === 'release' && bound.some((candidate) =>
      repo.provider === candidate.provider && repo.username.toLowerCase() === candidate.username.toLowerCase()
      && repo.repository.toLowerCase() === candidate.repository.toLowerCase())
      && children.some((child) => modReleaseRepositoryIds(child).has(repo.id))).map((repo) => repo.id)), config.value?.repository_sources)
    for (const discovery of communities) discovery.reveal(targets)
    const visibleIds: string[] = []
    function expandTargets(groups: SourceGroup[]): boolean {
      let found = false
      for (const group of groups) {
        let ownTarget = false
        for (const repoGroup of group.groups) {
          const matches = repoGroup.repositories.filter((repo) => targets.has(repo.id))
          if (!matches.length) continue
          visibleIds.push(...matches.map((repo) => repo.id))
          repositoryGroupCollapsed[repositoryGroupKey(group.source.id, repoGroup.key)] = false
          ownTarget = true
        }
        const childTarget = expandTargets(group.children)
        if (ownTarget || childTarget) {
          sourceCollapsed[group.source.id] = false
          found = true
        }
      }
      return found
    }
    if (!expandTargets(sourceGroups.value)) return
    repositoryFocusIds.value = visibleIds
    activeView.value = 'repositories'
  }
  function isModRepositoryRefreshing(mod: InstalledMod) {
    return Array.from(modReleaseRepositoryIds(mod)).some((id) => repositoryRefreshing[id])
  }
  function isModToggling(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    return Object.prototype.hasOwnProperty.call(modTogglePending, mod.dll_file)
      || !!packageId && Object.prototype.hasOwnProperty.call(modPackageTogglePending, packageId)
  }
  function isPackageToggling(mod: InstalledMod) {
    const packageId = packageIdForMod(mod)
    return !!packageId && (
      Object.prototype.hasOwnProperty.call(modPackageTogglePending, packageId)
      || packageModsForMod(mod).some((item) => Object.prototype.hasOwnProperty.call(modTogglePending, item.dll_file))
    )
  }
  function modEnabledForView(mod: InstalledMod) {
    const packagePending = packageTogglePendingForMod(mod)
    return packagePending !== undefined ? packagePending : modTogglePending[mod.dll_file] ?? mod.enabled
  }
  function displayModPath(mod: InstalledMod) {
    return `${mod.dll_file}${modEnabledForView(mod) ? '' : '.disabled'}`
  }
  function toggleModDetails(modId: string) { modDetailsExpanded[modId] = !modDetailsExpanded[modId] }

  return {
    modTogglePending, modPackageTogglePending, allInstalledMods, visibleInstalledMods,
    installedMods, mappedReposAvailable, updateAvailableModCount,
    packageIdForMod, packageNameForMod, packageModCountForMod, packageModsForMod,
    hasOtherEnabledSameGuid, isFirstModInPackage, isLastModInPackage, modDisplayIndex,
    packageChildIndex, packageIsCollapsed, toggleModPackageDetails, resetInstalledModExpansion,
    packageEnabledForMod, packageMixedForMod, packageTogglePendingForMod,
    modRepositoryIds, modReleaseRepositoryIds, repositoryInstallStatus, openRepositoryMods,
    modReleaseRepositories, modRepositoryVersionReady, modHasRemoteUpdate,
    legacyModStatusCode, backendModStatusCodes, modBoundRepositoriesSettled,
    installedModStatusCodes, installedModLabels, installedModOrderingStatus,
    modReleaseUpdateStatus, openModRepository, isModRepositoryRefreshing,
    isModToggling, isPackageToggling, modEnabledForView, displayModPath, toggleModDetails,
  }
}

export type InstalledModsModel = ReturnType<typeof useInstalledModsModel>
