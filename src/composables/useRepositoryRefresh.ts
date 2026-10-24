import { computed, reactive, ref, type Ref } from 'vue'
import type { Dashboard, InstalledMod } from '../types'
import type { Locale } from '../i18n'
import { t } from '../i18n'
import { api } from '../services/tauri'
import { repositoryName } from '../repositoryNames'

/**
 * Per-repository Release refresh engine. Each repository refreshes through one
 * deduplicated task and reports its own refreshing/ready state so repository
 * cards and bound MOD labels update independently instead of sharing one
 * global loading flag.
 */
export function useRepositoryRefresh(options: {
  dashboard: Ref<Dashboard | null>
  locale: Ref<Locale>
  repositoryReleaseReady: Record<string, boolean>
  run: <T>(label: string, action: () => Promise<T>) => Promise<T>
  showError: (value: unknown) => void
  showSuccess: (value: string) => void
  isUnmounted: () => boolean
  getRepos: () => import('../types').Repository[]
  getInstalledMods: () => InstalledMod[]
  getModReleaseRepositoryIds: (mod: InstalledMod) => Set<string>
  getMappedReposAvailable: () => boolean
  api: typeof api
}) {
  const {
    dashboard, locale, repositoryReleaseReady, run, showError, showSuccess,
    isUnmounted, getRepos, getInstalledMods, getModReleaseRepositoryIds, getMappedReposAvailable, api,
  } = options
  const repositoryRefreshing = reactive<Record<string, boolean>>({})
  const repositoryRefreshBatchActive = ref(false)
  const repositoryRefreshBatchTotal = ref(0)
  const repositoryRefreshBatchCompleted = ref(0)
  let repositoryRefreshBatchToken = 0
  const repositoryRefreshTasks = new Map<string, Promise<boolean>>()
  // True while "check MOD updates" is running. Each MOD derives its own state
  // from its bound repositories, so a finished repository reports immediately.
  const modUpdatesCheckActive = ref(false)

  function localizedRepositoryName(repo: import('../types').Repository) { return repositoryName(repo, locale.value) }

  // Any repository currently fetching its Release (startup/source refresh
  // included). MOD statuses derive from this so the installed page also shows
  // checking while the install page refreshes, not only during the explicit
  // update-check batch.
  const modRepositoryRefreshActive = computed(() => repositoryRefreshBatchActive.value
    || Object.values(repositoryRefreshing).some(Boolean))

  async function refreshRepositoryEntry(repo: import('../types').Repository) {
    if (repo.download_mode === 'repository_file') return false
    const currentTask = repositoryRefreshTasks.get(repo.id)
    if (currentTask) return currentTask
    const task = (async () => {
      repositoryRefreshing[repo.id] = true
      repositoryReleaseReady[repo.id] = false
      repo.cached_releases = []
      try {
        const releases = await api.refreshRepository(repo.id) ?? []
        const currentRepo = getRepos().find((item) => item.id === repo.id) ?? repo
        currentRepo.cached_releases = releases
        currentRepo.last_error = undefined
        repositoryReleaseReady[repo.id] = true
        return true
      } catch (reason) {
        const currentRepo = getRepos().find((item) => item.id === repo.id) ?? repo
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

  async function refreshRepo(repo: import('../types').Repository, notify = true, trackOverview = true) {
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

  async function refreshRepositoryBatch(repositories: import('../types').Repository[], concurrency = repositories.length) {
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
        while (nextIndex < repositories.length && !isUnmounted()) {
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

  function mappedUpdateRepositories() {
    const mappedIds = new Set(getInstalledMods().flatMap((mod) => Array.from(getModReleaseRepositoryIds(mod))))
    return getRepos().filter((repo) => repo.enabled && repo.download_mode === 'release' && mappedIds.has(repo.id))
  }

  async function refreshMappedRepositories(repositories = mappedUpdateRepositories()) {
    // Keep a few requests in flight so repositories finish one after another and
    // their MODs report as soon as their own repository completed.
    return refreshRepositoryBatch(repositories, 4)
  }

  async function refreshMappedRepositoryUpdates() {
    if (!getMappedReposAvailable()) return dashboard.value
    if (modUpdatesCheckActive.value) return dashboard.value
    const mapped = mappedUpdateRepositories()
    // A queued repository must read as not-ready, otherwise MODs bound to it keep
    // the previous "settled" result and skip the pending/checking state entirely.
    for (const repo of mapped) repositoryReleaseReady[repo.id] = false
    modUpdatesCheckActive.value = true
    try {
      const failures = await refreshMappedRepositories(mapped)
      // One final snapshot makes the backend labels match the fresh caches.
      dashboard.value = await api.dashboard()
      if (failures) throw new Error(t('仓库刷新失败摘要').replace('{count}', String(failures)))
      return dashboard.value
    } finally {
      modUpdatesCheckActive.value = false
    }
  }

  async function checkModUpdates() {
    if (!getMappedReposAvailable()) return
    await run('正在检查 MOD 更新', refreshMappedRepositoryUpdates)
  }

  return {
    repositoryRefreshing, repositoryRefreshBatchActive, repositoryRefreshBatchTotal,
    repositoryRefreshBatchCompleted, repositoryRefreshTasks, modUpdatesCheckActive,
    modRepositoryRefreshActive, refreshRepositoryEntry, refreshRepo, refreshRepositoryBatch,
    mappedUpdateRepositories, refreshMappedRepositories, refreshMappedRepositoryUpdates, checkModUpdates,
  }
}
