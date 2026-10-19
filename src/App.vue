<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  AlertTriangle, Boxes, Check, CheckCircle2, ChevronRight, CloudDownload, Coffee, Copy, Download,
  GripVertical, LayoutDashboard, PackageCheck, Pencil, Plus, RefreshCw,
  Save, Search, Trash2, Upload, X,
} from 'lucide-vue-next'
import { api, watchAppUpdateChannel, watchAppUpdatePhase, watchDownloadProgress, watchGameState } from './services/tauri'
import type { AppUpdateChannelEvent, AppUpdateCheckResult, AppUpdateInfo, AppUpdatePhaseEvent, BepInExInstallResult, ConfigEditorTarget, Dashboard, DownloadProgress, GameRuntime, GitHubProxyConfig, PublicConfig, Provider, ProxyMode, Repository, RepositoryPatch, RepositorySource, SettingsPatch, WinhttpState } from './types'
import { bepinexDownloadUrls as getBepInExDownloadUrls } from './gameRuntime'
import { localizedRepositorySourceName, t, useLocaleTranslation, type Locale } from './i18n'
import { normalizeRepositoryRegion, repositoryRegionMatches, repositorySourceKey } from './repositorySources'
import { repositoryMarkdownBaseUrl } from './markdown'
import MarkdownContent from './components/MarkdownContent.vue'
import { hasLocalUpdateVersion, modRepositoryReleaseState } from './modUpdates'
import { GITEE_COMMUNITY_SOURCE_ID, GITEE_COMMUNITY_TOPIC_URL, communityNavigationTargets, communityProviderMatchesRegion, isCommunitySource, isCommunityRepository, isCommunitySourceArchive } from './communityRepositories'
import sponsorImage from './assets/collectionCode.jpg'
import appIcon from './assets/bb4518aa-dc55-4cae-a432-08ccdc5f28f8.png'
import AppSidebar from './components/AppSidebar.vue'
import FrameworkView from './components/FrameworkView.vue'
import BepInExSetupGuide from './components/BepInExSetupGuide.vue'
import InstalledModsView from './components/InstalledModsView.vue'
import ModRecycleBin from './components/ModRecycleBin.vue'
import ModConfigEditor from './components/ModConfigEditor.vue'
import OverviewView from './components/OverviewView.vue'
import RepositoriesView from './components/RepositoriesView.vue'
import RepositoryDocumentDialog from './components/RepositoryDocumentDialog.vue'
import SponsorView from './components/SponsorView.vue'
import GitHubProxyManager from './components/GitHubProxyManager.vue'
import TutorialExperience from './components/TutorialExperience.vue'
import ConfirmationDialog from './components/ConfirmationDialog.vue'
import { useConfirmationDialog } from './composables/useConfirmationDialog'
import { useCommunityRepositories } from './composables/useCommunityRepositories'
import { useTutorialRuntime, type TutorialProgressState } from './tutorial/useTutorialRuntime'
import type { View } from './tutorial/steps'

type InstalledMod = import('./types').InstalledMod
type ModUninstallPending = {
  kind: 'mod' | 'package'
  phase: 'confirm' | 'package' | 'unmanaged'
  mod?: InstalledMod
  packageId?: string
  packageName?: string
  mods?: InstalledMod[]
  siblingNames?: string[]
}

const activeView = ref<View>('overview')
const repositoryFocusIds = ref<string[]>([])
const modFocusFiles = ref<string[]>([])
const { confirmation, requestConfirmation, answerConfirmation } = useConfirmationDialog()
const dashboard = ref<Dashboard | null>(null)
const busy = ref('')
const message = ref('')
const error = ref('')
const progress = ref<TutorialProgressState>({ label: '', percent: 0, speed: '', status: '' })
let downloadRequestContext: Pick<DownloadProgress, 'route' | 'source' | 'attempt' | 'attempt_total'> = {}
let downloadProgressActive = false
const downloadCancelRequested = ref(false)
const DEFAULT_UI_ZOOM = 1.5
const MIN_UI_ZOOM = 1
const MAX_UI_ZOOM = 2
const UI_ZOOM_STEP = 0.1
const uiZoom = ref(DEFAULT_UI_ZOOM)
let uiZoomSaveTimer: number | undefined
let uiZoomSaveQueue: Promise<void> = Promise.resolve()
let uiZoomSaveValue = DEFAULT_UI_ZOOM
const showRepoEditor = ref(false)
const showImport = ref(false)
const showRepositorySource = ref(false)
const showGithubProxyManager = ref(false)
const showModRecycleBin = ref(false)
const configEditorTarget = ref<ConfigEditorTarget | null>(null)
const repositorySourceUrl = ref('')
const repositorySourceNickname = ref('')
const repositorySourceUniqueId = ref('')
const repositorySourceRegion = ref('')
const editingRepositorySourceId = ref<string | null>(null)
const repositorySourceEditorOpen = ref(false)
const repositorySourceDraft = ref<RepositorySource[]>([])
const draggingSourceId = ref<string | null>(null)
const showExport = ref(false)
const showRustLog = ref(false)
const rustLog = ref({ path: '', content: '' })
const rustLogOutput = ref<HTMLElement | null>(null)
const logViewerKind = ref<'rust' | 'frontend' | 'crash'>('rust')
const showRelease = ref<Repository | null>(null)
const repositoryDocument = ref<{ repository: Repository; kind: 'note' | 'readme' } | null>(null)
const showInstall = ref<Repository | null>(null)
const showBepInExInstall = ref(false)
const bepinexInstallReinstall = ref(false)
const showBepInExSetup = ref(false)
const bepinexSetupResult = ref<BepInExInstallResult | null>(null)
watch([
  () => dashboard.value?.config.game.runtime,
  () => dashboard.value?.game.path,
  () => dashboard.value?.game.wine_prefix,
], () => {
  bepinexSetupResult.value = null
  showBepInExSetup.value = false
})
const selectedInstallTag = ref('')
const selectedInstallAsset = ref('')
const selectedAssetProxyMode = ref<ProxyMode>('auto_direct')
const selectedAssetProxyId = ref('')
const importText = ref('')
const importPreview = ref<{ valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] } | null>(null)
const repositoryRefreshing = reactive<Record<string, boolean>>({})
const sourceRefreshingManual = reactive<Record<string, boolean>>({})
const appUpdate = ref<AppUpdateInfo | null>(null)
const appUpdateCheck = ref<AppUpdateCheckResult | null>(null)
const startupUpdateChecking = ref(true)
const startupLoading = ref(false)
const startupLoadingError = ref('')
const updateCheckFailed = ref(false)
const updateSkipped = ref(false)
const startupUpdatePhase = ref<'manifest' | 'download' | 'install' | 'skipped'>('manifest')
const updateSkipCountdown = ref(0)
const updatePageOpening = ref(false)
const updatePageError = ref('')
const startupStage = ref('')
const startupError = ref('')
const visibleStartupUpdateChannels = computed(() => appUpdateCheck.value?.channels.filter((channel) => channel.status !== 'skipped') || [])
const sourceCollapsed = reactive<Record<string, boolean>>({})
const repositoryGroupCollapsed = reactive<Record<string, boolean>>({})
const sourceLoading = reactive<Record<string, boolean>>({})
const sourceDocumentsLoading = reactive<Record<string, boolean>>({})
const sourceRefreshPhase = reactive<Record<string, 'clearing' | 'loading' | 'adding' | 'refreshing'>>({})
const sourceErrors = reactive<Record<string, string>>({})
const sourceContentRevealing = reactive<Record<string, boolean>>({})
const sourceContentRevealTimers = new Map<string, number>()
const repositoryReleaseReady = reactive<Record<string, boolean>>({})
let initialRepositoryRefreshStarted = false
const initialRepositorySourcesLoaded = ref(false)
let initialProxySync: Promise<void> = Promise.resolve()
const modDetailsExpanded = reactive<Record<string, boolean>>({})
const modPackageCollapsed = reactive<Record<string, boolean>>({})
const modTogglePending = reactive<Record<string, boolean>>({})
const modPackageTogglePending = reactive<Record<string, boolean>>({})
const stopProgress = ref<(() => void) | null>(null)
const stopAppUpdateChannel = ref<(() => void) | null>(null)
const stopAppUpdatePhase = ref<(() => void) | null>(null)
const stopGameState = ref<(() => void) | null>(null)
let appUnmounted = false
let toastTimer: number | undefined
let updateSkipTimer: number | undefined
let uninstallFrontendLogging: (() => void) | undefined
let modalPointerDownTarget: Element | null = null
const editingMod = ref<import('./types').InstalledMod | null>(null)
const modNote = ref('')
const modUninstallPending = ref<ModUninstallPending | null>(null)
const installedModsPendingCheck = ref(false)
const installedModsChecking = ref(false)
const appVersion = ref('')
const { locale, setLocale } = useLocaleTranslation()
const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'ko', label: '한국어' },
]
const repoForm = reactive<RepositoryPatch>({
  name: '', group: '', provider: 'gitee', username: '', repository: '', download_mode: 'release', branch: 'main', file_path: '', channel: 'all', asset_rules: [],
  region: '', install_target: 'BepInEx/plugins', note: '', plugin_guid: '', local_product_name: '', local_original_filename: '', version_regex: '', max_release_count: 10, download_threads: 1, github_token: '', gitee_token: '', official_api_only: true, enabled: true,
})
const bepinexDownloadUrls = computed(() => getBepInExDownloadUrls(config.value?.platform, config.value?.game.runtime))

const STARTUP_IPC_TIMEOUT_MS = 8000

function withTimeout<T>(promise: Promise<T>, timeoutMs: number, label: string): Promise<T> {
  return new Promise((resolve, reject) => {
    let settled = false
    const timer = window.setTimeout(() => {
      settled = true
      reject(new Error(`${label} 超时`))
    }, timeoutMs)
    promise.then((value) => {
      if (settled) return
      settled = true
      window.clearTimeout(timer)
      resolve(value)
    }, (reason) => {
      if (settled) return
      settled = true
      window.clearTimeout(timer)
      reject(reason)
    })
  })
}

const navItems = [
  { id: 'overview' as View, label: '状态总览', hint: '当前状态和快捷操作', icon: LayoutDashboard },
  { id: 'framework' as View, label: '框架管理', hint: '框架安装与备份', icon: PackageCheck },
  { id: 'repositories' as View, label: 'MOD安装', hint: '在线MOD安装与管理', icon: CloudDownload },
  { id: 'installed' as View, label: 'MOD管理', hint: '本地MOD安装与管理', icon: Boxes },
  { id: 'sponsor' as View, label: '赞助渠道', hint: '支持项目开发', icon: Coffee },
]
const tutorialRuntime = useTutorialRuntime({
  api,
  dashboard,
  activeView,
  locale,
  uiZoom,
  zoomControls: { min: MIN_UI_ZOOM, max: MAX_UI_ZOOM, step: UI_ZOOM_STEP, set: (value) => setUiZoom(value) },
  progress,
  sourceLoading,
  repositoryReleaseReady,
  modDetailsExpanded,
  modPackageCollapsed,
  showBepInExInstall,
  bepinexInstallReinstall,
  showRepositorySource,
  showRepoEditor,
  showInstall,
  selectedInstallTag,
  selectedInstallAsset,
  getStoredSteps: () => dashboard.value?.config.tutorial_steps ?? [],
  getSeenStepIds: () => dashboard.value?.config.onboarding_seen_steps ?? [],
  persistSettings: async (patch: SettingsPatch) => {
    const saved = await api.saveSettings(patch)
    if (dashboard.value) {
      dashboard.value.config = saved
    }
    return saved
  },
  showError,
  showSuccess,
  run,
  getRepositories: () => repos.value,
  getActiveRepositorySources: () => activeRepositorySources.value,
  resetInstalledModExpansion,
  openRepositorySourceManager,
  editRepository: () => editRepo(),
  installReleases,
  matchingInstallAssets,
  startInitialRepositoryRefresh,
})
const tutorial = tutorialRuntime.controller
const displayDashboard = tutorialRuntime.displayDashboard
const game = computed(() => displayDashboard.value?.game)
const config = computed(() => displayDashboard.value?.config)
const realGame = computed(() => dashboard.value?.game)
function createCommunity(provider: Provider) {
  return useCommunityRepositories({
    provider,
    config: computed(() => dashboard.value?.config),
    canRefresh: () => !appUnmounted && initialRepositorySourcesLoaded.value
      && communityProviderMatchesRegion(provider, dashboard.value?.config.region)
      && !tutorial.blocksRepositoryRefresh.value
      && !(dashboard.value?.config.repository_sources ?? []).some((source) => source.builtin && !source.parent_id
        && [...sourceDescendantIds(source.id)].some((id) => sourceDocumentsLoading[id])),
    search: () => api.refreshCommunityRepositories(provider),
    cancel: () => api.cancelCommunitySearch(provider),
    beforeSearch: async () => {
      // Only this provider's old Releases gate a replacement search.
      await Promise.all([
        dashboardUpdateQueue,
        ...(dashboard.value?.config.repositories.filter((repo) => isCommunityRepository(repo, provider)) ?? [])
          .map((repo) => repositoryRefreshTasks.get(repo.id)),
      ])
      if (appUnmounted) return
      for (const repo of dashboard.value?.config.repositories.filter((repo) => isCommunityRepository(repo, provider)) ?? []) {
        repositoryReleaseReady[repo.id] = false
        repo.cached_releases = []
        repo.last_error = undefined
      }
    },
    refreshConfig: refreshConfigSerialized,
    refreshReleases: (repositories) => refreshRepositoryBatch(repositories, 4),
    refreshDashboard: refreshDashboardSerialized,
  })
}
const community = createCommunity('github')
const giteeCommunity = createCommunity('gitee')
const communities = [community, giteeCommunity]
function communityForSource(sourceId: string) {
  return communities.find((discovery) => discovery.sourceId === sourceId)
}
const displayProgress = tutorialRuntime.displayProgress
function sourceMatchesRegion(source: RepositorySource) {
  return (source.id !== GITEE_COMMUNITY_SOURCE_ID || communityProviderMatchesRegion('gitee', config.value?.region))
    && repositoryRegionMatches(source.region, config.value?.region)
}
function repositorySourceRegionLabel(region?: string) {
  const normalized = normalizeRepositoryRegion(region)
  if (normalized === 'chinese_mainland') return t('中国大陆地区')
  if (normalized === 'international') return t('国际地区')
  return t('全部地区')
}
const activeRepositorySources = computed(() => (config.value?.repository_sources ?? []).filter(sourceMatchesRegion))
const activeRepositorySourceIds = computed(() => new Set(activeRepositorySources.value.map((source) => source.id)))
const repos = computed(() => (config.value?.repositories ?? []).filter((repo) => repositoryRegionMatches(repo.region, config.value?.region)
  && (!repo.source_id || activeRepositorySourceIds.value.has(repo.source_id))))
function groupedRepositories(repositories: Repository[]) {
  const groups = new Map<string, Repository[]>()
  for (const repo of repositories) {
    const key = repo.group?.trim() || ''
    const entries = groups.get(key) || []
    entries.push(repo)
    groups.set(key, entries)
  }
  return Array.from(groups, ([key, grouped]) => ({
    key,
    label: key ? (grouped.find((repo) => repo.group_i18n?.[locale.value])?.group_i18n?.[locale.value] || key) : '',
    repositories: grouped,
  }))
}
type SourceGroup = {
  source: RepositorySource
  groups: ReturnType<typeof groupedRepositories>
  children: SourceGroup[]
  community?: ReturnType<typeof useCommunityRepositories>
}
type FlattenedSourceGroup = SourceGroup & { depth: number }
type RepositorySourceManagerEntry = { source: RepositorySource; depth: number }
function flattenSourceGroups(group: SourceGroup, depth = 0): FlattenedSourceGroup[] {
  const current = depth === 0 && group.groups.length === 0 ? [] : [{ ...group, depth }]
  return [
    ...current,
    ...group.children.flatMap((child) => flattenSourceGroups(child, depth + 1)),
  ]
}
const sourceGroups = computed(() => {
  const sources = activeRepositorySources.value
  const visibleSourceIds = new Set(sources.map((source) => source.id))
  const childrenByParent = new Map<string, RepositorySource[]>()
  for (const source of sources) {
    if (!source.parent_id || !visibleSourceIds.has(source.parent_id)) continue
    const children = childrenByParent.get(source.parent_id) || []
    children.push(source)
    childrenByParent.set(source.parent_id, children)
  }
  const buildGroup = (source: RepositorySource): SourceGroup => ({
    source,
    groups: groupedRepositories(repos.value.filter((repo) => repo.source_id === source.id)),
    children: (childrenByParent.get(source.id) || []).map(buildGroup),
  })
  const groups = sources
    .filter((source) => !isCommunitySource(source.id) && (!source.parent_id || !visibleSourceIds.has(source.parent_id)))
    .map(buildGroup)
  const unclassified = repos.value.filter((repo) => !repo.builtin
    && (!repo.source_id || !visibleSourceIds.has(repo.source_id)))
  if (unclassified.length) groups.push({ source: { id: 'unclassified', nickname: t('未分类仓库'), url: '', builtin: false }, groups: groupedRepositories(unclassified), children: [] })
  if (!tutorial.visible.value && !tutorial.previewing.value) {
    for (const discovery of communities) {
      if (!discovery.started.value || !communityProviderMatchesRegion(discovery.provider, config.value?.region)) continue
      const source = sources.find((source) => source.id === discovery.sourceId)
        ?? { id: discovery.sourceId, nickname: t(discovery.provider === 'gitee' ? 'Gitee 社区仓库' : 'GitHub 社区仓库'),
          url: discovery.provider === 'gitee' ? GITEE_COMMUNITY_TOPIC_URL : 'https://api.github.com/search/repositories', builtin: false }
      groups.push({ source, groups: [{ key: '', label: '', repositories: discovery.visibleRepositories.value }], children: [], community: discovery })
    }
  }
  return groups
})
function collectSourceGroupRepositoryIds(groups: SourceGroup[], ids = new Set<string>()) {
  for (const group of groups) {
    for (const repositoryGroup of group.groups) {
      for (const repository of repositoryGroup.repositories) ids.add(repository.id)
    }
    collectSourceGroupRepositoryIds(group.children, ids)
  }
  return ids
}
const displayedRepositoryIds = computed(() => collectSourceGroupRepositoryIds(sourceGroups.value))
const listedRepositoryIds = computed(() => new Set([...displayedRepositoryIds.value,
  ...(!tutorial.visible.value && !tutorial.previewing.value
    ? communities.flatMap((discovery) => discovery.started.value && communityProviderMatchesRegion(discovery.provider, config.value?.region)
      ? discovery.repositories.value.map((repo) => repo.id) : []) : [])]))
