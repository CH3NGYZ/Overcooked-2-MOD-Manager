<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, reactive, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  AlertTriangle, Boxes, Check, CheckCircle2, ChevronRight, CloudDownload, Coffee, Copy, Download,
  ExternalLink, FileText, FolderOpen, GripVertical, HardDriveDownload, LayoutDashboard,
  PackageCheck, Pencil, Plus, RefreshCw, RotateCcw, Save, Search, Settings2,
  ShieldCheck, Trash2, Upload, X,
} from 'lucide-vue-next'
import { api, watchDownloadProgress } from './services/tauri'
import type { Dashboard, Provider, Repository, RepositoryPatch, RepositorySource, WinhttpState } from './types'
import { t, useLocaleTranslation, type Locale } from './i18n'
import { renderMarkdown } from './markdown'
import sponsorImage from './assets/collectionCode.jpg'
import appIcon from './assets/bb4518aa-dc55-4cae-a432-08ccdc5f28f8.png'

type View = 'overview' | 'framework' | 'installed' | 'repositories' | 'sponsor'

const activeView = ref<View>('overview')
const dashboard = ref<Dashboard | null>(null)
const busy = ref('')
const message = ref('')
const error = ref('')
const progress = ref({ label: '', percent: 0, speed: '', status: '' })
const downloadCancelRequested = ref(false)
const dark = ref(true)
const showRepoEditor = ref(false)
const showImport = ref(false)
const showRepositorySource = ref(false)
const repositorySourceUrl = ref('')
const repositorySourceNickname = ref('')
const editingRepositorySourceId = ref<string | null>(null)
const repositorySourceEditorOpen = ref(false)
const repositorySourceDraft = ref<RepositorySource[]>([])
const draggingSourceId = ref<string | null>(null)
const showExport = ref(false)
const showRustLog = ref(false)
const rustLog = ref({ path: '', content: '' })
const rustLogOutput = ref<HTMLElement | null>(null)
const showRelease = ref<Repository | null>(null)
const showRepositoryNote = ref<Repository | null>(null)
const showInstall = ref<Repository | null>(null)
const showBepInExInstall = ref(false)
const bepinexInstallReinstall = ref(false)
const selectedInstallTag = ref('')
const selectedInstallAsset = ref('')
const releaseAssetSizeLoading = ref(false)
let releaseAssetSizeRequestToken = 0
let releaseAssetSizeRequest: { repositoryId: string; releaseTag: string } | null = null
const importText = ref('')
const importPreview = ref<{ valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] } | null>(null)
const repositoryRefreshing = reactive<Record<string, boolean>>({})
const sourceRefreshingManual = reactive<Record<string, boolean>>({})
const appUpdate = ref<{ version: string; current_version: string } | null>(null)
const startupUpdateChecking = ref(true)
const updateCheckFailed = ref(false)
const authoritativeSourceLoading = ref(false)
const authoritativeSourceFailed = ref(false)
const sourceCollapsed = reactive<Record<string, boolean>>({})
const repositoryGroupCollapsed = reactive<Record<string, boolean>>({})
const sourceLoading = reactive<Record<string, boolean>>({})
const repositoryReleaseReady = reactive<Record<string, boolean>>({})
const modDetailsExpanded = reactive<Record<string, boolean>>({})
const stopProgress = ref<(() => void) | null>(null)
let toastTimer: number | undefined
let rustLogTimer: number | undefined
const editingMod = ref<import('./types').InstalledMod | null>(null)
const modNote = ref('')
const installedModsPendingCheck = ref(false)
const appVersion = ref('')
const { locale, setLocale } = useLocaleTranslation()
const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'ko', label: '한국어' },
]
const repoForm = reactive<RepositoryPatch>({
  name: '', group: '', provider: 'gitee', url: '', channel: 'stable', asset_rules: [],
  install_target: 'BepInEx/plugins', note: '', plugin_guid: '', local_product_name: '', local_original_filename: '', download_threads: 1, api_proxy_mode: 'direct', api_proxy_url: '', release_proxy_mode: 'direct', release_proxy_url: '', github_token: '', gitee_token: '', official_api_only: true, enabled: true,
})
const bepinexDownloadUrls: Record<Provider, string> = {
  gitee: 'https://gitee.com/ch3ngyz/Overcooked-2-Installer-ResourceFiles/releases/download/bepinex/bepinex.zip',
  github: 'https://github.com/BepInEx/BepInEx/releases/download/v5.4.23.5/BepInEx_win_x86_5.4.23.5.zip',
}

const navItems = [
  { id: 'overview' as View, label: '总览', hint: '游戏状态和快捷操作', icon: LayoutDashboard },
  { id: 'framework' as View, label: '框架管理', hint: '框架安装与备份', icon: PackageCheck },
  { id: 'installed' as View, label: 'MOD管理', hint: '本地插件与备注', icon: Boxes },
  { id: 'repositories' as View, label: '仓库管理', hint: 'Release 源配置', icon: CloudDownload },
  { id: 'sponsor' as View, label: '赞助渠道', hint: '支持项目开发', icon: Coffee },
]

const game = computed(() => dashboard.value?.game)
const config = computed(() => dashboard.value?.config)
const repos = computed(() => config.value?.repositories ?? [])
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
const sourceGroups = computed(() => {
  const sources = config.value?.repository_sources ?? []
  const visibleSources = sources.filter((source) => !(source.builtin && authoritativeSourceFailed.value))
  const groups = visibleSources.map((source) => ({ source, groups: groupedRepositories(repos.value.filter((repo) => repo.source_id === source.id)) }))
  if (authoritativeSourceFailed.value) {
    const builtin = { id: 'builtin-fallback', nickname: t('内置仓库源'), url: '', builtin: true }
    groups.unshift({ source: builtin, groups: groupedRepositories(repos.value.filter((repo) => repo.builtin || repo.source_id === 'azhe')) })
  }
  const knownSourceIds = new Set(sources.map((source) => source.id))
  const unclassified = repos.value.filter((repo) => !repo.source_id || !knownSourceIds.has(repo.source_id))
  if (unclassified.length) groups.push({ source: { id: 'unclassified', nickname: t('未分类仓库'), url: '', builtin: false }, groups: groupedRepositories(unclassified) })
  return groups
})
const installedMods = computed(() => dashboard.value?.mods ?? [])
const mappedReposAvailable = computed(() => repos.value.some((repo) => repo.enabled && installedMods.value.some((mod) => mod.repository_id === repo.id)))
const gameBlocked = computed(() => game.value?.kitchen_running ?? false)
const currentWinhttp = computed<WinhttpState>(() => game.value?.winhttp_state ?? 'missing')
const repositoryRefreshBatchActive = ref(false)
const repositoryRefreshBatchTotal = ref(0)
const repositoryRefreshBatchCompleted = ref(0)
const installableRepositoryCount = computed(() => repos.value.filter((repo) => repositoryReleaseReady[repo.id] && repo.enabled && installReleases(repo).length > 0).length)
const updateAvailableModCount = computed(() => installedMods.value.filter((mod) => mod.update_available && !!mod.repository_id && !!repositoryReleaseReady[mod.repository_id]).length)
const allRepositoryRefreshComplete = computed(() => {
  const sources = config.value?.repository_sources ?? []
  const configured = repos.value.filter((repo) => repo.enabled && !!repo.source_id)
  return !authoritativeSourceLoading.value && !repositoryRefreshBatchActive.value
    && (sources.length > 0 || repos.value.length === 0)
    && Object.values(sourceLoading).every((loading) => !loading)
    && Object.values(sourceRefreshingManual).every((refreshing) => !refreshing)
    && configured.every((repo) => repositoryReleaseReady[repo.id])
})
let repositoryRefreshCount = 0
let dashboardRevision = 0
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

