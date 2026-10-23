<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, onUpdated, ref, toRefs, type PropType } from 'vue'
import {
  AlertTriangle, ArrowLeft, ArrowRight, ChevronRight, Download, FileText, Globe, Pencil, Plus, RefreshCw, Trash2, Upload,
} from 'lucide-vue-next'
import { localizedRepositorySourceName, t } from '../i18n'
import type { RepositorySource } from '../types'
import { useListNavigationFocus } from '../composables/useListNavigationFocus'
import { isCommunityRepository } from '../communityRepositories'
import type { useCommunityRepositories } from '../composables/useCommunityRepositories'
import RepositoryDescription from './RepositoryDescription.vue'

const props = defineProps({
  focusRepositoryIds: { type: Array as PropType<string[]>, default: () => [] },
  communities: { type: Array as PropType<ReturnType<typeof useCommunityRepositories>[]>, required: true },
  config: null, busy: null, gameBlocked: null, repositoryRefreshBatchActive: null, repositoryRefreshing: null,
  sourceBusy: null, sourceRefreshing: null, sourceCardLoading: null, sourceRefreshingManual: null, sourceCollapsed: null,
  sourceRefreshPhase: null, sourceErrors: null,
  toggleSource: null, flattenSourceGroups: null, repositoryGroupCollapsed: null, repositoryGroupKey: null,
  toggleRepositoryGroup: null, localizedRepositoryName: null, localizedRepositoryNote: null,
  repositoryInstallStatus: null, openRepositoryMods: null, latest: null,
  formatDate: null, installRepo: null, refreshAllRepositories: null,
  refreshSource: null, editRepo: null, deleteRepo: null, repos: null,
  openRepositorySourceManager: null, exportRepositories: null, sourceGroups: null,
  repositorySourceRegionLabel: null,
})
const dynamicProps = props as Record<string, any>
const { config, busy, gameBlocked, repositoryRefreshBatchActive, repositoryRefreshing, sourceBusy, sourceRefreshing, sourceCardLoading, sourceRefreshingManual, sourceCollapsed, sourceRefreshPhase, sourceErrors, toggleSource, flattenSourceGroups, repositoryGroupCollapsed, repositoryGroupKey, toggleRepositoryGroup, localizedRepositoryName, localizedRepositoryNote, repositoryInstallStatus, latest, formatDate, installRepo, refreshAllRepositories, refreshSource, editRepo, deleteRepo, repos, openRepositorySourceManager, exportRepositories, sourceGroups, repositorySourceRegionLabel } = toRefs(dynamicProps)
const showRepositoryEmptyState = computed(() => !repos.value.length
  && !props.communities.some((discovery) => discovery.started.value)
  && !repositoryRefreshBatchActive.value
  && !(config.value?.repository_sources ?? []).some((source: RepositorySource) => sourceRefreshing.value(source.id)))
const repositoryList = ref<HTMLElement | null>(null)
const SOURCE_STICKY_GAP = 12
const focusedRepositories = useListNavigationFocus({
  list: repositoryList,
  targetIds: () => props.focusRepositoryIds,
  targetAttribute: 'data-repository-id',
  stickyHeadingSelector: ':scope > .source-sticky-stack, :scope > .repository-group-heading',
  highlightAllTargets: true,
  onFinished: () => emit('focus-finished'),
})
let stickyLayoutFrame: number | undefined
let stickyScrollElement: HTMLElement | null = null
let stickyResizeObserver: ResizeObserver | undefined

function communityForSource(sourceId: string) {
  return props.communities.find((discovery) => discovery.sourceId === sourceId)
}

function sourceOverlayHasError(sourceId: string) {
  const discovery = communityForSource(sourceId)
  if (discovery) return !!discovery.error.value
  return !!sourceErrors.value?.[sourceId]
}

function sourceOverlayMessage(sourceId: string) {
  const discovery = communityForSource(sourceId)
  if (discovery) return discovery.error.value
    ? `${t('社区仓库搜索失败')}: ${t(discovery.error.value)}`
    : t(discovery.provider === 'gitee' ? '正在搜索 Gitee 社区仓库，必要时会打开验证窗口；关闭验证窗口可取消。' : '正在搜索 GitHub 社区仓库')
  const error = sourceErrors.value?.[sourceId]
  if (error) return `${t('仓库源读取失败')}: ${t(error)}`
  const phase = sourceRefreshPhase.value?.[sourceId]
  return t(phase === 'clearing' ? '正在清理旧仓库' : phase === 'loading' ? '正在读取仓库源' : phase === 'adding' ? '正在添加仓库卡片' : '正在刷新仓库 Release')
}