const displayedRepositoryCount = computed(() => listedRepositoryIds.value.size)
const installableRepositoryCount = computed(() => repos.value.filter((repo) => listedRepositoryIds.value.has(repo.id)
  && repo.enabled
  && (repo.download_mode === 'repository_file'
    ? !!repo.branch.trim() && !!repo.file_path.trim()
    : repositoryReleaseReady[repo.id] && installReleases(repo).length > 0)).length)
const repositorySourceManagerEntries = computed<RepositorySourceManagerEntry[]>(() => {
  const sources = repositorySourceDraft.value
  const sourceIds = new Set(sources.map((source) => source.id))
  const visibleSourceIds = new Set(sources.filter(sourceMatchesRegion).map((source) => source.id))
  const childrenByParent = new Map<string, RepositorySource[]>()
  for (const source of sources) {
    if (!sourceMatchesRegion(source) || !source.parent_id || !sourceIds.has(source.parent_id)) continue
    const children = childrenByParent.get(source.parent_id) || []
    children.push(source)
    childrenByParent.set(source.parent_id, children)
  }
  const entries: RepositorySourceManagerEntry[] = []
  const visited = new Set<string>()
  const visit = (source: RepositorySource, depth: number) => {
    if (visited.has(source.id) || !visibleSourceIds.has(source.id)) return
    visited.add(source.id)
    entries.push({ source, depth })
    for (const child of childrenByParent.get(source.id) || []) visit(child, depth + 1)
  }
  for (const source of sources) {
    if ((!source.parent_id || !sourceIds.has(source.parent_id)) && visibleSourceIds.has(source.id)) visit(source, 0)
  }
  return entries
})
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
  return repository ? localizedRepositoryName(repository) : mod.package_name || t('MOD 包')
}
function packageModCountForMod(mod: InstalledMod) {
  const packageId = packageIdForMod(mod)
  return packageId ? visibleInstalledMods.value.filter((item) => item.package_id === packageId).length : 0
}
const installedMods = computed(() => [...visibleInstalledMods.value].sort((left, right) => {
  const leftPackage = packageIdForMod(left)
  const rightPackage = packageIdForMod(right)
  const leftGroup = leftPackage ? packageNameForMod(left) : left.name
  const rightGroup = rightPackage ? packageNameForMod(right) : right.name
  return modNameCollator.compare(leftGroup, rightGroup)
    || (leftPackage !== rightPackage ? (leftPackage ? -1 : 1) : 0)
    || (leftPackage && rightPackage && leftPackage === rightPackage ? modNameCollator.compare(left.name, right.name) : 0)
    || modNameCollator.compare(left.name, right.name)
    || modNameCollator.compare(left.dll_file, right.dll_file)
}))
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
function formatModPackageDeleteWarning(pending: ModUninstallPending) {
  const current = pending.mod?.name || ''
  const siblings = (pending.siblingNames || []).map((name) => `【${name}】`).join('、')
  const others = siblings || `【${t('其他 MOD')}】`
  return t('当前 DLL【{current}】与{others}一并安装。仅将此 DLL 移入回收站，可能导致{missing}MOD 缺失依赖。可从回收站恢复，是否继续？')
    .replace('{current}', current)
    .replace('{others}', others)
    .replace('{missing}', others)
}
function modRepositoryIds(mod: import('./types').InstalledMod) {
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
const gameBlocked = computed(() => realGame.value?.kitchen_running ?? false)
const currentWinhttp = computed<WinhttpState>(() => game.value?.winhttp_state ?? 'missing')
const repositoryRefreshBatchActive = ref(false)
const repositoryRefreshBatchTotal = ref(0)
const repositoryRefreshBatchCompleted = ref(0)
const updateAvailableModCount = computed(() => installedMods.value.filter((mod) => modReleaseUpdateStatus(mod) === '有更新').length)
const repositoryRefreshActive = computed(() => repositoryRefreshBatchActive.value
  || communities.some((discovery) => discovery.refreshing.value)
  || Object.values(repositoryRefreshing).some(Boolean)
  || Object.values(sourceLoading).some(Boolean)
  || Object.values(sourceRefreshingManual).some(Boolean))
let repositoryRefreshBatchToken = 0
const repositoryRefreshTasks = new Map<string, Promise<boolean>>()
let dashboardUpdateQueue: Promise<void> = Promise.resolve()

async function refreshDashboardSerialized() {
  const next = dashboardUpdateQueue.then(async () => {
    dashboard.value = await api.dashboard()
  })
  dashboardUpdateQueue = next.catch(() => undefined)
  await next
  return dashboard.value
}

async function refreshConfigSerialized() {
  const next = dashboardUpdateQueue.then(async () => {
    const savedConfig = await api.config()
    if (dashboard.value) dashboard.value = { ...dashboard.value, config: savedConfig }
  })
  dashboardUpdateQueue = next.catch(() => undefined)
  await next
}

function statusLabel(status: WinhttpState) { return t(({ modenabled: '框架已启用', moddisabled: '框架已禁用', missing: config.value?.platform !== 'windows' && config.value?.game.runtime === 'native' ? '未找到框架加载器' : '未找到 winhttp.dll', conflict: '文件状态冲突' } as Record<WinhttpState, string>)[status]) }
function formatBytes(value: number) { if (!value) return '0 B'; const units = ['B', 'KB', 'MB', 'GB']; const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1); return `${(value / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}` }
function formatAssetMetadata(asset: import('./types').ReleaseAsset) {
  const size = formatBytes(asset.size)
  return asset.download_count == null ? size : `${size} · ${t('下载次数：{count}').replace('{count}', String(asset.download_count))}`
}
function scheduleToastClose() { if (toastTimer) window.clearTimeout(toastTimer); toastTimer = window.setTimeout(() => { message.value = ''; error.value = '' }, 5000) }
function clampUiZoom(value: number | undefined) {
  const numeric = Number.isFinite(value) ? value as number : DEFAULT_UI_ZOOM
  return Math.min(MAX_UI_ZOOM, Math.max(MIN_UI_ZOOM, Math.round(numeric * 100) / 100))
}
async function setUiZoom(value: number | undefined, persist = true, notify = false) {
  const next = clampUiZoom(value)
  uiZoom.value = next
  document.documentElement.style.setProperty('--ui-zoom', String(next))
  if (!persist) {
    if (uiZoomSaveTimer) {
      window.clearTimeout(uiZoomSaveTimer)
      uiZoomSaveTimer = undefined
    }
    uiZoomSaveValue = next
    return
  }
  uiZoomSaveValue = next
  if (uiZoomSaveTimer) window.clearTimeout(uiZoomSaveTimer)
  uiZoomSaveTimer = window.setTimeout(() => {
    uiZoomSaveTimer = undefined
    const valueToSave = uiZoomSaveValue
    uiZoomSaveQueue = uiZoomSaveQueue
      .then(async () => { await api.saveSettings({ ui_zoom: valueToSave }); if (dashboard.value) dashboard.value.config.ui_zoom = valueToSave })
      .catch((reason) => showError(reason))
  }, 250)
  if (notify) {
    showSuccess(t('界面缩放至').replace('{percent}', String(Math.round(next * 100))))
  }
}
async function handleUiZoomWheel(event: WheelEvent) {
  if (!event.ctrlKey || event.deltaY === 0) return
  event.preventDefault()
  const next = uiZoom.value + (event.deltaY < 0 ? UI_ZOOM_STEP : -UI_ZOOM_STEP)
  // Ctrl+wheel changes only the WebView content scale.
  await setUiZoom(next, true, true)
}
function formatDate(value?: string) { if (!value) return t('未知时间'); const language = ({ 'zh-CN': 'zh-CN', 'zh-TW': 'zh-TW', en: 'en-US', ko: 'ko-KR' } as Record<Locale, string>)[locale.value]; return new Intl.DateTimeFormat(language, { year: 'numeric', month: 'short', day: 'numeric' }).format(new Date(value)) }
function latest(repo: Repository) { return repo.cached_releases?.[0] }
function localizedRepositoryName(repo: Repository) { return repo.name_i18n?.[locale.value] || repo.name }
function localizedRepositoryNote(repo: Repository) { return repo.note_i18n?.[locale.value] || repo.note }
function repositoryInstallStatus(repo: Repository) {
  if (repo.download_mode === 'repository_file') return ''
  const boundMods = installedMods.value.filter((mod) => modReleaseRepositoryIds(mod).has(repo.id))
  if (!boundMods.length) return ''
  if (repositoryRefreshing[repo.id]) return '检查中'
  if (!repositoryReleaseReady[repo.id] && !(tutorial.previewing.value && tutorial.scenario.value !== 'live')) return '待检查'
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
function installedModStatus(mod: InstalledMod) {
  if (!modEnabledForView(mod)) return '已禁用'
  if (mod.duplicate_version) return '重复'
  if (mod.load_warning) return mod.load_warning
  if (mod.status === 'not_release_asset') return '不是 Release 资产, 无法检测更新'
  if (mod.status === 'unbound') return '未绑定在线仓库，无法检测更新'
  if (mod.status === 'bound_repository_missing') return '已绑定仓库未加载，无法检测更新'
  if (installedModsChecking.value || isModRepositoryRefreshing(mod)) return '检查中'
  if (installedModsPendingCheck.value) return '待检查'
  const confirmed = modReleaseUpdateStatus(mod)
  if (confirmed) return confirmed
  const candidates = modReleaseRepositories(mod)
  if (candidates.some((repo) => repo.last_error)) return '刷新失败，无法检测更新'
  if (!hasLocalUpdateVersion(mod) || candidates.some((repo) => repositoryReleaseReady[repo.id]
    && modRepositoryReleaseState(repo, mod) === 'unknown')) return '版本信息不足，无法检测更新'
  return '待检查'
}
function installedModStatusLabel(mod: InstalledMod) { return t(installedModStatus(mod)) }
function installedModStatusClass(mod: InstalledMod) {
  const status = installedModStatus(mod)
  if (status === '已禁用') return 'mod-disabled'
  if (status === '最新版') return 'success'
  if (['有更新', '检查中', '待检查', '重复'].includes(status)) return 'warning'
  if (status === '刷新失败，无法检测更新' || (mod.load_warning && status === mod.load_warning)) return 'danger'
  return 'muted'
}
function modReleaseUpdateStatus(mod: InstalledMod): '有更新' | '最新版' | '' {
  if (installedModsChecking.value || installedModsPendingCheck.value
    || ['unbound', 'not_release_asset', 'duplicate_lower_version'].includes(mod.status)
    || !hasLocalUpdateVersion(mod) || mod.duplicate_version || mod.load_warning || isModRepositoryRefreshing(mod)) return ''
  const candidates = modReleaseRepositories(mod)
  if (modHasRemoteUpdate(mod)) return '有更新'
  return candidates.length > 0 && candidates.every((repo) => modRepositoryVersionReady(repo, mod)) ? '最新版' : ''
}
function openModRepository(mod: InstalledMod, packageStatus?: string) {
  const status = packageStatus ?? installedModStatus(mod)
  if (status !== '有更新' && status !== '最新版') return
  const children = (packageStatus && mod.package_id
    ? visibleInstalledMods.value.filter((child) => child.package_id === mod.package_id)
    : [mod]).filter((child) => installedModStatus(child) === status && modReleaseUpdateStatus(child) === status)
  const bound = children.flatMap(modReleaseRepositories).filter((repo) => children.some((child) =>
    modReleaseRepositoryIds(child).has(repo.id) && modRepositoryVersionReady(repo, child)
    && modRepositoryReleaseState(repo, child) === (status === '有更新' ? 'update_available' : 'latest')))
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
function assetMatchesUi(name: string, rule: { type: string; value: string }) {
  if (rule.type === 'exact') return name.toLocaleLowerCase() === rule.value.toLocaleLowerCase()
  if (rule.type === 'glob') {
    const escaped = rule.value.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.')
    return new RegExp(`^${escaped}$`, 'i').test(name)
  }
  try { return new RegExp(rule.value, 'i').test(name) } catch { return false }
}
function showSuccess(value: string) { error.value = ''; message.value = `${t('成功：')}${value}`; scheduleToastClose() }
function showError(value: unknown) { error.value = `${t('错误：')}${t(value instanceof Error ? value.message : String(value))}`; message.value = ''; scheduleToastClose() }
function rememberModalPointerDown(event: PointerEvent) {
  const target = event.target instanceof Element ? event.target : null
  modalPointerDownTarget = target?.closest('.modal-backdrop') ? target : null
}
function clearModalPointerDownAfterPointerUp(event: PointerEvent) {
  const target = event.target instanceof Element ? event.target : null
  if (!target?.closest('.modal-backdrop')) modalPointerDownTarget = null
}
function preventDraggedModalBackdropClick(event: MouseEvent) {
  const target = event.target instanceof Element ? event.target : null
  if (!target?.classList.contains('modal-backdrop')) return
  const pointerDownTarget = modalPointerDownTarget
  modalPointerDownTarget = null
  if (pointerDownTarget && pointerDownTarget !== target && target.contains(pointerDownTarget)) {
    event.preventDefault()
    event.stopPropagation()
  }
}
function clearModalPointerDown() { modalPointerDownTarget = null }
function normalizeActionLabel(label: string) { return label.replace(/^正在(?=\S)/, '') }
async function saveHideUnidentifiedMods(event: Event) {
  const input = event.target as HTMLInputElement
  const checked = input.checked
  try {
    await run('正在保存 MOD 显示设置', async () => {
      const saved = await api.saveSettings({ hide_unidentified_mods: checked })
      if (dashboard.value) dashboard.value.config.hide_unidentified_mods = saved.hide_unidentified_mods
      return { ok: true, message: 'MOD 显示设置已保存' }
    })
  } catch {
    input.checked = !checked
  }
}
function isModRepositoryRefreshing(mod: import('./types').InstalledMod) {
  return Array.from(modReleaseRepositoryIds(mod)).some((id) => repositoryRefreshing[id])
}
async function run<T>(label: string, action: () => Promise<T>) { const actionLabel = normalizeActionLabel(label); busy.value = actionLabel; error.value = ''; message.value = ''; try { const result = await action(); const resultLabel = typeof result === 'object' && result && 'message' in result ? String((result as { message: string }).message) : actionLabel; showSuccess(t(normalizeActionLabel(resultLabel))); return result } catch (reason) { showError(reason); throw reason } finally { busy.value = '' } }
async function loadDashboard() {
  await run('正在读取厨房状态', async () => {
    dashboard.value = await api.dashboard()
    const nextZoom = clampUiZoom(dashboard.value.config.ui_zoom)
    await setUiZoom(nextZoom, false, false)
  })
}
async function detectGame() { await run('重新检测游戏目录', async () => { await api.detectGame(); dashboard.value = await api.dashboard() }) }
async function chooseGameDirectory() { const path = await api.chooseGameDirectory(); if (!path || Array.isArray(path)) return; await run('正在保存厨房目录', async () => { await api.setGamePath(path); dashboard.value = await api.dashboard() }) }
async function restoreDefaultGameDirectory() { await run('正在恢复默认游戏目录', async () => { await api.resetGamePath(); dashboard.value = await api.dashboard() }) }
async function changeGameRuntime(runtime: GameRuntime) {
  if (tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value || gameBlocked.value) return
  if (dashboard.value?.config.game.runtime === runtime) return
  await run('正在切换厨房运行模式', async () => {
    await api.saveSettings({ game_runtime: runtime })
    bepinexSetupResult.value = null
    showBepInExSetup.value = false
    await loadDashboard()
  })
}
async function refreshRepositoryEntry(repo: Repository) {
  if (repo.download_mode === 'repository_file') return false
  const currentTask = repositoryRefreshTasks.get(repo.id)
  if (currentTask) return currentTask
  const task = (async () => {
    repositoryRefreshing[repo.id] = true
    repositoryReleaseReady[repo.id] = false
    repo.cached_releases = []
    try {
      const releases = await api.refreshRepository(repo.id) ?? []
      const currentRepo = repos.value.find((item) => item.id === repo.id) ?? repo
      currentRepo.cached_releases = releases
      currentRepo.last_error = undefined
      repositoryReleaseReady[repo.id] = true
      return true
    } catch (reason) {
      const currentRepo = repos.value.find((item) => item.id === repo.id) ?? repo
      currentRepo.cached_releases = []
      currentRepo.last_error = reason instanceof Error ? reason.message : String(reason)
      repositoryReleaseReady[repo.id] = false
      return false
    } finally {
      repositoryRefreshing[repo.id] = false
      repositoryRefreshTasks.delete(repo.id)
    }
  })()
  repositoryRefreshTasks.set(repo.id, task)
  return task
}
async function refreshRepo(repo: Repository, notify = true, trackOverview = true) {
  const shouldTrackOverview = trackOverview && !repositoryRefreshBatchActive.value
  const trackingToken = shouldTrackOverview ? ++repositoryRefreshBatchToken : undefined
  if (shouldTrackOverview) {
    repositoryRefreshBatchTotal.value = 1
    repositoryRefreshBatchCompleted.value = 0
    repositoryRefreshBatchActive.value = true
  }
  try {
    const refreshed = await refreshRepositoryEntry(repo)
    if (notify) {
      error.value = ''
      message.value = ''
      if (refreshed) {
        showSuccess(`${localizedRepositoryName(repo)}${t('刷新完成')}`)
      } else {
        showError(repo.last_error || `${localizedRepositoryName(repo)}${t('刷新失败')}`)
      }
    }
    return refreshed
  } finally {
    if (trackingToken === repositoryRefreshBatchToken) {
      repositoryRefreshBatchCompleted.value = 1
      repositoryRefreshBatchActive.value = false
    }
  }
}
async function installRepo(repo: Repository) {
  if (repo.download_mode === 'release') {
    const release = installReleases(repo)[0]
    if (!release) {
      const refreshed = repositoryReleaseReady[repo.id] && !repo.last_error
      const hasReleases = repo.cached_releases.length > 0
      await requestConfirmation({
        title: t(!refreshed ? '请先刷新 Release' : hasReleases ? '没有匹配的 Release 资产' : '没有可安装的 Release'),
        message: t(!refreshed ? '请先刷新此仓库，直到版本号出现，再点击“下载并安装”。'
          : !hasReleases ? '此仓库尚未发布可安装的 Release，请查看仓库说明或发布页。'
            : isCommunityRepository(repo) ? (repo.provider === 'gitee'
              ? '此仓库的 Release 中没有 ZIP 或 DLL 资产，源码包不能作为 MOD 安装。'
              : '此仓库的 Release 中没有 ZIP 或 DLL 资产，GitHub 自动生成的源码包不能作为 MOD 安装。')
              : '此仓库的 Release 中没有符合资产规则的文件，请检查发布资产或资产规则。'),
        acknowledge: true,
      })
      return
    }
    selectedInstallTag.value = release.tag_name
    selectedInstallAsset.value = matchingInstallAssets(repo, release)[0]?.name || ''
  }
  if (!game.value?.bepinex.version) { showError('请先安装 BepInEx 框架'); return }
  selectedAssetProxyMode.value = defaultAssetProxyMode()
  selectedAssetProxyId.value = ''
  showInstall.value = repo
}
function defaultAssetProxyMode(): ProxyMode {
  return config.value?.region === 'chinese_mainland' ? 'auto_proxy' : 'auto_direct'
}
function assetProxyEntries(repo: Repository | null) {
  if (!repo || repo.provider !== 'github') return []
  const entries = repo.download_mode === 'repository_file'
    ? (config.value?.github_proxies.raw_proxies || [])
    : (config.value?.github_proxies.release_proxies || [])
  return entries.filter((proxy) => proxy.enabled)
}
function assetProxySelectionVisible(repo: Repository | null) {
  return repo?.provider === 'github'
}
function installReleases(repo: Repository | null) {
  if (!repo || repo.last_error) return []
  return repo.cached_releases.filter((release) => matchingInstallAssets(repo, release).length > 0)
}
function matchingInstallAssets(repo: Repository, release: import('./types').ReleaseInfo) {
  if (repo.download_mode === 'repository_file') return []
  return release.assets.filter((asset) => !!asset.download_url && !isCommunitySourceArchive(repo, release, asset.name)
    && repo.asset_rules.some((rule) => assetMatchesUi(asset.name, rule)))
}
function selectedInstallRelease(repo: Repository | null) { return installReleases(repo).find((release) => release.tag_name === selectedInstallTag.value) }
function selectedInstallAssets(repo: Repository | null) {
  const release = selectedInstallRelease(repo)
  return repo && release ? matchingInstallAssets(repo, release) : []
}
function closeInstallDialog() {
  showInstall.value = null
  selectedAssetProxyMode.value = defaultAssetProxyMode()
  selectedAssetProxyId.value = ''
}
function selectInstallRelease() {
  const repo = showInstall.value
  if (!repo) return
  const release = selectedInstallRelease(repo)
  selectedInstallAsset.value = release ? matchingInstallAssets(repo, release)[0]?.name || '' : ''
}
function clearDownloadProgress() {
  downloadProgressActive = false
  downloadRequestContext = {}
  progress.value = { label: '', percent: 0, speed: '', status: '' }
}
function beginDownloadProgress() {
  clearDownloadProgress()
  downloadCancelRequested.value = false
  downloadProgressActive = true
}
function receiveDownloadProgress(value: DownloadProgress) {
  if (!downloadProgressActive || downloadCancelRequested.value || appUnmounted) return
  if (progress.value.label !== value.label) downloadRequestContext = {}
  if (value.route) downloadRequestContext = {
    route: value.route, source: value.source,
    attempt: value.attempt, attempt_total: value.attempt_total,
  }
  const route = downloadRequestContext.route === 'proxy'
    ? t('代理：{name}').replace('{name}', downloadRequestContext.source || t('自动代理'))
    : downloadRequestContext.route === 'direct' ? t('直连') : ''
  const attempt = downloadRequestContext.attempt && downloadRequestContext.attempt_total
    ? ` (${downloadRequestContext.attempt}/${downloadRequestContext.attempt_total})` : ''
  const total = value.total && value.total > 0 ? value.total : undefined
  const waiting = !value.finished && value.downloaded === 0
    && (value.phase === 'connecting' || value.phase === 'switching')
  const stage = value.finished ? t('下载完成，正在处理')
    : waiting && value.phase === 'switching' ? t('正在切换下载渠道')
    : waiting ? t('正在请求，等待响应') : ''
  const parallel = !value.finished && (value.parallel_sources || 0) > 1
    ? t('并行下载源：{count}').replace('{count}', String(value.parallel_sources)) : ''
  progress.value = {
    label: value.label,
    percent: value.finished ? 100 : total ? Math.min(100, value.downloaded / total * 100) : 0,
    speed: value.finished || waiting ? '' : `${formatBytes(value.speed_bytes)}/s`,
    status: [stage, parallel, route && `${route}${attempt}`].filter(Boolean).join(' · '),
    indeterminate: !value.finished && (waiting || !total),
  }
}
async function confirmInstall() {
  if (tutorial.previewing.value || appUnmounted || busy.value) return
  const repo = showInstall.value
  if (!repo || (repo.download_mode === 'release' && (!selectedInstallTag.value || !selectedInstallAsset.value))) return
  beginDownloadProgress()
  showInstall.value = null
  try {
    const proxyMode = assetProxySelectionVisible(repo) ? selectedAssetProxyMode.value : 'direct'
    const proxyId = proxyMode === 'fixed' ? selectedAssetProxyId.value || undefined : undefined
    downloadRequestContext = {}
    const manualProxy = assetProxyEntries(repo).find((entry) => entry.id === proxyId)
    progress.value = {
      label: localizedRepositoryName(repo), percent: 0, speed: '', indeterminate: true,
      status: proxyMode === 'auto_proxy' || proxyMode === 'auto_direct' ? t('待加载，正在连接下载源')
        : proxyMode === 'fixed' ? t('代理：{name}').replace('{name}', manualProxy?.name || t('手动选择'))
        : t('正在建立直连'),
    }
    await run(`正在安装 ${localizedRepositoryName(repo)}`, () => api.installRepository(
      repo.id,
      repo.download_mode === 'release' ? selectedInstallTag.value : undefined,
      repo.download_mode === 'release' ? selectedInstallAsset.value : undefined,
      proxyMode,
      proxyId,
    ))
    await loadDashboard()
  } finally {
    clearDownloadProgress()
  }
}
function openBepInExInstall(reinstall: boolean) {
  bepinexInstallReinstall.value = reinstall
  showBepInExInstall.value = true
}
function openBepInExSetup() {
  if (tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value) return
  if (dashboard.value?.config.platform === 'windows') return
  showBepInExSetup.value = true
}
async function repairNativeBepInExOverlay(): Promise<boolean> {
  if (tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value || gameBlocked.value) return false
  if (dashboard.value?.config.platform !== 'macos' || dashboard.value.config.game.runtime !== 'native') return false
  await run('正在修复 macOS Steam Overlay', api.repairNativeBepInExOverlay)
  return true
}
async function installBepInEx(provider: Provider) {
  if (tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value) return
  const reinstall = bepinexInstallReinstall.value
  showBepInExInstall.value = false
  if (reinstall && !await requestConfirmation({ title: t('重新安装'), message: t('重新安装 BepInEx 会先备份当前框架并替换框架文件，是否继续？'), danger: true })) return
  beginDownloadProgress()
  try {
    progress.value = { label: 'BepInEx', percent: 0, speed: '', indeterminate: true, status: t('待加载，正在连接下载源') }
    const result = await run(reinstall ? '正在重新安装 BepInEx' : '正在安装 BepInEx', () => api.installBepInEx(reinstall, provider))
    await loadDashboard()
    bepinexSetupResult.value = result
    if (!appUnmounted && dashboard.value?.config.platform !== 'windows') showBepInExSetup.value = true
  } catch { /* run displays the error */
  } finally {
    clearDownloadProgress()
  }
}
async function uninstallBepInEx() { if (!await requestConfirmation({ title: t('BepInEx'), message: t('确定移除 BepInEx？程序会先创建时间戳备份。'), danger: true })) return; await run('正在移除 BepInEx', api.uninstallBepInEx); await loadDashboard() }
async function backupBepInEx() { await run('正在创建 BepInEx 备份', api.backupBepInEx); await loadDashboard() }
async function openBackupDirectory(backupPath?: string) {
  if (tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value) return
  await run('正在打开备份目录', () => api.openBackupDirectory(backupPath)).catch(() => undefined)
}
async function deleteBackup(path: string) { if (!await requestConfirmation({ title: t('确定删除备份'), message: `${t('确定删除备份')}“${path.split(/[\\/]/).pop()}”？${t('删除后无法恢复。')}`, danger: true })) return; await run('正在删除 BepInEx 备份', () => api.deleteBepInExBackup(path)); await loadDashboard() }
async function cancelDownload() {
  downloadCancelRequested.value = true
  clearDownloadProgress()
  try { await api.cancelDownload() } catch { /* 下载任务会自行结束，进度条已隐藏 */ }
}
async function refreshRustLog() {
  try {
    rustLog.value = logViewerKind.value === 'rust'
      ? await api.readRustLog()
      : logViewerKind.value === 'frontend'
        ? await api.readFrontendLog()
        : await api.readCrashLog()
    await nextTick()
    if (rustLogOutput.value) rustLogOutput.value.scrollTop = rustLogOutput.value.scrollHeight
  } catch (reason) {
    rustLog.value = { path: '', content: String(reason) }
  }
}
async function openLog(kind: 'rust' | 'frontend' | 'crash') {
  logViewerKind.value = kind
  showRustLog.value = true
  await refreshRustLog()
}
async function openRustLog() { await openLog('rust') }
async function openFrontendLog() { await openLog('frontend') }
async function openCrashLog() { await openLog('crash') }
function closeRustLog() {
  showRustLog.value = false
}
async function openDeveloperConsole() { await run('正在打开开发者控制台', api.openDevtools) }
async function resetUi() {
  if (!await requestConfirmation({ title: t('重置 UI'), message: `${t('重置 UI')}？` })) return
  await run('正在重置界面', api.resetUi)
  await loadDashboard()
}
async function restoreBackup(path: string) { await run('正在恢复 BepInEx', () => api.restoreBepInEx(path)); await loadDashboard() }
async function launchNormal() { await run('正在准备 Steam 启动', api.launchNormal); await loadDashboard() }
async function toggleWinhttp() {
  if (currentWinhttp.value === 'missing') return
  const enabled = currentWinhttp.value === 'moddisabled' || currentWinhttp.value === 'conflict'
  await run('正在切换框架状态', () => api.switchWinhttp(enabled))
  await loadDashboard()
}
async function openDirectory() { await run('正在打开游戏目录', api.openGameDirectory) }
async function openModDirectory() { await run('正在打开模组目录', api.openModDirectory) }
async function openModFileLocation(mod: InstalledMod) {
  if (tutorial.visible.value || tutorial.previewing.value || busy.value) return
  // run already displays failures; keep click errors out of unhandled rejections.
  await run('正在打开模组目录', () => api.openModFileLocation(mod.dll_file)).catch(() => undefined)
}
function openModConfig(mod: InstalledMod) {
  if (tutorial.visible.value || tutorial.previewing.value || busy.value || !mod.has_config || !mod.guid) return
  configEditorTarget.value = { kind: 'mod', mod }
}
function openBepInExConfig() {
  if (tutorial.visible.value || tutorial.previewing.value || busy.value || !game.value?.bepinex.has_config) return
  configEditorTarget.value = { kind: 'bepinex' }
}
function updateChannelName(id: string) {
  if (id.startsWith('github-proxy:')) return `${t('GitHub 内置代理')} (${id.slice('github-proxy:'.length)})`
  return t(({ 'gitee-direct': 'Gitee 更新清单', 'github-direct': 'GitHub 更新清单' } as Record<string, string>)[id] || id)
}
function updateChannelStatus(status: string) {
  return t(({ latest: '最新版', update_available: '有更新', failed: '失败', skipped: '未请求', cancelled: '已取消', checking: '检查中', downloading: '下载中', downloaded: '已下载' } as Record<string, string>)[status] || status)
}
function updateRegionLabel(region: string) {
  if (region === 'detecting') return t('检测中')
  return repositorySourceRegionLabel(region)
}

function formatFrontendLogValue(value: unknown): string {
  if (value instanceof Error) return `${value.name}: ${value.message}${value.stack ? `\n${value.stack}` : ''}`
  if (typeof value === 'string') return value
  try { return JSON.stringify(value) ?? String(value) } catch { return String(value) }
}

function redactFrontendLog(value: string): string {
  return value.replace(
    /(authorization|private[-_]?token|access[-_]?token|api[-_]?token|github[-_]?token|gitee[-_]?token)(\s*[:=]\s*(?:bearer\s+)?)[^\s,&}"']+/gi,
    '$1$2[REDACTED_SECRET]'
  )
}

function installFrontendLogging() {
  const methods = ['debug', 'info', 'log', 'warn', 'error'] as const
  const originals = new Map<string, (...args: unknown[]) => void>()
  const consoleTarget = console as unknown as Record<string, (...args: unknown[]) => void>
  for (const method of methods) {
    const original = consoleTarget[method].bind(console)
    originals.set(method, original)
    consoleTarget[method] = (...args: unknown[]) => {
      original(...args)
      const message = redactFrontendLog(args.map(formatFrontendLogValue).join(' '))
      void api.writeFrontendLog(method, message).catch(() => undefined)
    }
  }
  const onError = (event: ErrorEvent) => {
    const message = redactFrontendLog(`${event.message} at ${event.filename}:${event.lineno}:${event.colno}`)
    void api.writeFrontendLog('error', message).catch(() => undefined)
  }
  const onUnhandledRejection = (event: PromiseRejectionEvent) => {
    const message = redactFrontendLog(`Unhandled promise rejection: ${formatFrontendLogValue(event.reason)}`)
    void api.writeFrontendLog('error', message).catch(() => undefined)
  }
  window.addEventListener('error', onError)
  window.addEventListener('unhandledrejection', onUnhandledRejection)
  return () => {
    for (const method of methods) {
      const original = originals.get(method)
      if (original) consoleTarget[method] = original
    }
    window.removeEventListener('error', onError)
    window.removeEventListener('unhandledrejection', onUnhandledRejection)
  }
}

if (typeof window !== 'undefined') uninstallFrontendLogging = installFrontendLogging()

function initialUpdateCheck() {
  return {
    update: undefined,
    channels: [],
    all_failed: false,
    region: 'detecting',
  } as Awaited<ReturnType<typeof api.checkAppUpdate>>
}
function finishStartupUpdate() {
  startupUpdateChecking.value = false
  appUpdateCheck.value = null
}
function receiveAppUpdateChannel(event: AppUpdateChannelEvent) {
  if (appUnmounted || !startupUpdateChecking.value) return
  const current = appUpdateCheck.value || initialUpdateCheck()
  const baseChannels = current.region !== event.region
    ? current.channels.filter((channel) => event.region === 'mainland-china' || channel.provider === 'github')
    : current.channels
  const channels = baseChannels.some((channel) => channel.id === event.channel.id)
    ? baseChannels.map((channel) => channel.id === event.channel.id ? event.channel : channel)
    : [...baseChannels, event.channel]
  appUpdateCheck.value = { ...current, region: event.region, channels }
}
function receiveAppUpdatePhase(event: AppUpdatePhaseEvent) {
  if (appUnmounted || !startupUpdateChecking.value) return
  startupUpdatePhase.value = event.phase
  if (event.phase === 'install') startupStage.value = t('正在安装程序更新')
  if (event.update) appUpdate.value = event.update
  if (event.message && event.phase === 'skipped') startupError.value = event.message
}
async function checkAppUpdate() {
  const result = await run('正在检查程序更新', () => api.checkAppUpdate(locale.value))
  appUpdateCheck.value = result
  appUpdate.value = result.update || null
  startupUpdatePhase.value = 'manifest'
  if (!result.update && !result.all_failed) showSuccess(t('当前已是最新版本'))
}
function clearUpdateSkipTimer() {
  if (updateSkipTimer) window.clearInterval(updateSkipTimer)
  updateSkipTimer = undefined
}
async function openManualUpdatePage() {
  if (appUnmounted || !updateSkipped.value || updatePageOpening.value) return
  clearUpdateSkipTimer()
  updateSkipCountdown.value = 0
  updatePageOpening.value = true
  updatePageError.value = ''
  try {
    await api.openUpdateRepository()
  } catch (reason) {
    if (!appUnmounted && updateSkipped.value) {
      updatePageError.value = t('无法打开更新页面：{error}')
        .replace('{error}', reason instanceof Error ? reason.message : String(reason))
    }
  } finally {
    updatePageOpening.value = false
  }
}
function markUpdateSkipped(message?: string) {
  clearUpdateSkipTimer()
  updateSkipped.value = true
  updateCheckFailed.value = true
  startupUpdatePhase.value = 'skipped'
  startupError.value = message || '请手动更新管理器, 本次跳过更新.'
  updatePageError.value = ''
  updateSkipCountdown.value = 3
  updateSkipTimer = window.setInterval(() => {
    updateSkipCountdown.value -= 1
    if (updateSkipCountdown.value > 0) return
    void openManualUpdatePage()
  }, 1000)
}
function continueAfterUpdateSkip() {
  clearUpdateSkipTimer()
  updateSkipped.value = false
  updateCheckFailed.value = false
  startupError.value = ''
  updatePageError.value = ''
  updateSkipCountdown.value = 0
}
async function installAppUpdate() {
  startupUpdatePhase.value = 'download'
  beginDownloadProgress()
  try {
    const result = await run('正在安装程序更新', () => api.installAppUpdate())
    if (!result.ok && result.message !== '当前已是最新版本') markUpdateSkipped(result.message)
  } finally {
    clearDownloadProgress()
  }
}
async function refreshRepositoryBatch(repositories: Repository[], concurrency = repositories.length) {
  const trackingToken = repositories.length > 0 ? ++repositoryRefreshBatchToken : undefined
  if (trackingToken !== undefined && !repositoryRefreshBatchActive.value) {
    repositoryRefreshBatchTotal.value = 0
    repositoryRefreshBatchCompleted.value = 0
  }
  repositoryRefreshBatchTotal.value += repositories.length
  if (repositories.length > 0) repositoryRefreshBatchActive.value = true
  try {
    const results: boolean[] = []
    let nextIndex = 0
    await Promise.all(Array.from({ length: Math.min(Math.max(1, concurrency), repositories.length) }, async () => {
      while (nextIndex < repositories.length && !appUnmounted) {
        const repo = repositories[nextIndex++]!
        results.push(await refreshRepo(repo, false, false))
        repositoryRefreshBatchCompleted.value += 1
      }
    }))
    return results.filter((success) => !success).length
  } finally {
    if (trackingToken !== undefined && trackingToken === repositoryRefreshBatchToken) repositoryRefreshBatchActive.value = false
  }
}
async function refreshMappedRepositories() {
  const mappedIds = new Set(installedMods.value.flatMap((mod) => Array.from(modReleaseRepositoryIds(mod))))
  const mapped = repos.value.filter((repo) => repo.enabled && repo.download_mode === 'release' && mappedIds.has(repo.id))
  return refreshRepositoryBatch(mapped)
}
async function refreshMappedRepositoryUpdates() {
  if (!mappedReposAvailable.value) return dashboard.value
  installedModsPendingCheck.value = true
  installedModsChecking.value = false
  await nextTick()
  installedModsPendingCheck.value = false
  installedModsChecking.value = true
  try {
    const failures = await refreshMappedRepositories()
    dashboard.value = await api.dashboard()
    if (failures) throw new Error(t('仓库刷新失败摘要').replace('{count}', String(failures)))
    return dashboard.value
  } finally {
    installedModsPendingCheck.value = false
    installedModsChecking.value = false
  }
}
type SourceLoadResult = { ok: boolean; error?: string; accessFailed?: boolean }
type SourceRefreshOptions = { updateDashboard?: boolean; notify?: boolean; builtinFallback?: boolean; onSourcesLoaded?: (result: SourceLoadResult) => void }
type SourceRefreshResult = SourceLoadResult & { releaseFailures: number }

function startSourceRefresh(sourceId: string, options: SourceRefreshOptions = {}, visited = new Set<string>()) {
  let completeLoading!: (result: SourceLoadResult) => void
  const loaded = new Promise<SourceLoadResult>((resolve) => { completeLoading = resolve })
  const result = refreshSource(sourceId, {
    ...options,
    onSourcesLoaded: (result) => {
      completeLoading(result)
      options.onSourcesLoaded?.(result)
    },
  }, visited).catch((reason) => {
    const result = { ok: false, releaseFailures: 0, error: reason instanceof Error ? reason.message : String(reason) }
    completeLoading(result)
    return result
  })
  return { loaded, result }
}

async function refreshRootSources(sources: RepositorySource[]) {
  const previouslyLoaded = initialRepositorySourcesLoaded.value
  initialRepositorySourcesLoaded.value = false
  try {
    await prepareRootSourceRefresh(sources)
  } catch (reason) {
    initialRepositorySourcesLoaded.value = previouslyLoaded
    throw reason
  }
  const tasks = sources.map((source) => ({ source, ...startSourceRefresh(source.id, { updateDashboard: false, notify: false }) }))
  const officialTasks = tasks.filter((task) => task.source.builtin)
  const communityRefresh = (async () => {
    await Promise.all((officialTasks.length ? officialTasks : tasks).map((task) => task.loaded))
    await initialProxySync
    if (appUnmounted) return false
    initialRepositorySourcesLoaded.value = true
    return Promise.all(communities.map((discovery) => discovery.refresh()))
  })()
  return {
    results: Promise.all(tasks.map(async (task) => ({ source: task.source, result: await task.result }))),
    communityRefresh,
  }
}

function sourceDescendantIds(sourceId: string) {
  const ids = new Set([sourceId])
  let changed = true
  while (changed) {
    changed = false
    for (const source of config.value?.repository_sources ?? []) {
      if (source.parent_id && ids.has(source.parent_id) && !ids.has(source.id)) {
        ids.add(source.id)
        changed = true
      }
    }
  }
  return ids
}

function clearSourceContentLocally(sourceId: string) {
  const sourceIds = sourceDescendantIds(sourceId)
  for (const id of sourceIds) delete sourceErrors[id]
  if (!dashboard.value) return
  const removedRepositoryIds = dashboard.value.config.repositories
    .filter((repo) => repo.source_id && sourceIds.has(repo.source_id))
    .map((repo) => repo.id)
  dashboard.value = {
    ...dashboard.value,
    config: {
      ...dashboard.value.config,
      repositories: dashboard.value.config.repositories.filter((repo) => !repo.source_id || !sourceIds.has(repo.source_id)),
      // Keep the source being refreshed so its heading can show the loading
      // state, but remove every old child source until the new document adds
      // it back. This prevents stale child cards from flashing during reload.
      repository_sources: dashboard.value.config.repository_sources.filter((source) => source.id === sourceId || !sourceIds.has(source.id)),
    },
  }
  for (const id of removedRepositoryIds) {
    repositoryReleaseReady[id] = false
    delete repositoryRefreshing[id]
  }
}

async function prepareRootSourceRefresh(sources: RepositorySource[]) {
  console.info('[repository-refresh] preparing sources', sources.map((source) => source.id))
  // Prime every root first so a full refresh immediately replaces all old
  // descendants with their own loading shells instead of clearing roots one
  // by one while the user is watching.
  for (const source of sources) {
    sourceRefreshingManual[source.id] = true
    sourceLoading[source.id] = true
    sourceDocumentsLoading[source.id] = true
    sourceRefreshPhase[source.id] = 'clearing'
  }
  for (const source of sources) clearSourceContentLocally(source.id)
  await nextTick()
  try {
    // Let previous Release requests and queued snapshots finish before
    // clearing persisted content, so this batch cannot reuse their results.
    await Promise.all([...repositoryRefreshTasks.values(), dashboardUpdateQueue])
    const clearedConfig = await api.clearRepositorySourceCache(sources.map((source) => source.id))
    if (dashboard.value) dashboard.value = { ...dashboard.value, config: clearedConfig }
  } catch (reason) {
    for (const source of sources) {
      sourceLoading[source.id] = false
      sourceDocumentsLoading[source.id] = false
      sourceRefreshingManual[source.id] = false
      delete sourceRefreshPhase[source.id]
    }
    throw reason
  }
}

async function refreshAllRepositories() {
  if (tutorial.blocksRepositoryRefresh.value) {
    showError(t('教程完成后才能刷新内置源'))
    return
  }
  try {
    await (async () => {
      const sources = activeRepositorySources.value.filter((source) => !source.parent_id && !isCommunitySource(source.id))
      for (const repo of repos.value) repositoryReleaseReady[repo.id] = false
      const batch = await refreshRootSources(sources)
      const sourceResults = await batch.results
      const failedSources = sourceResults.filter((entry) => !entry.result.ok)
      const releaseFailures = sourceResults
        .reduce((total, entry) => total + entry.result.releaseFailures, 0)
      await initialProxySync
      await refreshDashboardSerialized()
      await batch.communityRefresh

      const failures = failedSources
        .map((entry) => `${localizedRepositorySourceName(entry.source)}: ${entry.result.error || t('刷新失败')}`)
      if (releaseFailures) failures.push(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
      if (failures.length) throw new Error(failures.join('；'))
      return { ok: true, message: '全部仓库刷新完成' }
    })()
    showSuccess(t('全部仓库刷新完成'))
  } catch (reason) {
    showError(reason)
  } finally {
    for (const source of config.value?.repository_sources ?? []) {
      sourceLoading[source.id] = false
      sourceDocumentsLoading[source.id] = false
      sourceRefreshingManual[source.id] = false
    }
  }
}
async function checkModUpdates() {
  if (!mappedReposAvailable.value) return
  await run('正在检查 MOD 更新', refreshMappedRepositoryUpdates)
}
async function refreshSourceReleases(sourceId: string, updateDashboard = true, manageManualState = true) {
  if (manageManualState) sourceRefreshingManual[sourceId] = true
  try {
    const sourceRepos = repos.value.filter((repo) => repo.source_id === sourceId && repo.enabled && repo.download_mode === 'release')
    const failures = await refreshRepositoryBatch(sourceRepos)
    if (updateDashboard) dashboard.value = await api.dashboard()
    return failures
  } finally {
    if (manageManualState) sourceRefreshingManual[sourceId] = false
  }
}
async function syncInitialGithubProxies() {
  try {
    const saved = await api.refreshGithubProxies()
    if (!appUnmounted && dashboard.value) dashboard.value.config.github_proxies = saved.github_proxies
  } catch (reason) {
    console.warn('[proxy-sync] initial proxy synchronization failed; bundled proxies remain available', reason)
  }
}
function startInitialRepositoryRefresh() {
  if (appUnmounted) return
  if (tutorial.blocksRepositoryRefresh.value) {
    console.info('[repository-refresh] waiting for tutorial to finish')
    return
  }
  if (initialRepositoryRefreshStarted) return
  initialRepositoryRefreshStarted = true
  const sources = activeRepositorySources.value.filter((source) => !source.parent_id && !isCommunitySource(source.id))
  console.info('[repository-refresh] starting initial refresh', sources.map((source) => source.id))
  // Do not start this from startupChecks until the first-run tutorial has
  // finished. Once started, each source and repository refreshes independently
  // and updates the visible dashboard as soon as its content is available.
  void (async () => {
    initialProxySync = syncInitialGithubProxies()
    const batch = await refreshRootSources(sources)
    const results = await batch.results
    const failedSources = results.filter((entry) => !entry.result.ok)
    const releaseFailures = results.reduce((total, entry) => total + entry.result.releaseFailures, 0)
    // The final snapshot must include the proxy commit, even if its download
    // finishes after a source. Individual source cards reveal independently.
    await initialProxySync
    if (appUnmounted) return
    await refreshDashboardSerialized()
    if (appUnmounted) return
    if (failedSources.length) showError(failedSources.map((entry) => `${localizedRepositorySourceName(entry.source)}: ${entry.result.error || t('刷新失败')}`).join('；'))
    if (releaseFailures) showError(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
    await batch.communityRefresh
  })().catch((reason) => showError(reason))
}
async function startupChecks() {
  appUpdateCheck.value = initialUpdateCheck()
  startupStage.value = t('识别系统地区')
  beginDownloadProgress()
  try {
    startupStage.value = t('检查安装器更新')
    // One pipeline finishes Gitee first, then races GitHub manifests/packages.
    // A separate checkAppUpdate call would repeat discovery before download.
    const installResult = await api.installAppUpdate()
    if (!installResult.ok && installResult.message !== '当前已是最新版本') {
      markUpdateSkipped(installResult.message)
    }
  } catch (reason) {
    console.warn('[startup] installer update failed', reason)
    markUpdateSkipped('请手动更新管理器, 本次跳过更新.')
  } finally {
    clearDownloadProgress()
    finishStartupUpdate()
  }
  startupLoading.value = true
  startupStage.value = t('读取厨房状态')
  try {
    await withTimeout(loadDashboard(), STARTUP_IPC_TIMEOUT_MS, '读取厨房状态')
  } finally {
    startupLoading.value = false
  }
  resetInstalledModExpansion()
  for (const repo of repos.value) repositoryReleaseReady[repo.id] = false
  if (!await tutorialRuntime.startInitialOnboarding()) {
    startInitialRepositoryRefresh()
  } else {
    console.info('[repository-refresh] waiting for initial tutorial; sources have not been loaded yet')
  }
}
function editRepo(repo?: Repository) {
  Object.assign(repoForm, repo ? {
    id: repo.id, name: repo.name, group: repo.group || '', provider: repo.provider, username: repo.username, repository: repo.repository,
    download_mode: repo.download_mode, branch: repo.branch || 'main', file_path: repo.file_path || '',
    region: normalizeRepositoryRegion(repo.region), channel: repo.channel,
    asset_rules: repo.asset_rules.map((rule) => ({ ...rule })), install_target: repo.install_target,
    note: repo.note || '', plugin_guid: repo.plugin_guid, local_product_name: repo.local_product_name || '',
    local_original_filename: repo.local_original_filename || '', version_regex: repo.version_regex || '',
    max_release_count: repo.max_release_count || 10,
    download_threads: repo.download_threads || (repo.provider === 'github' ? 16 : 1),
    github_token: '', gitee_token: '', official_api_only: true, enabled: repo.enabled,
  } : {
    id: undefined, name: '', group: '', provider: 'gitee', username: '', repository: '', download_mode: 'release',
    branch: 'main', file_path: '', region: '', channel: 'all', asset_rules: [],
    install_target: 'BepInEx/plugins', note: '', plugin_guid: '', local_product_name: '',
    local_original_filename: '', version_regex: '', max_release_count: 10, download_threads: 1,
    github_token: '', gitee_token: '',
    official_api_only: true, enabled: true,
  })
  showRepoEditor.value = true
}
async function saveRepo() {
  if (repoForm.download_mode === 'release' && (!repoForm.asset_rules.length || repoForm.asset_rules.some((rule) => !rule.value.trim()))) {
    showError(t('请创建至少一个有效的资产规则'))
    return
  }
  let refreshError = ''
  let refreshed = false
  try {
    await run('正在保存仓库', async () => {
      const savedConfig = await api.saveRepository(repoForm)
      const savedRepo = savedConfig.repositories.find((repo) => repo.id === repoForm.id)
        ?? savedConfig.repositories.find((repo) => repo.name === repoForm.name && repo.username === repoForm.username.trim()
          && repo.repository === repoForm.repository.trim() && repo.provider === repoForm.provider
          && (repo.region || '') === (repoForm.region?.trim().toLowerCase() || ''))
      if (!savedRepo) throw new Error('保存后找不到仓库配置')
      dashboard.value!.config = savedConfig
      showRepoEditor.value = false
      if (savedRepo.enabled && savedRepo.download_mode === 'release' && repos.value.some((repo) => repo.id === savedRepo.id)) {
        refreshed = true
        if (!await refreshRepo(savedRepo, false)) refreshError = savedRepo.last_error || `${localizedRepositoryName(savedRepo)}${t('刷新失败')}`
      }
      dashboard.value = await api.dashboard()
    })
  } catch {
    return
  }
  if (refreshError) showError(refreshError)
  else {
    showSuccess(t(refreshed ? '仓库已保存并刷新' : '仓库已保存'))
  }
}
async function saveGithubProxyConfig(proxyConfig: GitHubProxyConfig) {
  const result = await run('正在保存 GitHub 代理配置', async () => {
    const saved = await api.saveGithubProxies(proxyConfig)
    if (dashboard.value) dashboard.value.config = saved
    return { config: saved.github_proxies, message: 'GitHub 代理配置已保存' }
  })
  return result.config
}
async function refreshGithubProxyConfig(proxyConfig: GitHubProxyConfig) {
  const result = await run('正在同步内置 GitHub 代理', async () => {
    const saved = await api.saveGithubProxies(proxyConfig)
    if (dashboard.value) dashboard.value.config = saved
    const refreshed = await api.refreshGithubProxies()
    if (dashboard.value) dashboard.value.config = refreshed
    return { config: refreshed.github_proxies, message: 'GitHub 代理列表已同步' }
  })
  return result.config
}
function providerChanged() {
  repoForm.download_threads = repoForm.provider === 'github' ? 16 : 1
}
async function deleteRepo(repo: Repository) { if (repo.builtin || !await requestConfirmation({ title: t('删除仓库'), message: `${t('删除仓库')} “${localizedRepositoryName(repo)}”?`, danger: true })) return; await run('正在删除仓库', async () => { await api.deleteRepository(repo.id); await loadDashboard() }) }
function addRule() { repoForm.asset_rules.push({ type: 'exact', value: '' }) }
function removeRule(index: number) { repoForm.asset_rules.splice(index, 1) }
async function exportRepositories() {
  showExport.value = true
}
async function continueExportRepositories() { const path = await api.chooseExportFile(); if (!path || Array.isArray(path)) return; await run('正在导出仓库 TOML', () => api.exportFile(path, null)); showExport.value = false }
async function copyRepositories() { await run('正在复制仓库 TOML', () => api.copyRepositories(null)); showExport.value = false }
async function importFromClipboard() { importText.value = await api.readClipboard(); await previewImport() }
async function importFromFile() { const path = await api.chooseImportFile(); if (!path || Array.isArray(path)) return; importText.value = await api.readTextFile(path); await previewImport() }
async function previewImport() { importPreview.value = await run('正在检查仓库 TOML', () => api.previewImport(importText.value)) }
async function applyImport(conflict: string) {
  if (conflict === 'overwrite' && !await requestConfirmation({ title: t('导入仓库配置'), message: t('本地的仓库会直接覆盖成粘贴的内容，并且无法恢复，是否确定？'), danger: true })) return
  await run('正在导入仓库配置', async () => { await api.importRepositories(importText.value, conflict); showImport.value = false; await loadDashboard() })
}
function resetRepositorySourceEditor() {
  repositorySourceUrl.value = ''
  repositorySourceNickname.value = ''
  repositorySourceUniqueId.value = ''
  repositorySourceRegion.value = ''
  editingRepositorySourceId.value = null
  repositorySourceEditorOpen.value = false
}
function openRepositorySourceManager() {
  repositorySourceDraft.value = (config.value?.repository_sources ?? []).filter((source) => !isCommunitySource(source.id)).map((source) => ({ ...source }))
  resetRepositorySourceEditor()
  showRepositorySource.value = true
}
function closeRepositorySourceManager() {
  showRepositorySource.value = false
  repositorySourceDraft.value = []
  resetRepositorySourceEditor()
}
function startAddingRepositorySource() {
  repositorySourceUrl.value = ''
  repositorySourceNickname.value = ''
  repositorySourceUniqueId.value = ''
  repositorySourceRegion.value = ''
  editingRepositorySourceId.value = null
  repositorySourceEditorOpen.value = true
}
function editRepositorySource(source: RepositorySource) {
  if (source.parent_id) return
  editingRepositorySourceId.value = source.id
  repositorySourceNickname.value = source.nickname
  repositorySourceUrl.value = source.url
  repositorySourceUniqueId.value = source.id
  repositorySourceRegion.value = normalizeRepositoryRegion(source.region)
  repositorySourceEditorOpen.value = true
}
function applyRepositorySourceDraft() {
  const sourceId = repositorySourceUniqueId.value.trim()
  const sourceUrl = repositorySourceUrl.value.trim()
  const nickname = repositorySourceNickname.value.trim()
  if (!sourceId || !sourceUrl || !nickname) return
  if (!editingRepositorySourceId.value && !/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(sourceId)) {
    showError(t('仓库唯一ID格式无效'))
    return
  }
  const duplicateId = repositorySourceDraft.value.some((source) => source.id !== editingRepositorySourceId.value && source.id === sourceId)
  if (duplicateId) {
    showError(t('仓库唯一ID已存在'))
    return
  }
  const sourceKey = repositorySourceKey({ url: sourceUrl, region: repositorySourceRegion.value })
  const duplicate = repositorySourceDraft.value.some((source) => source.id !== editingRepositorySourceId.value && repositorySourceKey(source) === sourceKey)
  if (duplicate) {
    showError(t('仓库源地址已存在'))
    return
  }
  if (editingRepositorySourceId.value) {
    const source = repositorySourceDraft.value.find((item) => item.id === editingRepositorySourceId.value)
    if (source && !source.builtin) {
      source.nickname = nickname
      source.url = sourceUrl
      source.region = repositorySourceRegion.value.trim() || undefined
    }
  } else {
    repositorySourceDraft.value.push({
      id: sourceId,
      nickname,
      url: sourceUrl,
      region: repositorySourceRegion.value.trim() || undefined,
      builtin: false,
    })
  }
  resetRepositorySourceEditor()
}
async function removeRepositorySource(source: RepositorySource) {
  if (source.builtin || source.parent_id || !await requestConfirmation({ title: t('删除仓库源'), message: `${t('删除仓库源')} “${localizedRepositorySourceName(source)}”?`, danger: true })) return
  const removedIds = new Set([source.id])
  let changed = true
  while (changed) {
    changed = false
    for (const item of repositorySourceDraft.value) {
      if (item.parent_id && removedIds.has(item.parent_id) && !removedIds.has(item.id)) {
        removedIds.add(item.id)
        changed = true
      }
    }
  }
  repositorySourceDraft.value = repositorySourceDraft.value.filter((item) => !removedIds.has(item.id))
  if (editingRepositorySourceId.value === source.id) resetRepositorySourceEditor()
}
function startSourceDrag(id: string) {
  const entry = repositorySourceManagerEntries.value.find((item) => item.source.id === id)
  if (!entry || entry.depth > 0 || entry.source.parent_id || entry.source.builtin) return
  draggingSourceId.value = id
}
function dropSource(id: string) {
  if (!draggingSourceId.value || draggingSourceId.value === id) return
  const sources = [...repositorySourceDraft.value]
  const entries = repositorySourceManagerEntries.value
  const movingEntry = entries.find((entry) => entry.source.id === draggingSourceId.value)
  const targetEntry = entries.find((entry) => entry.source.id === id)
  if (!movingEntry || !targetEntry || movingEntry.depth > 0 || targetEntry.depth > 0 || movingEntry.source.parent_id || targetEntry.source.parent_id || movingEntry.source.builtin || targetEntry.source.builtin) {
    draggingSourceId.value = null
    return
  }
  const roots = entries.filter((entry) => entry.depth === 0).map((entry) => entry.source)
  const from = roots.findIndex((source) => source.id === movingEntry.source.id)
  const to = roots.findIndex((source) => source.id === targetEntry.source.id)
  if (from < 0 || to < 0) {
    draggingSourceId.value = null
    return
  }
  const [source] = roots.splice(from, 1)
  roots.splice(to, 0, source)
  const childrenByParent = new Map<string, RepositorySource[]>()
  for (const item of sources) {
    if (!item.parent_id) continue
    const children = childrenByParent.get(item.parent_id) || []
    children.push(item)
    childrenByParent.set(item.parent_id, children)
  }
  const ordered: RepositorySource[] = []
  const visited = new Set<string>()
  const append = (item: RepositorySource) => {
    if (visited.has(item.id)) return
    visited.add(item.id)
    ordered.push(item)
    for (const child of childrenByParent.get(item.id) || []) append(child)
  }
  roots.forEach(append)
  sources.forEach(append)
  repositorySourceDraft.value = ordered
  draggingSourceId.value = null
}
async function saveRepositorySourceChanges() {
  if (repositorySourceEditorOpen.value) {
    applyRepositorySourceDraft()
    if (repositorySourceEditorOpen.value) return
  }
  const draft = repositorySourceDraft.value.map((source) => ({ ...source, nickname: source.nickname.trim(), url: source.url.trim() }))
  if (!draft.length || draft.some((source) => !source.nickname || !source.url)) {
    showError(t('仓库源昵称和地址不能为空'))
    return
  }
  const normalizedUrls = draft.map(repositorySourceKey)
  if (new Set(normalizedUrls).size !== normalizedUrls.length) {
    showError(t('仓库源地址已存在'))
    return
  }
  let savedSourceConfig: PublicConfig | null = null
  try {
    await run('正在保存仓库源', async () => {
      const sourceConfig = await api.saveRepositorySources(draft)
      closeRepositorySourceManager()
      const savedDashboard = await api.dashboard()
      const savedSourceDashboard = { ...savedDashboard, config: sourceConfig }
      dashboard.value = savedSourceDashboard
      savedSourceConfig = sourceConfig
      return sourceConfig
    })
  } catch {
    return
  }
  if (!savedSourceConfig) return
  if (tutorial.blocksRepositoryRefresh.value) {
    showError(t('教程完成后才能刷新内置源'))
    return
  }
  const refreshedSourceConfig = savedSourceConfig as PublicConfig
  void (async () => {
    try {
      const rootSources = refreshedSourceConfig.repository_sources
        .filter((item) => !item.parent_id && !isCommunitySource(item.id) && sourceMatchesRegion(item))
      const batch = await refreshRootSources(rootSources)
      const sourceResults = await batch.results
      const failures = sourceResults
        .filter((entry) => !entry.result.ok)
        .map((entry) => `${localizedRepositorySourceName(entry.source)}: ${entry.result.error || t('刷新失败')}`)
      const releaseFailures = sourceResults.reduce((total, entry) => total + entry.result.releaseFailures, 0)
      if (releaseFailures) failures.push(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
      await initialProxySync
      await refreshDashboardSerialized()
      await batch.communityRefresh
      if (failures.length) showError(failures.join('；'))
    } catch (reason) {
      showError(reason)
    }
  })()
}
function sourceRefreshing(sourceId: string) {
  const discovery = communityForSource(sourceId)
  if (discovery) return discovery.searching.value
  const tutorialRootSourceId = activeRepositorySources.value[0]?.id
  if (tutorial.repositoryLoadingActive.value && sourceId === tutorialRootSourceId) return true
  return !!sourceLoading[sourceId]
}
function sourceCardLoading(sourceId: string) {
  return sourceRefreshing(sourceId) || !!sourceContentRevealing[sourceId] || !!sourceErrors[sourceId]
    || !!communityForSource(sourceId)?.error.value
}
function sourceBusy(sourceId: string) {
  const discovery = communityForSource(sourceId)
  return discovery ? discovery.refreshing.value || !discovery.available.value : !!sourceLoading[sourceId] || !!sourceRefreshingManual[sourceId]
}
function finishSourceContentReveal(sourceId: string) {
  sourceContentRevealing[sourceId] = false
  const timer = sourceContentRevealTimers.get(sourceId)
  if (timer !== undefined) window.clearTimeout(timer)
  sourceContentRevealTimers.delete(sourceId)
}
function startSourceContentReveal(sourceId: string) {
  finishSourceContentReveal(sourceId)
  sourceContentRevealing[sourceId] = true
  sourceContentRevealTimers.set(sourceId, window.setTimeout(() => {
    finishSourceContentReveal(sourceId)
  }, 700))
}
function onSourceContentEntered(sourceId: string) {
  if (sourceRefreshPhase[sourceId] === 'clearing' || sourceRefreshPhase[sourceId] === 'loading') return
  finishSourceContentReveal(sourceId)
  sourceLoading[sourceId] = false
}
function toggleSource(sourceId: string) { sourceCollapsed[sourceId] = !sourceCollapsed[sourceId] }
function toggleModDetails(modId: string) { modDetailsExpanded[modId] = !modDetailsExpanded[modId] }
function repositoryGroupKey(sourceId: string, group: string) { return `${sourceId}:${group}` }
function toggleRepositoryGroup(sourceId: string, group: string) {
  if (!group) return
  const key = repositoryGroupKey(sourceId, group)
  repositoryGroupCollapsed[key] = !repositoryGroupCollapsed[key]
}
async function refreshSource(sourceId: string, options: SourceRefreshOptions = {}, visited = new Set<string>()): Promise<SourceRefreshResult> {
  const discovery = communityForSource(sourceId)
  if (discovery) return { ok: await discovery.refresh(), releaseFailures: discovery.releaseFailures.value, error: discovery.error.value }
  if (visited.has(sourceId)) {
    options.onSourcesLoaded?.({ ok: true })
    return { ok: true, releaseFailures: 0 }
  }
  let sourcesLoaded = false
  function completeSourceLoading(result: SourceLoadResult) {
    if (sourcesLoaded) return
    sourcesLoaded = true
    sourceDocumentsLoading[sourceId] = false
    options.onSourcesLoaded?.(result)
  }
  if (tutorial.blocksRepositoryRefresh.value) {
    const error = t('教程完成后才能刷新内置源')
    completeSourceLoading({ ok: false, error })
    if (options.notify !== false) showError(error)
    return { ok: false, releaseFailures: 0, error }
  }
  visited.add(sourceId)
  const updateDashboard = options.updateDashboard ?? true
  const notify = options.notify ?? true
  const source = config.value?.repository_sources.find((item) => item.id === sourceId)
  let accessFailed = false
  let builtinFallbackAvailable = false
  async function loadBuiltinFallback(reason: string) {
    console.warn('[repository-refresh] using bundled index', { sourceId, reason })
    const retryVisited = new Set(visited)
    retryVisited.delete(sourceId)
    return refreshSource(sourceId, { ...options, builtinFallback: true, onSourcesLoaded: completeSourceLoading }, retryVisited)
  }
  delete sourceErrors[sourceId]
  sourceRefreshingManual[sourceId] = true
  sourceLoading[sourceId] = true
  sourceDocumentsLoading[sourceId] = true
  sourceRefreshPhase[sourceId] = 'clearing'
  try {
    if (!source) throw new Error(t('仓库源不存在'))
    if (!sourceMatchesRegion(source)) {
      completeSourceLoading({ ok: true })
      return { ok: true, releaseFailures: 0 }
    }
    clearSourceContentLocally(sourceId)
    await nextTick()
    sourceRefreshPhase[sourceId] = 'loading'
    const imported = await api.importRepositorySource(source.id, source.url, options.builtinFallback ?? false)
    builtinFallbackAvailable = imported.builtin_fallback_available
    if (imported.access_error) {
      accessFailed = true
      throw new Error(imported.access_error)
    }
    await refreshConfigSerialized()
    const childSources = (config.value?.repository_sources ?? []).filter((item) => item.parent_id === sourceId && sourceMatchesRegion(item))
    sourceRefreshPhase[sourceId] = 'adding'
    if (sourceCollapsed[sourceId]) {
      finishSourceContentReveal(sourceId)
    } else {
      startSourceContentReveal(sourceId)
    }
    await nextTick()
    // The source document has been parsed and its repository cards are now
    // mounted. Release the source-level mask immediately; Release requests
    // continue independently in their own row states.
    sourceLoading[sourceId] = false
    sourceRefreshPhase[sourceId] = 'refreshing'
    await nextTick()
    const childTasks = childSources.map((child) =>
      startSourceRefresh(child.id, { updateDashboard: false, notify: false }, new Set(visited)))
    const releaseResultsPromise = Promise.all([
      refreshSourceReleases(sourceId, false, false),
      Promise.all(childTasks.map((task) => task.result)),
    ])
    void releaseResultsPromise.catch(() => undefined)
    const childLoadResults = await Promise.all(childTasks.map((task) => task.loaded))
    const sourceIds = sourceDescendantIds(sourceId)
    const hasRepositories = (config.value?.repositories ?? []).some((repo) => repo.source_id && sourceIds.has(repo.source_id))
    if (builtinFallbackAvailable && !options.builtinFallback && childLoadResults.length > 0
      && childLoadResults.every((result) => result.accessFailed) && !hasRepositories) {
      await releaseResultsPromise
      return await loadBuiltinFallback('all_children_unreachable_without_repositories')
    }
    const childLoadFailures = childLoadResults.filter((result) => !result.ok)
    completeSourceLoading({ ok: childLoadFailures.length === 0, error: childLoadFailures.length
      ? childLoadFailures.map((result) => result.error || t('刷新失败')).join('；') : undefined })
    const [releaseFailures, childResults] = await releaseResultsPromise
    const childReleaseFailures = childResults.reduce((total, result) => total + result.releaseFailures, 0)
    const childFailures = childResults.filter((result) => !result.ok)
    if (updateDashboard) await refreshDashboardSerialized()
    if (releaseFailures + childReleaseFailures && notify) showError(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures + childReleaseFailures)))
    if (childFailures.length) {
      return { ok: false, releaseFailures: releaseFailures + childReleaseFailures, error: childFailures.map((result) => result.error || t('刷新失败')).join('；') }
    }
    return { ok: true, releaseFailures: releaseFailures + childReleaseFailures }
  } catch (reason) {
    if (accessFailed && builtinFallbackAvailable && !options.builtinFallback) {
      return await loadBuiltinFallback('index_unreachable')
    }
    const detail = reason instanceof Error ? reason.message : String(reason)
    sourceErrors[sourceId] = detail
    await (updateDashboard ? refreshDashboardSerialized() : refreshConfigSerialized()).catch(() => undefined)
    completeSourceLoading({ ok: false, error: detail, accessFailed })
    if (notify) showError(detail)
    return { ok: false, releaseFailures: 0, error: detail, accessFailed }
  } finally {
    completeSourceLoading({ ok: false, error: sourceErrors[sourceId] })
    finishSourceContentReveal(sourceId)
    sourceRefreshingManual[sourceId] = false
    sourceLoading[sourceId] = false
    delete sourceRefreshPhase[sourceId]
  }
}
function openModRecycleBin() {
  if (tutorial.visible.value || tutorial.previewing.value || busy.value) return
  showModRecycleBin.value = true
}
function handleRecycledModRestored() {
  void refreshDashboardSerialized().catch(showError)
}
function editModNote(mod: import('./types').InstalledMod) { editingMod.value = mod; modNote.value = mod.note ?? '' }
async function saveModNote() { if (!editingMod.value) return; await run('正在保存 MOD 备注', async () => { await api.saveModNote(editingMod.value!.dll_file, modNote.value); editingMod.value = null; await loadDashboard() }) }
function uninstallModPackage(mod: InstalledMod) {
  const packageId = packageIdForMod(mod)
  if (!packageId) return
  modUninstallPending.value = {
    kind: 'package',
    phase: 'confirm',
    packageId,
    packageName: packageNameForMod(mod),
    mods: packageModsForMod(mod),
  }
}
async function uninstallMod(mod: InstalledMod) {
  const packageId = packageIdForMod(mod)
  const manifestHasOtherFiles = !!mod.package_id && (mod.package_mod_count || 0) > 1
  const hasSameGuid = hasOtherEnabledSameGuid(mod)
  if (!packageId && manifestHasOtherFiles && !hasSameGuid) {
    modUninstallPending.value = {
      kind: 'package',
      phase: 'confirm',
      packageId: mod.package_id,
      packageName: packageNameForMod(mod),
    }
    return
  }
  const siblings = packageModsForMod(mod).filter((item) => item.dll_file !== mod.dll_file)
  modUninstallPending.value = packageId && siblings.length && !hasSameGuid
    ? { kind: 'mod', phase: 'package', mod, packageId, siblingNames: siblings.map((item) => item.name) }
    : { kind: 'mod', phase: 'confirm', mod }
}
function cancelModUninstall() {
  modUninstallPending.value = null
}
async function confirmModUninstall() {
  const pending = modUninstallPending.value
  if (!pending) return
  if (pending.kind === 'package') {
    await run(`正在卸载 ${pending.packageName || t('MOD 包')}`, () => api.uninstallRepository(pending.packageId!))
    cancelModUninstall()
    await loadDashboard()
    return
  }
  const mod = pending.mod
  if (!mod) return
  if (pending.phase === 'package') {
    await run(`正在删除 ${mod.name}`, () => api.uninstallModDll(mod.dll_file, pending.packageId!))
    cancelModUninstall()
    await loadDashboard()
    return
  }
  if (pending.phase === 'unmanaged') {
    await run(`正在删除 ${mod.name}`, () => api.uninstallLocalMod(mod.dll_file, true))
    cancelModUninstall()
    await loadDashboard()
    return
  }
  const result = await run(`正在删除 ${mod.name}`, () => api.uninstallLocalMod(mod.dll_file))
  if (result?.requires_package_confirmation) {
    message.value = ''
    const packageId = result.package_id || packageIdForMod(mod)
    const siblingNames = result.package_mod_names?.length
      ? result.package_mod_names
      : packageModsForMod(mod).filter((item) => item.dll_file !== mod.dll_file).map((item) => item.name)
    if (packageId) {
      modUninstallPending.value = { kind: 'mod', phase: 'package', mod, packageId, siblingNames }
    }
    return
  }
  if (result?.requires_confirmation) {
    message.value = ''
    modUninstallPending.value = { kind: 'mod', phase: 'unmanaged', mod }
    return
  }
  cancelModUninstall()
  await loadDashboard()
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
async function toggleMod(mod: InstalledMod) {
  const key = mod.dll_file
  if (isModToggling(mod) || gameBlocked.value || busy.value) return
  const enabled = !modEnabledForView(mod)
  modTogglePending[key] = enabled
  await nextTick()
  try {
    const result = await api.toggleLocalMod(key, enabled)
    const current = dashboard.value?.mods.find((item) => item.dll_file === key)
    if (current) current.enabled = enabled
    delete modTogglePending[key]
    showSuccess(t(result.message))
    try {
      await refreshDashboardSerialized()
    } catch (reason) {
      showError(reason)
    }
  } catch (reason) {
    delete modTogglePending[key]
    showError(reason)
  }
}
async function toggleModPackage(mod: InstalledMod) {
  const packageId = packageIdForMod(mod)
  const packageMods = packageModsForMod(mod)
  if (!packageId || !packageMods.length || gameBlocked.value || busy.value || isPackageToggling(mod)) return
  const enabled = !packageEnabledForMod(mod)
  modPackageTogglePending[packageId] = enabled
  await nextTick()
  try {
    const result = await api.toggleLocalModPackage(packageId, enabled)
    for (const current of packageMods) {
      const dashboardMod = dashboard.value?.mods.find((item) => item.dll_file === current.dll_file)
      if (dashboardMod) dashboardMod.enabled = enabled
    }
    delete modPackageTogglePending[packageId]
    showSuccess(t(result.message))
    try {
      await refreshDashboardSerialized()
    } catch (reason) {
      showError(reason)
    }
  } catch (reason) {
    delete modPackageTogglePending[packageId]
    showError(reason)
  }
}
function displayModPath(mod: InstalledMod) {
  return `${mod.dll_file}${modEnabledForView(mod) ? '' : '.disabled'}`
}
const manualInstallMode = ref<'choose' | 'online' | null>(null)
const manualInstallUrl = ref('')
function installLocalMod() {
  if (tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value || gameBlocked.value) return
  manualInstallUrl.value = ''
  manualInstallMode.value = 'choose'
}
async function chooseLocalMod() {
  manualInstallMode.value = null
  const path = await api.chooseModFile()
  if (!path || Array.isArray(path)) return
  if (busy.value || gameBlocked.value || appUnmounted) return
  try {
    await run('正在安装本地 MOD', () => api.installLocalMod(path))
    await loadDashboard()
  } catch { /* run displays the error */ }
}
async function installOnlineMod() {
  if (busy.value || gameBlocked.value || appUnmounted || !manualInstallUrl.value.trim()) return
  const url = manualInstallUrl.value.trim()
  try {
    const parsed = new URL(url)
    if (!['https:', 'http:'].includes(parsed.protocol) || parsed.username || parsed.password) throw new Error()
  } catch { showError(t('请输入有效的 HTTP 或 HTTPS 下载直链')); return }
  manualInstallMode.value = null
  beginDownloadProgress()
  progress.value = { label: t('在线文件'), percent: 0, speed: '', indeterminate: true, status: t('正在建立直连') }
  try {
    await run('正在安装在线 MOD', () => api.installOnlineMod(url))
    await loadDashboard()
  } catch { /* run displays the error */ }
  finally { clearDownloadProgress() }
}
async function rescanInstalledMods() {
  installedModsPendingCheck.value = true
  installedModsChecking.value = false
  try {
    await run('正在重新扫描', async () => {
      dashboard.value = await api.dashboard()
      return refreshMappedRepositoryUpdates()
    })
  } finally {
    if (!installedModsChecking.value) installedModsPendingCheck.value = false
  }
}

onMounted(async () => {
  window.addEventListener('wheel', handleUiZoomWheel, { passive: false })
  window.addEventListener('blur', clearModalPointerDown)
  document.addEventListener('pointerdown', rememberModalPointerDown, true)
  document.addEventListener('pointerup', clearModalPointerDownAfterPointerUp, true)
  document.addEventListener('pointercancel', clearModalPointerDown, true)
  document.addEventListener('click', preventDraggedModalBackdropClick, true)
  startupStage.value = t('初始化应用')
  try {
    const gameStateSubscription = watchGameState((game) => {
      // Update only real game state, including while forms are open. Config
      // drafts and tutorial scenario data remain owned by their host.
      if (!appUnmounted && dashboard.value) dashboard.value.game = game
    })
    try {
      const stop = await withTimeout(gameStateSubscription, STARTUP_IPC_TIMEOUT_MS, '注册游戏状态监听')
      if (appUnmounted) stop()
      else stopGameState.value = stop
    } catch (reason) {
      // A timed-out IPC can still finish later; release that late listener.
      void gameStateSubscription.then((stop) => stop()).catch(() => undefined)
      console.warn('[startup] game state listener IPC failed', reason)
    }
    try {
      appVersion.value = await withTimeout(getVersion(), STARTUP_IPC_TIMEOUT_MS, '读取程序版本')
      document.title = `${t('OC2 模组管理器')} v${appVersion.value}`
      void getCurrentWindow().setTitle(document.title).catch(() => undefined)
    } catch (reason) {
      console.warn('[startup] getVersion IPC failed', reason)
    }
    if (!localStorage.getItem('oc2-locale')) {
      try {
        const installLanguage = await withTimeout(api.installLanguage(), STARTUP_IPC_TIMEOUT_MS, '读取安装语言')
        if (installLanguage) setLocale(installLanguage)
      } catch (reason) {
        console.warn('[startup] install language IPC failed', reason)
      }
    }
    try {
      stopAppUpdateChannel.value = await withTimeout(watchAppUpdateChannel(receiveAppUpdateChannel), STARTUP_IPC_TIMEOUT_MS, '注册更新状态监听')
    } catch (reason) {
      console.warn('[startup] app update channel IPC failed', reason)
    }
    try {
      stopAppUpdatePhase.value = await withTimeout(watchAppUpdatePhase(receiveAppUpdatePhase), STARTUP_IPC_TIMEOUT_MS, '注册更新阶段监听')
    } catch (reason) {
      console.warn('[startup] app update phase IPC failed', reason)
    }
    let progressListenerExpired = false
    void withTimeout(watchDownloadProgress(receiveDownloadProgress).then((stop) => {
      if (appUnmounted || progressListenerExpired) { stop(); return () => undefined }
      return stop
    }), STARTUP_IPC_TIMEOUT_MS, '注册下载进度监听').then((stop) => {
      if (appUnmounted) stop()
      else stopProgress.value = stop
    }).catch((reason) => {
      progressListenerExpired = true
      console.warn('[startup] download progress IPC failed', reason)
    })
    startupStage.value = t('检查安装器更新')
    await startupChecks()
  } catch (reason) {
    console.error('[startup] startup checks failed', reason)
    if (startupUpdateChecking.value) {
      if (!updateSkipped.value) startupError.value = t('检查更新失败，请重新启动MOD管理器。')
      updateCheckFailed.value = true
    } else {
      startupLoadingError.value = t('初始化失败，请重新启动 MOD 管理器。')
    }
  } finally {
    startupUpdateChecking.value = false
    startupLoading.value = false
  }
})
onUnmounted(() => {
  appUnmounted = true
  window.removeEventListener('wheel', handleUiZoomWheel)
  window.removeEventListener('blur', clearModalPointerDown)
  document.removeEventListener('pointerdown', rememberModalPointerDown, true)
  document.removeEventListener('pointerup', clearModalPointerDownAfterPointerUp, true)
  document.removeEventListener('pointercancel', clearModalPointerDown, true)
  document.removeEventListener('click', preventDraggedModalBackdropClick, true)
  stopProgress.value?.()
  stopAppUpdateChannel.value?.()
  stopAppUpdatePhase.value?.()
  stopGameState.value?.()
  clearUpdateSkipTimer()
  uninstallFrontendLogging?.()
  if (toastTimer) window.clearTimeout(toastTimer)
  if (uiZoomSaveTimer) window.clearTimeout(uiZoomSaveTimer)
})
</script>

<template>
  <ConfirmationDialog :request="confirmation" @answer="answerConfirmation" />
  <RepositoryDocumentDialog :target="repositoryDocument" @close="repositoryDocument = null" />
  <div :class="['app-shell', { 'tutorial-layer-active': tutorial.visible.value }]">
    <div v-if="startupUpdateChecking || updateCheckFailed" class="startup-overlay">
      <div v-if="startupUpdateChecking && !updateSkipped" class="startup-card">
        <div class="startup-spinner"></div>
        <h2>{{ startupStage || t('检查安装器更新') }}</h2>
        <p>{{ t('请耐心等待') }}</p>
        <div v-if="appUpdateCheck" class="update-region">
          <span>{{ t('当前更新地区') }}</span><strong>{{ updateRegionLabel(appUpdateCheck.region) }}</strong>
        </div>
        <div v-if="startupUpdatePhase === 'manifest' && visibleStartupUpdateChannels.length" class="update-channel-statuses">
          <div v-for="channel in visibleStartupUpdateChannels" :key="channel.id" class="update-channel-status">
            <span>{{ updateChannelName(channel.id) }}</span><span :class="`update-channel-${channel.status}`">{{ updateChannelStatus(channel.status) }}</span>
          </div>
        </div>
        <div v-if="startupUpdatePhase === 'download' && appUpdate" class="update-release-log">
          <div class="update-release-head"><strong>{{ t('安装器更新') }} v{{ appUpdate.version }}</strong><small>{{ appUpdate.date || t('未知时间') }}</small></div>
          <p>{{ appUpdate.body || t('Release 没有更新日志。') }}</p>
        </div>
        <div v-if="startupUpdatePhase === 'download' && displayProgress.label" class="startup-download-progress">
          <div class="download-meta"><span>{{ displayProgress.label }}<small v-if="displayProgress.status"> · {{ displayProgress.status }}</small></span><span><template v-if="!displayProgress.indeterminate">{{ displayProgress.percent.toFixed(0) }}%</template><small v-if="displayProgress.speed"> · {{ displayProgress.speed }}</small></span></div>
          <div class="progress" :class="{ 'progress-indeterminate': displayProgress.indeterminate }"><i :style="{ width: displayProgress.indeterminate ? '28%' : `${displayProgress.percent}%` }"></i></div>
        </div>
      </div>
      <div v-else class="startup-card startup-failed">
        <div class="startup-exclamation">!</div>
        <h2>{{ startupError || t('检查更新失败，请重新启动MOD管理器。') }}</h2>
        <p v-if="updateSkipped && updateSkipCountdown > 0">{{ updateSkipCountdown }} {{ t('秒后打开管理器仓库') }}</p>
        <p v-if="updatePageError" class="startup-update-error" role="alert">{{ updatePageError }}</p>
        <div v-if="updateSkipped" class="startup-update-actions">
          <button class="secondary-button" @click="continueAfterUpdateSkip">{{ t('继续') }}</button>
          <button class="primary-button" :disabled="updatePageOpening" @click="openManualUpdatePage">{{ t('手动更新') }}</button>
        </div>
        <button v-else class="primary-button" @click="api.exitApp()">{{ t('确定') }}</button>
      </div>
    </div>
    <div v-else-if="startupLoading || startupLoadingError" class="startup-overlay startup-loading-overlay">
      <div class="startup-card" :class="{ 'startup-failed': !!startupLoadingError }">
        <div v-if="!startupLoadingError" class="startup-spinner"></div>
        <div v-else class="startup-exclamation">!</div>
        <h2>{{ startupLoadingError || t('读取厨房状态') }}</h2>
        <p v-if="!startupLoadingError">{{ t('请耐心等待') }}</p>
        <button v-else class="primary-button" @click="api.exitApp()">{{ t('确定') }}</button>
      </div>
    </div>
    <TutorialExperience :controller="tutorial" :nav-items="navItems" />
    <AppSidebar :nav-items="navItems" :active-view="activeView" :game="game" :app-version="appVersion"
      :app-icon="appIcon" @change-view="activeView = $event" />
    <main class="main-area">
      <div class="main-fixed">
        <header class="topbar">
        <div>
          <h1>{{t(navItems.find((item) => item.id === activeView)?.label || '')}}</h1>
        </div>
        <div class="top-actions"><select v-model="locale" @change="setLocale(locale)" class="language-select"
            :aria-label="t('界面语言')">
            <option v-for="option in localeOptions" :key="option.value" :value="option.value">{{ option.label }}
            </option>
          </select>
          <div class="connection-chip"><span class="pulse success-bg"></span><span>{{ t('本地服务正常') }}</span></div>
        </div>
        </header>
        <div v-if="game?.kitchen_running && ['framework', 'installed', 'repositories'].includes(activeView)"
          class="kitchen-warning">{{ t('厨房进程正在运行中，文件操作已禁止') }}</div>
        <div v-if="message || error" class="toast" :class="{ error: !!error }">
          <CheckCircle2 v-if="!error" :size="17" />
          <AlertTriangle v-else :size="17" /><span>{{ error || message }}</span><button @click="message = ''; error = ''">
            <X :size="15" />
          </button>
        </div>
        <Transition name="download-notch">
          <div v-if="displayProgress.label" class="download-strip">
            <div class="download-meta"><span>{{ displayProgress.label }}<small v-if="displayProgress.status"> · {{ displayProgress.status
                }}</small></span><span><template v-if="!displayProgress.indeterminate">{{ displayProgress.percent.toFixed(0) }}%</template><template v-if="displayProgress.speed"> ·
                {{ displayProgress.speed }}</template></span></div>
            <div class="download-progress-row">
              <div class="progress" :class="{ 'progress-indeterminate': displayProgress.indeterminate }"><i :style="{ width: displayProgress.indeterminate ? '28%' : `${displayProgress.percent}%` }"></i></div><button
                class="secondary-button download-cancel" @click="cancelDownload">{{ t('取消下载') }}</button>
            </div>
          </div>
        </Transition>
      </div>
      <div class="main-scroll">
      <OverviewView v-if="activeView === 'overview'"
        :game="game"
        :update-available-mod-count="updateAvailableModCount"
        :installed-mods="installedMods"
        :installable-repository-count="installableRepositoryCount"
        :repository-count="displayedRepositoryCount"
        :repository-refresh-active="repositoryRefreshActive"
        :current-winhttp="currentWinhttp"
        :native-loader="config?.platform !== 'windows' && config?.game.runtime === 'native'"
        :game-blocked="gameBlocked"
        :busy="busy"
        :app-icon="appIcon"
        :status-label="statusLabel"
        :launch-normal="launchNormal"
        :toggle-winhttp="toggleWinhttp"
        @change-view="activeView = $event" />
      <FrameworkView v-else-if="activeView === 'framework'"
        :game="game"
        :config="config"
        :tutorial-active="tutorial.visible.value || tutorial.previewing.value"
        :game-blocked="gameBlocked"
        :busy="busy"
        :open-directory="openDirectory"
        :choose-game-directory="chooseGameDirectory"
        :restore-default-game-directory="restoreDefaultGameDirectory"
        :detect-game="detectGame"
        :change-game-runtime="changeGameRuntime"
        :open-bep-in-ex-setup="openBepInExSetup"
        :open-bep-in-ex-install="openBepInExInstall"
        :uninstall-bep-in-ex="uninstallBepInEx"
        :open-bep-in-ex-config="openBepInExConfig"
        :open-backup-directory="openBackupDirectory"
        :backup-bep-in-ex="backupBepInEx"
        :restore-backup="restoreBackup"
        :delete-backup="deleteBackup" />
      <InstalledModsView v-else-if="activeView === 'installed'"
        :focus-mod-files="modFocusFiles"
        @focus-finished="modFocusFiles = []"
        :config="config"
        :busy="busy"
        :save-hide-unidentified-mods="saveHideUnidentifiedMods"
        :open-mod-directory="openModDirectory"
        :open-mod-file-location="openModFileLocation"
        :open-mod-config="openModConfig"
        :open-mod-recycle-bin="openModRecycleBin"
        :game-blocked="gameBlocked"
        :install-local-mod="installLocalMod"
        :rescan-installed-mods="rescanInstalledMods"
        :mapped-repos-available="mappedReposAvailable"
        :check-mod-updates="checkModUpdates"
        :installed-mods="installedMods"
        :is-first-mod-in-package="isFirstModInPackage"
        :package-is-collapsed="packageIsCollapsed"
        :mod-display-index="modDisplayIndex"
        :package-enabled-for-mod="packageEnabledForMod"
        :package-mixed-for-mod="packageMixedForMod"
        :package-mod-count-for-mod="packageModCountForMod"
        :open-mod-repository="openModRepository"
        :package-name-for-mod="packageNameForMod"
        :toggle-mod-package="toggleModPackage"
        :toggle-mod-package-details="toggleModPackageDetails"
        :package-id-for-mod="packageIdForMod"
        :mod-details-expanded="modDetailsExpanded"
        :is-last-mod-in-package="isLastModInPackage"
        :package-child-index="packageChildIndex"
        :mod-enabled-for-view="modEnabledForView"
        :is-mod-toggling="isModToggling"
        :is-package-toggling="isPackageToggling"
        :toggle-mod="toggleMod"
        :toggle-mod-details="toggleModDetails"
        :installed-mod-status="installedModStatus"
        :installed-mod-status-label="installedModStatusLabel"
        :installed-mod-status-class="installedModStatusClass"
        :edit-mod-note="editModNote"
        :uninstall-mod="uninstallMod"
        :uninstall-mod-package="uninstallModPackage"
        :display-mod-path="displayModPath"
        :mod-uninstall-pending="modUninstallPending"
        :cancel-mod-uninstall="cancelModUninstall"
        :format-mod-package-delete-warning="formatModPackageDeleteWarning"
        :confirm-mod-uninstall="confirmModUninstall" />
      <RepositoriesView v-else-if="activeView === 'repositories'"
        :communities="communities"
        :focus-repository-ids="repositoryFocusIds"
        @focus-finished="repositoryFocusIds = []"
        :config="config"
        :busy="busy"
        :game-blocked="gameBlocked"
        :repository-refresh-batch-active="repositoryRefreshBatchActive"
        :repository-refreshing="repositoryRefreshing"
        :source-busy="sourceBusy"
        :source-refreshing="sourceRefreshing"
        :source-card-loading="sourceCardLoading"
        :source-refreshing-manual="sourceRefreshingManual"
        :source-refresh-phase="sourceRefreshPhase"
        :source-errors="sourceErrors"
        :source-collapsed="sourceCollapsed"
        :toggle-source="toggleSource"
        :flatten-source-groups="flattenSourceGroups"
        :repository-group-collapsed="repositoryGroupCollapsed"
        :repository-group-key="repositoryGroupKey"
        :toggle-repository-group="toggleRepositoryGroup"
        :localized-repository-name="localizedRepositoryName"
        :localized-repository-note="localizedRepositoryNote"
        :repository-install-status="repositoryInstallStatus"
        :open-repository-mods="openRepositoryMods"
        :latest="latest"
        :format-date="formatDate"
        :install-repo="installRepo"
        :refresh-repo="refreshRepo"
        :refresh-all-repositories="refreshAllRepositories"
        :refresh-source="refreshSource"
        :api="api"
        :edit-repo="editRepo"
        :delete-repo="deleteRepo"
        :repos="repos"
        :open-repository-source-manager="openRepositorySourceManager"
        :export-repositories="exportRepositories"
        :source-groups="sourceGroups"
        :repository-source-region-label="repositorySourceRegionLabel"
        @show-import="showImport = true"
        @show-github-proxies="showGithubProxyManager = true"
        @show-repository-note="repositoryDocument = { repository: $event, kind: 'note' }"
        @show-repository-readme="repositoryDocument = { repository: $event, kind: 'readme' }"
        @show-release="showRelease = $event"
        @source-content-entered="onSourceContentEntered" />
      <GitHubProxyManager
        :open="showGithubProxyManager"
        :busy="!!busy"
        :config="config?.github_proxies || { release_proxies: [], raw_proxies: [], github_api_proxies: [], gitee_api_proxies: [] }"
        :t="t"
        :save-config="saveGithubProxyConfig"
        :refresh-config="refreshGithubProxyConfig"
        @close="showGithubProxyManager = false" />
      <div v-if="showRepositorySource" class="modal-backdrop" @click.self="closeRepositorySourceManager">
        <div class="modal source-manager-modal">
          <div class="modal-head">
            <div>
              <h3>{{ t('管理仓库源') }}</h3>
              <p class="muted-text">{{ t('先编辑仓库源，点击保存后统一请求数据') }}</p>
            </div><button class="icon-button" :disabled="!!busy" @click="closeRepositorySourceManager">
              <X :size="17" />
            </button>
          </div>
          <div class="source-manager-list">
            <div v-for="entry in repositorySourceManagerEntries" :key="entry.source.id"
              :class="['source-manager-row', { 'source-manager-child-row': entry.depth > 0 }]"
              :draggable="entry.depth === 0 && !entry.source.parent_id && !entry.source.builtin"
              @dragstart="startSourceDrag(entry.source.id)" @dragover.prevent
              @drop="dropSource(entry.source.id)">
              <GripVertical v-if="entry.depth === 0" :size="17" class="source-drag-handle" />
              <ChevronRight v-else :size="15" class="source-child-marker" />
              <div><b>{{ localizedRepositorySourceName(entry.source) }}</b><small>{{ t('仓库唯一ID') }}: {{ entry.source.id }}</small><small v-if="entry.depth === 0">{{ t('当前MOD下载渠道') }}: {{ repositorySourceRegionLabel(entry.source.region) }}</small><small>{{ entry.source.url }}</small></div>
              <span v-if="entry.source.builtin" class="badge muted">{{ t('内置') }}</span>
              <button v-if="entry.depth === 0 && !entry.source.builtin" class="icon-button small" :title="t('编辑仓库源')"
                @click="editRepositorySource(entry.source)">
                <Pencil :size="14" />
              </button>
              <button v-if="entry.depth === 0 && !entry.source.builtin" class="icon-button small danger-icon" :title="t('删除仓库源')"
                @click="removeRepositorySource(entry.source)">
                <Trash2 :size="14" />
              </button>
            </div>
          </div>
          <div class="source-manager-toolbar"><button class="secondary-button"
              :disabled="!!busy || repositorySourceEditorOpen" @click="startAddingRepositorySource">
              <Plus :size="15" />{{ t('添加仓库源') }}
            </button></div>
          <div v-if="repositorySourceEditorOpen" class="source-editor-fields"><label><span>{{ t('仓库唯一ID') }}</span><input
                v-model="repositorySourceUniqueId" :disabled="!!editingRepositorySourceId" :placeholder="t('例如：my-source-id')" /></label><label><span>{{ t('仓库源昵称') }}</span><input
                v-model="repositorySourceNickname" :placeholder="t('例如：我的仓库源')" /></label><label><span>{{ t('仓库源地址')
                }}</span><input v-model="repositorySourceUrl"
                placeholder="https://example.com/repositories.oc2repo.toml" /></label><label><span>{{ t('适用地区') }}</span><select v-model="repositorySourceRegion"><option value="chinese_mainland">{{ t('中国大陆地区') }}</option><option value="international">{{ t('国际地区') }}</option><option value="">{{ t('全部地区') }}</option></select></label>
            <div class="source-editor-actions"><button class="ghost-button" @click="resetRepositorySourceEditor">{{
              t('取消')
                }}</button><button class="secondary-button"
                :disabled="!repositorySourceUniqueId.trim() || !repositorySourceNickname.trim() || !repositorySourceUrl.trim()"
                @click="applyRepositorySourceDraft">
                <Save :size="15" />{{ t('应用修改') }}
              </button></div>
          </div>
          <div class="modal-actions"><button class="secondary-button" :disabled="!!busy"
              @click="closeRepositorySourceManager">{{ t('关闭') }}</button><button class="primary-button"
              :disabled="!!busy || !repositorySourceDraft.length" @click="saveRepositorySourceChanges">
              <Save :size="16" />{{ t('保存仓库源') }}
            </button></div>
        </div>
      </div>
      <div v-if="showExport" class="modal-backdrop" @click.self="showExport = false">
        <div class="modal export-modal">
          <div class="modal-head">
            <div>
              <h3>{{ t('导出仓库配置') }}</h3>
              <p class="muted-text">{{ t('选择导出方式') }}</p>
            </div><button class="icon-button" @click="showExport = false">
              <X :size="17" />
            </button>
          </div>
          <div class="modal-actions"><button class="secondary-button" @click="copyRepositories">
              <Copy :size="15" />{{ t('复制文件内容') }}
            </button><button class="primary-button" @click="continueExportRepositories">
              <Upload :size="15" />{{ t('继续导出') }}
            </button></div>
        </div>
      </div>
      <SponsorView v-if="activeView === 'sponsor'"
        :busy="busy"
        :sponsor-image="sponsorImage"
        :open-developer-console="openDeveloperConsole"
        :open-tutorial-replay-menu="tutorial.openReplayMenu"
        :reset-ui="resetUi"
        :open-rust-log="openRustLog"
        :open-frontend-log="openFrontendLog"
        :open-crash-log="openCrashLog" />
      </div>
    </main>
    <button v-if="activeView === 'overview'" class="update-fab" :disabled="!!busy"
      @click="appUpdate ? installAppUpdate() : checkAppUpdate()">
      <Download :size="15" /><span>{{ appUpdate ? `${t('安装')} ${appUpdate.version}` : t('检查程序更新') }}</span>
    </button>
    <div v-if="showRepoEditor" class="modal-backdrop" @click.self="showRepoEditor = false">
      <div class="modal repo-editor-modal">
        <div class="modal-head repo-editor-head">
          <div class="repo-editor-title">
            <h3>{{ t(repoForm.id ? '编辑仓库' : '添加仓库') }}</h3>
            <label class="check-control mod-switch-control repo-editor-enabled">
              <input class="mod-switch-input" v-model="repoForm.enabled" type="checkbox" />
              <span class="mod-switch" :class="{ 'mod-switch-on': repoForm.enabled }" aria-hidden="true"><span class="mod-switch-thumb"></span></span>
              <span>{{ t('启用此仓库') }}</span>
            </label>
          </div>
          <button class="icon-button" @click="showRepoEditor = false">
            <X :size="17" />
          </button>
        </div>
        <div class="repo-form-sections">
          <section class="repo-form-section">
            <div class="repo-form-section-title"><b>{{ t('仓库基本信息') }}</b><small>{{ t('配置仓库名称、来源平台、分组和地址') }}</small></div>
            <div class="form-grid">
              <label><span>{{ t('仓库名称') }}</span><input v-model="repoForm.name" :placeholder="t('例如：我的 MOD 正式版')" /></label>
              <label><span>{{ t('来源平台') }}</span><select v-model="repoForm.provider" :aria-label="t('来源平台')" @change="providerChanged">
                <option value="gitee">Gitee</option>
                <option value="github">GitHub</option>
              </select></label>
              <label><span>{{ t('仓库分组') }}</span><input v-model="repoForm.group" :placeholder="t('例如：前置必装')" /></label>
              <label><span>{{ t('适用地区') }}</span><select v-model="repoForm.region" :aria-label="t('适用地区')">
                <option value="chinese_mainland">{{ t('中国大陆地区') }}</option>
                <option value="international">{{ t('国际地区') }}</option>
                <option value="">{{ t('全部地区') }}</option>
              </select></label>
              <div class="wide repository-homepage-field">
                <div class="repository-homepage-fields">
                  <label class="repository-owner-field">
                    <span class="repository-field-label">{{ t('用户名') }}</span>
                    <input v-model="repoForm.username" :placeholder="t('用户名')" :aria-label="t('用户名')" autocomplete="off" />
                  </label>
                  <span aria-hidden="true">/</span>
                  <label class="repository-name-field">
                    <span class="repository-field-label">{{ t('仓库名') }}</span>
                    <input v-model="repoForm.repository" :placeholder="t('仓库名')" :aria-label="t('仓库名')" autocomplete="off" />
                  </label>
                </div>
              </div>
            </div>
          </section>

          <section class="repo-form-section">
            <div class="repo-form-section-title"><b>{{ t('下载方式') }}</b><small>{{ t('选择通过 Release 或仓库文件下载') }}</small></div>
            <div class="form-grid">
              <label class="wide"><span>{{ t('下载方式') }}</span><select v-model="repoForm.download_mode" :aria-label="t('下载方式')">
                <option value="release">{{ t('Release 文件') }}</option>
                <option value="repository_file">{{ t('仓库文件') }}</option>
              </select></label>
            </div>
          </section>

          <section v-if="repoForm.download_mode === 'release'" class="repo-form-section">
            <div class="repo-form-section-title"><b>{{ t('Release 参数') }}</b><small>{{ t('配置版本筛选、请求和资产获取方式') }}</small></div>
            <div class="form-grid">
              <label><span>{{ t('版本频道') }}</span><select v-model="repoForm.channel" :aria-label="t('版本频道')">
                <option value="all">{{ t('全部版本') }}</option>
                <option value="stable">{{ t('稳定版') }}</option>
                <option value="beta">{{ t('测试版') }}</option>
              </select></label>
              <label><span>{{ t('每个仓库最多获取 Release 数量') }}</span><input v-model.number="repoForm.max_release_count" type="number" min="1" max="100" /></label>
              <label><span>{{ t('下载线程数') }}</span><input v-model.number="repoForm.download_threads" type="number" min="1" max="32" :disabled="repoForm.provider === 'gitee'" /></label>
              <label class="wide"><span>{{ t('API 直连 Token') }} ({{ repoForm.provider === 'gitee' ? 'Gitee' : 'GitHub' }})</span>
                <input v-if="repoForm.provider === 'gitee'" v-model="repoForm.gitee_token" type="password" :placeholder="t('留空保持原设置')" />
                <input v-else v-model="repoForm.github_token" type="password" :placeholder="t('留空保持原设置')" />
              </label>
            </div>
          </section>

          <section v-else class="repo-form-section">
            <div class="repo-form-section-title"><b>{{ t('仓库文件参数') }}</b><small>{{ t('从指定分支下载单个文件，不读取 Release 版本') }}</small></div>
            <div class="form-grid">
              <label><span>{{ t('分支') }}</span><input v-model="repoForm.branch" placeholder="main" /></label>
              <label><span>{{ t('下载线程数') }}</span><input v-model.number="repoForm.download_threads" type="number" min="1" max="32" :disabled="repoForm.provider === 'gitee'" /></label>
            </div>
          </section>

          <section v-if="repoForm.download_mode === 'release'" class="repo-form-section">
            <div class="repo-asset-rules">
              <div class="repo-form-section-title repo-rule-title">
                <div><b>{{ t('版本资产规则') }}</b><small>{{ t('支持精确文件名、通配符和正则表达式') }}</small></div>
                <button class="ghost-button" @click="addRule"><Plus :size="14" />{{ t('添加规则') }}</button>
              </div>
              <div v-for="(rule, index) in repoForm.asset_rules" :key="index" class="rule-row">
                <select v-model="rule.type" :aria-label="t('规则类型')"><option value="exact">{{ t('精确') }}</option><option value="glob">{{ t('通配符') }}</option><option value="regex">{{ t('正则') }}</option></select>
                <input v-model="rule.value" :aria-label="t('资产规则')" />
                <button class="icon-button small danger-icon" :title="t('删除规则')" @click="removeRule(index)"><Trash2 :size="14" /></button>
              </div>
            </div>
          </section>

          <section v-else class="repo-form-section">
            <div class="repo-form-section-title"><b>{{ t('仓库资产规则') }}</b><small>{{ t('仅支持具体文件，不支持通配和正则') }}</small></div>
            <div class="rule-row repository-file-rule-row">
              <span class="repository-file-rule-type">{{ t('精确') }}</span>
              <input v-model="repoForm.file_path" :aria-label="t('文件相对路径')" :placeholder="t('例如：bin/Release/Example.dll')" />
            </div>
          </section>

          <section class="repo-form-section repo-binding-section">
            <div class="repo-form-section-title">
              <b>{{ t('本地 MOD 识别绑定') }}</b><small>{{ t('GUID、产品名称或原始文件名任一匹配即可绑定') }}</small><small>{{ t('填写版本号正则后，版本也必须匹配；留空则忽略版本条件') }}</small>
            </div>
            <div class="form-grid identity-grid">
              <label><span>{{ t('BepInEx.GUID') }}</span><input v-model="repoForm.plugin_guid" :placeholder="t('可选')" /></label>
              <label><span>{{ t('DLL.产品名称') }}</span><input v-model="repoForm.local_product_name" :placeholder="t('可选')" /></label>
              <label><span>{{ t('DLL.原始文件名') }}</span><input v-model="repoForm.local_original_filename" :placeholder="t('可选')" /></label>
              <label><span>{{ t('版本号正则') }}</span><input v-model="repoForm.version_regex" :placeholder="t('例如：^1\\.2\\.\\d+$')" /></label>
            </div>
          </section>

          <section class="repo-form-section repo-install-section">
            <div class="repo-form-section-title"><b>{{ t('安装目录') }}</b></div>
            <div class="form-grid">
              <label class="wide"><input v-model="repoForm.install_target" :aria-label="t('安装目录')" :placeholder="t('留空按 ZIP 结构安装')" /></label>
            </div>
          </section>

          <section class="repo-form-section repo-note-section">
            <div class="repo-form-section-title">
              <b>{{ t('仓库备注') }}</b><small>{{ t('可选，多行备注') }}</small>
            </div>
            <textarea v-model="repoForm.note" class="repository-note-input" rows="3" :placeholder="t('可选，多行备注')"></textarea>
          </section>
        </div>
        <div class="modal-actions"><button class="secondary-button" @click="showRepoEditor = false">{{ t('取消')
            }}</button><button class="primary-button"
            :disabled="!repoForm.name || !repoForm.username.trim() || !repoForm.repository.trim() || (repoForm.download_mode === 'release' && (!repoForm.asset_rules.length || repoForm.asset_rules.some((rule) => !rule.value.trim()))) || (repoForm.download_mode === 'repository_file' && (!repoForm.branch.trim() || !repoForm.file_path.trim())) || !!busy"
            @click="saveRepo">
            <Save :size="16" />{{ t('保存仓库') }}
          </button></div>
      </div>
    </div>
    <div v-if="showRustLog" class="modal-backdrop" @click.self="closeRustLog">
      <div class="modal rust-log-modal">
        <div class="modal-head">
          <div>
            <h3>{{ t(logViewerKind === 'rust' ? 'Rust 运行日志' : logViewerKind === 'frontend' ? '前端运行日志' : '崩溃日志') }}</h3><small class="rust-log-path">{{ rustLog.path }}</small>
          </div><button class="icon-button" @click="closeRustLog">
            <X :size="17" />
          </button>
        </div>
        <pre ref="rustLogOutput" class="rust-log-output">{{ rustLog.content || t('暂无 Rust 日志') }}</pre>
        <div class="modal-actions"><button class="secondary-button" @click="refreshRustLog">
            <RefreshCw :size="15" />{{ t('立即刷新') }}
          </button><button class="primary-button" @click="closeRustLog">{{ t('关闭') }}</button></div>
      </div>
    </div>
    <div v-if="showImport" class="modal-backdrop" @click.self="showImport = false">
      <div class="modal import-modal">
        <div class="modal-head">
          <div>
            <h3>{{ t('导入仓库配置') }}</h3>
          </div><button class="icon-button" @click="showImport = false">
            <X :size="17" />
          </button>
        </div>
        <div class="import-actions"><button class="secondary-button" @click="importFromFile">
            <Download :size="15" />{{ t('选择 .oc2repo.toml') }}
          </button><button class="secondary-button" @click="importFromClipboard">
            <Copy :size="15" />{{ t('读取剪贴板') }}
          </button></div><textarea v-model="importText" :placeholder="t('粘贴仓库 TOML 内容')"></textarea><button
          class="ghost-button full-button" :disabled="!importText" @click="previewImport">
          <Search :size="15" />{{ t('检查导入内容') }}
        </button>
        <div v-if="importPreview" class="import-preview">
          <div class="preview-state" :class="importPreview.valid ? 'success' : 'danger'">
            <CheckCircle2 :size="16" />{{ t(importPreview.valid ? '格式有效' : '格式无效') }}
          </div>
          <div class="preview-grid"><span>{{ t('新增') }} {{ importPreview.additions.length }}</span><span>{{ t('重复') }}
              {{
                importPreview.duplicates.length }}</span><span>{{ t('冲突') }} {{ importPreview.conflicts.length
              }}</span></div><small v-if="importPreview.errors.length">{{ importPreview.errors.map(t).join('；')
              }}</small>
          <div v-if="importPreview.valid" class="modal-actions"><button class="secondary-button"
              @click="applyImport('keep_both')">{{ t('合并保存') }}</button><button class="primary-button"
              @click="applyImport('overwrite')">
              <Check :size="15" />{{ t('覆盖保存') }}
            </button></div>
        </div>
      </div>
    </div>
    <div v-if="showRelease" class="modal-backdrop" @click.self="showRelease = null">
      <div class="modal release-modal">
        <div class="modal-head">
          <div>
            <h3>{{ showRelease.name }}</h3>
          </div><button class="icon-button" @click="showRelease = null">
            <X :size="17" />
          </button>
        </div>
        <div v-if="latest(showRelease)" class="release-detail">
          <div class="release-detail-meta"><span class="release-version">{{ latest(showRelease)?.version ||
            latest(showRelease)?.tag_name }}</span><span>{{
                formatDate(latest(showRelease)?.published_at) }}</span><span class="badge"
              :class="latest(showRelease)?.prerelease ? 'warning' : 'success'">{{
                t(latest(showRelease)?.prerelease ? 'BETA' : 'STABLE') }}</span></div>
          <h4>{{ latest(showRelease)?.name || latest(showRelease)?.tag_name }}</h4>
          <p class="release-body">{{ latest(showRelease)?.body || t('Release 没有更新日志。') }}</p>
          <div class="asset-list">
            <div v-for="asset in latest(showRelease)?.assets" :key="asset.name"><span>{{ asset.name
                }}</span><small>{{ formatAssetMetadata(asset) }}</small></div>
          </div>
        </div>
      </div>
    </div>
    <div v-if="editingMod" class="modal-backdrop" @click.self="editingMod = null">
      <div class="modal">
        <div class="modal-head">
          <div>
            <h3>{{ editingMod.name }}</h3>
          </div><button class="icon-button" @click="editingMod = null">
            <X :size="17" />
          </button>
        </div>
        <p class="muted-text">{{ t('产品名称：') }}{{ editingMod.product_name || t('未知') }} · {{ displayModPath(editingMod) }}</p>
        <textarea v-model="modNote" rows="6" :placeholder="t('例如：这是街机模组，包含房主工具、房间控制和快捷设置功能。')" />
        <div class="modal-actions"><button class="secondary-button" @click="editingMod = null">{{ t('取消')
            }}</button><button class="primary-button" :disabled="!!busy" @click="saveModNote">
            <Save :size="16" />{{ t('保存备注') }}
          </button></div>
      </div>
    </div>
  </div>
  <ModRecycleBin :open="showModRecycleBin" :game-blocked="gameBlocked" :busy="!!busy"
    @close="showModRecycleBin = false" @restored="handleRecycledModRestored" />
  <ModConfigEditor v-if="configEditorTarget" :target="configEditorTarget" :game-blocked="gameBlocked" :busy="!!busy"
    :confirm-discard="() => requestConfirmation({ title: t('未保存的配置'), message: t('配置尚未保存，确定放弃修改？'), danger: true })"
    @close="configEditorTarget = null" />
  <div v-if="showInstall" class="modal-backdrop" @click.self="closeInstallDialog">
    <div class="modal install-modal">
      <div class="modal-head">
        <div>
          <h3>{{ showInstall.download_mode === 'release' ? t('选择版本和文件') : t('选择下载方式') }}</h3>
        </div><button class="icon-button" @click="closeInstallDialog">
          <X :size="17" />
        </button>
      </div><template v-if="showInstall.download_mode === 'release'"><label class="install-field"><span>{{ t('Release 版本') }}</span><select v-model="selectedInstallTag" :aria-label="t('Release 版本')" @change="selectInstallRelease">
          <option v-for="release in installReleases(showInstall)" :key="release.tag_name" :value="release.tag_name">{{
            release.version || release.tag_name }} · {{ release.name }}
          </option>
        </select></label>
      <div v-if="selectedInstallRelease(showInstall)" class="install-release-log">
        <div class="install-release-log-head"><strong>{{ selectedInstallRelease(showInstall)?.name || selectedInstallRelease(showInstall)?.tag_name }}</strong><span>{{ selectedInstallRelease(showInstall)?.version || selectedInstallRelease(showInstall)?.tag_name }}</span></div>
        <MarkdownContent class="install-release-log-body"
          :source="selectedInstallRelease(showInstall)?.body || t('Release 没有更新日志。')" :base-url="repositoryMarkdownBaseUrl(showInstall)" />
      </div>
      <label class="install-field"><span>{{ t('下载文件') }}</span><small class="install-field-hint">{{ t('仅显示符合当前仓库资产规则的文件。') }}</small><select
          v-model="selectedInstallAsset" :aria-label="t('下载文件')">
          <option v-for="asset in selectedInstallAssets(showInstall)" :key="asset.name" :value="asset.name">{{
            asset.name
            }} · {{ formatAssetMetadata(asset) }}</option>
        </select></label></template><div v-else class="install-source-file"><span>{{ t('仓库文件') }}</span><b>{{ showInstall.file_path }}</b></div>
      <section v-if="assetProxySelectionVisible(showInstall)" class="install-proxy-choice">
        <div class="install-field-label">{{ t('GitHub 资产下载方式') }}</div>
        <div class="install-proxy-modes">
          <button type="button" :class="['proxy-mode-button', { active: selectedAssetProxyMode === 'auto_proxy' }]" :aria-pressed="selectedAssetProxyMode === 'auto_proxy'" @click="selectedAssetProxyMode = 'auto_proxy'">{{ t('自动代理') }}</button>
          <button type="button" :class="['proxy-mode-button', { active: selectedAssetProxyMode === 'auto_direct' }]" :aria-pressed="selectedAssetProxyMode === 'auto_direct'" @click="selectedAssetProxyMode = 'auto_direct'">{{ t('自动直连') }}</button>
          <button type="button" :class="['proxy-mode-button', { active: selectedAssetProxyMode === 'direct' }]" @click="selectedAssetProxyMode = 'direct'">{{ t('直链') }}</button>
          <button type="button" :class="['proxy-mode-button', { active: selectedAssetProxyMode === 'fixed' }]" @click="selectedAssetProxyMode = 'fixed'">{{ t('手动选择') }}</button>
        </div>
        <select v-if="selectedAssetProxyMode === 'fixed'" v-model="selectedAssetProxyId" class="install-proxy-select" :aria-label="t('手动选择')">
          <option value="">{{ t('请选择代理') }}</option>
          <option v-for="proxy in assetProxyEntries(showInstall)" :key="proxy.id" :value="proxy.id">{{ proxy.name }}<template v-if="proxy.regions.length"> · {{ proxy.regions.join('、') }}</template></option>
        </select>
      </section>
      <div class="modal-actions"><button class="secondary-button" @click="closeInstallDialog">{{ t('取消')
          }}</button><button class="primary-button" :disabled="(showInstall.download_mode === 'release' && !selectedInstallAsset) || (assetProxySelectionVisible(showInstall) && selectedAssetProxyMode === 'fixed' && !selectedAssetProxyId) || !!busy" @click="confirmInstall">
          <Download :size="15" />{{ t('下载并安装') }}
        </button></div>
    </div>
  </div>
  <div v-if="manualInstallMode" class="modal-backdrop" @click.self="manualInstallMode = null" @keydown.esc.stop="manualInstallMode = null">
    <form class="modal manual-install-modal" @submit.prevent="installOnlineMod">
      <div class="modal-head">
        <h3>{{ t('手动安装模组') }}</h3>
        <button type="button" class="icon-button" :aria-label="t('关闭')" @click="manualInstallMode = null"><X :size="17" /></button>
      </div>
      <div v-if="manualInstallMode === 'choose'" class="modal-actions">
        <button type="button" class="secondary-button" @click="chooseLocalMod">{{ t('本地文件') }}</button>
        <button type="button" class="primary-button" @click="manualInstallMode = 'online'">{{ t('在线文件') }}</button>
      </div>
      <template v-else>
        <label class="manual-install-field"><span>{{ t('下载直链') }}</span><input v-model="manualInstallUrl" type="url" required :placeholder="t('请输入 DLL 或 ZIP 文件的下载直链')" :aria-label="t('下载直链')" /></label>
        <p class="muted-text">{{ t('仅支持 DLL 或 ZIP 文件，使用 32 线程直连下载。') }}</p>
        <div class="modal-actions">
          <button type="button" class="secondary-button" @click="manualInstallMode = 'choose'">{{ t('返回') }}</button>
          <button type="submit" class="primary-button" :disabled="!!busy || gameBlocked || !manualInstallUrl.trim()">{{ t('下载并安装') }}</button>
        </div>
      </template>
    </form>
  </div>
  <BepInExSetupGuide :visible="showBepInExSetup && !tutorial.visible.value && !tutorial.previewing.value" :config="dashboard?.config" :game="realGame"
    :result="bepinexSetupResult" :busy="!!busy" :repair-native-overlay="repairNativeBepInExOverlay" @close="showBepInExSetup = false" />

  <div v-if="showBepInExInstall" class="modal-backdrop" @click.self="showBepInExInstall = false">
    <div class="modal bepinex-channel-modal">
      <div class="modal-head">
        <div>
          <h3>{{ t('选择 BepInEx 安装渠道') }}</h3>
          <p class="muted-text">{{ t('国内用户请使用 Gitee 渠道，海外用户请使用 GitHub 渠道。') }}</p>
        </div><button class="icon-button" @click="showBepInExInstall = false">
          <X :size="17" />
        </button>
      </div>
      <div class="bepinex-channel-grid">
        <button class="bepinex-channel-card" :disabled="!!busy" @click="installBepInEx('gitee')">
          <span class="bepinex-channel-icon gitee"><svg class="provider-icon" aria-hidden="true" viewBox="0 0 24 24">
              <use href="/icons.svg#gitee-icon"></use>
            </svg></span>
          <span class="bepinex-channel-copy"><b>{{ t('Gitee 渠道') }}</b><small>{{ bepinexDownloadUrls.gitee
              }}</small><em>{{ t('直连 Release 文件') }}</em></span>
        </button>
        <button class="bepinex-channel-card" :disabled="!!busy" @click="installBepInEx('github')">
          <span class="bepinex-channel-icon github"><svg class="provider-icon" aria-hidden="true" viewBox="0 0 19 19">
              <use href="/icons.svg#github-icon"></use>
            </svg></span>
          <span class="bepinex-channel-copy"><b>{{ t('GitHub 渠道') }}</b><small>{{ bepinexDownloadUrls.github
              }}</small><em>{{ t('直连 Release 文件') }}</em></span>
        </button>
      </div>
    </div>
  </div>
</template>
