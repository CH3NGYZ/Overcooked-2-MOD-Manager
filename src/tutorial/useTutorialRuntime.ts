import { computed, onUnmounted, shallowRef, type Ref } from 'vue'
import type { Locale } from '../i18n'
import { t } from '../i18n'
import type {
  Dashboard,
  PublicConfig,
  ReleaseAsset,
  ReleaseInfo,
  Repository,
  RepositorySource,
  SettingsPatch,
  TutorialRepositoryConfig,
} from '../types'
import { createTutorialDashboard } from './scenarios'
import { tutorialRepositoryLoadingStepIds, type TutorialStep, type View } from './steps'
import { useTutorialController, type TutorialApi, type TutorialHost, type TutorialZoomControls } from './useTutorialController'

export type TutorialProgressState = {
  label: string
  percent: number
  speed: string
  status: string
  indeterminate?: boolean
}

export type TutorialRuntimeHost = {
  api: TutorialApi & { getBuiltinTutorialRepositories: () => Promise<TutorialRepositoryConfig[]> }
  dashboard: Ref<Dashboard | null>
  activeView: Ref<View>
  locale: Ref<Locale>
  uiZoom: Ref<number>
  zoomControls?: TutorialZoomControls
  progress: Ref<TutorialProgressState>
  sourceLoading: Record<string, boolean>
  repositoryReleaseReady: Record<string, boolean>
  modDetailsExpanded: Record<string, boolean>
  modPackageCollapsed: Record<string, boolean>
  showBepInExInstall: Ref<boolean>
  bepinexInstallReinstall: Ref<boolean>
  showRepositorySource: Ref<boolean>
  showRepoEditor: Ref<boolean>
  showInstall: Ref<Repository | null>
  selectedInstallTag: Ref<string>
  selectedInstallAsset: Ref<string>
  getStoredSteps: () => import('../types').TutorialStepConfig[]
  getSeenStepIds: () => string[]
  persistSettings: (patch: SettingsPatch) => Promise<PublicConfig>
  showError: (reason: unknown) => void
  showSuccess: (message: string) => void
  run: (label: string, action: () => Promise<unknown>) => Promise<unknown>
  getRepositories: () => Repository[]
  getActiveRepositorySources: () => RepositorySource[]
  resetInstalledModExpansion: () => void
  openRepositorySourceManager: () => void
  editRepository: () => void
  installReleases: (repo: Repository | null) => ReleaseInfo[]
  matchingInstallAssets: (repo: Repository, release: ReleaseInfo) => ReleaseAsset[]
  startInitialRepositoryRefresh: () => void
}

function createControllerHost(host: TutorialRuntimeHost, prepareStep: (step: TutorialStep) => void | Promise<void>, finishPresentation: () => void): TutorialHost {
  return {
    api: host.api,
    getStoredSteps: host.getStoredSteps,
    getSeenStepIds: host.getSeenStepIds,
    persistSettings: host.persistSettings,
    getLocale: () => host.locale.value,
    getZoom: () => host.uiZoom.value,
    zoomControls: host.zoomControls,
    getActiveView: () => host.activeView.value,
    setActiveView: (view) => { host.activeView.value = view },
    prepareStep,
    finishPresentation,
    showError: host.showError,
    showSuccess: host.showSuccess,
    run: host.run,
  }
}

