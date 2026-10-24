<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  AlertTriangle, Boxes, Check, CheckCircle2, CloudDownload, Coffee, Copy, Download,
  LayoutDashboard, PackageCheck, Plus,
  Save, Search, Trash2, Upload, X,
} from 'lucide-vue-next'
import { api, watchAppUpdateChannel, watchAppUpdatePhase, watchDownloadProgress, watchGameState } from './services/tauri'
import type { BepInExInstallResult, ConfigEditorTarget, Dashboard, GitHubProxyConfig, Provider, Repository, RepositoryPatch, SettingsPatch, WinhttpState } from './types'
import { bepinexDownloadUrls as getBepInExDownloadUrls } from './gameRuntime'
import { t, useLocaleTranslation, type Locale } from './i18n'
import { normalizeRepositoryRegion, repositoryRegionMatches } from './repositorySources'
import { repositoryGroup, repositoryName, repositoryNote } from './repositoryNames'
import { installedModLabelActionable, installedModLabelClass, installedModLabelText } from './installedModStatus'
import { installReleases, matchingInstallAssets } from './repositoryAssets'
import { formatAssetMetadata, formatDate as formatDateForLocale } from './formatting'
import { communityProviderMatchesRegion, isCommunityRepository } from './communityRepositories'
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
import { useUiZoom } from './composables/useUiZoom'
import { useDownloadProgress } from './composables/useDownloadProgress'
import { useLogViewer } from './composables/useLogViewer'
import { installFrontendLogging } from './composables/useFrontendLogging'
import { useAppUpdate } from './composables/useAppUpdate'
import { useDashboard } from './composables/useDashboard'
import { useRepositoryRefresh } from './composables/useRepositoryRefresh'
import { useSourceRefresh } from './composables/useSourceRefresh'
import { useInstalledModsModel } from './composables/useInstalledModsModel'
import { useRepositorySourceManager } from './composables/useRepositorySourceManager'
import { useModOperations } from './composables/useModOperations'
import { repositorySourceRegionLabel } from './repositorySources'
import DownloadStrip from './components/DownloadStrip.vue'
import LogViewerDialog from './components/LogViewerDialog.vue'
import StartupOverlay from './components/StartupOverlay.vue'
import RepositorySourceManagerDialog from './components/RepositorySourceManagerDialog.vue'
import InstallRepositoryDialog from './components/InstallRepositoryDialog.vue'
import ManualInstallDialog from './components/ManualInstallDialog.vue'
import ModNoteDialog from './components/ModNoteDialog.vue'
import BepInExChannelDialog from './components/BepInExChannelDialog.vue'
import { useTutorialRuntime } from './tutorial/useTutorialRuntime'
import type { View } from './tutorial/steps'

