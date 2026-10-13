import { computed, onScopeDispose, ref, watch, type Ref } from 'vue'
import { COMMUNITY_PAGE_SIZE, communityPage, communityPageCount, isCommunityRepository, orderedCommunityRepositories } from '../communityRepositories'
import type { PublicConfig, Repository } from '../types'

interface CommunityHost {
  config: Readonly<Ref<PublicConfig | undefined>>
  canRefresh: () => boolean
  search: () => Promise<PublicConfig>
  cancel: () => Promise<void>
  beforeSearch: () => Promise<void>
  refreshConfig: () => Promise<void>
  refreshReleases: (repositories: Repository[]) => Promise<number>
  refreshDashboard: () => Promise<unknown>
}

export function useCommunityRepositories(host: CommunityHost) {
  const started = ref(false)
  const searching = ref(false)
  const refreshing = ref(false)
  const error = ref('')
  const releaseFailures = ref(0)
  const page = ref(1)
  const available = computed(host.canRefresh)
  const repositories = computed(() => orderedCommunityRepositories(host.config.value))
  const pageCount = computed(() => communityPageCount(repositories.value))
  const visibleRepositories = computed(() => communityPage(repositories.value, page.value))
  const discovery = computed(() => host.config.value?.community_discovery)
  const discoveredIds = computed(() => new Set(discovery.value?.repository_ids ?? []))
  const discoveredCount = computed(() => repositories.value.filter((repo) => discoveredIds.value.has(repo.id)).length)
  let disposed = false
  let pending: Promise<boolean> | undefined

  watch(pageCount, (count) => { page.value = Math.min(page.value, count) })

  function setPage(value: number) {
    if (searching.value || !Number.isFinite(value)) return
    page.value = Math.max(1, Math.min(Math.trunc(value), pageCount.value))
  }

  function reveal(ids: Set<string>) {
    const index = repositories.value.findIndex((repo) => ids.has(repo.id))
    if (index >= 0) setPage(Math.floor(index / COMMUNITY_PAGE_SIZE) + 1)
  }

  function refresh() {
    if (pending) return pending
    if (disposed || !available.value) return Promise.resolve(false)
    started.value = true
    refreshing.value = true
    searching.value = true
    error.value = ''
    releaseFailures.value = 0
    pending = (async () => {
      try {
        await host.beforeSearch()
        if (disposed || !available.value) return false
        await host.search()
        if (disposed) return false
        await host.refreshConfig()
        if (disposed) return false
        page.value = 1
        searching.value = false
        // Hidden installed duplicates still need Releases for their original Tag/channel binding.
        const refreshTargets = (host.config.value?.repositories ?? []).filter((repo) => isCommunityRepository(repo) && repo.enabled)
        releaseFailures.value = await host.refreshReleases(refreshTargets)
        if (disposed) return false
        await host.refreshDashboard()
        return true
      } catch (reason) {
        if (!disposed) error.value = reason instanceof Error ? reason.message : String(reason)
        return false
      } finally {
        searching.value = false
        refreshing.value = false
        pending = undefined
      }
    })()
    return pending
  }

  onScopeDispose(() => {
    disposed = true
    if (searching.value) void host.cancel().catch(() => undefined)
  })

  return { started, searching, refreshing, available, error, releaseFailures, page, pageCount, repositories, visibleRepositories, discovery, discoveredIds, discoveredCount, refresh, setPage, reveal }
}