export function useTutorialRuntime(host: TutorialRuntimeHost) {
  const previousRepositoryReady = new Map<string, boolean | undefined>()
  const tutorialLoadingSourceIds = new Set<string>()
  let repositoryReadyCaptured = false
  let initialRefreshPending = false
  const builtinRepositories = shallowRef<TutorialRepositoryConfig[]>([])
  let builtinRepositoryLoad: Promise<void> | undefined
  let disposed = false
  let presentationVersion = 0
  onUnmounted(() => { disposed = true })

  async function prepareStep(step: TutorialStep) {
    const version = ++presentationVersion
    if (step.scenario !== 'live') {
      builtinRepositoryLoad ??= host.api.getBuiltinTutorialRepositories()
        .then((repositories) => { if (!disposed) builtinRepositories.value = repositories })
        .catch((reason) => { if (!disposed) host.showError(reason) })
      await builtinRepositoryLoad
      if (disposed || version !== presentationVersion) return
    }
    host.showBepInExInstall.value = false
    host.showRepositorySource.value = false
    host.showRepoEditor.value = false
    host.showInstall.value = null

    if (step.scenario === 'mod-management') {
      const childModsVisible = ['mod-package-child', 'mod-package-child-details', 'mod-package-child-detail-panel', 'mod-package-child-actions'].includes(step.id)
      host.modPackageCollapsed['arcade-mod-package'] = !childModsVisible
      const showTutorialChildDetails = step.id === 'mod-package-child-detail-panel' || step.id === 'mod-package-child-actions'
      host.modDetailsExpanded['HostUtilities.dll'] = false
      host.modDetailsExpanded['ConfigurationManager.dll'] = showTutorialChildDetails
    }

    const rootSourceId = host.getActiveRepositorySources()[0]?.id
    for (const key of Object.keys(host.sourceLoading)) {
      if (key === 'tutorial-source' || key === rootSourceId) host.sourceLoading[key] = false
    }
    if (tutorialRepositoryLoadingStepIds.has(step.id)) {
      const loadingSourceId = rootSourceId || 'tutorial-source'
      host.sourceLoading[loadingSourceId] = true
      tutorialLoadingSourceIds.add(loadingSourceId)
    }

    if (['overview', 'framework-missing', 'mod-management', 'repository-ready', 'repository-install', 'release-select', 'download-progress'].includes(step.scenario)) {
      if (!repositoryReadyCaptured) {
        for (const repo of host.getRepositories()) previousRepositoryReady.set(repo.id, host.repositoryReleaseReady[repo.id])
        repositoryReadyCaptured = true
      }
      for (const repo of host.getRepositories()) host.repositoryReleaseReady[repo.id] = true
    }

    if (step.scenario === 'framework-channel') {
      host.bepinexInstallReinstall.value = false
      host.showBepInExInstall.value = true
    } else if (step.scenario === 'source-manager') {
      host.openRepositorySourceManager()
    } else if (step.scenario === 'repository-editor') {
      host.editRepository()
    } else if (step.scenario === 'release-select' || step.scenario === 'repository-install') {
      const repo = host.getRepositories()[0]
      if (repo) {
        const release = host.installReleases(repo)[0]
        host.selectedInstallTag.value = release?.tag_name || ''
        host.selectedInstallAsset.value = release
          ? host.matchingInstallAssets(repo, release)[0]?.name || ''
          : ''
        host.showInstall.value = repo
      }
    }
  }

  function finishPresentation() {
    presentationVersion += 1
    host.showBepInExInstall.value = false
    host.showRepositorySource.value = false
    host.showRepoEditor.value = false
    host.showInstall.value = null
    host.resetInstalledModExpansion()
    host.sourceLoading['tutorial-source'] = false
    for (const sourceId of tutorialLoadingSourceIds) host.sourceLoading[sourceId] = false
    tutorialLoadingSourceIds.clear()
    if (repositoryReadyCaptured) {
      for (const [id, previous] of previousRepositoryReady) {
        if (previous === undefined) delete host.repositoryReleaseReady[id]
        else host.repositoryReleaseReady[id] = previous
      }
    }
    previousRepositoryReady.clear()
    repositoryReadyCaptured = false
    if (initialRefreshPending) {
      initialRefreshPending = false
      console.info('[repository-refresh] tutorial finished; releasing initial refresh')
      host.startInitialRepositoryRefresh()
    }
  }

  const controller = useTutorialController(createControllerHost(host, prepareStep, finishPresentation))
  const previewDashboard = computed(() => controller.previewing.value
    ? createTutorialDashboard(host.dashboard.value, controller.scenario.value, t, builtinRepositories.value)
    : host.dashboard.value)
  const displayDashboard = computed(() => previewDashboard.value || host.dashboard.value)
  const displayProgress = computed<TutorialProgressState>(() => {
    if (controller.previewing.value && controller.scenario.value === 'download-progress') {
      return { label: t('下载 HostUtilities'), percent: controller.fakeProgressPercent.value, speed: '1.8 MB/s', status: t('写入临时文件') }
    }
    if (controller.previewing.value && controller.scenario.value === 'framework-installing') {
      return { label: t('安装 BepInEx'), percent: controller.fakeProgressPercent.value, speed: '3.2 MB/s', status: t('解压框架文件') }
    }
    return host.progress.value
  })

  async function startInitialOnboarding() {
    const started = await controller.startOnboarding()
    if (started) initialRefreshPending = true
    return started
  }

  return {
    controller,
    previewDashboard,
    displayDashboard,
    displayProgress,
    startInitialOnboarding,
  }
}
