import { nextTick, ref, type ComputedRef, type Ref } from 'vue'
import type { Dashboard, InstalledMod, ProxyMode, PublicConfig, Repository } from '../types'
import { t } from '../i18n'
import { repositoryName } from '../repositoryNames'
import { locale } from '../i18n'
import { isCommunityRepository } from '../communityRepositories'
import { installReleases, matchingInstallAssets } from '../repositoryAssets'
import type { TutorialProgressState } from '../tutorial/useTutorialRuntime'
import type { InstalledModsModel } from './useInstalledModsModel'

type ModUninstallPending = {
  kind: 'mod' | 'package'
  phase: 'confirm' | 'package' | 'unmanaged'
  mod?: InstalledMod
  packageId?: string
  packageName?: string
  mods?: InstalledMod[]
  siblingNames?: string[]
}

/**
 * Installed/online MOD operations: install dialog and proxy selection, the
 * three-stage uninstall confirmation, per-MOD and package toggles, manual
 * local/online install, notes, and config opening.
 */
export function useModOperations(options: {
  showInstall: Ref<Repository | null>
  selectedInstallTag: Ref<string>
  selectedInstallAsset: Ref<string>
  dashboard: Ref<Dashboard | null>
  config: ComputedRef<PublicConfig | undefined>
  game: ComputedRef<import('../types').GameInfo | undefined>
  repositoryReleaseReady: Record<string, boolean>
  model: InstalledModsModel
  busy: Ref<string>
  message: Ref<string>
  configEditorTarget: Ref<{ kind: 'mod'; mod: InstalledMod } | { kind: 'bepinex' } | null>
  progress: Ref<TutorialProgressState>
  beginDownload: () => void
  clearDownload: () => void
  run: <T>(label: string, action: () => Promise<T>) => Promise<T>
  showError: (value: unknown) => void
  showSuccess: (value: string) => void
  requestConfirmation: (request: { title: string; message: string; danger?: boolean; acknowledge?: boolean }) => Promise<boolean>
  loadDashboard: () => Promise<void>
  refreshDashboardSerialized: () => Promise<unknown>
  isUnmounted: () => boolean
  isTutorialPreviewing: () => boolean
  isBlocked: () => boolean
  api: typeof import('../services/tauri').api
}) {
  const {
    showInstall, selectedInstallTag, selectedInstallAsset, dashboard, config, game, configEditorTarget,
    repositoryReleaseReady, model, busy, message, progress, beginDownload, clearDownload,
    run, showError, showSuccess, requestConfirmation, loadDashboard, refreshDashboardSerialized,
    isUnmounted, isTutorialPreviewing, isBlocked, api,
  } = options
  const selectedAssetProxyMode = ref<ProxyMode>('auto_direct')
  const selectedAssetProxyId = ref('')
  const editingMod = ref<InstalledMod | null>(null)
  const modNote = ref('')
  const modUninstallPending = ref<ModUninstallPending | null>(null)
  const manualInstallMode = ref<'choose' | 'online' | null>(null)
  const manualInstallUrl = ref('')
  const { packageIdForMod, packageNameForMod, packageModsForMod, hasOtherEnabledSameGuid,
    modEnabledForView, packageEnabledForMod, modTogglePending, modPackageTogglePending,
    displayModPath, isModToggling, isPackageToggling } = model

  function localizedRepositoryName(repo: Repository) { return repositoryName(repo, locale.value) }

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

async function confirmInstall() {
  if (isTutorialPreviewing() || isUnmounted() || busy.value) return
  const repo = showInstall.value
  if (!repo || (repo.download_mode === 'release' && (!selectedInstallTag.value || !selectedInstallAsset.value))) return
  beginDownload()
  showInstall.value = null
  try {
    const proxyMode = assetProxySelectionVisible(repo) ? selectedAssetProxyMode.value : 'direct'
    const proxyId = proxyMode === 'fixed' ? selectedAssetProxyId.value || undefined : undefined
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
    clearDownload()
  }
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

function formatModPackageDeleteWarning(pending: ModUninstallPending) {
  const current = pending.mod?.name || ''
  const siblings = (pending.siblingNames || []).map((name) => `【${name}】`).join('、')
  const others = siblings || `【${t('其他 MOD')}】`
  return t('当前 DLL【{current}】与{others}一并安装。仅将此 DLL 移入回收站，可能导致{missing}MOD 缺失依赖。可从回收站恢复，是否继续？')
    .replace('{current}', current)
    .replace('{others}', others)
    .replace('{missing}', others)
}

function editModNote(mod: import('../types').InstalledMod) { editingMod.value = mod; modNote.value = mod.note ?? '' }
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
async function toggleMod(mod: InstalledMod) {
  const key = mod.dll_file
  if (isModToggling(mod) || isBlocked()) return
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
  if (!packageId || !packageMods.length || isBlocked() || isPackageToggling(mod)) return
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
function installLocalMod() {
  if (isTutorialPreviewing() || isUnmounted() || isBlocked()) return
  manualInstallUrl.value = ''
  manualInstallMode.value = 'choose'
}
async function chooseLocalMod() {
  manualInstallMode.value = null
  const path = await api.chooseModFile()
  if (!path || Array.isArray(path)) return
  if (isBlocked() || isUnmounted()) return
  try {
    await run('正在安装本地 MOD', () => api.installLocalMod(path))
    await loadDashboard()
  } catch { /* run displays the error */ }
}
async function installOnlineMod() {
  if (isBlocked() || !manualInstallUrl.value.trim()) return
  const url = manualInstallUrl.value.trim()
  try {
    const parsed = new URL(url)
    if (!['https:', 'http:'].includes(parsed.protocol) || parsed.username || parsed.password) throw new Error()
  } catch { showError(t('请输入有效的 HTTP 或 HTTPS 下载直链')); return }
  manualInstallMode.value = null
  beginDownload()
  progress.value = { label: t('在线文件'), percent: 0, speed: '', indeterminate: true, status: t('正在建立直连') }
  try {
    await run('正在安装在线 MOD', () => api.installOnlineMod(url))
    await loadDashboard()
  } catch { /* run displays the error */ }
  finally { clearDownload() }
}
async function rescanInstalledMods() {
  await run('正在重新扫描', async () => {
    dashboard.value = await api.dashboard()
    return dashboard.value
  })
}

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
  function openModFileLocation(mod: InstalledMod) {
    if (isTutorialPreviewing() || busy.value) return
    // run already displays failures; keep click errors out of unhandled rejections.
    void run('正在打开模组目录', () => api.openModFileLocation(mod.dll_file)).catch(() => undefined)
  }
  function openModConfig(mod: InstalledMod) {
    if (isTutorialPreviewing() || busy.value || !mod.has_config || !mod.guid) return
    configEditorTarget.value = { kind: 'mod', mod }
  }
  return {
    showInstall, selectedInstallTag, selectedInstallAsset,
    selectedAssetProxyMode, selectedAssetProxyId, editingMod, modNote, modUninstallPending,
    manualInstallMode, manualInstallUrl,
    installRepo, confirmInstall, defaultAssetProxyMode, assetProxyEntries,
    assetProxySelectionVisible, selectedInstallRelease, selectedInstallAssets,
    closeInstallDialog, selectInstallRelease,
    formatModPackageDeleteWarning, editModNote, saveModNote,
    uninstallModPackage, uninstallMod, cancelModUninstall, confirmModUninstall,
    toggleMod, toggleModPackage,
    installLocalMod, chooseLocalMod, installOnlineMod, rescanInstalledMods,
    saveHideUnidentifiedMods, openModFileLocation, openModConfig, displayModPath,
  }
}

export type ModOperations = ReturnType<typeof useModOperations>
