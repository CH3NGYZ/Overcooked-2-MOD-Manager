import { reactive, type ComputedRef, type Ref } from 'vue'
import type { Dashboard, Repository, RepositoryPatch } from '../types'
import { t, type Locale } from '../i18n'
import { normalizeRepositoryRegion } from '../repositorySources'
import { repositoryGroup, repositoryName, repositoryNote } from '../repositoryNames'

/**
 * Repository add/edit form: fills the patch from an existing repository or a
 * blank template, keeps the i18n fields in sync while typing, and saves
 * through the backend with an immediate Release refresh for release-mode rows.
 */
export function useRepositoryEditor(options: {
  showRepoEditor: Ref<boolean>
  dashboard: Ref<Dashboard | null>
  repos: ComputedRef<Repository[]>
  locale: Ref<Locale>
  run: <T>(label: string, action: () => Promise<T>) => Promise<T>
  showError: (value: unknown) => void
  showSuccess: (value: string) => void
  requestConfirmation: (request: { title: string; message: string; danger?: boolean }) => Promise<boolean>
  loadDashboard: () => Promise<void>
  refreshRepo: (repo: Repository, notify?: boolean, trackOverview?: boolean) => Promise<boolean>
  api: typeof import('../services/tauri').api
}) {
  const { showRepoEditor, dashboard, repos, locale, run, showError, showSuccess, requestConfirmation, loadDashboard, refreshRepo, api } = options
  const repoForm = reactive<RepositoryPatch>({
    name: '', name_i18n: {}, group: '', group_i18n: {}, provider: 'gitee', username: '', repository: '', download_mode: 'release', branch: 'main', file_path: '', channel: 'all', asset_rules: [],
    region: '', install_target: 'BepInEx/plugins', note: '', note_i18n: {}, plugin_guid: '', local_product_name: '', local_original_filename: '', version_regex: '', max_release_count: 10, download_threads: 1, github_token: '', gitee_token: '', official_api_only: true, enabled: true,
  })
  function localizedRepositoryName(repo: Repository) { return repositoryName(repo, locale.value) }
  function localizedRepositoryNote(repo: Repository) { return repositoryNote(repo, locale.value) }

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

function providerChanged() {
  repoForm.download_threads = repoForm.provider === 'github' ? 16 : 1
}

async function deleteRepo(repo: Repository) { if (repo.builtin || !await requestConfirmation({ title: t('删除仓库'), message: `${t('删除仓库')} “${localizedRepositoryName(repo)}”?`, danger: true })) return; await run('正在删除仓库', async () => { await api.deleteRepository(repo.id); await loadDashboard() }) }
function addRule() { repoForm.asset_rules.push({ type: 'exact', value: '' }) }
function removeRule(index: number) { repoForm.asset_rules.splice(index, 1) }
function updateRepositoryTranslation(field: 'name' | 'group' | 'note', event: Event) {
  const input = event.target as HTMLInputElement | HTMLTextAreaElement
  repoForm[field] = input.value
  const translations = field === 'name' ? repoForm.name_i18n : field === 'group' ? repoForm.group_i18n : repoForm.note_i18n
  if (translations) translations[locale.value] = input.value
}

  return { showRepoEditor, repoForm, editRepo, saveRepo, providerChanged, addRule, removeRule, updateRepositoryTranslation, deleteRepo }
}

export type RepositoryEditor = ReturnType<typeof useRepositoryEditor>