function applyStickyPush(element: HTMLElement, stickyTop: number, boundaryTop: number) {
  const height = element.getBoundingClientRect().height
  const push = Math.min(0, boundaryTop - (stickyTop + height))
  element.style.setProperty('--sticky-push', `${push}px`)
  return push
}

function directRepositoryGroups(panel: HTMLElement) {
  const groups: HTMLElement[] = []
  for (const child of Array.from(panel.children)) {
    if (!(child instanceof HTMLElement)) continue
    if (child.classList.contains('repository-group')) {
      groups.push(child)
      continue
    }
    // The root source keeps its groups in one wrapper, while child source
    // panels expose groups directly. Do not descend into another child source
    // block, or a parent would treat the child's categories as its own.
    if (child.classList.contains('source-child-block')) continue
    groups.push(...Array.from(child.children).filter((entry): entry is HTMLElement =>
      entry instanceof HTMLElement && entry.classList.contains('repository-group')))
  }
  return groups
}

function repositoryGroupHeadings(panel: HTMLElement) {
  return directRepositoryGroups(panel)
    .map((group) => group.querySelector<HTMLElement>(':scope > .repository-group-heading'))
    .filter((heading): heading is HTMLElement => !!heading)
}

function sourceEndBoundary(block: HTMLElement) {
  const panel = block.querySelector<HTMLElement>(':scope > .source-content.child-source-panel')
  const groups = panel ? directRepositoryGroups(panel) : []
  const lastGroup = groups.at(-1)
  const items = lastGroup?.querySelector<HTMLElement>(':scope > .repository-group-items')
  return items?.getBoundingClientRect().top
    ?? lastGroup?.getBoundingClientRect().bottom
    ?? block.getBoundingClientRect().bottom
}

function updateStickyBoundaries() {
  stickyLayoutFrame = undefined
  const list = repositoryList.value
  const scrollElement = stickyScrollElement
  if (!list || !scrollElement) return

  const scrollRect = scrollElement.getBoundingClientRect()
  const sourceShells = Array.from(list.querySelectorAll<HTMLElement>(':scope > .source-section-shell'))

  for (const shell of sourceShells) {
    const stickyStack = shell.querySelector<HTMLElement>(':scope > .source-sticky-stack')
    const heading = stickyStack?.querySelector<HTMLElement>(':scope > .source-section-heading')
    if (!stickyStack || !heading) continue
    const panel = shell.querySelector<HTMLElement>(':scope > .source-content')
    const childBlocks = panel
      ? Array.from(panel.querySelectorAll<HTMLElement>(':scope > .source-child-block'))
      : []
    const rootGroupHeadings = panel ? repositoryGroupHeadings(panel) : []
    const lastChildBlock = childBlocks.at(-1)
    const lastRootGroup = panel ? directRepositoryGroups(panel).at(-1) : undefined
    const boundaryTop = lastChildBlock
      ? sourceEndBoundary(lastChildBlock)
      : lastRootGroup
        ? (lastRootGroup.querySelector<HTMLElement>(':scope > .repository-group-items')?.getBoundingClientRect().top
          ?? lastRootGroup.getBoundingClientRect().bottom)
        : shell.getBoundingClientRect().bottom
    const parentPush = applyStickyPush(stickyStack, scrollRect.top, boundaryTop)

    const stickyStackRect = stickyStack.getBoundingClientRect()
    const parentHeight = stickyStack.offsetHeight
    const visualScale = parentHeight > 0 ? stickyStackRect.height / parentHeight : 1
    const rootGroupTop = Math.max(0, parentHeight + parentPush / visualScale)
    for (const groupHeading of rootGroupHeadings) {
      groupHeading.style.setProperty('--repository-group-sticky-top', `${rootGroupTop}px`)
    }
    for (const block of childBlocks) {
      const childStack = block.querySelector<HTMLElement>(':scope > .child-source-sticky-stack')
      const childHeading = childStack?.querySelector<HTMLElement>(':scope > .child-source-section-heading')
      if (!childStack || !childHeading) continue
      const depth = Math.max(1, Number(block.dataset.sourceDepth) || 1)
      const stickyGap = SOURCE_STICKY_GAP * depth
      const stickyOffset = parentHeight * depth
      const stickyTopOffset = Math.max(stickyGap, stickyOffset + parentPush / visualScale)
      childStack.style.top = `${stickyTopOffset}px`
      const childBoundaryTop = sourceEndBoundary(block)
      applyStickyPush(childStack, scrollRect.top + stickyTopOffset * visualScale, childBoundaryTop)
      const childStackRect = childStack.getBoundingClientRect()
      const childTop = (childStackRect.top - scrollRect.top) / visualScale
      const childPanel = block.querySelector<HTMLElement>(':scope > .source-content.child-source-panel')
      if (childPanel) {
        for (const groupHeading of repositoryGroupHeadings(childPanel)) {
          groupHeading.style.setProperty(
            '--repository-group-sticky-top',
            `${Math.max(0, childTop + childStack.offsetHeight)}px`,
          )
        }
      }
    }
  }
}

