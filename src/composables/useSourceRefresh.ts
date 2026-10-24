import { computed, reactive, ref, nextTick, type ComputedRef, type Ref } from 'vue'
import type { Dashboard, PublicConfig, Repository, RepositorySource } from '../types'
import { t, localizedRepositorySourceName, type Locale } from '../i18n'
import { api } from '../services/tauri'
import { communityProviderMatchesRegion, isCommunitySource, GITEE_COMMUNITY_SOURCE_ID, GITEE_COMMUNITY_TOPIC_URL } from '../communityRepositories'
import { repositoryRegionMatches } from '../repositorySources'
import { repositoryGroup, repositoryGroupKey as repositoryStableGroupKey } from '../repositoryNames'
import { installReleases } from '../repositoryAssets'
import type { useCommunityRepositories } from './useCommunityRepositories'

type CommunityDiscovery = ReturnType<typeof useCommunityRepositories>

export type SourceGroup = {
  source: RepositorySource
  groups: Array<{ key: string; label: string; repositories: Repository[] }>
  children: SourceGroup[]
  community?: CommunityDiscovery
}

/**
 * Source-tree state and refresh engine: parent/child source documents, group
 * folding, per-source loading phases and reveal timers, the bounded builtin
 * fallback, and the initial (post-tutorial) refresh pipeline.
 */
