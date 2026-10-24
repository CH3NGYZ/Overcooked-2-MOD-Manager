import { computed, ref, type ComputedRef, type Ref } from 'vue'
import type { Dashboard, PublicConfig, RepositorySource } from '../types'
import { t, localizedRepositorySourceName } from '../i18n'
import { repositorySourceKey, normalizeRepositoryRegion } from '../repositorySources'
import { isCommunitySource } from '../communityRepositories'
import type { SourceRefresh } from './useSourceRefresh'

export type RepositorySourceManagerEntry = { source: RepositorySource; depth: number }

/**
 * Draft-based repository source manager: add/edit/remove custom root sources,
 * drag-to-reorder roots with their descendants, then save and refresh through
 * the real source refresh engine.
 */
export function useRepositorySourceManager(options: {
  showRepositorySource: Ref<boolean>
  dashboard: Ref<Dashboard | null>
  config: ComputedRef<PublicConfig | undefined>
  sourceMatchesRegion: SourceRefresh['sourceMatchesRegion']
  refreshRootSources: SourceRefresh['refreshRootSources']
  getInitialProxySync: SourceRefresh['getInitialProxySync']
  refreshDashboardSerialized: () => Promise<unknown>
  isRepositoryRefreshBlocked: () => boolean
  run: <T>(label: string, action: () => Promise<T>) => Promise<T>
  requestConfirmation: (request: { title: string; message: string; danger?: boolean }) => Promise<boolean>
  showError: (value: unknown) => void
  api: typeof import('../services/tauri').api
}) {
  const { showRepositorySource, dashboard, config, sourceMatchesRegion, refreshRootSources, getInitialProxySync, refreshDashboardSerialized,
    isRepositoryRefreshBlocked, run, requestConfirmation, showError, api } = options
  const repositorySourceUrl = ref('')
  const repositorySourceNickname = ref('')
  const repositorySourceUniqueId = ref('')
  const repositorySourceRegion = ref('')
  const editingRepositorySourceId = ref<string | null>(null)
  const repositorySourceEditorOpen = ref(false)
  const repositorySourceDraft = ref<RepositorySource[]>([])
  const draggingSourceId = ref<string | null>(null)

const repositorySourceManagerEntries = computed<RepositorySourceManagerEntry[]>(() => {
  const sources = repositorySourceDraft.value
  const sourceIds = new Set(sources.map((source) => source.id))
  const visibleSourceIds = new Set(sources.filter(sourceMatchesRegion).map((source) => source.id))
  const childrenByParent = new Map<string, RepositorySource[]>()
  for (const source of sources) {
    if (!sourceMatchesRegion(source) || !source.parent_id || !sourceIds.has(source.parent_id)) continue
    const children = childrenByParent.get(source.parent_id) || []
    children.push(source)
    childrenByParent.set(source.parent_id, children)
  }
  const entries: RepositorySourceManagerEntry[] = []
  const visited = new Set<string>()
  const visit = (source: RepositorySource, depth: number) => {
    if (visited.has(source.id) || !visibleSourceIds.has(source.id)) return
    visited.add(source.id)
    entries.push({ source, depth })
    for (const child of childrenByParent.get(source.id) || []) visit(child, depth + 1)
  }
  for (const source of sources) {
    if ((!source.parent_id || !sourceIds.has(source.parent_id)) && visibleSourceIds.has(source.id)) visit(source, 0)
  }
  return entries
})

function resetRepositorySourceEditor() {
  repositorySourceUrl.value = ''
  repositorySourceNickname.value = ''
  repositorySourceUniqueId.value = ''
  repositorySourceRegion.value = ''
  editingRepositorySourceId.value = null
  repositorySourceEditorOpen.value = false
}
function openRepositorySourceManager() {
  repositorySourceDraft.value = (config.value?.repository_sources ?? []).filter((source) => !isCommunitySource(source.id)).map((source) => ({ ...source }))
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
  repositorySourceUniqueId.value = ''
  repositorySourceRegion.value = ''
  editingRepositorySourceId.value = null
  repositorySourceEditorOpen.value = true
}
function editRepositorySource(source: RepositorySource) {
  if (source.parent_id) return
  editingRepositorySourceId.value = source.id
  repositorySourceNickname.value = source.nickname
  repositorySourceUrl.value = source.url
  repositorySourceUniqueId.value = source.id
  repositorySourceRegion.value = normalizeRepositoryRegion(source.region)
  repositorySourceEditorOpen.value = true
}
function applyRepositorySourceDraft() {
  const sourceId = repositorySourceUniqueId.value.trim()
  const sourceUrl = repositorySourceUrl.value.trim()
  const nickname = repositorySourceNickname.value.trim()
  if (!sourceId || !sourceUrl || !nickname) return
  if (!editingRepositorySourceId.value && !/^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/.test(sourceId)) {
    showError(t('仓库唯一ID格式无效'))
    return
  }
  const duplicateId = repositorySourceDraft.value.some((source) => source.id !== editingRepositorySourceId.value && source.id === sourceId)
  if (duplicateId) {
    showError(t('仓库唯一ID已存在'))
    return
  }
  const sourceKey = repositorySourceKey({ url: sourceUrl, region: repositorySourceRegion.value })
  const duplicate = repositorySourceDraft.value.some((source) => source.id !== editingRepositorySourceId.value && repositorySourceKey(source) === sourceKey)
  if (duplicate) {
    showError(t('仓库源地址已存在'))
    return
  }
  if (editingRepositorySourceId.value) {
    const source = repositorySourceDraft.value.find((item) => item.id === editingRepositorySourceId.value)
    if (source && !source.builtin) {
      source.nickname = nickname
      source.url = sourceUrl
      source.region = repositorySourceRegion.value.trim() || undefined
    }
  } else {
    repositorySourceDraft.value.push({
      id: sourceId,
      nickname,
      url: sourceUrl,
      region: repositorySourceRegion.value.trim() || undefined,
      builtin: false,
    })
  }
  resetRepositorySourceEditor()
}
async function removeRepositorySource(source: RepositorySource) {
  if (source.builtin || source.parent_id || !await requestConfirmation({ title: t('删除仓库源'), message: `${t('删除仓库源')} “${localizedRepositorySourceName(source)}”?`, danger: true })) return
  const removedIds = new Set([source.id])
  let changed = true
  while (changed) {
    changed = false
    for (const item of repositorySourceDraft.value) {
      if (item.parent_id && removedIds.has(item.parent_id) && !removedIds.has(item.id)) {
        removedIds.add(item.id)
        changed = true
      }
    }
  }
  repositorySourceDraft.value = repositorySourceDraft.value.filter((item) => !removedIds.has(item.id))
  if (editingRepositorySourceId.value === source.id) resetRepositorySourceEditor()
}
function startSourceDrag(id: string) {
  const entry = repositorySourceManagerEntries.value.find((item) => item.source.id === id)
  if (!entry || entry.depth > 0 || entry.source.parent_id || entry.source.builtin) return
  draggingSourceId.value = id
}
function dropSource(id: string) {
  if (!draggingSourceId.value || draggingSourceId.value === id) return
  const sources = [...repositorySourceDraft.value]
  const entries = repositorySourceManagerEntries.value
  const movingEntry = entries.find((entry) => entry.source.id === draggingSourceId.value)
  const targetEntry = entries.find((entry) => entry.source.id === id)
  if (!movingEntry || !targetEntry || movingEntry.depth > 0 || targetEntry.depth > 0 || movingEntry.source.parent_id || targetEntry.source.parent_id || movingEntry.source.builtin || targetEntry.source.builtin) {
    draggingSourceId.value = null
    return
  }
  const roots = entries.filter((entry) => entry.depth === 0).map((entry) => entry.source)
  const from = roots.findIndex((source) => source.id === movingEntry.source.id)
  const to = roots.findIndex((source) => source.id === targetEntry.source.id)
  if (from < 0 || to < 0) {
    draggingSourceId.value = null
    return
  }
  const [source] = roots.splice(from, 1)
  roots.splice(to, 0, source)
  const childrenByParent = new Map<string, RepositorySource[]>()
  for (const item of sources) {
    if (!item.parent_id) continue
    const children = childrenByParent.get(item.parent_id) || []
    children.push(item)
    childrenByParent.set(item.parent_id, children)
  }
  const ordered: RepositorySource[] = []
  const visited = new Set<string>()
  const append = (item: RepositorySource) => {
    if (visited.has(item.id)) return
    visited.add(item.id)
    ordered.push(item)
    for (const child of childrenByParent.get(item.id) || []) append(child)
  }
  roots.forEach(append)
  sources.forEach(append)
  repositorySourceDraft.value = ordered
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
  const normalizedUrls = draft.map(repositorySourceKey)
  if (new Set(normalizedUrls).size !== normalizedUrls.length) {
    showError(t('仓库源地址已存在'))
    return
  }
  let savedSourceConfig: PublicConfig | null = null
  try {
    await run('正在保存仓库源', async () => {
      const sourceConfig = await api.saveRepositorySources(draft)
      closeRepositorySourceManager()
      const savedDashboard = await api.dashboard()
      const savedSourceDashboard = { ...savedDashboard, config: sourceConfig }
      dashboard.value = savedSourceDashboard
      savedSourceConfig = sourceConfig
      return sourceConfig
    })
  } catch {
    return
  }
  if (!savedSourceConfig) return
  if (isRepositoryRefreshBlocked()) {
    showError(t('教程完成后才能刷新内置源'))
    return
  }
  const refreshedSourceConfig = savedSourceConfig as PublicConfig
  void (async () => {
    try {
      const rootSources = refreshedSourceConfig.repository_sources
        .filter((item) => !item.parent_id && !isCommunitySource(item.id) && sourceMatchesRegion(item))
      const batch = await refreshRootSources(rootSources)
      const sourceResults = await batch.results
      const failures = sourceResults
        .filter((entry) => !entry.result.ok)
        .map((entry) => `${localizedRepositorySourceName(entry.source)}: ${entry.result.error || t('刷新失败')}`)
      const releaseFailures = sourceResults.reduce((total, entry) => total + entry.result.releaseFailures, 0)
      if (releaseFailures) failures.push(t('仓库刷新失败摘要').replace('{count}', String(releaseFailures)))
      await getInitialProxySync()
      await refreshDashboardSerialized()
      await batch.communityRefresh
      if (failures.length) showError(failures.join('；'))
    } catch (reason) {
      showError(reason)
    }
  })()
}

  return {
    showRepositorySource, repositorySourceUrl, repositorySourceNickname, repositorySourceUniqueId,
    repositorySourceRegion, editingRepositorySourceId, repositorySourceEditorOpen, repositorySourceDraft,
    draggingSourceId, repositorySourceManagerEntries, resetRepositorySourceEditor, openRepositorySourceManager, closeRepositorySourceManager,
    startAddingRepositorySource, editRepositorySource, applyRepositorySourceDraft, removeRepositorySource,
    startSourceDrag, dropSource, saveRepositorySourceChanges,
  }
}

export type RepositorySourceManager = ReturnType<typeof useRepositorySourceManager>
