<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from 'vue'
import { getVersion } from '@tauri-apps/api/app'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  AlertTriangle, Boxes, Check, CheckCircle2, ChevronRight, CloudDownload, Coffee, Copy, Download,
  ExternalLink, FolderOpen, Github, HardDriveDownload, LayoutDashboard,
  PackageCheck, Pencil, Plus, RefreshCw, RotateCcw, Save, Search, Settings2,
  ShieldCheck, Trash2, Upload, X,
} from 'lucide-vue-next'
import { api, watchDownloadProgress } from './services/tauri'
import type { Dashboard, Repository, RepositoryPatch, WinhttpState } from './types'
import { t, useLocaleTranslation, type Locale } from './i18n'
import sponsorImage from './assets/collectionCode.jpg'

type View = 'overview' | 'framework' | 'installed' | 'repositories' | 'sponsor'

const activeView = ref<View>('overview')
const dashboard = ref<Dashboard | null>(null)
const busy = ref('')
const message = ref('')
const error = ref('')
const progress = ref({ label: '', percent: 0, speed: '' })
const dark = ref(true)
const showRepoEditor = ref(false)
const showImport = ref(false)
const showExport = ref(false)
const showRelease = ref<Repository | null>(null)
const showInstall = ref<Repository | null>(null)
const selectedInstallTag = ref('')
const selectedInstallAsset = ref('')
const importText = ref('')
const importPreview = ref<{ valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] } | null>(null)
const repositoryRefreshing = reactive<Record<string, boolean>>({})
const appUpdate = ref<{ version: string; current_version: string } | null>(null)
const startupUpdateChecking = ref(true)
const updateCheckFailed = ref(false)
const stopProgress = ref<(() => void) | null>(null)
let toastTimer: number | undefined
const editingMod = ref<import('./types').InstalledMod | null>(null)
const modNote = ref('')
const appVersion = ref('')
const { locale, setLocale } = useLocaleTranslation()
const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁体中文' },
  { value: 'en', label: '英语' },
  { value: 'ko', label: '韩语' },
]
const repoForm = reactive<RepositoryPatch>({
  name: '', provider: 'gitee', url: '', channel: 'stable', asset_rules: [],
  install_target: 'BepInEx/plugins', plugin_guid: '', download_threads: 1, api_proxy_mode: 'direct', api_proxy_url: '', release_proxy_mode: 'direct', release_proxy_url: '', github_token: '', gitee_token: '', official_api_only: true, enabled: true,
})

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
const installedMods = computed(() => dashboard.value?.mods ?? [])
const mappedReposAvailable = computed(() => repos.value.some((repo) => repo.enabled && installedMods.value.some((mod) => mod.repository_id === repo.id)))
const updateCount = computed(() => dashboard.value?.updates.filter((item) => item.state === 'update_available').length ?? 0)
const gameBlocked = computed(() => game.value?.kitchen_running ?? false)
const currentWinhttp = computed<WinhttpState>(() => game.value?.winhttp_state ?? 'missing')
let repositoryRefreshCount = 0
let dashboardRevision = 0
const repositoryRefreshTasks = new Map<string, Promise<boolean>>()