const activeView = ref<View>('overview')
const repositoryFocusIds = ref<string[]>([])
const modFocusFiles = ref<string[]>([])
const { confirmation, requestConfirmation, answerConfirmation } = useConfirmationDialog()
const dashboard = ref<Dashboard | null>(null)
const busy = ref('')
const message = ref('')
const error = ref('')
const {
  progress, clear: clearDownloadProgress, begin: beginDownloadProgress,
  receive: receiveDownloadProgress, cancelDownload,
} = useDownloadProgress({ isUnmounted: () => appUnmounted })
const {
  uiZoom, min: MIN_UI_ZOOM, max: MAX_UI_ZOOM, step: UI_ZOOM_STEP,
  clamp: clampUiZoom, setUiZoom, handleWheel: handleUiZoomWheel, cleanup: cleanupUiZoom,
} = useUiZoom({ dashboard, showError, showSuccess })
const showRepoEditor = ref(false)
const showImport = ref(false)
const showRepositorySource = ref(false)
const showGithubProxyManager = ref(false)
const showModRecycleBin = ref(false)
const configEditorTarget = ref<ConfigEditorTarget | null>(null)
const showExport = ref(false)
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
const importText = ref('')
const importPreview = ref<{ valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] } | null>(null)
const sourceRefreshingManual = reactive<Record<string, boolean>>({})
const startupLoading = ref(false)
const startupLoadingError = ref('')
const startupStage = ref('')
const startupError = ref('')
const sourceLoading = reactive<Record<string, boolean>>({})
const repositoryReleaseReady = reactive<Record<string, boolean>>({})
const modDetailsExpanded = reactive<Record<string, boolean>>({})
const modPackageCollapsed = reactive<Record<string, boolean>>({})
const stopProgress = ref<(() => void) | null>(null)
const stopAppUpdateChannel = ref<(() => void) | null>(null)
const stopAppUpdatePhase = ref<(() => void) | null>(null)
const stopGameState = ref<(() => void) | null>(null)
let appUnmounted = false
let toastTimer: number | undefined
let modalPointerDownTarget: Element | null = null
// Sidebar collapse is layout state, so the shell owns it: the background image
// and the content gutter follow the same width as the rail.
const SIDEBAR_COLLAPSE_KEY = 'oc2-sidebar-collapsed'
const sidebarCollapsed = ref(localStorage.getItem(SIDEBAR_COLLAPSE_KEY) === '1')
watch(sidebarCollapsed, (value) => localStorage.setItem(SIDEBAR_COLLAPSE_KEY, value ? '1' : '0'))
const appVersion = ref('')
const { locale, setLocale } = useLocaleTranslation()
const {
  appUpdate, appUpdateCheck, startupUpdateChecking, updateCheckFailed, updateSkipped,
  startupUpdatePhase, updateSkipCountdown, updatePageOpening, updatePageError,
  initialUpdateCheck, finishStartupUpdate, receiveAppUpdateChannel, receiveAppUpdatePhase,
  checkAppUpdate, installAppUpdate, markUpdateSkipped, continueAfterUpdateSkip,
  openManualUpdatePage, clearUpdateSkipTimer,
} = useAppUpdate({
  run,
  locale,
  startupStage,
  startupError,
  showSuccess,
  isUnmounted: () => appUnmounted,
  beginDownload: beginDownloadProgress,
  clearDownload: clearDownloadProgress,
})
const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'ko', label: '한국어' },
  { value: 'ja', label: '日本語' },
]
const repoForm = reactive<RepositoryPatch>({
  name: '', name_i18n: {}, group: '', group_i18n: {}, provider: 'gitee', username: '', repository: '', download_mode: 'release', branch: 'main', file_path: '', channel: 'all', asset_rules: [],
  region: '', install_target: 'BepInEx/plugins', note: '', note_i18n: {}, plugin_guid: '', local_product_name: '', local_original_filename: '', version_regex: '', max_release_count: 10, download_threads: 1, github_token: '', gitee_token: '', official_api_only: true, enabled: true,
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
  resetInstalledModExpansion: () => resetInstalledModExpansion(),
  openRepositorySourceManager: () => sourceManager.openRepositorySourceManager(),
  editRepository: () => editRepo(),
  installReleases,
  matchingInstallAssets,
  startInitialRepositoryRefresh: () => startInitialRepositoryRefresh(),
})
const tutorial = tutorialRuntime.controller
const displayDashboard = tutorialRuntime.displayDashboard
const game = computed(() => displayDashboard.value?.game)
const config = computed(() => displayDashboard.value?.config)
const realGame = computed(() => dashboard.value?.game)
function createCommunity(provider: Provider): ReturnType<typeof useCommunityRepositories> {
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
        getUpdateQueue(),
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
const displayProgress = tutorialRuntime.displayProgress
const repos = computed(() => (config.value?.repositories ?? []).filter((repo) => repositoryRegionMatches(repo.region, config.value?.region)
  && (!repo.source_id || activeRepositorySourceIds.value.has(repo.source_id))))
const gameBlocked = computed(() => realGame.value?.kitchen_running ?? false)
const currentWinhttp = computed<WinhttpState>(() => game.value?.winhttp_state ?? 'missing')
const repositoryRefreshActive = computed(() => repositoryRefreshBatchActive.value
  || communities.some((discovery) => discovery.refreshing.value)
  || Object.values(repositoryRefreshing).some(Boolean)
  || Object.values(sourceLoading).some(Boolean)
  || Object.values(sourceRefreshingManual).some(Boolean))
const {
  getUpdateQueue, refreshDashboardSerialized, refreshConfigSerialized, loadDashboard,
  detectGame, chooseGameDirectory, restoreDefaultGameDirectory, changeGameRuntime,
  launchNormal, toggleWinhttp, openDirectory, openModDirectory, resetUi,
} = useDashboard({
  dashboard,
  run,
  requestConfirmation,
  applyStartupZoom: (value) => setUiZoom(clampUiZoom(value), false, false),
  isBlocked: () => Boolean(tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value || gameBlocked.value),
  onRuntimeChanged: () => { bepinexSetupResult.value = null; showBepInExSetup.value = false },
  currentWinhttp: () => currentWinhttp.value,
  api,
})
const {
  repositoryRefreshing, repositoryRefreshBatchActive, repositoryRefreshTasks,
  modUpdatesCheckActive, modRepositoryRefreshActive, refreshRepo,
  refreshRepositoryBatch, checkModUpdates,
} = useRepositoryRefresh({
  dashboard,
  locale,
  repositoryReleaseReady,
  run,
  showError,
  showSuccess,
  isUnmounted: () => appUnmounted,
  getRepos: () => repos.value,
  getInstalledMods: () => installedMods.value,
  getModReleaseRepositoryIds: (mod) => modReleaseRepositoryIds(mod),
  getMappedReposAvailable: () => mappedReposAvailable.value,
  api,
})
const {
  sourceCollapsed, repositoryGroupCollapsed, sourceDocumentsLoading, sourceRefreshPhase,
  sourceErrors, initialRepositorySourcesLoaded, activeRepositorySources, activeRepositorySourceIds, sourceGroups,
  flattenSourceGroups, displayedRepositoryCount, installableRepositoryCount,
  sourceMatchesRegion, sourceDescendantIds, refreshRootSources, refreshSource,
  refreshAllRepositories, startInitialRepositoryRefresh, sourceRefreshing, sourceCardLoading,
  sourceBusy, onSourceContentEntered, toggleSource, repositoryGroupKey, toggleRepositoryGroup,
  getInitialProxySync,
} = useSourceRefresh({
  dashboard,
  config,
  repos,
  locale,
  communities,
  sourceLoading,
  sourceRefreshingManual,
  repositoryRefreshing,
  repositoryReleaseReady,
  repositoryRefreshTasks,
  refreshRepositoryBatch,
  refreshDashboardSerialized,
  refreshConfigSerialized,
  getUpdateQueue,
  isUnmounted: () => appUnmounted,
  isRepositoryRefreshBlocked: () => tutorial.blocksRepositoryRefresh.value,
  isTutorialOverlayActive: () => tutorial.visible.value || tutorial.previewing.value,
  getTutorialRepositoryLoadingActive: () => tutorial.repositoryLoadingActive.value,
  showError,
  api,
})
const modsModel = useInstalledModsModel({
  displayDashboard,
  config,
  repos,
  locale,
  repositoryReleaseReady,
  repositoryRefreshing,
  modUpdatesCheckActive,
  modRepositoryRefreshActive,
  modDetailsExpanded,
  modPackageCollapsed,
  sourceGroups,
  sourceCollapsed,
  repositoryGroupCollapsed,
  repositoryGroupKey,
  communities,
  repositoryFocusIds,
  modFocusFiles,
  activeView,
  isTutorialPreviewing: () => tutorial.previewing.value,
  getTutorialScenario: () => tutorial.scenario.value,
})
const {
  installedMods, mappedReposAvailable, updateAvailableModCount,
  packageIdForMod, packageNameForMod, packageModCountForMod,
  isFirstModInPackage, isLastModInPackage, modDisplayIndex,
  packageChildIndex, packageIsCollapsed, toggleModPackageDetails, resetInstalledModExpansion,
  packageEnabledForMod, packageMixedForMod,
  modReleaseRepositoryIds, repositoryInstallStatus, openRepositoryMods,
  installedModLabels, openModRepository,
  isModToggling, isPackageToggling, modEnabledForView, displayModPath, toggleModDetails,
} = modsModel
const sourceManager = useRepositorySourceManager({
  showRepositorySource,
  dashboard,
  config,
  sourceMatchesRegion,
  refreshRootSources,
  getInitialProxySync,
  refreshDashboardSerialized,
  isRepositoryRefreshBlocked: () => tutorial.blocksRepositoryRefresh.value,
  run,
  requestConfirmation,
  showError,
  api,
})
const { openRepositorySourceManager } = sourceManager
const modOps = useModOperations({
  showInstall,
  selectedInstallTag,
  selectedInstallAsset,
  dashboard,
  config,
  game,
  configEditorTarget,
  repositoryReleaseReady,
  model: modsModel,
  busy,
  message,
  progress,
  beginDownload: beginDownloadProgress,
  clearDownload: clearDownloadProgress,
  run,
  showError,
  showSuccess,
  requestConfirmation,
  loadDashboard,
  refreshDashboardSerialized,
  isUnmounted: () => appUnmounted,
  isTutorialPreviewing: () => tutorial.previewing.value,
  isBlocked: () => Boolean(tutorial.visible.value || tutorial.previewing.value || appUnmounted || busy.value || gameBlocked.value),
  api,
})
const {
  installRepo, formatModPackageDeleteWarning, editModNote, uninstallMod, uninstallModPackage,
  cancelModUninstall, confirmModUninstall, toggleMod, toggleModPackage,
  installLocalMod, rescanInstalledMods, saveHideUnidentifiedMods,
  openModFileLocation, openModConfig,
} = modOps

function statusLabel(status: WinhttpState) { return t(({ modenabled: '框架已启用', moddisabled: '框架已禁用', missing: config.value?.platform !== 'windows' && config.value?.game.runtime === 'native' ? '未找到框架加载器' : '未找到 winhttp.dll', conflict: '文件状态冲突' } as Record<WinhttpState, string>)[status]) }
function formatDate(value?: string) { return formatDateForLocale(value, locale.value) }
function scheduleToastClose() { if (toastTimer) window.clearTimeout(toastTimer); toastTimer = window.setTimeout(() => { message.value = ''; error.value = '' }, 5000) }
function latest(repo: Repository) { return repo.cached_releases?.[0] }
function localizedRepositoryName(repo: Repository) { return repositoryName(repo, locale.value) }
function localizedRepositoryNote(repo: Repository) { return repositoryNote(repo, locale.value) }
function updateRepositoryTranslation(field: 'name' | 'group' | 'note', event: Event) {
  const input = event.target as HTMLInputElement | HTMLTextAreaElement
  repoForm[field] = input.value
  const translations = field === 'name' ? repoForm.name_i18n : field === 'group' ? repoForm.group_i18n : repoForm.note_i18n
  if (translations) translations[locale.value] = input.value
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
async function run<T>(label: string, action: () => Promise<T>) { const actionLabel = normalizeActionLabel(label); busy.value = actionLabel; error.value = ''; message.value = ''; try { const result = await action(); const resultLabel = typeof result === 'object' && result && 'message' in result ? String((result as { message: string }).message) : actionLabel; showSuccess(t(normalizeActionLabel(resultLabel))); return result } catch (reason) { showError(reason); throw reason } finally { busy.value = '' } }
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
const {
  showRustLog, rustLog, logViewerKind, refresh: refreshRustLog,
  openRustLog, openFrontendLog, openCrashLog, openDeveloperConsole, close: closeRustLog,
} = useLogViewer({ run })
const uninstallFrontendLogging = typeof window !== 'undefined' ? installFrontendLogging() : undefined
async function restoreBackup(path: string) { await run('正在恢复 BepInEx', () => api.restoreBepInEx(path)); await loadDashboard() }
function openBepInExConfig() {
  if (tutorial.visible.value || tutorial.previewing.value || busy.value || !game.value?.bepinex.has_config) return
  configEditorTarget.value = { kind: 'bepinex' }
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
    id: repo.id, name: localizedRepositoryName(repo), name_i18n: { ...(repo.name_i18n || {}) },
    group: repositoryGroup(repo, locale.value), group_i18n: { ...(repo.group_i18n || {}) },
    provider: repo.provider, username: repo.username, repository: repo.repository,
    download_mode: repo.download_mode, branch: repo.branch || 'main', file_path: repo.file_path || '',
    region: normalizeRepositoryRegion(repo.region), channel: repo.channel,
    asset_rules: repo.asset_rules.map((rule) => ({ ...rule })), install_target: repo.install_target,
    note: localizedRepositoryNote(repo), note_i18n: { ...(repo.note_i18n || {}) },
    plugin_guid: repo.plugin_guid, local_product_name: repo.local_product_name || '',
    local_original_filename: repo.local_original_filename || '', version_regex: repo.version_regex || '',
    max_release_count: repo.max_release_count || 10,
    download_threads: repo.download_threads || (repo.provider === 'github' ? 16 : 1),
    github_token: '', gitee_token: '', official_api_only: true, enabled: repo.enabled,
  } : {
    id: undefined, name: '', name_i18n: {}, group: '', group_i18n: {}, provider: 'gitee', username: '', repository: '', download_mode: 'release',
    branch: 'main', file_path: '', region: '', channel: 'all', asset_rules: [],
    install_target: 'BepInEx/plugins', note: '', note_i18n: {}, plugin_guid: '', local_product_name: '',
    local_original_filename: '', version_regex: '', max_release_count: 10, download_threads: 1,
    github_token: '', gitee_token: '',
    official_api_only: true, enabled: true,
  })
  showRepoEditor.value = true
}
function ensureRepositoryChineseName() {
  const nameTranslations = repoForm.name_i18n || (repoForm.name_i18n = {})
  if (!nameTranslations['zh-CN']?.trim()) {
    nameTranslations['zh-CN'] = (locale.value === 'zh-CN' ? repoForm.name : nameTranslations[locale.value] || repoForm.name).trim()
  }
}
async function saveRepo() {
  if (repoForm.download_mode === 'release' && (!repoForm.asset_rules.length || repoForm.asset_rules.some((rule) => !rule.value.trim()))) {
    showError(t('请创建至少一个有效的资产规则'))
    return
  }
  ensureRepositoryChineseName()
  let refreshError = ''
  let refreshed = false
  try {
    await run('正在保存仓库', async () => {
      const savedConfig = await api.saveRepository(repoForm)
      const savedRepo = savedConfig.repositories.find((repo) => repo.id === repoForm.id)
        ?? savedConfig.repositories.find((repo) => repo.username === repoForm.username.trim()
          && repo.repository === repoForm.repository.trim() && repo.provider === repoForm.provider
          && (repo.region || '') === (repoForm.region?.trim().toLowerCase() || '')
          && repo.download_mode === repoForm.download_mode)
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
function openModRecycleBin() {
  if (tutorial.visible.value || tutorial.previewing.value || busy.value) return
  showModRecycleBin.value = true
}
function handleRecycledModRestored() {
  void refreshDashboardSerialized().catch(showError)
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
  cleanupUiZoom()
})
</script>

<template>
  <ConfirmationDialog :request="confirmation" @answer="answerConfirmation" />
  <RepositoryDocumentDialog :target="repositoryDocument" @close="repositoryDocument = null" />
  <div :class="['app-shell', { 'tutorial-layer-active': tutorial.visible.value, 'sidebar-rail': sidebarCollapsed }]">
    <StartupOverlay v-if="startupUpdateChecking || updateCheckFailed" :checking="startupUpdateChecking" :check-failed="updateCheckFailed"
      :skipped="updateSkipped" :stage="startupStage" :error="startupError" :phase="startupUpdatePhase" :check="appUpdateCheck"
      :update="appUpdate" :countdown="updateSkipCountdown" :page-opening="updatePageOpening" :page-error="updatePageError"
      :progress="displayProgress" @continue="continueAfterUpdateSkip" @manual-update="openManualUpdatePage" @exit="api.exitApp()" />
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
      :app-icon="appIcon" :collapsed="sidebarCollapsed" @change-view="activeView = $event"
      @toggle="sidebarCollapsed = !sidebarCollapsed" />
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
        <DownloadStrip :progress="displayProgress" @cancel="cancelDownload" />
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
        :installed-mod-labels="installedModLabels"
        :installed-mod-label-text="installedModLabelText"
        :installed-mod-label-class="installedModLabelClass"
        :installed-mod-label-actionable="installedModLabelActionable"
        :edit-mod-note="editModNote"
        :uninstall-mod="uninstallMod"
        :uninstall-mod-package="uninstallModPackage"
        :display-mod-path="displayModPath"
        :mod-uninstall-pending="modOps.modUninstallPending.value"
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
        :refresh-all-repositories="refreshAllRepositories"
        :refresh-repository="refreshRepo"
        :refresh-source="refreshSource"
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
      <RepositorySourceManagerDialog :manager="sourceManager" :busy="busy" />
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
              <label><span>{{ t('仓库名称') }}</span><input v-model="repoForm.name" :placeholder="t('例如：我的 MOD 正式版')" @input="updateRepositoryTranslation('name', $event)" /></label>
              <label><span>{{ t('来源平台') }}</span><select v-model="repoForm.provider" :aria-label="t('来源平台')" @change="providerChanged">
                <option value="gitee">Gitee</option>
                <option value="github">GitHub</option>
              </select></label>
              <label><span>{{ t('仓库分组') }}</span><input v-model="repoForm.group" :placeholder="t('例如：前置必装')" @input="updateRepositoryTranslation('group', $event)" /></label>
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
            <textarea v-model="repoForm.note" class="repository-note-input" rows="3" :placeholder="t('可选，多行备注')" @input="updateRepositoryTranslation('note', $event)"></textarea>
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
    <LogViewerDialog :open="showRustLog" :kind="logViewerKind" :log="rustLog" @close="closeRustLog" @refresh="refreshRustLog" />
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
    <ModNoteDialog :ops="modOps" :busy="busy" />
  </div>
  <ModRecycleBin :open="showModRecycleBin" :game-blocked="gameBlocked" :busy="!!busy"
    @close="showModRecycleBin = false" @restored="handleRecycledModRestored" />
  <ModConfigEditor v-if="configEditorTarget" :target="configEditorTarget" :game-blocked="gameBlocked" :busy="!!busy"
    :confirm-discard="() => requestConfirmation({ title: t('未保存的配置'), message: t('配置尚未保存，确定放弃修改？'), danger: true })"
    @close="configEditorTarget = null" />
  <InstallRepositoryDialog :ops="modOps" :busy="busy" />
  <ManualInstallDialog :ops="modOps" :busy="busy" :game-blocked="gameBlocked" />
  <BepInExSetupGuide :visible="showBepInExSetup && !tutorial.visible.value && !tutorial.previewing.value" :config="dashboard?.config" :game="realGame"
    :result="bepinexSetupResult" :busy="!!busy" :repair-native-overlay="repairNativeBepInExOverlay" @close="showBepInExSetup = false" />

  <BepInExChannelDialog :open="showBepInExInstall" :urls="bepinexDownloadUrls" :busy="busy"
    @close="showBepInExInstall = false" @install="installBepInEx" />
</template>