function scheduleStickyBoundaries() {
  if (stickyLayoutFrame !== undefined) return
  stickyLayoutFrame = window.requestAnimationFrame(updateStickyBoundaries)
}

function connectStickyScrollElement() {
  const nextScrollElement = repositoryList.value?.closest<HTMLElement>('.main-scroll') || null
  if (nextScrollElement === stickyScrollElement) return
  stickyScrollElement?.removeEventListener('scroll', scheduleStickyBoundaries)
  stickyScrollElement = nextScrollElement
  stickyScrollElement?.addEventListener('scroll', scheduleStickyBoundaries, { passive: true })
}

onMounted(() => {
  connectStickyScrollElement()
  stickyResizeObserver = new ResizeObserver(scheduleStickyBoundaries)
  if (repositoryList.value) stickyResizeObserver.observe(repositoryList.value)
  void nextTick(scheduleStickyBoundaries)
})

onUpdated(() => {
  connectStickyScrollElement()
  void nextTick(scheduleStickyBoundaries)
})

onUnmounted(() => {
  if (stickyLayoutFrame !== undefined) window.cancelAnimationFrame(stickyLayoutFrame)
  stickyScrollElement?.removeEventListener('scroll', scheduleStickyBoundaries)
  stickyResizeObserver?.disconnect()
  stickyScrollElement = null
  stickyResizeObserver = undefined
})
const emit = defineEmits<{
  'focus-finished': []
  'show-import': []
  'show-repository-note': [repository: any]
  'show-repository-readme': [repository: any]
  'show-release': [repository: any]
  'close-export': []
  'source-content-entered': [sourceId: string]
  'show-github-proxies': []
}>()
</script>