export function useSourceRefresh(options: {
  dashboard: Ref<Dashboard | null>
  config: ComputedRef<PublicConfig | undefined>
  repos: ComputedRef<Repository[]>
  locale: Ref<Locale>
  communities: CommunityDiscovery[]
  sourceLoading: Record<string, boolean>
  sourceRefreshingManual: Record<string, boolean>
  repositoryRefreshing: Record<string, boolean>
  repositoryReleaseReady: Record<string, boolean>
  repositoryRefreshTasks: Map<string, Promise<boolean>>
  refreshRepositoryBatch: (repositories: Repository[], concurrency?: number) => Promise<number>
  refreshDashboardSerialized: () => Promise<Dashboard | null>
  refreshConfigSerialized: () => Promise<void>
  getUpdateQueue: () => Promise<void>
  isUnmounted: () => boolean
  isRepositoryRefreshBlocked: () => boolean
  isTutorialOverlayActive: () => boolean
  getTutorialRepositoryLoadingActive: () => boolean
  showError: (value: unknown) => void
  api: typeof api
}) {
  const {
    dashboard, config, repos, locale, communities, sourceLoading, sourceRefreshingManual,
    repositoryRefreshing, repositoryReleaseReady, repositoryRefreshTasks,
    refreshRepositoryBatch, refreshDashboardSerialized, refreshConfigSerialized,
    getUpdateQueue, isUnmounted, isRepositoryRefreshBlocked, isTutorialOverlayActive,
    getTutorialRepositoryLoadingActive, showError, api,
  } = options
  const sourceCollapsed = reactive<Record<string, boolean>>({})
  const repositoryGroupCollapsed = reactive<Record<string, boolean>>({})
  const sourceDocumentsLoading = reactive<Record<string, boolean>>({})
  const sourceRefreshPhase = reactive<Record<string, 'clearing' | 'loading' | 'adding' | 'refreshing'>>({})
  const sourceErrors = reactive<Record<string, string>>({})
  const sourceContentRevealing = reactive<Record<string, boolean>>({})
  const sourceContentRevealTimers = new Map<string, number>()
  let initialRepositoryRefreshStarted = false
  const initialRepositorySourcesLoaded = ref(false)
  let initialProxySync: Promise<void> = Promise.resolve()

  function sourceMatchesRegion(source: RepositorySource) {
    return (source.id !== GITEE_COMMUNITY_SOURCE_ID || communityProviderMatchesRegion('gitee', config.value?.region))
      && repositoryRegionMatches(source.region, config.value?.region)
  }

  const activeRepositorySources = computed(() => (config.value?.repository_sources ?? []).filter(sourceMatchesRegion))
  const activeRepositorySourceIds = computed(() => new Set(activeRepositorySources.value.map((source) => source.id)))

  function groupedRepositories(repositories: Repository[]) {
    const groups = new Map<string, Repository[]>()
    for (const repo of repositories) {
      const key = repositoryStableGroupKey(repo)
      const entries = groups.get(key) || []
      entries.push(repo)
      groups.set(key, entries)
    }
    return Array.from(groups, ([key, grouped]) => ({
      key,
      label: repositoryGroup(grouped[0], locale.value),
      repositories: grouped,
    }))
  }

  function flattenSourceGroups(group: SourceGroup, depth = 0): Array<SourceGroup & { depth: number }> {
    const current = depth === 0 && group.groups.length === 0 ? [] : [{ ...group, depth }]
    return [
      ...current,
      ...group.children.flatMap((child) => flattenSourceGroups(child, depth + 1)),
    ]
  }

  const sourceGroups = computed<SourceGroup[]>(() => {
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
    if (!isTutorialOverlayActive()) {
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
    ...(!isTutorialOverlayActive()
      ? communities.flatMap((discovery) => discovery.started.value && communityProviderMatchesRegion(discovery.provider, config.value?.region)
        ? discovery.repositories.value.map((repo) => repo.id) : []) : [])]))
  const displayedRepositoryCount = computed(() => listedRepositoryIds.value.size)
  const installableRepositoryCount = computed(() => repos.value.filter((repo) => listedRepositoryIds.value.has(repo.id)
    && repo.enabled
    && (repo.download_mode === 'repository_file'
      ? !!repo.branch.trim() && !!repo.file_path.trim()
      : repositoryReleaseReady[repo.id] && installReleases(repo).length > 0)).length)

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
      await Promise.all([...repositoryRefreshTasks.values(), getUpdateQueue()])
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
      if (isUnmounted()) return false
      initialRepositorySourcesLoaded.value = true
      return Promise.all(communities.map((discovery) => discovery.refresh()))
    })()
    return {
      results: Promise.all(tasks.map(async (task) => ({ source: task.source, result: await task.result }))),
      communityRefresh,
    }
  }

  async function refreshSource(sourceId: string, options: SourceRefreshOptions = {}, visited = new Set<string>()): Promise<SourceRefreshResult> {
    const discovery = communities.find((discovery) => discovery.sourceId === sourceId)
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
    if (isRepositoryRefreshBlocked()) {
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

  async function refreshAllRepositories() {
    if (isRepositoryRefreshBlocked()) {
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

  async function syncInitialGithubProxies() {
    try {
      const saved = await api.refreshGithubProxies()
      if (!isUnmounted() && dashboard.value) dashboard.value.config.github_proxies = saved.github_proxies
    } catch (reason) {
      console.warn('[proxy-sync] initial proxy synchronization failed; bundled proxies remain available', reason)
    }
  }

  function startInitialRepositoryRefresh() {
    if (isUnmounted()) return
    if (isRepositoryRefreshBlocked()) {
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
      if (isUnmounted()) return
      await refreshDashboardSerialized()
      if (isUnmounted()) return
      if (failedSources.length) showError(failedSources.map((entry) => `${localizedRepositorySourceName(entry.source)}: ${entry.result.error || t('刷新失败')}`).join('；'))
      if (releaseFailures) showError(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
      await batch.communityRefresh
    })().catch((reason) => showError(reason))
  }

  function communityForSource(sourceId: string) {
    return communities.find((discovery) => discovery.sourceId === sourceId)
  }

  function sourceRefreshing(sourceId: string) {
    const discovery = communityForSource(sourceId)
    if (discovery) return discovery.searching.value
    const tutorialRootSourceId = activeRepositorySources.value[0]?.id
    if (getTutorialRepositoryLoadingActive() && sourceId === tutorialRootSourceId) return true
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

  function repositoryGroupKey(sourceId: string, group: string) { return `${sourceId}:${group}` }

  function toggleRepositoryGroup(sourceId: string, group: string) {
    if (!group) return
    const key = repositoryGroupKey(sourceId, group)
    repositoryGroupCollapsed[key] = !repositoryGroupCollapsed[key]
  }

  function dispose() {
    for (const timer of sourceContentRevealTimers.values()) window.clearTimeout(timer)
    sourceContentRevealTimers.clear()
  }

  return {
    sourceCollapsed, repositoryGroupCollapsed, sourceDocumentsLoading, sourceRefreshPhase,
    sourceErrors, initialRepositorySourcesLoaded, activeRepositorySources, activeRepositorySourceIds,
    sourceGroups, flattenSourceGroups, displayedRepositoryCount, installableRepositoryCount,
    sourceMatchesRegion, sourceDescendantIds, refreshRootSources, refreshSource, refreshAllRepositories,
    refreshSourceReleases, startInitialRepositoryRefresh, sourceRefreshing, sourceCardLoading, sourceBusy,
    onSourceContentEntered, toggleSource, repositoryGroupKey, toggleRepositoryGroup,
    getInitialProxySync: () => initialProxySync, dispose,
  }
}

export type SourceRefresh = ReturnType<typeof useSourceRefresh>