function statusLabel(status: WinhttpState) { return t(({ modenabled: 'mod 已启用', moddisabled: 'mod 未启用', missing: '未找到 winhttp.dll', conflict: '文件状态冲突' } as Record<WinhttpState, string>)[status]) }
function formatBytes(value: number) { if (!value) return '0 B'; const units = ['B', 'KB', 'MB', 'GB']; const index = Math.min(Math.floor(Math.log(value) / Math.log(1024)), units.length - 1); return `${(value / 1024 ** index).toFixed(index ? 1 : 0)} ${units[index]}` }
function scheduleToastClose() { if (toastTimer) window.clearTimeout(toastTimer); toastTimer = window.setTimeout(() => { message.value = ''; error.value = '' }, 5000) }
function formatDate(value?: string) { if (!value) return t('未知时间'); const language = ({ 'zh-CN': 'zh-CN', 'zh-TW': 'zh-TW', en: 'en-US', ko: 'ko-KR' } as Record<Locale, string>)[locale.value]; return new Intl.DateTimeFormat(language, { year: 'numeric', month: 'short', day: 'numeric' }).format(new Date(value)) }
function latest(repo: Repository) { return repo.cached_releases?.[0] }
function assetMatchesUi(name: string, rule: { type: string; value: string }) {
  if (rule.type === 'exact') return name.toLocaleLowerCase() === rule.value.toLocaleLowerCase()
  if (rule.type === 'glob') {
    const escaped = rule.value.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.')
    return new RegExp(`^${escaped}$`, 'i').test(name)
  }
  try { return new RegExp(rule.value, 'i').test(name) } catch { return false }
}
function showError(value: unknown) { error.value = t(value instanceof Error ? value.message : String(value)); message.value = ''; scheduleToastClose() }
async function run<T>(label: string, action: () => Promise<T>) { busy.value = label; error.value = ''; message.value = ''; try { const result = await action(); message.value = typeof result === 'object' && result && 'message' in result ? t(String((result as { message: string }).message)) : `${t(label)}${t('完成')}`; scheduleToastClose(); return result } catch (reason) { showError(reason); throw reason } finally { busy.value = '' } }
async function loadDashboard() { await run('正在读取厨房状态', async () => { dashboard.value = await api.dashboard() }) }
async function detectGame() { await run('正在重新检测', async () => { await api.detectGame(); dashboard.value = await api.dashboard() }) }
async function chooseGameDirectory() { const path = await api.chooseGameDirectory(); if (!path || Array.isArray(path)) return; await run('正在保存厨房目录', async () => { await api.setGamePath(path); dashboard.value = await api.dashboard() }) }
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
      return true
    } catch (reason) {
      const currentRepo = repos.value.find((item) => item.id === repo.id) ?? repo
      currentRepo.last_error = reason instanceof Error ? reason.message : String(reason)
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
async function refreshRepo(repo: Repository, notify = true) {
  const refreshed = await refreshRepositoryEntry(repo)
  if (notify) {
    error.value = ''
    message.value = ''
    if (refreshed) {
      message.value = `${repo.name}${t('刷新完成')}`
      scheduleToastClose()
    } else {
      showError(repo.last_error || `${repo.name}${t('刷新失败')}`)
    }
  }
  return refreshed
}
function installRepo(repo: Repository) {
  if (!game.value?.bepinex.version) { showError('请先安装 BepInEx 框架'); return }
  const release = installReleases(repo)[0]
  if (!release) { showError('请先刷新 Release'); return }
  selectedInstallTag.value = release.tag_name
  selectedInstallAsset.value = matchingInstallAssets(repo, release)[0]?.name || ''
  showInstall.value = repo
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
async function confirmInstall() {
  const repo = showInstall.value
  if (!repo || !selectedInstallTag.value || !selectedInstallAsset.value) return
  showInstall.value = null
  await run(`正在安装 ${repo.name}`, () => api.installRepository(repo.id, selectedInstallTag.value, selectedInstallAsset.value))
  await loadDashboard()
}
async function uninstallRepo(repo: Repository) { if (!confirm(`${t('确定卸载')} ${repo.name}? ${t('只有安装清单中的文件会被删除。')}`)) return; await run(`正在卸载 ${repo.name}`, () => api.uninstallRepository(repo.id)); await loadDashboard() }
void uninstallRepo
async function installBepInEx(reinstall: boolean) {
  if (reinstall && !confirm('重新安装 BepInEx 会先备份当前框架并替换框架文件，是否继续？')) return
  await run(reinstall ? '正在重新安装 BepInEx' : '正在安装 BepInEx', () => api.installBepInEx(reinstall)); await loadDashboard()
}
async function uninstallBepInEx() { if (!confirm('确定移除 BepInEx？程序会先创建时间戳备份。')) return; await run('正在移除 BepInEx', api.uninstallBepInEx); await loadDashboard() }
async function backupBepInEx() { await run('正在创建 BepInEx 备份', api.backupBepInEx); await loadDashboard() }
async function openBackupDirectory() { await run('正在打开备份目录', api.openBackupDirectory) }
async function deleteBackup(path: string) { if (!confirm(`确定删除备份“${path.split(/[\\/]/).pop()}”？删除后无法恢复。`)) return; await run('正在删除 BepInEx 备份', () => api.deleteBepInExBackup(path)); await loadDashboard() }
async function cancelDownload() { await api.cancelDownload() }
async function restoreBackup(path: string) { await run('正在恢复 BepInEx', () => api.restoreBepInEx(path)); await loadDashboard() }
async function launchNormal() { await run('正在准备 Steam 启动', api.launchNormal); await loadDashboard() }
async function toggleWinhttp() {
  if (currentWinhttp.value === 'missing') return
  const enabled = currentWinhttp.value === 'moddisabled' || currentWinhttp.value === 'conflict'
  await run(enabled ? '正在切换有 MOD 状态' : '正在切换无 MOD 状态', () => api.switchWinhttp(enabled))
  await loadDashboard()
}
async function openDirectory() { await run('正在打开游戏目录', api.openGameDirectory) }
async function checkAppUpdate() { const result = await run('正在检查程序更新', api.checkAppUpdate); appUpdate.value = result ?? null; if (!result) message.value = '当前已是最新版本' }
async function refreshEnabledRepositories() {
  const enabled = repos.value.filter((repo) => repo.enabled)
  const results = await Promise.all(enabled.map((repo) => refreshRepo(repo, false)))
  return results.filter((success) => !success).length
}
async function refreshMappedRepositories() {
  const mappedIds = new Set(installedMods.value.map((mod) => mod.repository_id).filter((id): id is string => !!id))
  const mapped = repos.value.filter((repo) => repo.enabled && mappedIds.has(repo.id))
  const results = await Promise.all(mapped.map((repo) => refreshRepo(repo, false)))
  return results.filter((success) => !success).length
}
async function refreshAllRepositories() {
  error.value = ''
  message.value = ''
  const failures = await refreshEnabledRepositories()
  if (failures) showError(t('仓库刷新失败摘要').replace('{count}', String(failures)))
  else { message.value = '全部仓库刷新完成'; scheduleToastClose() }
}
async function checkModUpdates() {
  await run('正在检查 Mod 更新', async () => {
    const failures = await refreshMappedRepositories()
    if (failures) throw new Error(t('仓库刷新失败摘要').replace('{count}', String(failures)))
    dashboard.value = await api.dashboard()
    return dashboard.value
  })
}
async function refreshRepositoryEntriesOnStartup() {
  await refreshEnabledRepositories()
}
async function startupChecks() {
  let update: Awaited<ReturnType<typeof api.checkAppUpdate>> = null
  try {
    update = await api.checkAppUpdate()
  } catch {
    updateCheckFailed.value = true
    startupUpdateChecking.value = false
    return
  }
  if (update) {
    appUpdate.value = update
    try { await api.installAppUpdate() } catch { updateCheckFailed.value = true; startupUpdateChecking.value = false }
    return
  }
  startupUpdateChecking.value = false
  await loadDashboard()
  if (dashboard.value?.config.network.check_updates_on_start !== false) {
    try {
      await refreshRepositoryEntriesOnStartup()
    } catch {
      // A failed repository check must not prevent the manager from opening.
    }
  }
}
function editRepo(repo?: Repository) { Object.assign(repoForm, repo ? { id: repo.id, name: repo.name, provider: repo.provider, url: repo.url, channel: repo.channel, asset_rules: repo.asset_rules.map((rule) => ({ ...rule })), install_target: repo.install_target, plugin_guid: repo.plugin_guid, download_threads: repo.download_threads || (repo.provider === 'github' ? 16 : 1), api_proxy_mode: repo.api_proxy_mode, api_proxy_url: repo.api_proxy_url, release_proxy_mode: repo.release_proxy_mode, release_proxy_url: repo.release_proxy_url, github_token: '', gitee_token: '', official_api_only: true, enabled: repo.enabled } : { id: undefined, name: '', provider: 'gitee', url: '', channel: 'stable', asset_rules: [], install_target: 'BepInEx/plugins', plugin_guid: '', download_threads: 1, api_proxy_mode: 'direct', api_proxy_url: '', release_proxy_mode: 'direct', release_proxy_url: '', github_token: '', gitee_token: '', official_api_only: true, enabled: true }); showRepoEditor.value = true }
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
      if (!await refreshRepositoryEntry(savedRepo)) refreshError = savedRepo.last_error || `${savedRepo.name}${t('刷新失败')}`
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
async function deleteRepo(repo: Repository) { if (repo.builtin || !confirm(`${t('删除仓库')} “${repo.name}”?`)) return; await run('正在删除仓库', async () => { await api.deleteRepository(repo.id); await loadDashboard() }) }
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
  if (conflict === 'overwrite' && !confirm('本地的仓库会直接覆盖成粘贴的内容，并且无法恢复，是否确定？')) return
  await run('正在导入仓库配置', async () => { await api.importRepositories(importText.value, conflict); showImport.value = false; await loadDashboard() })
}
function editModNote(mod: import('./types').InstalledMod) { editingMod.value = mod; modNote.value = mod.note ?? '' }
async function saveModNote() { if (!editingMod.value) return; await run('正在保存 MOD 备注', async () => { await api.saveModNote(editingMod.value!.dll_file, modNote.value); editingMod.value = null; await loadDashboard() }) }
async function uninstallMod(mod: import('./types').InstalledMod) { if (!confirm(`${t('确定删除 MOD')} ${mod.name}?`)) return; await run(`正在删除 ${mod.name}`, () => api.uninstallLocalMod(mod.dll_file)); await loadDashboard() }
async function installLocalMod() { const path = await api.chooseModFile(); if (!path || Array.isArray(path)) return; await run('正在安装本地 MOD', () => api.installLocalMod(path)); await loadDashboard() }
let kitchenPoll: number | undefined
onMounted(async () => {
  appVersion.value = await getVersion()
  document.title = `OC2 Mod Manager v${appVersion.value}`
  await getCurrentWindow().setTitle(document.title)
  await startupChecks()
  stopProgress.value = await watchDownloadProgress((value) => { progress.value = { label: value.label, percent: value.total ? Math.min(100, value.downloaded / value.total * 100) : 0, speed: `${formatBytes(value.speed_bytes)}/s` }; if (value.finished) window.setTimeout(() => { progress.value = { label: '', percent: 0, speed: '' } }, 900) })
  kitchenPoll = window.setInterval(async () => {
    if (busy.value || repositoryRefreshCount > 0) return
    const revision = dashboardRevision
    try {
      const next = await api.dashboard()
      if (revision === dashboardRevision && repositoryRefreshCount === 0) dashboard.value = next
    } catch { /* 状态轮询失败时保留上一状态 */ }
  }, 1000)
})
onUnmounted(() => { stopProgress.value?.(); if (kitchenPoll) window.clearInterval(kitchenPoll); if (toastTimer) window.clearTimeout(toastTimer) })
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
        <h2>{{ t('安装器检查更新失败') }}</h2>
        <p>{{ t('请重启软件后再试。') }}</p><button class="primary-button" @click="api.exitApp()">{{ t('确定') }}</button>
      </div>
    </div>
    <aside class="sidebar">
      <div class="brand">
        <div class="brand-mark">OC</div>
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
      <header class="topbar">
        <div>
          <h1>{{ t(navItems.find((item) => item.id === activeView)?.label || '') }}</h1>
        </div>
        <div class="top-actions"><select v-model="locale" @change="setLocale(locale)" class="language-select"
            :aria-label="t('界面语言')">
            <option v-for="option in localeOptions" :key="option.value" :value="option.value">{{
              t(option.label) }}</option>
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
      <div v-if="progress.label" class="download-strip">
        <div class="download-meta"><span>{{ progress.label }}</span><span>{{ progress.percent.toFixed(0) }}% ·
            {{ progress.speed }}</span></div>
        <div class="progress"><i :style="{ width: `${progress.percent}%` }"></i></div>
      </div>
      <section v-if="activeView === 'overview'" class="view-content">
        <div class="hero-row">
          <div>
            <div class="status-line"><span class="status-dot"
                :class="game?.game_exists ? 'success-bg' : 'danger-bg'"></span><span>{{
                game?.game_exists ? t('已识别厨房目录') : t('等待选择厨房目录') }}</span></div>
            <h2>{{ t('把 MOD 管理变成') }}<br /><em>{{ t('一次点击的事。') }}</em></h2>
            <p class="lead">{{ t('统一管理 BepInEx、Release 仓库和本地插件，安装过程会自动保护正在运行的厨房。') }}</p>
            <div class="path-row">
              <FolderOpen :size="16" /><span>{{ game?.path || t('尚未找到 Overcooked2.exe') }}</span><button
                class="text-button" @click="openDirectory" :disabled="!game?.path">{{ t('打开目录') }}
                <ExternalLink :size="13" />
              </button>
            </div>
          </div>
          <div class="hero-orbit">
            <div class="orbit-ring"></div>
            <div class="orbit-core"><span>OC2</span></div>
          </div>
        </div>
        <div class="metric-grid">
          <div class="metric" @click="activeView = 'framework'"><span class="metric-label">BepInEx</span><strong>{{
            game?.bepinex.version ||
            (game?.bepinex.installed ? t('版本未知') : t('未安装')) }}</strong><span class="metric-note"
              :class="game?.bepinex.complete ? 'good' : ''">
              <Check v-if="game?.bepinex.complete" :size="13" /> {{ game?.bepinex.complete ? t('完整可用') :
                t('需要安装') }}
            </span></div>
          <div class="metric" @click="activeView = 'installed'"><span class="metric-label">{{ t('已安装 MOD') }}</span><strong>{{ installedMods.length.toString().padStart(2, '0') }}</strong><span
              class="metric-note">BepInEx/plugins</span></div>
          <div class="metric" @click="activeView = 'installed'"><span class="metric-label">{{ t('待更新') }}</span><strong
              class="accent-text">{{ updateCount.toString().padStart(2, '0') }}</strong><span class="metric-note"
              :class="updateCount ? 'warn' : ''">{{ updateCount ? t('发现新版本') : t('全部最新')
              }}</span></div><button class="metric metric-button"
            :disabled="gameBlocked || !game?.game_exists || !!busy" @click="toggleWinhttp"><span
              class="metric-label">{{ t('启动状态') }}</span><strong>{{ statusLabel(currentWinhttp) }}</strong><span
              class="metric-note">{{ game?.kitchen_running ? t('操作已锁定') : currentWinhttp === 'conflict' ?
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
              class="action-icon">▶</span><span><b>{{ t('启动厨房') }}</b><small>{{ t('通过 Steam 启动游戏') }}</small></span><span>›</span></button></div>
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
              @click="installBepInEx(false)">
              <Download :size="16" />{{ t('安装 BepInEx') }}
            </button><button class="secondary-button" :disabled="gameBlocked || !!busy" @click="installBepInEx(true)">
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
          <div class="button-row"><button class="primary-button"
              :disabled="!mappedReposAvailable || !!busy" @click="checkModUpdates">
              <RefreshCw :size="16" />{{ t('检查模组更新') }}
            </button><button class="secondary-button" :disabled="gameBlocked || !!busy" @click="installLocalMod">
              <Plus :size="16" />{{ t('手动安装模组') }}
            </button><button class="secondary-button" :disabled="gameBlocked || !!busy" @click="loadDashboard">
              <RefreshCw :size="16" />{{ t('重新扫描') }}
            </button></div>
        </div>
        <div class="panel table-panel">
          <div class="table-head"><span>{{ t('MOD / GUID') }}</span><span>{{ t('本地版本') }}</span><span>{{ t('来源') }}</span><span>{{ t('状态') }}</span><span></span></div>
          <div v-for="mod in installedMods" :key="mod.dll_file" class="table-row">
            <div class="mod-cell">
              <div class="mod-avatar">{{ mod.name.slice(0, 2).toUpperCase() }}</div>
              <div><b>{{ mod.name }}</b><span>{{ mod.product_name || t('产品名称未知') }} · {{ mod.dll_file
              }}<small v-if="mod.guid"> · {{ mod.guid }}</small></span><small v-if="mod.note" class="mod-note">{{ t('备注：') }}{{ mod.note }}</small>
              </div>
            </div><span class="mono">{{ mod.version || t('版本未知') }}</span><span class="muted-text">{{
              mod.version_source }}</span><span class="badge"
              :class="mod.load_warning ? 'danger' : mod.update_available ? 'warning' : 'success'">{{
                mod.load_warning || (mod.update_available ? t('有更新') : t('已安装')) }}</span><span class="mod-actions"><button
                class="icon-button small" :title="t('编辑 MOD 备注')" @click="editModNote(mod)">
                <Pencil :size="14" />
              </button><button class="icon-button small danger-icon" :title="t('删除 MOD')" :disabled="gameBlocked || !!busy" @click="uninstallMod(mod)">
                <Trash2 :size="14" />
              </button></span>
          </div>
          <div v-if="!installedMods.length" class="empty-state">{{ t('没有扫描到 BepInEx/plugins/**/*.dll') }}</div>
        </div>
      </section>
      <section v-else-if="activeView === 'repositories'" class="view-content">
        <div class="page-intro">
          <div>
            <p>{{ t('内置和自定义仓库均从 Gitee/GitHub Release 获取版本与资产。') }}</p>
          </div><div class="button-row"><button class="secondary-button" :disabled="!!busy || !repos.some((repo) => repo.enabled)" @click="refreshAllRepositories">
              <RefreshCw :size="16" />{{ t('刷新全部仓库') }}
            </button><button class="primary-button" @click="editRepo()"><Plus :size="16" />{{ t('添加仓库') }}</button></div>
        </div>
        <div class="toolbar">
          <div class="toolbar-group"><button class="secondary-button" @click="showImport = true">
              <Upload :size="15" />{{ t('导入配置') }}
            </button><button class="secondary-button" :disabled="!repos.length" @click="exportRepositories">
              <Download :size="15" />{{ t('导出配置') }}
            </button></div>
        </div>
        <div class="repository-list">
          <div v-for="repo in repos" :key="repo.id"
            :class="['repository-row', { 'repo-refreshing': repositoryRefreshing[repo.id] }]">
            <div class="repo-logo" :class="repo.provider">
              <Github v-if="repo.provider === 'github'" :size="17" /><span v-else>码</span>
            </div>
            <div class="repo-info">
              <div class="repo-title"><b>{{ repo.name }}</b><span v-if="repo.builtin" class="badge muted">{{ t('内置') }}</span><span
                  class="badge" :class="repo.enabled ? 'success' : 'muted'">{{ repo.enabled ? t('启用') : t('停用') }}</span>
              </div><span>{{ repo.url }}</span><small>{{ t(repo.channel === 'stable' ? '稳定版' : repo.channel === 'beta' ? '测试版' : '全部版本') }} · {{ repo.install_target }} · {{
                repo.asset_rules.map((rule) => rule.value).join(', ')}}</small>
            </div>
            <div class="repo-release"><b>{{ latest(repo)?.version || '—' }}</b><span>{{ latest(repo) ?
              formatDate(latest(repo)?.published_at) : t('待刷新') }}</span></div>
            <div class="row-actions"><button class="icon-button small" :title="t('刷新 Release')"
                :disabled="!!busy || !!repositoryRefreshing[repo.id]" @click="refreshRepo(repo)">
                <RefreshCw :size="14" />
              </button><button class="icon-button small" :title="t('查看 Release 日志')" :disabled="!latest(repo)"
                @click="showRelease = repo">
                <Search :size="14" />
              </button><button class="secondary-button small-button" :title="t('打开发布页')"
                :disabled="!!repositoryRefreshing[repo.id]" @click="api.openReleasePage(repo.url)">
                <ExternalLink :size="14" />{{ t('打开发布页') }}
              </button><button class="primary-button small-button" :title="t('下载并安装')"
                :disabled="gameBlocked || !installReleases(repo).length || !!busy || !!repositoryRefreshing[repo.id]"
                @click="installRepo(repo)">
                <Download :size="14" />{{ t('下载并安装') }}
              </button><button class="icon-button small" :title="t('编辑仓库')" @click="editRepo(repo)">
                <Pencil :size="14" />
              </button><button class="icon-button small danger-icon" :title="t('删除仓库')" :disabled="repo.builtin"
                @click="deleteRepo(repo)">
                <Trash2 :size="14" />
              </button></div>
          </div>
          <div v-if="!repos.length" class="empty-state">{{ t('暂无仓库配置') }}</div>
        </div>
      </section>
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
              <Download :size="15" />{{ t('继续导出') }}
            </button></div>
        </div>
      </div>
      <section v-if="activeView === 'sponsor'" class="view-content sponsor-view">
        <div class="sponsor-panel">
          <h2>{{ t('觉得好用的话，请我喝杯咖啡吧！') }}</h2><img :src="sponsorImage" alt="赞助二维码" />
            <p>{{ t('感谢你的支持。') }}</p>
        </div>
      </section>
    </main>
    <button v-if="progress.label" class="secondary-button download-cancel" @click="cancelDownload">{{ t('取消下载') }}</button>
    <button v-if="activeView === 'overview'" class="update-fab" :disabled="!!busy"
      @click="appUpdate ? run('正在安装程序更新', api.installAppUpdate) : checkAppUpdate()">
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
              :placeholder="t('例如：我的 MOD 正式版')" /></label><label><span>{{ t('来源平台') }}</span><select v-model="repoForm.provider" @change="providerChanged">
              <option value="gitee">Gitee</option>
              <option value="github">GitHub</option>
            </select></label><label class="wide"><span>{{ t('版本地址') }}</span><input v-model="repoForm.url"
              placeholder="https://github.com/owner/repo/releases" /></label><label><span>{{ t('版本频道') }}</span><select
              v-model="repoForm.channel">
              <option value="stable">{{ t('稳定版') }}</option>
              <option value="beta">{{ t('测试版') }}</option>
              <option value="all">{{ t('全部版本') }}</option>
            </select></label><label><span>{{ t('安装目录') }}</span><input v-model="repoForm.install_target"
              :placeholder="t('BepInEx 插件目录')" /></label><label><span>{{ t('插件 GUID') }}</span><input v-model="repoForm.plugin_guid"
              :placeholder="t('可选')" /></label><label><span>{{ t('下载线程数') }}</span><input v-model.number="repoForm.download_threads" type="number" min="1" max="32" :disabled="repoForm.provider === 'gitee'" /></label><label><span>{{ t('API 代理策略') }}</span><select v-model="repoForm.api_proxy_mode">
              <option value="direct">{{ t('直连') }}</option>
              <option value="auto">{{ t('自动选择') }}</option>
              <option value="fixed">{{ t('固定代理') }}</option>
            </select></label><label><span>API {{ t('代理地址') }}</span><input v-model="repoForm.api_proxy_url"
              placeholder="https://" /></label><label class="wide"><span>{{ t('API 直连 Token') }} ({{ repoForm.provider === 'gitee' ? 'Gitee' : 'GitHub' }})</span><input
              v-if="repoForm.provider === 'gitee'" v-model="repoForm.gitee_token" type="password" :placeholder="t('留空保持原设置')" /><input
              v-else v-model="repoForm.github_token" type="password" :placeholder="t('留空保持原设置')" /></label><label><span>{{ t('Release 代理策略') }}</span><select
              v-model="repoForm.release_proxy_mode">
              <option value="direct">{{ t('直连') }}</option>
              <option value="auto">{{ t('自动选择') }}</option>
              <option value="fixed">{{ t('固定代理') }}</option>
            </select></label><label><span>Release {{ t('代理地址') }}</span><input v-model="repoForm.release_proxy_url"
              placeholder="https://" /></label></div>
        <div class="rules-heading">
          <div><b>{{ t('Release 资产规则') }}</b><small>{{ t('支持精确文件名、通配符和正则表达式') }}</small></div><button class="ghost-button" @click="addRule">
            <Plus :size="14" />{{ t('添加规则') }}
          </button>
        </div>
        <div v-for="(rule, index) in repoForm.asset_rules" :key="index" class="rule-row"><select v-model="rule.type">
            <option value="exact">{{ t('精确') }}</option>
            <option value="glob">{{ t('通配符') }}</option>
            <option value="regex">{{ t('正则') }}</option>
          </select><input v-model="rule.value" /><button
            class="icon-button small danger-icon" :title="t('删除规则')"
            @click="removeRule(index)">
            <Trash2 :size="14" />
          </button></div><label class="check-control"><input v-model="repoForm.enabled" type="checkbox" />{{ t('启用此仓库') }}</label>
        <div class="modal-actions"><button class="secondary-button" @click="showRepoEditor = false">{{ t('取消') }}</button><button
            class="primary-button"
            :disabled="!repoForm.name || !repoForm.url || !repoForm.install_target || !repoForm.asset_rules.length || repoForm.asset_rules.some((rule) => !rule.value.trim()) || !!busy"
            @click="saveRepo">
            <Save :size="16" />{{ t('保存仓库') }}
          </button></div>
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
            <Upload :size="15" />{{ t('选择 .oc2repo.toml') }}
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
          <div class="preview-grid"><span>{{ t('新增') }} {{ importPreview.additions.length }}</span><span>{{ t('重复') }} {{
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
            }}</span><small>{{ formatBytes(asset.size) }}</small></div>
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
        <p class="muted-text">{{ t('产品名称：') }}{{ editingMod.product_name || t('未知') }} · {{ editingMod.dll_file }}</p><textarea v-model="modNote" rows="6" :placeholder="t('例如：这是街机模组，包含房主工具、房间控制和快捷设置功能。')" />
        <div class="modal-actions"><button class="secondary-button" @click="editingMod = null">{{ t('取消') }}</button><button
            class="primary-button" :disabled="!!busy" @click="saveModNote">
            <Save :size="16" />{{ t('保存备注') }}
          </button></div>
      </div>
    </div>
  </div>
  <div v-if="showInstall" class="modal-backdrop" @click.self="showInstall = null">
    <div class="modal install-modal">
      <div class="modal-head">
        <div>
          <h3>{{ t('选择版本和文件') }}</h3>
        </div><button class="icon-button" @click="showInstall = null">
          <X :size="17" />
        </button>
      </div><label class="install-field"><span>{{ t('Release 版本') }}</span><select v-model="selectedInstallTag">
          <option v-for="release in installReleases(showInstall)" :key="release.tag_name" :value="release.tag_name">{{
            release.version || release.tag_name }} · {{ release.name }}
          </option>
      </select></label><label class="install-field"><span>{{ t('下载文件') }}</span><select v-model="selectedInstallAsset">
          <option
            v-for="asset in selectedInstallAssets(showInstall)"
            :key="asset.name" :value="asset.name">{{ asset.name }} · {{ formatBytes(asset.size) }}</option>
        </select></label>
      <p class="muted-text">{{ t('仅显示符合当前仓库资产规则的文件。') }}</p>
      <div class="modal-actions"><button class="secondary-button" @click="showInstall = null">{{ t('取消') }}</button><button
          class="primary-button" :disabled="!selectedInstallAsset || !!busy" @click="confirmInstall">
          <Download :size="15" />{{ t('下载并安装') }}
        </button></div>
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
  position: relative;
  padding-right: 110px;
}

.download-cancel {
  position: fixed;
  right: 42px;
  top: 118px;
  z-index: 12;
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