<template>
<section class="view-content">
        <div ref="tutorialRepositoriesActions" class="page-intro">
          <div>
            <p>{{ t('内置和自定义仓库均从 Gitee/GitHub Release 获取版本与资产。MOD管理器仅做收集，不对文件做任何保证。') }}</p>
            <small class="muted-text">{{ t('当前MOD下载渠道') }}: {{ repositorySourceRegionLabel(config?.region) }}</small>
          </div>
          <div class="button-row"><button class="secondary-button"
              :disabled="!!busy || repositoryRefreshBatchActive || communities.some((discovery) => discovery.refreshing.value) || Object.values(sourceRefreshingManual).some(Boolean) || !(config?.repository_sources || []).length" @click="refreshAllRepositories">
              <RefreshCw :size="16" />{{ t('刷新全部仓库源') }}
            </button><button class="primary-button" @click="editRepo()">
              <Plus :size="16" />{{ t('添加仓库') }}
            </button><button class="primary-button" @click="openRepositorySourceManager">
              <Plus :size="16" />{{ t('管理仓库源') }}
            </button></div>
        </div>
        <div class="toolbar">
          <div class="toolbar-group"><button class="secondary-button" @click="emit('show-import')">
              <Download :size="15" />{{ t('导入配置') }}
            </button><button class="secondary-button" :disabled="!repos.length" @click="exportRepositories">
              <Upload :size="15" />{{ t('导出配置') }}
            </button><button class="secondary-button" @click="emit('show-github-proxies')">
              <Globe :size="15" />{{ t('GitHub 代理配置') }}
            </button></div>
        </div>
        <div ref="repositoryList" class="repository-list">
          <template v-for="group in sourceGroups" :key="group.source.id">
            <div :class="['source-section-shell', { 'source-section-loading': sourceCardLoading(group.source.id), 'community-source': !!group.community }]" :data-source-id="group.source.id">
            <div :class="['source-sticky-stack', { 'source-sticky-stack-collapsed': sourceCollapsed[group.source.id] && !sourceRefreshing(group.source.id) }]">
              <div :class="['source-section-heading', { 'source-section-heading-collapsed': sourceCollapsed[group.source.id] && !sourceRefreshing(group.source.id), 'community-source-heading': !!group.community }]">
                <button class="source-collapse-button"
                  :title="sourceCollapsed[group.source.id] ? t('展开') : t('折叠')" @click="toggleSource(group.source.id)">
                  <ChevronRight :size="18" :class="{ 'source-expanded': !sourceCollapsed[group.source.id] }" />
                </button>
                <div class="source-section-title">
                  <h3>{{ localizedRepositorySourceName(group.source) }}</h3>
                </div>
                <div v-if="group.community" class="community-pagination" :aria-label="t('社区仓库分页')">
                  <button class="icon-button small" :title="t('上一页')" :aria-label="t('上一页')"
                    :disabled="group.community.searching.value || group.community.page.value <= 1" @click="group.community.setPage(group.community.page.value - 1)"><ArrowLeft :size="16" /></button>
                  <span class="community-page-label" role="status">{{ group.community.page.value }} / {{ group.community.pageCount.value }}</span>
                  <button class="icon-button small" :title="t('下一页')" :aria-label="t('下一页')"
                    :disabled="group.community.searching.value || group.community.page.value >= group.community.pageCount.value" @click="group.community.setPage(group.community.page.value + 1)"><ArrowRight :size="16" /></button>
                </div>
                <button v-if="group.source.id !== 'unclassified'" class="icon-button small" :title="t('刷新仓库源')"
                  :disabled="!!busy || sourceBusy(group.source.id)"
                  @click="refreshSource(group.source.id)">
                  <RefreshCw :size="15"
                    :class="{ 'spin-icon': group.community ? group.community.refreshing.value : sourceRefreshing(group.source.id) }" />
                </button>
              </div>
              <div v-if="!sourceCollapsed[group.source.id] || sourceRefreshing(group.source.id)" class="source-sticky-gap" aria-hidden="true"></div>
            </div>
            <Transition name="source-collapse" @after-enter="emit('source-content-entered', group.source.id)">
              <div
                v-if="!sourceCollapsed[group.source.id] && sourceRefreshPhase[group.source.id] !== 'clearing' && sourceRefreshPhase[group.source.id] !== 'loading'"
                class="source-repository-panel source-content">
                <div v-if="group.community" class="community-discovery-notice">
                  <p class="community-safety-warning"><AlertTriangle :size="18" />{{ t(group.community.provider === 'gitee'
                    ? '以下为 Gitee 上带有 overcooked-2、bepinex、mod 标签的仓库，来源未知，安全性未知。请仔细检查仓库源码，再决定是否下载。'
                    : '以下为 GitHub 上带有 overcooked-2、bepinex、mod 标签的仓库，来源未知，安全性未知。请仔细检查仓库源码，再决定是否下载。') }}</p>
                  <p v-if="group.community.discovery.value" class="muted-text">{{ t('本轮发现 {count} 个社区仓库，每页显示 10 个。').replace('{count}', String(group.community.discoveredCount.value)) }}</p>
                  <p v-if="group.community.discovery.value?.truncated" class="community-discovery-warning">{{ t(group.community.provider === 'gitee'
                    ? 'Gitee 网页发现已达到安全上限，尚有仓库未展示。' : 'GitHub 搜索最多返回前 1000 个结果，尚有仓库未展示。') }}</p>
                  <p v-if="group.community.discovery.value?.incomplete_results" class="community-discovery-warning">{{ t(group.community.provider === 'gitee'
                    ? 'Gitee 网页结果未能完整识别，可稍后刷新重试。' : 'GitHub 返回的搜索结果不完整，可稍后刷新重试。') }}</p>
                  <p v-if="group.community.releaseFailures.value" class="community-discovery-warning">{{ t('部分社区仓库 Release 刷新失败，可在对应仓库单独刷新。') }}</p>
                  <p v-if="!group.community.searching.value && !group.community.repositories.value.length && !group.community.error.value" class="community-empty">{{ t('未发现符合标签的公开社区仓库') }}</p>
                </div>
                <template v-for="nestedGroup in flattenSourceGroups(group)"
                  :key="nestedGroup.source.id">
                  <div :class="nestedGroup.source.id === group.source.id ? undefined : ['source-child-block', { 'source-child-section-loading': sourceCardLoading(nestedGroup.source.id) }]"
                    :data-source-depth="nestedGroup.source.id === group.source.id ? undefined : nestedGroup.depth">
                    <div v-if="nestedGroup.source.id !== group.source.id"
                      :class="['source-sticky-stack', 'child-source-sticky-stack', { 'source-sticky-stack-collapsed': sourceCollapsed[nestedGroup.source.id] && !sourceRefreshing(nestedGroup.source.id) }]">
                      <div :class="['source-section-heading', 'child-source-section-heading', { 'source-section-heading-collapsed': sourceCollapsed[nestedGroup.source.id] && !sourceRefreshing(nestedGroup.source.id) }]">
                        <button class="source-collapse-button"
                          :title="sourceCollapsed[nestedGroup.source.id] ? t('展开') : t('折叠')"
                          @click="toggleSource(nestedGroup.source.id)">
                          <ChevronRight :size="18" :class="{ 'source-expanded': !sourceCollapsed[nestedGroup.source.id] }" />
                        </button>
                        <div class="source-section-title">
                          <h3>{{ localizedRepositorySourceName(nestedGroup.source) }}</h3>
                        </div>
                        <button class="icon-button small" :title="t('刷新仓库源')"
                          :disabled="!!busy || sourceBusy(nestedGroup.source.id)"
                          @click="refreshSource(nestedGroup.source.id)">
                          <RefreshCw :size="15" :class="{ 'spin-icon': sourceRefreshing(nestedGroup.source.id) }" />
                        </button>
                      </div>
                      <div v-if="!sourceCollapsed[nestedGroup.source.id] || sourceRefreshing(nestedGroup.source.id)" class="source-sticky-gap" aria-hidden="true"></div>
                    </div>
                    <Transition name="source-collapse" @after-enter="nestedGroup.source.id === group.source.id ? undefined : emit('source-content-entered', nestedGroup.source.id)">
                      <div v-if="nestedGroup.source.id === group.source.id || (!sourceCollapsed[nestedGroup.source.id] && sourceRefreshPhase[nestedGroup.source.id] !== 'clearing' && sourceRefreshPhase[nestedGroup.source.id] !== 'loading')"
                        :class="nestedGroup.source.id === group.source.id ? undefined : 'source-repository-panel source-content child-source-panel'">
                        <TransitionGroup name="repository-group-list" tag="div" class="repository-group-list">
                        <div v-for="repoGroup in nestedGroup.groups" :key="`${nestedGroup.source.id}:${repoGroup.key || 'ungrouped'}`"
                          :class="['repository-group', { 'repository-group-ungrouped': !repoGroup.label }]">
                          <button v-if="repoGroup.label" class="repository-group-heading"
                            :title="repositoryGroupCollapsed[repositoryGroupKey(nestedGroup.source.id, repoGroup.key)] ? t('展开分组') : t('折叠分组')"
                            @click="toggleRepositoryGroup(nestedGroup.source.id, repoGroup.key)">
                            <ChevronRight :size="15"
                              :class="{ 'source-expanded': !repositoryGroupCollapsed[repositoryGroupKey(nestedGroup.source.id, repoGroup.key)] }" />
                            <span>{{ repoGroup.label }}</span><span class="badge muted">{{ repoGroup.repositories.length }}</span>
                          </button>
                          <Transition name="repository-group-collapse">
                            <div
                              v-if="!repoGroup.label || !repositoryGroupCollapsed[repositoryGroupKey(nestedGroup.source.id, repoGroup.key)]"
                              class="repository-group-items">
                              <div v-for="repo in repoGroup.repositories" :key="repo.id"
                                :data-repository-id="repo.id" tabindex="-1"
                                :class="['repository-row', { 'repo-refreshing': repositoryRefreshing[repo.id], 'repository-focus': focusedRepositories.includes(repo.id) }]">
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
                                      class="badge muted">{{ t('内置') }}</span><span v-if="isCommunityRepository(repo)" class="badge muted">{{ t('社区 · 来源未知') }}</span><span v-if="isCommunityRepository(repo) && !group.community?.discoveredIds.value.has(repo.id)" class="badge muted">{{ t('已安装仓库，本轮未发现') }}</span><span v-if="!repo.enabled" class="badge muted">{{ t('未启用') }}</span><button v-if="['有更新', '最新版'].includes(repositoryInstallStatus(repo))" type="button"
                                      class="badge mod-update-link" :class="repositoryInstallStatus(repo) === '有更新' ? 'warning' : 'success'"
                                      @click="dynamicProps.openRepositoryMods(repo)">{{ t(repositoryInstallStatus(repo)) }}</button><span v-else-if="repositoryInstallStatus(repo)" class="badge"
                                      :class="['有更新', '检查中', '待检查'].includes(repositoryInstallStatus(repo)) ? 'warning' : 'success'">{{
                                        t(repositoryInstallStatus(repo)) }}</span>
                                  </div>
                                  <RepositoryDescription :repository="repo" :note="localizedRepositoryNote(repo)" :community="isCommunityRepository(repo)"
                                    @note="emit('show-repository-note', repo)" />
                                </div>
                                <div class="repo-controls">
                                  <button v-if="repo.download_mode === 'release'" type="button" class="repo-release"
                                    :title="t('查看 Release 日志')" :disabled="!latest(repo)" @click="$emit('show-release', repo)"><b>{{ latest(repo)?.version || latest(repo)?.tag_name || '—' }}</b><span>{{ latest(repo) ?
                                      formatDate(latest(repo)?.published_at) : t('待刷新') }}</span></button>
                                  <button class="primary-button small-button" :title="t('下载并安装')"
                                    :disabled="gameBlocked || !!repositoryRefreshing[repo.id] || (repo.download_mode === 'repository_file' && (!repo.branch?.trim() || !repo.file_path?.trim())) || !!busy"
                                    @click="installRepo(repo)">
                                    <Download :size="14" />{{ t('下载并安装') }}
                                  </button>
                                  <div class="row-actions">
                                    <button class="icon-button small" :title="t('查看仓库自述文件')"
                                      @click="$emit('show-repository-readme', repo)">
                                      <FileText :size="14" />
                                    </button><button v-if="!isCommunityRepository(repo)" class="icon-button small" :title="t('编辑仓库')" @click="editRepo(repo)">
                                      <Pencil :size="14" />
                                    </button><button v-if="!isCommunityRepository(repo)" class="icon-button small danger-icon" :title="t('删除仓库')"
                                      :disabled="repo.builtin" @click="deleteRepo(repo)">
                                      <Trash2 :size="14" />
                                    </button>
                                  </div>
                                </div>
                              </div>
                            </div>
                          </Transition>
                        </div>
                        </TransitionGroup>
                      </div>
                    </Transition>
                    <Transition name="source-loading">
                      <div v-if="nestedGroup.source.id !== group.source.id && (sourceRefreshing(nestedGroup.source.id) || sourceOverlayHasError(nestedGroup.source.id))"
                        class="source-loading-overlay child-source-loading">
                        <RefreshCw :size="42" :class="{ 'spin-icon': !sourceOverlayHasError(nestedGroup.source.id) }" />
                        <span class="source-loading-stage">{{ sourceOverlayMessage(nestedGroup.source.id) }}</span>
                      </div>
                    </Transition>
                  </div>
                </template>
              </div>
            </Transition>
            <Transition name="source-loading">
              <div v-if="sourceRefreshing(group.source.id) || sourceOverlayHasError(group.source.id)"
                class="source-loading-overlay">
                <RefreshCw :size="42" :class="{ 'spin-icon': !sourceOverlayHasError(group.source.id) }" />
                <span class="source-loading-stage">{{ sourceOverlayMessage(group.source.id) }}</span>
              </div>
            </Transition>
            </div>
          </template>
          <div v-if="showRepositoryEmptyState" class="empty-state">{{ t('暂无仓库配置') }}</div>
        </div>
      </section>
</template>