function statusLabel(status: WinhttpState) { return t(({ modenabled: 'MOD 已启用', moddisabled: 'MOD 已关闭', missing: '未找到 winhttp.dll', conflict: '文件状态冲突' } as Record<WinhttpState, string>)[status]) }
function formatBytes(value: number) { if (!value) return '0 B'; const units = ['B', 'KB', 'MB', 'GB']; const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1); return `${(value / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}` }
function formatAssetSize(asset: import('./types').ReleaseAsset) { return asset.size > 0 ? formatBytes(asset.size) : t(releaseAssetSizeLoading.value ? '读取文件大小中' : '大小未知') }
function scheduleToastClose() { if (toastTimer) window.clearTimeout(toastTimer); toastTimer = window.setTimeout(() => { message.value = ''; error.value = '' }, 5000) }
function formatDate(value?: string) { if (!value) return t('未知时间'); const language = ({ 'zh-CN': 'zh-CN', 'zh-TW': 'zh-TW', en: 'en-US', ko: 'ko-KR' } as Record<Locale, string>)[locale.value]; return new Intl.DateTimeFormat(language, { year: 'numeric', month: 'short', day: 'numeric' }).format(new Date(value)) }
function latest(repo: Repository) { return repo.cached_releases?.[0] }
function localizedRepositoryName(repo: Repository) { return repo.name_i18n?.[locale.value] || repo.name }
function localizedRepositoryNote(repo: Repository) { return repo.note_i18n?.[locale.value] || repo.note }
function repositoryInstallStatus(repo: Repository) {
  const boundMods = installedMods.value.filter((mod) => mod.repository_id === repo.id)
  if (!boundMods.length) return ''
  return boundMods.some((mod) => mod.update_available) ? '可更新' : '已安装'
}
function repositoryNoteIsLong(repo: Repository) {
  const note = localizedRepositoryNote(repo)
  return note.split(/\r?\n/).length > 4 || note.length > 320
}
function assetRuleLabel(type: string) { return t(type === 'regex' ? '正则' : type === 'glob' ? '通配符' : '精确') }
function assetMatchesUi(name: string, rule: { type: string; value: string }) {
  if (rule.type === 'exact') return name.toLocaleLowerCase() === rule.value.toLocaleLowerCase()
  if (rule.type === 'glob') {
    const escaped = rule.value.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.')
    return new RegExp(`^${escaped}$`, 'i').test(name)
  }
  try { return new RegExp(rule.value, 'i').test(name) } catch { return false }
}
function showError(value: unknown) { error.value = t(value instanceof Error ? value.message : String(value)); message.value = ''; scheduleToastClose() }
function isModRepositoryRefreshing(mod: import('./types').InstalledMod) { return !!mod.repository_id && !!repositoryRefreshing[mod.repository_id] }
async function run<T>(label: string, action: () => Promise<T>) { busy.value = label; error.value = ''; message.value = ''; try { const result = await action(); message.value = typeof result === 'object' && result && 'message' in result ? t(String((result as { message: string }).message)) : `${t(label)}${t('完成')}`; scheduleToastClose(); return result } catch (reason) { showError(reason); throw reason } finally { busy.value = '' } }
async function loadDashboard() { await run('正在读取厨房状态', async () => { dashboard.value = await api.dashboard() }) }
async function detectGame() { await run('正在重新检测', async () => { await api.detectGame(); dashboard.value = await api.dashboard() }) }
async function chooseGameDirectory() { const path = await api.chooseGameDirectory(); if (!path || Array.isArray(path)) return; await run('正在保存厨房目录', async () => { await api.setGamePath(path); dashboard.value = await api.dashboard() }) }
async function restoreDefaultGameDirectory() { await run('正在恢复默认游戏目录', async () => { await api.resetGamePath(); dashboard.value = await api.dashboard() }) }
async function refreshRepositoryEntry(repo: Repository) {
  const currentTask = repositoryRefreshTasks.get(repo.id)
  if (currentTask) return currentTask
  const task = (async () => {
    dashboardRevision += 1
    repositoryRefreshCount += 1
    repositoryRefreshing[repo.id] = true
    try {
      const releases = await api.refreshRepository(repo.id) ?? []
      const currentRepo = repos.value.find((item) => item.id === repo.id) ?? repo
      currentRepo.cached_releases = releases
      currentRepo.last_error = undefined
      repositoryReleaseReady[repo.id] = true
      return true
    } catch (reason) {
      const currentRepo = repos.value.find((item) => item.id === repo.id) ?? repo
      currentRepo.last_error = reason instanceof Error ? reason.message : String(reason)
      repositoryReleaseReady[repo.id] = false
      return false
    } finally {
      repositoryRefreshing[repo.id] = false
      repositoryRefreshCount -= 1
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
        message.value = `${localizedRepositoryName(repo)}${t('刷新完成')}`
        scheduleToastClose()
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
function installRepo(repo: Repository) {
  if (!game.value?.bepinex.version) { showError('请先安装 BepInEx 框架'); return }
  const release = installReleases(repo)[0]
  if (!release) { showError('请先刷新 Release'); return }
  selectedInstallTag.value = release.tag_name
  selectedInstallAsset.value = matchingInstallAssets(repo, release)[0]?.name || ''
  showInstall.value = repo
  void refreshSelectedReleaseAssetSizes(repo, release)
}
function installReleases(repo: Repository | null) {
  if (!repo) return []
  return repo.cached_releases.filter((release) => matchingInstallAssets(repo, release).length > 0)
}
function matchingInstallAssets(repo: Repository, release: import('./types').ReleaseInfo) {
  return release.assets.filter((asset) => !!asset.download_url && repo.asset_rules.some((rule) => assetMatchesUi(asset.name, rule)))
}
function selectedInstallRelease(repo: Repository | null) { return installReleases(repo).find((release) => release.tag_name === selectedInstallTag.value) }
function selectedInstallAssets(repo: Repository | null) {
  const release = selectedInstallRelease(repo)
  return repo && release ? matchingInstallAssets(repo, release) : []
}
async function cancelReleaseAssetSizeRequest() {
  const request = releaseAssetSizeRequest
  if (!request) return
  releaseAssetSizeRequestToken += 1
  releaseAssetSizeRequest = null
  releaseAssetSizeLoading.value = false
  try {
    await api.cancelReleaseAssetSizes(request.repositoryId, request.releaseTag)
  } catch {
    // The request may have completed while cancellation was being sent.
  }
}
function closeInstallDialog() {
  void cancelReleaseAssetSizeRequest()
  showInstall.value = null
}
async function refreshSelectedReleaseAssetSizes(repo: Repository, release: import('./types').ReleaseInfo | undefined) {
  if (repo.provider !== 'gitee' || !release?.release_id || release.assets.every((asset) => asset.size > 0)) return
  await cancelReleaseAssetSizeRequest()
  const token = ++releaseAssetSizeRequestToken
  releaseAssetSizeRequest = { repositoryId: repo.id, releaseTag: release.tag_name }
  releaseAssetSizeLoading.value = true
  try {
    const updated = await api.refreshReleaseAssetSizes(repo.id, release.tag_name)
    if (token !== releaseAssetSizeRequestToken) return
    const current = repo.cached_releases.find((item) => item.tag_name === updated.tag_name)
    if (current) Object.assign(current, updated)
  } catch (reason) {
    if (token === releaseAssetSizeRequestToken && !String(reason).includes('已取消')) showError(reason)
  } finally {
    if (token === releaseAssetSizeRequestToken) {
      releaseAssetSizeRequest = null
      releaseAssetSizeLoading.value = false
    }
  }
}
function selectInstallRelease() {
  const repo = showInstall.value
  if (!repo) return
  const release = selectedInstallRelease(repo)
  selectedInstallAsset.value = release ? matchingInstallAssets(repo, release)[0]?.name || '' : ''
  void refreshSelectedReleaseAssetSizes(repo, release)
}
function clearDownloadProgress() { progress.value = { label: '', percent: 0, speed: '', status: '' } }
async function confirmInstall() {
  const repo = showInstall.value
  if (!repo || !selectedInstallTag.value || !selectedInstallAsset.value) return
  await cancelReleaseAssetSizeRequest()
  downloadCancelRequested.value = false
  showInstall.value = null
  try {
    await run(`正在安装 ${localizedRepositoryName(repo)}`, () => api.installRepository(repo.id, selectedInstallTag.value, selectedInstallAsset.value))
    await loadDashboard()
  } finally {
    clearDownloadProgress()
  }
}
async function uninstallRepo(repo: Repository) { if (!confirm(`${t('确定卸载')} ${localizedRepositoryName(repo)}? ${t('只有安装清单中的文件会被删除。')}`)) return; await run(`正在卸载 ${localizedRepositoryName(repo)}`, () => api.uninstallRepository(repo.id)); await loadDashboard() }
void uninstallRepo
function openBepInExInstall(reinstall: boolean) {
  bepinexInstallReinstall.value = reinstall
  showBepInExInstall.value = true
}
async function installBepInEx(provider: Provider) {
  const reinstall = bepinexInstallReinstall.value
  showBepInExInstall.value = false
  if (reinstall && !confirm(t('重新安装 BepInEx 会先备份当前框架并替换框架文件，是否继续？'))) return
  downloadCancelRequested.value = false
  try {
    await run(reinstall ? '正在重新安装 BepInEx' : '正在安装 BepInEx', () => api.installBepInEx(reinstall, provider)); await loadDashboard()
  } finally {
    clearDownloadProgress()
  }
}
async function uninstallBepInEx() { if (!confirm(t('确定移除 BepInEx？程序会先创建时间戳备份。'))) return; await run('正在移除 BepInEx', api.uninstallBepInEx); await loadDashboard() }
async function backupBepInEx() { await run('正在创建 BepInEx 备份', api.backupBepInEx); await loadDashboard() }
async function openBackupDirectory() { await run('正在打开备份目录', api.openBackupDirectory) }
async function deleteBackup(path: string) { if (!confirm(`${t('确定删除备份')}“${path.split(/[\\/]/).pop()}”？${t('删除后无法恢复。')}`)) return; await run('正在删除 BepInEx 备份', () => api.deleteBepInExBackup(path)); await loadDashboard() }
async function cancelDownload() {
  downloadCancelRequested.value = true
  clearDownloadProgress()
  try { await api.cancelDownload() } catch { /* 下载任务会自行结束，进度条已隐藏 */ }
}
async function refreshRustLog() {
  try {
    rustLog.value = await api.readRustLog()
    await nextTick()
    if (rustLogOutput.value) rustLogOutput.value.scrollTop = rustLogOutput.value.scrollHeight
  } catch (reason) {
    rustLog.value = { path: '', content: String(reason) }
  }
}
async function openRustLog() {
  showRustLog.value = true
  await refreshRustLog()
  if (rustLogTimer) window.clearInterval(rustLogTimer)
  rustLogTimer = window.setInterval(() => { void refreshRustLog() }, 1000)
}
function closeRustLog() {
  showRustLog.value = false
  if (rustLogTimer) window.clearInterval(rustLogTimer)
  rustLogTimer = undefined
}
async function openDeveloperConsole() { await run('正在打开开发者控制台', api.openDevtools) }
async function resetUi() {
  if (!confirm(`${t('重置到最新 UI')}？`)) return
  await run('正在重置界面', api.resetUi)
  await loadDashboard()
}
async function restoreBackup(path: string) { await run('正在恢复 BepInEx', () => api.restoreBepInEx(path)); await loadDashboard() }
async function launchNormal() { await run('正在准备 Steam 启动', api.launchNormal); await loadDashboard() }
async function toggleWinhttp() {
  if (currentWinhttp.value === 'missing') return
  const enabled = currentWinhttp.value === 'moddisabled' || currentWinhttp.value === 'conflict'
  await run(enabled ? '正在切换有 MOD 状态' : '正在切换无 MOD 状态', () => api.switchWinhttp(enabled))
  await loadDashboard()
}
async function openDirectory() { await run('正在打开游戏目录', api.openGameDirectory) }
async function openModDirectory() { await run('正在打开模组目录', api.openModDirectory) }
async function checkAppUpdate() { const result = await run('正在检查程序更新', () => api.checkAppUpdate(locale.value)); appUpdate.value = result ?? null; if (!result) message.value = t('当前已是最新版本') }
async function installAppUpdate() { await run('正在安装程序更新', () => api.installAppUpdate(locale.value)) }
async function refreshRepositoryBatch(repositories: Repository[]) {
  const trackingToken = repositories.length > 0 ? ++repositoryRefreshBatchToken : undefined
  if (trackingToken !== undefined && !repositoryRefreshBatchActive.value) {
    repositoryRefreshBatchTotal.value = 0
    repositoryRefreshBatchCompleted.value = 0
  }
  repositoryRefreshBatchTotal.value += repositories.length
  if (repositories.length > 0) repositoryRefreshBatchActive.value = true
  try {
    const results = await Promise.all(repositories.map(async (repo) => {
      const success = await refreshRepo(repo, false, false)
      repositoryRefreshBatchCompleted.value += 1
      return success
    }))
    return results.filter((success) => !success).length
  } finally {
    if (trackingToken !== undefined && trackingToken === repositoryRefreshBatchToken) repositoryRefreshBatchActive.value = false
  }
}
async function refreshMappedRepositories() {
  const mappedIds = new Set(installedMods.value.map((mod) => mod.repository_id).filter((id): id is string => !!id))
  const mapped = repos.value.filter((repo) => repo.enabled && mappedIds.has(repo.id))
  return refreshRepositoryBatch(mapped)
}
async function refreshMappedRepositoryUpdates() {
  installedModsPendingCheck.value = true
  try {
    const failures = await refreshMappedRepositories()
    dashboard.value = await api.dashboard()
    if (failures) throw new Error(t('仓库刷新失败摘要').replace('{count}', String(failures)))
    return dashboard.value
  } finally {
    installedModsPendingCheck.value = false
  }
}
type SourceRefreshOptions = { updateDashboard?: boolean; notify?: boolean }
type SourceRefreshResult = { ok: boolean; releaseFailures: number; error?: string }

async function refreshAllRepositories() {
  try {
    await run('正在刷新全部仓库源', async () => {
      const sources = config.value?.repository_sources ?? []
      if (!sources.length) return { ok: true, message: '全部仓库刷新完成' }
      authoritativeSourceFailed.value = false
      for (const repo of repos.value) repositoryReleaseReady[repo.id] = false
      const sourceResults = await Promise.all(sources.map(async (source) => ({
        source,
        result: await refreshSource(source.id, { updateDashboard: false, notify: false }),
      })))
      const failedSources = sourceResults.filter((entry) => !entry.result.ok)
      if (failedSources.some((entry) => entry.source.id === 'azhe')) authoritativeSourceFailed.value = true
      const releaseFailures = sourceResults
        .reduce((total, entry) => total + entry.result.releaseFailures, 0)
      dashboard.value = await api.dashboard()

      const failures = failedSources.map((entry) => `${entry.source.nickname}: ${entry.result.error || t('刷新失败')}`)
      if (releaseFailures) failures.push(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
      if (failures.length) throw new Error(failures.join('；'))
      return { ok: true, message: '全部仓库刷新完成' }
    })
  } catch {
    // run() has already displayed the aggregated error.
  } finally {
    for (const source of config.value?.repository_sources ?? []) {
      sourceLoading[source.id] = false
      sourceRefreshingManual[source.id] = false
    }
  }
}
async function checkModUpdates() {
  await run('正在检查 Mod 更新', refreshMappedRepositoryUpdates)
}
async function refreshSourceReleases(sourceId: string, updateDashboard = true) {
  sourceRefreshingManual[sourceId] = true
  try {
    const sourceRepos = repos.value.filter((repo) => repo.source_id === sourceId && repo.enabled)
    const failures = await refreshRepositoryBatch(sourceRepos)
    if (updateDashboard) dashboard.value = await api.dashboard()
    return failures
  } finally {
    sourceRefreshingManual[sourceId] = false
  }
}
async function startupChecks() {
  let update: Awaited<ReturnType<typeof api.checkAppUpdate>> = null
  try {
    update = await api.checkAppUpdate(locale.value)
  } catch {
    updateCheckFailed.value = true
    startupUpdateChecking.value = false
    return
  }
  if (update) {
    appUpdate.value = update
    try { await api.installAppUpdate(locale.value) } catch { updateCheckFailed.value = true; startupUpdateChecking.value = false }
    return
  }
  startupUpdateChecking.value = false
  await loadDashboard()
  for (const repo of repos.value) repositoryReleaseReady[repo.id] = false
  authoritativeSourceLoading.value = true
  authoritativeSourceFailed.value = false
  const sources = dashboard.value?.config.repository_sources ?? []
  const results = await Promise.all(sources.map(async (source) => ({
    source,
    result: await refreshSource(source.id, { updateDashboard: false, notify: false }),
  })))
  const failedSources = results.filter((entry) => !entry.result.ok)
  if (failedSources.some((entry) => entry.source.id === 'azhe')) authoritativeSourceFailed.value = true
  const releaseFailures = results
    .reduce((total, entry) => total + entry.result.releaseFailures, 0)
  dashboard.value = await api.dashboard()
  authoritativeSourceLoading.value = false
  const otherFailures = failedSources.filter((entry) => entry.source.id !== 'azhe')
  if (otherFailures.length) showError(otherFailures.map((entry) => `${entry.source.nickname}: ${entry.result.error || t('刷新失败')}`).join('；'))
  if (releaseFailures) showError(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
}
function editRepo(repo?: Repository) { Object.assign(repoForm, repo ? { id: repo.id, name: repo.name, group: repo.group || '', provider: repo.provider, url: repo.url, channel: repo.channel, asset_rules: repo.asset_rules.map((rule) => ({ ...rule })), install_target: repo.install_target, note: repo.note || '', plugin_guid: repo.plugin_guid, local_product_name: repo.local_product_name || '', local_original_filename: repo.local_original_filename || '', download_threads: repo.download_threads || (repo.provider === 'github' ? 16 : 1), api_proxy_mode: repo.api_proxy_mode, api_proxy_url: repo.api_proxy_url, release_proxy_mode: repo.release_proxy_mode, release_proxy_url: repo.release_proxy_url, github_token: '', gitee_token: '', official_api_only: true, enabled: repo.enabled } : { id: undefined, name: '', group: '', provider: 'gitee', url: '', channel: 'stable', asset_rules: [], install_target: 'BepInEx/plugins', note: '', plugin_guid: '', local_product_name: '', local_original_filename: '', download_threads: 1, api_proxy_mode: 'direct', api_proxy_url: '', release_proxy_mode: 'direct', release_proxy_url: '', github_token: '', gitee_token: '', official_api_only: true, enabled: true }); showRepoEditor.value = true }
async function saveRepo() {
  if (!repoForm.asset_rules.length || repoForm.asset_rules.some((rule) => !rule.value.trim())) {
    showError(t('请创建至少一个有效的资产规则'))
    return
  }
  let refreshError = ''
  try {
    await run('正在保存仓库', async () => {
      const savedConfig = await api.saveRepository(repoForm)
      const savedRepo = savedConfig.repositories.find((repo) => repo.id === repoForm.id)
        ?? savedConfig.repositories.find((repo) => repo.name === repoForm.name && repo.url === repoForm.url && repo.provider === repoForm.provider)
      if (!savedRepo) throw new Error('保存后找不到仓库配置')
      dashboard.value!.config = savedConfig
      showRepoEditor.value = false
      if (!await refreshRepo(savedRepo, false)) refreshError = savedRepo.last_error || `${localizedRepositoryName(savedRepo)}${t('刷新失败')}`
      dashboard.value = await api.dashboard()
    })
  } catch {
    return
  }
  if (refreshError) showError(refreshError)
  else {
    message.value = t('仓库已保存并刷新')
    scheduleToastClose()
  }
}
function providerChanged() { repoForm.download_threads = repoForm.provider === 'github' ? 16 : 1; repoForm.api_proxy_url = ''; repoForm.release_proxy_url = '' }
async function deleteRepo(repo: Repository) { if (repo.builtin || !confirm(`${t('删除仓库')} “${localizedRepositoryName(repo)}”?`)) return; await run('正在删除仓库', async () => { await api.deleteRepository(repo.id); await loadDashboard() }) }
function addRule() { repoForm.asset_rules.push({ type: 'exact', value: '' }) }
function removeRule(index: number) { repoForm.asset_rules.splice(index, 1) }
async function exportRepositories() {
  showExport.value = true
}
async function continueExportRepositories() { const path = await api.chooseExportFile(); if (!path || Array.isArray(path)) return; await run('正在导出仓库 TOML', () => api.exportFile(path, null, true)); showExport.value = false }
async function copyRepositories() { await run('正在复制仓库 TOML', () => api.copyRepositories(null, true)); showExport.value = false }
async function importFromClipboard() { importText.value = await api.readClipboard(); await previewImport() }
async function importFromFile() { const path = await api.chooseImportFile(); if (!path || Array.isArray(path)) return; importText.value = await api.readTextFile(path); await previewImport() }
async function previewImport() { importPreview.value = await run('正在检查仓库 TOML', () => api.previewImport(importText.value)) }
async function applyImport(conflict: string) {
  if (conflict === 'overwrite' && !confirm(t('本地的仓库会直接覆盖成粘贴的内容，并且无法恢复，是否确定？'))) return
  await run('正在导入仓库配置', async () => { await api.importRepositories(importText.value, conflict); showImport.value = false; await loadDashboard() })
}
function resetRepositorySourceEditor() {
  repositorySourceUrl.value = ''
  repositorySourceNickname.value = ''
  editingRepositorySourceId.value = null
  repositorySourceEditorOpen.value = false
}
function openRepositorySourceManager() {
  repositorySourceDraft.value = (config.value?.repository_sources ?? []).map((source) => ({ ...source }))
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
  editingRepositorySourceId.value = null
  repositorySourceEditorOpen.value = true
}
function editRepositorySource(source: RepositorySource) {
  editingRepositorySourceId.value = source.id
  repositorySourceNickname.value = source.nickname
  repositorySourceUrl.value = source.url
  repositorySourceEditorOpen.value = true
}
function applyRepositorySourceDraft() {
  const sourceUrl = repositorySourceUrl.value.trim()
  const nickname = repositorySourceNickname.value.trim()
  if (!sourceUrl || !nickname) return
  const duplicate = repositorySourceDraft.value.some((source) => source.id !== editingRepositorySourceId.value && source.url.trim().toLowerCase() === sourceUrl.toLowerCase())
  if (duplicate) {
    showError(t('仓库源地址已存在'))
    return
  }
  if (editingRepositorySourceId.value) {
    const source = repositorySourceDraft.value.find((item) => item.id === editingRepositorySourceId.value)
    if (source && !source.builtin) {
      source.nickname = nickname
      source.url = sourceUrl
    }
  } else {
    repositorySourceDraft.value.push({
      id: `draft-${crypto.randomUUID()}`,
      nickname,
      url: sourceUrl,
      builtin: false,
    })
  }
  resetRepositorySourceEditor()
}
function removeRepositorySource(source: RepositorySource) {
  if (source.builtin || !confirm(`${t('删除仓库源')} “${source.nickname}”?`)) return
  repositorySourceDraft.value = repositorySourceDraft.value.filter((item) => item.id !== source.id)
  if (editingRepositorySourceId.value === source.id) resetRepositorySourceEditor()
}
function startSourceDrag(id: string) { draggingSourceId.value = id }
function dropSource(id: string) {
  if (!draggingSourceId.value || draggingSourceId.value === id) return
  const sources = [...repositorySourceDraft.value]
  const from = sources.findIndex((source) => source.id === draggingSourceId.value)
  const to = sources.findIndex((source) => source.id === id)
  if (from < 0 || to < 0 || sources[to].builtin) {
    draggingSourceId.value = null
    return
  }
  const [source] = sources.splice(from, 1)
  sources.splice(to, 0, source)
  repositorySourceDraft.value = sources
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
  const normalizedUrls = draft.map((source) => source.url.toLowerCase())
  if (new Set(normalizedUrls).size !== normalizedUrls.length) {
    showError(t('仓库源地址已存在'))
    return
  }
  try {
    await run('正在保存仓库源', async () => {
      const sourceConfig = await api.saveRepositorySources(draft)
      const savedDashboard = await api.dashboard()
      const savedSourceDashboard = { ...savedDashboard, config: sourceConfig }
      dashboard.value = savedSourceDashboard
      const sourceResults = await Promise.all(sourceConfig.repository_sources.map(async (source) => ({
        source,
        result: await refreshSource(source.id, { updateDashboard: false, notify: false }),
      })))
      const failures = sourceResults
        .filter((entry) => !entry.result.ok)
        .map((entry) => `${entry.source.nickname}: ${entry.result.error || t('刷新失败')}`)
      const releaseFailures = sourceResults
        .reduce((total, entry) => total + entry.result.releaseFailures, 0)
      if (releaseFailures) failures.push(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
      if (failures.length) throw new Error(failures.join('；'))
      const refreshedDashboard = await api.dashboard()
      dashboard.value = refreshedDashboard
      closeRepositorySourceManager()
      return refreshedDashboard
    })
  } catch {
    // run() has already displayed the error; keep the draft open for retry.
  }
}
function sourceRefreshing(sourceId: string) { return !!sourceLoading[sourceId] }
function sourceBusy(sourceId: string) { return !!sourceLoading[sourceId] || !!sourceRefreshingManual[sourceId] }
function toggleSource(sourceId: string) { sourceCollapsed[sourceId] = !sourceCollapsed[sourceId] }
function toggleModDetails(modId: string) { modDetailsExpanded[modId] = !modDetailsExpanded[modId] }
function repositoryGroupKey(sourceId: string, group: string) { return `${sourceId}:${group}` }
function toggleRepositoryGroup(sourceId: string, group: string) {
  if (!group) return
  const key = repositoryGroupKey(sourceId, group)
  repositoryGroupCollapsed[key] = !repositoryGroupCollapsed[key]
}
async function refreshSource(sourceId: string, options: SourceRefreshOptions = {}): Promise<SourceRefreshResult> {
  const updateDashboard = options.updateDashboard ?? true
  const notify = options.notify ?? true
  sourceRefreshingManual[sourceId] = true
  sourceLoading[sourceId] = true
  if (sourceId === 'azhe') { authoritativeSourceLoading.value = true; authoritativeSourceFailed.value = false }
  try {
    const source = config.value?.repository_sources.find((item) => item.id === sourceId)
    if (!source) throw new Error(t('仓库源不存在'))
    await api.importRepositorySource(source.id, source.url)
    await refreshDashboardSerialized()
    sourceLoading[sourceId] = false
    if (sourceId === 'azhe') authoritativeSourceLoading.value = false
    const releaseFailures = await refreshSourceReleases(sourceId, false)
    if (updateDashboard) dashboard.value = await api.dashboard()
    if (releaseFailures && notify) showError(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
    return { ok: true, releaseFailures }
  } catch (reason) {
    const detail = reason instanceof Error ? reason.message : String(reason)
    if (sourceId === 'azhe') {
      authoritativeSourceFailed.value = true
      authoritativeSourceLoading.value = false
    }
    if (notify) showError(detail)
    return { ok: false, releaseFailures: 0, error: detail }
  } finally {
    sourceRefreshingManual[sourceId] = false
    sourceLoading[sourceId] = false
    if (sourceId === 'azhe') authoritativeSourceLoading.value = false
  }
}
function editModNote(mod: import('./types').InstalledMod) { editingMod.value = mod; modNote.value = mod.note ?? '' }
async function saveModNote() { if (!editingMod.value) return; await run('正在保存 MOD 备注', async () => { await api.saveModNote(editingMod.value!.dll_file, modNote.value); editingMod.value = null; await loadDashboard() }) }
async function uninstallMod(mod: import('./types').InstalledMod) { if (!confirm(`${t('确定删除 MOD')} ${mod.name}?`)) return; await run(`正在删除 ${mod.name}`, () => api.uninstallLocalMod(mod.dll_file)); await loadDashboard() }
async function installLocalMod() { const path = await api.chooseModFile(); if (!path || Array.isArray(path)) return; await run('正在安装本地 MOD', () => api.installLocalMod(path)); await loadDashboard() }
async function rescanInstalledMods() {
  await run('正在重新扫描', async () => {
    dashboard.value = await api.dashboard()
    return refreshMappedRepositoryUpdates()
  })
}
let kitchenPoll: number | undefined
onMounted(async () => {
  appVersion.value = await getVersion()
  document.title = `OC2 Mod Manager v${appVersion.value}`
  await getCurrentWindow().setTitle(document.title)
  if (!localStorage.getItem('oc2-locale')) {
    const installLanguage = await api.installLanguage()
    if (installLanguage) setLocale(installLanguage)
  }
  await startupChecks()
  stopProgress.value = await watchDownloadProgress((value) => {
    if (downloadCancelRequested.value) return
    progress.value = {
      label: value.label,
      percent: value.total ? Math.min(100, value.downloaded / value.total * 100) : 0,
      speed: value.finished ? '' : `${formatBytes(value.speed_bytes)}/s`,
      status: value.finished ? t('下载完成，正在处理') : '',
    }
  })
  kitchenPoll = window.setInterval(async () => {
    if (busy.value || repositoryRefreshCount > 0) return
    const revision = dashboardRevision
    try {
      const next = await api.dashboard()
      if (revision === dashboardRevision && repositoryRefreshCount === 0) dashboard.value = next
    } catch { /* 状态轮询失败时保留上一状态 */ }
  }, 1000)
})
onUnmounted(() => { stopProgress.value?.(); if (kitchenPoll) window.clearInterval(kitchenPoll); if (toastTimer) window.clearTimeout(toastTimer); if (rustLogTimer) window.clearInterval(rustLogTimer) })
</script>

<template>
  <div class="app-shell" :class="{ light: !dark }">
    <div v-if="startupUpdateChecking || updateCheckFailed" class="startup-overlay">
      <div v-if="startupUpdateChecking" class="startup-card">
        <div class="startup-spinner"></div>
        <h2>{{ t('正在检查安装器更新') }}</h2>
        <p>{{ t('请耐心等待') }}</p>
      </div>
      <div v-else class="startup-card startup-failed">
        <div class="startup-exclamation">!</div>
        <h2>{{ t('检查更新失败，请重新启动MOD管理器。') }}</h2>
        <button class="primary-button" @click="api.exitApp()">{{ t('确定') }}</button>
      </div>
    </div>
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark"><img :src="appIcon" alt="" /></div>
        <div><strong>OC2 MOD MANAGER</strong></div>
      </div>
      <div class="sidebar-section-label">{{ t('工作区') }}</div>
      <nav class="nav-list"><button v-for="item in navItems" :key="item.id" class="nav-item"
          :class="{ active: activeView === item.id }" @click="activeView = item.id">
          <component :is="item.icon" :size="18" stroke-width="1.8" /><span><b>{{ t(item.label) }}</b><small>{{
            t(item.hint) }}</small></span>
          <ChevronRight :size="14" class="nav-chevron" />
        </button></nav>
      <div class="sidebar-foot">
        <div class="side-health"><span class="pulse"
            :class="game?.kitchen_running ? 'danger-bg' : 'success-bg'"></span>{{ game?.kitchen_running ?
              t('厨房正在运行') : t('厨房已关闭') }}</div><span class="version-tag">{{ appVersion ? `v${appVersion}` : '' }}</span>
      </div>
    </aside>
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
          </select><button class="icon-button" :title="t('重新检测游戏目录')" :disabled="!!busy" @click="detectGame">
            <RefreshCw :size="17" />
          </button><button class="icon-button" :title="t('切换主题')" @click="dark = !dark">
            <Settings2 :size="17" />
          </button>
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
          <div v-if="progress.label" class="download-strip">
            <div class="download-meta"><span>{{ progress.label }}<small v-if="progress.status"> · {{ progress.status
                }}</small></span><span>{{ progress.percent.toFixed(0) }}%<template v-if="progress.speed"> ·
                {{ progress.speed }}</template></span></div>
            <div class="download-progress-row">
              <div class="progress"><i :style="{ width: `${progress.percent}%` }"></i></div><button
                class="secondary-button download-cancel" @click="cancelDownload">{{ t('取消下载') }}</button>
            </div>
          </div>
        </Transition>
      </div>
      <div class="main-scroll">
        <section v-if="activeView === 'overview'" class="view-content">
        <div class="hero-row">
          <div>
            <div class="status-line"><span class="status-dot"
                :class="game?.game_exists ? 'success-bg' : 'danger-bg'"></span><span>{{
                  game?.game_exists ? t('已识别厨房目录') : t('等待选择厨房目录') }}</span></div>
            <h2>{{ t('把 MOD 管理变成') }}<br /><em>{{ t('一次点击的事。') }}</em></h2>
            <p class="lead">{{ t('统一管理 BepInEx、Release 仓库和本地插件，安装过程会自动保护正在运行的厨房。') }}</p>
            <div class="path-row">
              <FolderOpen :size="16" /><span>{{ game?.path || t('尚未找到 Overcooked2.exe') }}</span>
              <button class="text-button" @click="openDirectory" :disabled="!game?.path">
                <ExternalLink :size="13" />{{ t('打开目录') }}
              </button>
              <button class="text-button" @click="chooseGameDirectory" :disabled="!!busy">
                <Pencil :size="13" />{{ t('修改目录') }}
              </button>
              <button class="text-button" @click="restoreDefaultGameDirectory"
                :disabled="!!busy || !!config?.game.auto_detect">
                <RotateCcw :size="13" />{{ t('恢复默认') }}
              </button>
            </div>
          </div>
          <div class="hero-orbit"><img :src="appIcon" alt="" /></div>
        </div>
        <div class="metric-grid">
          <div class="metric" @click="activeView = 'framework'"><span class="metric-label">BepInEx</span><strong>{{
            game?.bepinex.version ||
            (game?.bepinex.installed ? t('版本未知') : t('未安装')) }}</strong><span class="metric-note"
              :class="game?.bepinex.complete ? 'good' : ''">
              <Check v-if="game?.bepinex.complete" :size="13" /> {{ game?.bepinex.complete ? t('完整可用') :
                t('需要安装') }}
            </span></div>
          <div class="metric" @click="activeView = 'installed'"><span class="metric-label">{{ t('已安装 MOD')
              }}</span><strong class="metric-pair"><span class="metric-primary">{{ updateAvailableModCount
                }}</span><span class="metric-divider">/</span><span class="metric-secondary">{{ installedMods.length
                }}</span></strong><span class="metric-note">{{ `${t('可更新')} / ${t('已安装 MOD')}` }}</span></div>
          <div class="metric" @click="activeView = 'repositories'"><span class="metric-label">{{ t('MOD仓库')
              }}</span><strong class="metric-pair"><span class="metric-primary">{{ installableRepositoryCount
                }}</span><span class="metric-divider">/</span><span class="metric-secondary"
                :class="{ 'metric-ready': allRepositoryRefreshComplete, 'metric-pending': repos.length > 0 && !allRepositoryRefreshComplete }">{{
                  repos.length }}</span></strong><span class="metric-note">{{ `${t('可安装仓库')} / ${t('总仓库')}`
                }}</span></div><button class="metric metric-button"
            :disabled="gameBlocked || !game?.game_exists || !!busy" @click="toggleWinhttp"><span class="metric-label">{{
              t('启动状态') }}</span><strong>{{ statusLabel(currentWinhttp) }}</strong><span class="metric-note">{{
                game?.kitchen_running ? t('操作已锁定') : currentWinhttp === 'conflict' ?
                  t('点击清理冲突并启用 MOD') : t('点击切换有无 MOD') }}</span></button>
        </div>
        <div class="section-heading">
          <div>
            <h3>{{ t('厨房控制台') }}</h3>
          </div><button class="ghost-button" @click="detectGame">
            <RefreshCw :size="15" />{{ t('重新检测') }}
          </button>
        </div>
        <div class="action-grid"><button class="action-card primary kitchen-launch-card"
            :disabled="gameBlocked || !game?.game_exists || !!busy" @click="launchNormal"><span
              class="action-icon">▶</span><span><b>{{ t('启动厨房') }}</b><small>{{ t('通过 Steam 启动游戏')
                }}</small></span><span>›</span></button></div>
      </section>
      <section v-else-if="activeView === 'framework'" class="view-content">
        <div class="page-intro">
          <div>
            <p>{{ t('安装和维护框架文件，所有变更都会在厨房关闭时执行。') }}</p>
          </div>
          <div class="large-status" :class="game?.bepinex.complete ? 'good' : 'warn'">
            <ShieldCheck :size="22" /><span>{{ game?.bepinex.complete ? t('完整') : game?.bepinex.installed ?
              t('不完整') : t('未安装') }}</span>
          </div>
        </div>
        <div class="panel framework-panel">
          <div class="framework-status">
            <div class="framework-icon">
              <PackageCheck :size="32" />
            </div>
            <div>
              <h3>{{ game?.bepinex.version || t('未检测到版本') }}</h3>
              <p>{{ game?.bepinex.version_source || t('未读取到 BepInEx 核心 DLL 元数据') }}</p>
            </div><span class="badge" :class="game?.bepinex.complete ? 'success' : 'warning'">{{
              game?.bepinex.complete ? t('READY') : t('ACTION REQUIRED') }}</span>
          </div>
          <div class="divider"></div>
          <div class="button-row"><button class="primary-button" :disabled="gameBlocked || !!busy"
              @click="openBepInExInstall(false)">
              <Download :size="16" />{{ t('安装 BepInEx') }}
            </button><button class="secondary-button" :disabled="gameBlocked || !!busy"
              @click="openBepInExInstall(true)">
              <RotateCcw :size="16" />{{ t('重新安装') }}
            </button><button class="danger-button" :disabled="gameBlocked || !game?.bepinex.installed || !!busy"
              @click="uninstallBepInEx">
              <Trash2 :size="16" />{{ t('卸载') }}
            </button></div>
          <p v-if="gameBlocked" class="guard-note">
            <AlertTriangle :size="15" />{{ t('厨房正在运行，文件操作已禁用。') }}
          </p>
        </div>
        <div class="section-heading compact">
          <div>
            <h3>{{ t('框架备份') }}</h3>
          </div>
          <div class="button-row"><button class="ghost-button" :disabled="!!busy" @click="openBackupDirectory">
              <FolderOpen :size="15" />{{ t('打开备份目录') }}
            </button><button class="ghost-button" :disabled="gameBlocked || !!busy" @click="backupBepInEx">
              <HardDriveDownload :size="15" />{{ t('立即备份') }}
            </button></div>
        </div>
        <div class="backup-list">
          <div v-for="backup in (game?.bepinex.backups || [])" :key="backup" class="backup-row">
            <div class="backup-icon">
              <HardDriveDownload :size="16" />
            </div>
            <div><b>{{ backup.split(/[\\/]/).pop() }}</b><span>{{ backup }}</span></div>
            <div class="button-row"><button class="secondary-button small-button" :disabled="gameBlocked || !!busy"
                @click="restoreBackup(backup)">
                <RotateCcw :size="14" />{{ t('恢复') }}
              </button><button class="danger-button small-button" :disabled="gameBlocked || !!busy"
                @click="deleteBackup(backup)">
                <Trash2 :size="14" />{{ t('删除') }}
              </button></div>
          </div>
          <div v-if="!(game?.bepinex.backups?.length)" class="empty-state">{{ t('还没有框架备份') }}</div>
        </div>
      </section>
      <section v-else-if="activeView === 'installed'" class="view-content">
        <div class="page-intro">
          <div>
            <p>{{ t('这里只显示当前 BepInEx/plugins 中已经安装的插件。') }}</p>
          </div>
          <div class="button-row"><button class="secondary-button" :disabled="!!busy" @click="openModDirectory">
              <FolderOpen :size="16" />{{ t('打开模组目录') }}
            </button><button class="secondary-button" :disabled="gameBlocked || !!busy" @click="installLocalMod">
              <Plus :size="16" />{{ t('手动安装模组') }}
            </button><button class="secondary-button" :disabled="!!busy" @click="rescanInstalledMods">
              <RefreshCw :size="16" />{{ t('重新扫描') }}
            </button><button class="primary-button" :disabled="!mappedReposAvailable || !!busy"
              @click="checkModUpdates">
              <RefreshCw :size="16" />{{ t('检查模组更新') }}
            </button></div>
        </div>
        <div class="panel table-panel">
          <div class="table-head"><span>{{ t('序号') }}</span><span>{{ t('模组') }}</span><span>{{ t('操作') }}</span></div>
          <div v-for="(mod, index) in installedMods" :key="mod.dll_file" class="table-row">
            <span class="mod-index">{{ index + 1 }}</span>
            <div class="mod-cell">
              <div class="mod-avatar">{{ mod.name.slice(0, 2).toUpperCase() }}</div>
              <div class="mod-details">
                <div class="mod-heading"><button class="mod-details-toggle" :title="modDetailsExpanded[mod.dll_file] ? t('折叠') : t('展开')"
                    @click="toggleModDetails(mod.dll_file)"><ChevronRight :class="{ 'mod-details-expanded': modDetailsExpanded[mod.dll_file] }" :size="15" /></button><b>{{ mod.name }}</b><span class="mod-version mono">{{ mod.version || t('版本未知') }}</span><span class="badge"
                    :class="mod.status === 'unbound' ? 'muted' : isModRepositoryRefreshing(mod) || installedModsPendingCheck ? 'warning' : mod.load_warning ? 'danger' : mod.update_available ? 'warning' : 'success'">{{
                      mod.status === 'unbound' ? t('未绑定') : isModRepositoryRefreshing(mod) ? t('检查中') :
                        installedModsPendingCheck ? t('待检查') : mod.load_warning || (mod.update_available ? t('可更新') : t('最新版'))
                    }}</span></div>
                <small v-if="mod.title"><strong>{{ t('DLL.程序集标题') }}</strong><span>{{ mod.title }}</span></small>
                <small><strong>{{ t('DLL.产品名称') }}</strong><span>{{ mod.product_name || t('未知') }}</span></small>
                <div v-if="modDetailsExpanded[mod.dll_file]" class="mod-detail-table">
                  <small v-if="mod.description"><strong>{{ t('DLL.文件描述') }}</strong><span>{{ mod.description }}</span></small>
                  <small><strong>{{ t('DLL.原始文件名') }}</strong><span>{{ mod.original_filename || t('未识别') }}</span></small>
                  <small v-if="mod.company"><strong>{{ t('DLL.开发者信息') }}</strong><span>{{ mod.company }}</span></small>
                  <small v-if="mod.copyright"><strong>{{ t('DLL.版权信息') }}</strong><span>{{ mod.copyright }}</span></small>
                  <small><strong>{{ t('BepInEx.GUID') }}</strong><span class="mod-guid">{{ mod.guid || t('未识别') }}</span></small>
                  <small><strong>{{ t('BepInEx.插件路径') }}</strong><span class="mod-path">{{ mod.dll_file }}</span></small>
                  <small v-if="mod.note" class="mod-note"><strong>{{ t('备注') }}</strong><span>{{ mod.note
                      }}</span></small>
                </div>
              </div>
            </div>
            <div class="mod-controls">
              <span class="mod-actions"><button class="icon-button small" :title="t('编辑 MOD 备注')"
                  @click="editModNote(mod)">
                  <Pencil :size="14" />
                </button><button class="icon-button small danger-icon" :title="t('删除 MOD')"
                  :disabled="gameBlocked || !!busy" @click="uninstallMod(mod)">
                  <Trash2 :size="14" />
                </button></span>
            </div>
          </div>
          <div v-if="!installedMods.length" class="empty-state">{{ t('没有扫描到 BepInEx/plugins/**/*.dll') }}</div>
        </div>
      </section>
      <section v-else-if="activeView === 'repositories'" class="view-content">
        <div class="page-intro">
          <div>
            <p>{{ t('内置和自定义仓库均从 Gitee/GitHub Release 获取版本与资产。') }}</p>
          </div>
          <div class="button-row"><button class="secondary-button"
              :disabled="!!busy || !(config?.repository_sources || []).length" @click="refreshAllRepositories">
              <RefreshCw :size="16" />{{ t('刷新全部仓库源') }}
            </button><button class="primary-button" @click="editRepo()">
              <Plus :size="16" />{{ t('添加仓库') }}
            </button><button class="primary-button" @click="openRepositorySourceManager">
              <Plus :size="16" />{{ t('管理仓库源') }}
            </button></div>
        </div>
        <div class="toolbar">
          <div class="toolbar-group"><button class="secondary-button" @click="showImport = true">
              <Download :size="15" />{{ t('导入配置') }}
            </button><button class="secondary-button" :disabled="!repos.length" @click="exportRepositories">
              <Upload :size="15" />{{ t('导出配置') }}
            </button></div>
        </div>
        <div class="repository-list">
          <template v-for="group in sourceGroups" :key="group.source.id">
            <div class="source-section-heading"><button class="source-collapse-button"
                :title="sourceCollapsed[group.source.id] ? t('展开') : t('折叠')" @click="toggleSource(group.source.id)">
                <ChevronRight :size="18" :class="{ 'source-expanded': !sourceCollapsed[group.source.id] }" />
              </button>
              <div class="source-section-title">
                <h3>{{ group.source.nickname }}</h3><small v-if="group.source.url">{{ group.source.url }}</small>
              </div><button v-if="group.source.id !== 'unclassified'" class="icon-button small" :title="t('刷新仓库源')"
                :disabled="!!busy || sourceBusy(group.source.id) || (group.source.id === 'azhe' && authoritativeSourceLoading)"
                @click="refreshSource(group.source.id)">
                <RefreshCw :size="15"
                  :class="{ 'spin-icon': sourceRefreshing(group.source.id) || (group.source.id === 'azhe' && authoritativeSourceLoading) }" />
              </button>
            </div>
            <div v-if="sourceRefreshing(group.source.id) || (group.source.id === 'azhe' && authoritativeSourceLoading)"
              class="source-loading-overlay">
              <RefreshCw :size="42" class="spin-icon" />
            </div>
            <Transition name="source-collapse">
              <div
                v-if="!sourceCollapsed[group.source.id] && !sourceRefreshing(group.source.id) && !(group.source.id === 'azhe' && authoritativeSourceLoading)"
                class="source-repository-panel">
                <div v-for="repoGroup in group.groups" :key="`${group.source.id}:${repoGroup.key || 'ungrouped'}`"
                  class="repository-group"><button v-if="repoGroup.label" class="repository-group-heading"
                    :title="repositoryGroupCollapsed[repositoryGroupKey(group.source.id, repoGroup.key)] ? t('展开分组') : t('折叠分组')"
                    @click="toggleRepositoryGroup(group.source.id, repoGroup.key)">
                    <ChevronRight :size="15"
                      :class="{ 'source-expanded': !repositoryGroupCollapsed[repositoryGroupKey(group.source.id, repoGroup.key)] }" />
                    <span>{{ repoGroup.label }}</span><span class="badge muted">{{ repoGroup.repositories.length
                      }}</span>
                  </button>
                  <Transition name="repository-group-collapse">
                    <div
                      v-if="!repoGroup.label || !repositoryGroupCollapsed[repositoryGroupKey(group.source.id, repoGroup.key)]"
                      class="repository-group-items">
                      <div v-for="repo in repoGroup.repositories" :key="repo.id"
                        :class="['repository-row', { 'repo-refreshing': repositoryRefreshing[repo.id] }]">
                        <div class="repo-logo" :class="repo.provider">
                          <svg class="provider-icon" :aria-label="repo.provider === 'github' ? 'GitHub' : 'Gitee'"
                            role="img" :viewBox="repo.provider === 'github' ? '0 0 19 19' : '0 0 24 24'">
                            <use
                              :href="repo.provider === 'github' ? '/icons.svg#github-icon' : '/icons.svg#gitee-icon'">
                            </use>
                          </svg>
                        </div>
                        <div class="repo-info">
                          <div class="repo-title"><b>{{ localizedRepositoryName(repo) }}</b><span v-if="repo.builtin"
                              class="badge muted">{{ t('内置') }}</span><span class="badge"
                              :class="repo.enabled ? 'success' : 'muted'">{{ repo.enabled ? t('启用') : t('停用')
                              }}</span><span v-if="repositoryInstallStatus(repo)" class="badge"
                              :class="repositoryInstallStatus(repo) === '可更新' ? 'warning' : 'success'">{{
                                t(repositoryInstallStatus(repo)) }}</span>
                          </div><span>{{ repo.url }}</span><small>{{ t(repo.channel === 'stable' ? '稳定版' : repo.channel
                            === 'beta' ? '测试版' : '全部版本') }} · {{ repo.install_target || t('按压缩包结构安装') }} · {{
                              repo.asset_rules.map((rule) => `${assetRuleLabel(rule.type)}: ${rule.value}`).join(', ')}}</small>
                          <div v-if="localizedRepositoryNote(repo) && !repositoryNoteIsLong(repo)"
                            class="repo-note markdown-body" v-html="renderMarkdown(localizedRepositoryNote(repo))">
                          </div><button v-else-if="localizedRepositoryNote(repo)" class="repo-note-button"
                            @click="showRepositoryNote = repo">
                            <FileText :size="13" />{{ t('查看仓库简介') }}
                          </button>
                        </div>
                        <div class="repo-controls">
                          <div class="repo-release"><b>{{ latest(repo)?.version || '—' }}</b><span>{{ latest(repo) ?
                            formatDate(latest(repo)?.published_at) : t('待刷新') }}</span></div>
                          <button class="primary-button small-button" :title="t('下载并安装')"
                            :disabled="gameBlocked || !installReleases(repo).length || !!busy || !!repositoryRefreshing[repo.id]"
                            @click="installRepo(repo)">
                            <Download :size="14" />{{ t('下载并安装') }}
                          </button>
                          <div class="row-actions">
                            <button class="icon-button small" :title="t('刷新 Release')"
                              :disabled="!!busy || !!repositoryRefreshing[repo.id]" @click="refreshRepo(repo)">
                              <RefreshCw :size="14" :class="{ 'spin-icon': repositoryRefreshing[repo.id] }" />
                            </button><button class="icon-button small" :title="t('查看 Release 日志')"
                              :disabled="!latest(repo)" @click="showRelease = repo">
                              <Search :size="14" />
                            </button><button class="icon-button small" :title="t('打开发布页')"
                              :disabled="!!repositoryRefreshing[repo.id]" @click="api.openReleasePage(repo.url)">
                              <ExternalLink :size="14" />
                            </button><button class="icon-button small" :title="t('编辑仓库')" @click="editRepo(repo)">
                              <Pencil :size="14" />
                            </button><button class="icon-button small danger-icon" :title="t('删除仓库')"
                              :disabled="repo.builtin" @click="deleteRepo(repo)">
                              <Trash2 :size="14" />
                            </button>
                          </div>
                        </div>
                      </div>
                    </div>
                  </Transition>
                </div>
              </div>
            </Transition>
          </template>
          <div v-if="!repos.length" class="empty-state">{{ t('暂无仓库配置') }}</div>
        </div>
      </section>
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
            <div v-for="source in repositorySourceDraft" :key="source.id" class="source-manager-row"
              :draggable="!source.builtin" @dragstart="startSourceDrag(source.id)" @dragover.prevent
              @drop="dropSource(source.id)">
              <GripVertical :size="17" class="source-drag-handle" />
              <div><b>{{ source.nickname }}</b><small>{{ source.url }}</small></div>
              <span v-if="source.builtin" class="badge muted">{{ t('内置') }}</span>
              <button v-if="!source.builtin" class="icon-button small" :title="t('编辑仓库源')"
                @click="editRepositorySource(source)">
                <Pencil :size="14" />
              </button>
              <button v-if="!source.builtin" class="icon-button small danger-icon" :title="t('删除仓库源')"
                @click="removeRepositorySource(source)">
                <Trash2 :size="14" />
              </button>
            </div>
          </div>
          <div class="source-manager-toolbar"><button class="secondary-button"
              :disabled="!!busy || repositorySourceEditorOpen" @click="startAddingRepositorySource">
              <Plus :size="15" />{{ t('添加仓库源') }}
            </button></div>
          <div v-if="repositorySourceEditorOpen" class="source-editor-fields"><label><span>{{ t('仓库源昵称') }}</span><input
                v-model="repositorySourceNickname" :placeholder="t('例如：我的仓库源')" /></label><label><span>{{ t('仓库源地址')
                }}</span><input v-model="repositorySourceUrl"
                placeholder="https://example.com/repositories.oc2repo.toml" /></label>
            <div class="source-editor-actions"><button class="ghost-button" @click="resetRepositorySourceEditor">{{
              t('取消')
                }}</button><button class="secondary-button"
                :disabled="!repositorySourceNickname.trim() || !repositorySourceUrl.trim()"
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
        <section v-if="activeView === 'sponsor'" class="view-content sponsor-view">
        <div class="sponsor-panel">
          <h2>{{ t('觉得好用的话，请我喝杯咖啡吧！') }}</h2><img :src="sponsorImage" alt="赞助二维码" />
          <p>{{ t('感谢你的支持。') }}</p>
          <div class="button-row sponsor-actions"><button class="secondary-button" :disabled="!!busy"
              @click="openDeveloperConsole">
              <ExternalLink :size="15" />{{ t('打开开发者控制台') }}
            </button><button class="secondary-button" :disabled="!!busy" @click="resetUi">
              <RotateCcw :size="15" />{{ t('重置到最新 UI') }}
            </button><button class="secondary-button" @click="openRustLog">
              <FileText :size="15" />{{ t('打开 Rust 日志') }}
            </button></div>
        </div>
        </section>
      </div>
    </main>
    <button v-if="activeView === 'overview'" class="update-fab" :disabled="!!busy"
      @click="appUpdate ? installAppUpdate() : checkAppUpdate()">
      <Download :size="15" /><span>{{ appUpdate ? `${t('安装')} ${appUpdate.version}` : t('检查程序更新') }}</span>
    </button>
    <button v-if="!game?.game_exists" class="path-fab" :disabled="!!busy" @click="chooseGameDirectory">
      <FolderOpen :size="15" /><span>{{ t('选择厨房目录') }}</span>
    </button>
    <div v-if="showRepoEditor" class="modal-backdrop" @click.self="showRepoEditor = false">
      <div class="modal">
        <div class="modal-head">
          <div>
            <h3>{{ t(repoForm.id ? '编辑仓库' : '添加仓库') }}</h3>
          </div><button class="icon-button" @click="showRepoEditor = false">
            <X :size="17" />
          </button>
        </div>
        <div class="form-grid"><label><span>{{ t('仓库名称') }}</span><input v-model="repoForm.name"
              :placeholder="t('例如：我的 MOD 正式版')" /></label><label><span>{{ t('仓库分组') }}</span><input
              v-model="repoForm.group" :placeholder="t('例如：前置必装')" /></label><label><span>{{ t('来源平台') }}</span><select
              v-model="repoForm.provider" @change="providerChanged">
              <option value="gitee">Gitee</option>
              <option value="github">GitHub</option>
            </select></label><label class="wide"><span>{{ t('版本地址') }}</span><input v-model="repoForm.url"
              placeholder="https://github.com/owner/repo/releases" /></label><label><span>{{ t('版本频道') }}</span><select
              v-model="repoForm.channel">
              <option value="stable">{{ t('稳定版') }}</option>
              <option value="beta">{{ t('测试版') }}</option>
              <option value="all">{{ t('全部版本') }}</option>
            </select></label><label><span>{{ t('安装目录') }}</span><input v-model="repoForm.install_target"
              :placeholder="t('留空按 ZIP 结构安装')" /></label><label><span>{{ t('下载线程数') }}</span><input
              v-model.number="repoForm.download_threads" type="number" min="1" max="32"
              :disabled="repoForm.provider === 'gitee'" /></label><label><span>{{ t('API 代理策略') }}</span><select
              v-model="repoForm.api_proxy_mode">
              <option value="direct">{{ t('直连') }}</option>
              <option value="auto">{{ t('自动选择') }}</option>
              <option value="fixed">{{ t('固定代理') }}</option>
            </select></label><label><span>API {{ t('代理地址') }}</span><input v-model="repoForm.api_proxy_url"
              placeholder="https://" /></label><label class="wide"><span>{{ t('API 直连 Token') }} ({{ repoForm.provider
                ===
                'gitee' ? 'Gitee' : 'GitHub' }})</span><input v-if="repoForm.provider === 'gitee'"
              v-model="repoForm.gitee_token" type="password" :placeholder="t('留空保持原设置')" /><input v-else
              v-model="repoForm.github_token" type="password" :placeholder="t('留空保持原设置')" /></label><label><span>{{
                t('Release 代理策略') }}</span><select v-model="repoForm.release_proxy_mode">
              <option value="direct">{{ t('直连') }}</option>
              <option value="auto">{{ t('自动选择') }}</option>
              <option value="fixed">{{ t('固定代理') }}</option>
            </select></label><label><span>Release {{ t('代理地址') }}</span><input v-model="repoForm.release_proxy_url"
              placeholder="https://" /></label></div>
        <div class="rules-heading identity-heading">
          <div><b>{{ t('本地 MOD 识别绑定') }}</b><small>{{ t('GUID、产品名称或原始文件名任一匹配即可绑定') }}</small></div>
        </div>
        <div class="form-grid identity-grid"><label><span>{{ t('BepInEx Plugin GUID') }}</span><input
              v-model="repoForm.plugin_guid" :placeholder="t('可选')" /></label><label><span>{{ t('DLL 产品名称')
              }}</span><input v-model="repoForm.local_product_name" :placeholder="t('可选')" /></label><label
            class="wide"><span>{{ t('DLL 原始文件名') }}</span><input v-model="repoForm.local_original_filename" :placeholder="t('可选')" /></label></div>
        <div class="rules-heading">
          <div><b>{{ t('Release 资产规则') }}</b><small>{{ t('支持精确文件名、通配符和正则表达式') }}</small></div><button
            class="ghost-button" @click="addRule">
            <Plus :size="14" />{{ t('添加规则') }}
          </button>
        </div>
        <div v-for="(rule, index) in repoForm.asset_rules" :key="index" class="rule-row"><select v-model="rule.type">
            <option value="exact">{{ t('精确') }}</option>
            <option value="glob">{{ t('通配符') }}</option>
            <option value="regex">{{ t('正则') }}</option>
          </select><input v-model="rule.value" /><button class="icon-button small danger-icon" :title="t('删除规则')"
            @click="removeRule(index)">
            <Trash2 :size="14" />
          </button></div><label class="check-control"><input v-model="repoForm.enabled" type="checkbox" />{{ t('启用此仓库')
          }}</label>
        <div class="rules-heading">
          <div><b>{{ t('仓库备注') }}</b><small>{{ t('可选，多行备注') }}</small></div>
        </div>
        <textarea v-model="repoForm.note" class="repository-note-input" rows="5" :placeholder="t('可选，多行备注')"></textarea>
        <div class="modal-actions"><button class="secondary-button" @click="showRepoEditor = false">{{ t('取消')
            }}</button><button class="primary-button"
            :disabled="!repoForm.name || !repoForm.url || !repoForm.asset_rules.length || repoForm.asset_rules.some((rule) => !rule.value.trim()) || !!busy"
            @click="saveRepo">
            <Save :size="16" />{{ t('保存仓库') }}
          </button></div>
      </div>
    </div>
    <div v-if="showRustLog" class="modal-backdrop" @click.self="closeRustLog">
      <div class="modal rust-log-modal">
        <div class="modal-head">
          <div>
            <h3>{{ t('Rust 运行日志') }}</h3><small class="rust-log-path">{{ rustLog.path }}</small>
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
                }}</span><small>{{ formatAssetSize(asset) }}</small></div>
          </div>
        </div>
      </div>
    </div>
    <div v-if="showRepositoryNote" class="modal-backdrop" @click.self="showRepositoryNote = null">
      <div class="modal repository-note-modal">
        <div class="modal-head">
          <div>
            <h3>{{ localizedRepositoryName(showRepositoryNote) }}</h3>
            <p class="muted-text">{{ t('仓库简介') }}</p>
          </div><button class="icon-button" :title="t('关闭')" @click="showRepositoryNote = null">
            <X :size="17" />
          </button>
        </div>
        <article class="markdown-body repository-note-content"
          v-html="renderMarkdown(localizedRepositoryNote(showRepositoryNote))"></article>
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
        <p class="muted-text">{{ t('产品名称：') }}{{ editingMod.product_name || t('未知') }} · {{ editingMod.dll_file }}</p>
        <textarea v-model="modNote" rows="6" :placeholder="t('例如：这是街机模组，包含房主工具、房间控制和快捷设置功能。')" />
        <div class="modal-actions"><button class="secondary-button" @click="editingMod = null">{{ t('取消')
            }}</button><button class="primary-button" :disabled="!!busy" @click="saveModNote">
            <Save :size="16" />{{ t('保存备注') }}
          </button></div>
      </div>
    </div>
  </div>
  <div v-if="showInstall" class="modal-backdrop" @click.self="closeInstallDialog">
    <div class="modal install-modal">
      <div class="modal-head">
        <div>
          <h3>{{ t('选择版本和文件') }}</h3>
        </div><button class="icon-button" @click="closeInstallDialog">
          <X :size="17" />
        </button>
      </div><label class="install-field"><span>{{ t('Release 版本') }}</span><select v-model="selectedInstallTag" @change="selectInstallRelease">
          <option v-for="release in installReleases(showInstall)" :key="release.tag_name" :value="release.tag_name">{{
            release.version || release.tag_name }} · {{ release.name }}
          </option>
        </select></label><label class="install-field"><span>{{ t('下载文件') }}</span><select
          v-model="selectedInstallAsset">
          <option v-for="asset in selectedInstallAssets(showInstall)" :key="asset.name" :value="asset.name">{{
            asset.name
            }} · {{ formatAssetSize(asset) }}</option>
        </select></label>
      <p class="muted-text">{{ t('仅显示符合当前仓库资产规则的文件。') }}</p>
      <div class="modal-actions"><button class="secondary-button" @click="closeInstallDialog">{{ t('取消')
          }}</button><button class="primary-button" :disabled="!selectedInstallAsset || !!busy" @click="confirmInstall">
          <Download :size="15" />{{ t('下载并安装') }}
        </button></div>
    </div>
  </div>
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

<style>
.metric {
  cursor: pointer;
}

.nav-item {
  position: relative;
}

  .download-strip {
  position: absolute;
  z-index: 12;
  top: 0;
  left: 50%;
  width: min(540px, calc(100% - 32px));
  margin: 0;
  padding: 11px 15px 13px;
  border: 1px solid var(--line-strong);
  border-top: 0;
  border-radius: 0 0 18px 18px;
  background: var(--surface-2);
  box-shadow: 0 14px 32px rgba(0, 0, 0, .28);
  transform: translateX(-50%);
}

.download-notch-enter-active,
.download-notch-leave-active {
  transition: transform .32s cubic-bezier(.22, .8, .25, 1), opacity .24s ease;
}

.download-notch-enter-from,
.download-notch-leave-to {
  opacity: 0;
  transform: translate(-50%, -110%);
}

.download-notch-enter-to,
.download-notch-leave-from {
  opacity: 1;
  transform: translate(-50%, 0);
}

.download-cancel {
  flex: 0 0 auto;
  white-space: nowrap;
}

.download-progress-row {
  display: flex;
  align-items: center;
  gap: 10px;
}

.download-progress-row .progress {
  flex: 1;
  min-width: 0;
}

.metric-button {
  width: 100%;
  text-align: left;
  font: inherit;
  background: transparent;
  color: inherit;
  border-top: 0;
  border-bottom: 0;
  border-left: 0;
  cursor: pointer;
}

.metric-button:disabled {
  cursor: not-allowed;
  opacity: .65;
}

.install-field {
  display: flex;
  flex-direction: column;
  gap: 7px;
  color: var(--muted);
  font-size: 11px;
  margin-top: 13px;
}

.install-field select {
  border: 1px solid var(--line);
  background: var(--surface-2);
  color: var(--text);
  padding: 10px 11px;
  font-size: 12px;
}

.bepinex-channel-modal {
  width: min(760px, 100%);
}

.bepinex-channel-modal .modal-head {
  margin-bottom: 18px;
  padding-bottom: 15px;
  border-bottom: 1px solid var(--line);
}

.bepinex-channel-modal .modal-head p {
  margin: 7px 0 0;
}

.bepinex-channel-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 12px;
}

.bepinex-channel-card {
  display: flex;
  align-items: flex-start;
  gap: 13px;
  min-width: 0;
  padding: 17px;
  border: 1px solid var(--line-strong);
  border-radius: 8px;
  background: var(--surface-2);
  color: var(--text);
  text-align: left;
  cursor: pointer;
  transition: border-color .18s ease, background .18s ease, transform .18s ease;
}

.bepinex-channel-card:hover:not(:disabled) {
  border-color: var(--accent);
  background: var(--surface-3);
  transform: translateY(-1px);
}

.bepinex-channel-card:disabled {
  cursor: wait;
  opacity: .58;
}

.bepinex-channel-icon {
  flex: 0 0 38px;
  width: 38px;
  height: 38px;
  display: grid;
  place-items: center;
  border-radius: 8px;
  background: var(--surface);
}

.bepinex-channel-icon.gitee {
  color: #c71d23;
}

.bepinex-channel-icon.github {
  color: var(--text);
}

.bepinex-channel-icon .provider-icon {
  width: 21px;
  height: 21px;
}

.bepinex-channel-copy {
  display: grid;
  gap: 5px;
  min-width: 0;
}

.bepinex-channel-copy b {
  font-size: 13px;
}

.bepinex-channel-copy small {
  color: var(--faint);
  font: 10px/1.45 Consolas, monospace;
  overflow-wrap: anywhere;
}

.bepinex-channel-copy em {
  color: var(--muted);
  font-size: 10px;
  font-style: normal;
}

@media (max-width: 640px) {
  .bepinex-channel-grid {
    grid-template-columns: 1fr;
  }
}

.repository-row {
  position: relative;
}

.repository-row.repo-refreshing {
  pointer-events: none;
}

.repository-row.repo-refreshing::before {
  content: '';
  position: absolute;
  inset: 0;
  z-index: 2;
  background: color-mix(in srgb, var(--surface) 78%, transparent);
}

.repository-row.repo-refreshing::after {
  content: '';
  position: absolute;
  z-index: 3;
  left: 50%;
  top: 50%;
  width: 28px;
  height: 28px;
  margin: -14px;
  border: 3px solid var(--line-strong);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: startup-spin .8s linear infinite;
}

.kitchen-warning {
  margin: 18px 0 -18px;
  padding: 10px 13px;
  border: 1px solid #755c32;
  background: rgba(74, 55, 25, .36);
  color: var(--warning);
  font-size: 12px;
}

.sponsor-panel {
  min-height: 520px;
  display: grid;
  place-items: center;
  align-content: center;
  gap: 18px;
  text-align: center;
}

.sponsor-panel h2 {
  font-size: 26px;
}

.sponsor-panel img {
  width: min(420px, 80vw);
  max-height: 520px;
  object-fit: contain;
  border: 1px solid var(--line);
  background: #fff;
}

.sponsor-panel p {
  color: var(--muted);
  font-size: 12px;
}

.sponsor-actions {
  justify-content: center;
  flex-wrap: wrap;
}

.rust-log-modal {
  width: min(980px, 100%);
}

.rust-log-path {
  display: block;
  max-width: 760px;
  margin-top: 7px;
  color: var(--faint);
  font: 10px Consolas, monospace;
  overflow-wrap: anywhere;
}

.rust-log-output {
  min-height: 360px;
  max-height: 58vh;
  margin: 0;
  padding: 13px;
  overflow: auto;
  border: 1px solid var(--line);
  background: #0b1114;
  color: #b9d8ce;
  font: 11px/1.55 Consolas, monospace;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}

.startup-overlay {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: grid;
  place-items: center;
  background: rgba(5, 9, 12, .86);
  backdrop-filter: blur(8px);
  cursor: wait;
}

.startup-card {
  min-width: 300px;
  padding: 34px;
  text-align: center;
  border: 1px solid var(--line-strong);
  background: var(--surface);
  box-shadow: var(--shadow);
  cursor: default;
}

.startup-card h2 {
  margin: 17px 0 7px;
  font-size: 19px;
}

.startup-card p {
  color: var(--muted);
  font-size: 12px;
}

.startup-spinner {
  width: 42px;
  height: 42px;
  margin: 0 auto;
  border: 3px solid var(--line);
  border-top-color: var(--accent);
  border-radius: 50%;
  animation: startup-spin .8s linear infinite;
}

.startup-exclamation {
  width: 42px;
  height: 42px;
  margin: 0 auto;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: var(--warning);
  color: #1b1510;
  font-size: 28px;
  font-weight: 800;
}

.startup-failed .primary-button {
  margin-top: 20px;
  min-width: 90px;
}

@keyframes startup-spin {
  to {
    transform: rotate(360deg);
  }
}
</style>
