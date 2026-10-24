<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  AlertTriangle, Boxes, CheckCircle2, CloudDownload, Coffee, Download,
  LayoutDashboard, PackageCheck, X,
} from 'lucide-vue-next'
import { api, watchAppUpdateChannel, watchAppUpdatePhase, watchDownloadProgress, watchGameState } from './services/tauri'
import type { BepInExInstallResult, ConfigEditorTarget, Dashboard, GitHubProxyConfig, Provider, Repository, SettingsPatch, WinhttpState } from './types'
import { bepinexDownloadUrls as getBepInExDownloadUrls } from './gameRuntime'
import { t, useLocaleTranslation, type Locale } from './i18n'
import { repositoryRegionMatches } from './repositorySources'
import { repositoryName, repositoryNote } from './repositoryNames'
import { installedModLabelActionable, installedModLabelClass, installedModLabelText } from './installedModStatus'
import { installReleases, matchingInstallAssets } from './repositoryAssets'
import { formatDate as formatDateForLocale } from './formatting'
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
import { useRepositoryEditor } from './composables/useRepositoryEditor'
import { useImportExport } from './composables/useImportExport'
import { repositorySourceRegionLabel } from './repositorySources'
import DownloadStrip from './components/DownloadStrip.vue'
import LogViewerDialog from './components/LogViewerDialog.vue'
import StartupOverlay from './components/StartupOverlay.vue'
import RepositorySourceManagerDialog from './components/RepositorySourceManagerDialog.vue'
import InstallRepositoryDialog from './components/InstallRepositoryDialog.vue'
import ManualInstallDialog from './components/ManualInstallDialog.vue'
import ModNoteDialog from './components/ModNoteDialog.vue'
import BepInExChannelDialog from './components/BepInExChannelDialog.vue'
import RepositoryEditorDialog from './components/RepositoryEditorDialog.vue'
import RepositoryImportDialog from './components/RepositoryImportDialog.vue'
import RepositoryExportDialog from './components/RepositoryExportDialog.vue'
import ReleaseDetailDialog from './components/ReleaseDetailDialog.vue'
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
const repoEditor = useRepositoryEditor({
  showRepoEditor,
  dashboard,
  repos,
  locale,
  run,
  showError,
  showSuccess,
  requestConfirmation,
  loadDashboard,
  refreshRepo,
  api,
})
const { editRepo, deleteRepo } = repoEditor
const repositoryIo = useImportExport({ run, requestConfirmation, loadDashboard, api })
const { exportRepositories } = repositoryIo

function statusLabel(status: WinhttpState) { return t(({ modenabled: '框架已启用', moddisabled: '框架已禁用', missing: config.value?.platform !== 'windows' && config.value?.game.runtime === 'native' ? '未找到框架加载器' : '未找到 winhttp.dll', conflict: '文件状态冲突' } as Record<WinhttpState, string>)[status]) }
function formatDate(value?: string) { return formatDateForLocale(value, locale.value) }
function scheduleToastClose() { if (toastTimer) window.clearTimeout(toastTimer); toastTimer = window.setTimeout(() => { message.value = ''; error.value = '' }, 5000) }
function latest(repo: Repository) { return repo.cached_releases?.[0] }
function localizedRepositoryName(repo: Repository) { return repositoryName(repo, locale.value) }
function localizedRepositoryNote(repo: Repository) { return repositoryNote(repo, locale.value) }
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
      <RepositoryExportDialog :io="repositoryIo" />
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
    <RepositoryEditorDialog :editor="repoEditor" :busy="busy" />
    <LogViewerDialog :open="showRustLog" :kind="logViewerKind" :log="rustLog" @close="closeRustLog" @refresh="refreshRustLog" />
    <RepositoryImportDialog :io="repositoryIo" />
    <ReleaseDetailDialog :repo="showRelease" @close="showRelease = null" />
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
