<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NCollapse, NCollapseItem, NDrawer, NDrawerContent, NEmpty, NFloatButton, NInput, NSpin, NTag } from 'naive-ui'
import { ArrowUpCircleOutline, DownloadOutline, LinkOutline, RefreshOutline, SearchOutline } from '@vicons/ionicons5'
import { useDashboard } from '../composables/useDashboardStore'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import { formatVersion, isPrereleaseVersion, modsForRepository } from '../modStatus'
import { repositoryInstallAction, repositoryStatus, type InstallAction, type StatusTone } from '../statusTags'
import { useFocusHighlight, useLocalModLink } from '../crossLink'
import { locale, t } from '../i18n'
import { buttonColorFor, tagColorFor, tagColorForAny } from '../theme'
import { repositoryMatchesRegion, repositoryRegionLabel, sourceMatchesRegion } from '../repositorySources'
import { markdownSummary } from '../markdown'
import RepositoryDetail from '../components/RepositoryDetail.vue'
import ModInstallDialog from '../components/ModInstallDialog.vue'
import ModInstallSourceDialog from '../components/ModInstallSourceDialog.vue'
import { useModInstallFlow } from '../composables/useModInstallFlow'
import { useRepositoryPreview } from '../composables/useRepositoryPreview'
import ProviderIcon from '../components/ProviderIcon.vue'
import type { Repository, RepositorySource } from '../types'

const message = useMessage()
const { load, ensureLoaded, loading, config, game, mods } = useDashboard()

/** 刷新引擎是模块级单例：启动刷新与页面挂载解耦，教程门控也在那里。 */
const engine = useRepositoryRefresh()
const { refreshing, sourceRefreshing, anyRefreshing } = engine
/** 状态标签跳转：点标签跳到 MOD 管理页，并高亮绑定该仓库的 MOD 卡片 3 秒。 */
const { linkToLocalMods } = useLocalModLink()
/** 从 MOD 管理页跳回来时，高亮对应仓库卡片 3 秒并滚动到它。 */
const { highlightId } = useFocusHighlight()
const installing = ref<Record<string, boolean>>({})
const search = ref('')
/** 当前展开详情的仓库（同页展开，不做二级路由）。 */
const detailId = ref<string | null>(null)
/**
 * 安装流程：来源选择 → 预览 → 确认安装。
 *
 * 仓库卡片的「安装 / 重装 / 更新」也走同一套——先用 preview_only 让后端
 * 下载解析，再把结果交给预览弹窗，用户在弹窗里确认后才写入游戏目录。
 */
const {
  sourceOpen: installSourceOpen,
  previewOpen: installPreviewOpen,
  preview: installPreview,
  openSource: openInstallSource,
  openPreview: openInstallPreview,
  discardPreview: discardInstallPreview,
  handleInstalled: handleInstallInstalled,
} = useModInstallFlow(() => load())

/** 仓库卡片 / 详情抽屉的「安装 / 重装 / 更新」：先下载预览，再交给统一弹窗。 */
const { previewRepository } = useRepositoryPreview(openInstallPreview, {
  gameBlocked,
  onBusyChange: (repositoryId, busy) => {
    if (busy) installing.value[repositoryId] = true
    else delete installing.value[repositoryId]
  },
})
function openInstall() {
  openInstallSource()
}

const COMMUNITY_IDS = new Set(['github-community', 'gitee-community'])

// 进入页面只复用现有数据，不触发刷新也不重新扫描；
// 刷新由手动按钮、安装器启动或教程结束后的链路触发。
onMounted(() => { void ensureLoaded() })

/**
 * 主按钮动作：刷新失败 → 刷新，否则安装 / 重装 / 更新。
 *
 * 刷新失败时仓库没有可用 Release，此时「安装」点下去只会提示先刷新，
 * 用户还得再找一个刷新入口。把同一个按钮换成「刷新」，
 * 用户对着同一个位置点一次就能重试。
 */
function installActionOf(repo: Repository): InstallAction | 'refresh' {
  if (statusOf(repo).key === 'refresh_failed') return 'refresh'
  return repositoryInstallAction(statusOf(repo), repo)
}

const INSTALL_LABELS: Record<InstallAction | 'refresh', string> = {
  install: '安装',
  reinstall: '重装',
  update: '更新',
  refresh: '刷新',
}
function installLabel(repo: Repository) {
  return t(INSTALL_LABELS[installActionOf(repo)])
}

/**
 * 主按钮的配色：动作 → 语义色调 + 视觉强度。
 *
 * 规则与标签共用同一个 tone 刻度（src/theme.ts）：
 *   安装 / 重装 → accent 实心（「装到最新」这一档，不需要警告）
 *   更新        → moderate 实心（需要用户注意的动作）
 *   刷新（失败重试）→ moderate 浅底（提示可以点这里，不该比旁边的标签更抢眼）
 */
function installButtonStyle(repo: Repository) {
  const action = installActionOf(repo)
  if (action === 'refresh') return buttonColorFor('moderate', 'soft')
  if (action === 'update') return buttonColorFor('moderate', 'solid')
  return buttonColorFor('accent', 'solid')
}

/** 主按钮：刷新失败时重试该仓库的 Release，其余走安装预览链路。 */
function runCardAction(repo: Repository) {
  if (installActionOf(repo) === 'refresh') void refreshRepository(repo)
  else void install(repo)
}

/** 单仓刷新：与批量刷新共用引擎，只更新这一条仓库的缓存。 */
async function refreshRepository(repo: Repository) {
  const error = await engine.refreshOne(repo)
  if (error) message.error(error)
}

/** 该仓库是否有已安装的本地 MOD（决定状态标签能否跳到 MOD 管理）。 */
function hasLocalInstall(repo: Repository) {
  return modsForRepository(mods.value, repo).length > 0
}

/** 点击状态标签：跳到 MOD 管理页并高亮绑定该仓库的 MOD 卡片。 */
function openInLocalMods(repo: Repository) {
  if (hasLocalInstall(repo)) linkToLocalMods(repo.id)
}

const currentRegion = computed(() => config.value?.region)
const regionLabel = computed(() => repositoryRegionLabel(currentRegion.value))

/** 仅保留当前下载渠道适用的仓库。 */
const repositories = computed<Repository[]>(() =>
  (config.value?.repositories ?? []).filter((repo) => repositoryMatchesRegion(repo, currentRegion.value)),
)
const sources = computed<RepositorySource[]>(() =>
  (config.value?.repository_sources ?? []).filter((source) => sourceMatchesRegion(source, currentRegion.value)),
)
const detailRepo = computed(() => repositories.value.find((repo) => repo.id === detailId.value) ?? null)

/** 社区仓库（github-community / gitee-community）不展示分组标题，全部按未分组处理。 */
function isCommunityRepo(repo: Repository) {
  return COMMUNITY_IDS.has(repo.source_id ?? '')
}

/**
 * 官方仓库源里已经包含的仓库，在社区源里要隐藏（同一 owner/repository 视为重复）。
 * 比较使用 provider + username + repository 的小写组合，避免大小写差异漏判。
 */
const officialRepoKeys = computed(() => {
  const keys = new Set<string>()
  for (const repo of repositories.value) {
    if (isCommunityRepo(repo)) continue
    keys.add(`${repo.provider}:${repo.username.toLowerCase()}/${repo.repository.toLowerCase()}`)
  }
  return keys
})
function repoKey(repo: Repository) {
  return `${repo.provider}:${repo.username.toLowerCase()}/${repo.repository.toLowerCase()}`
}
function isDuplicateOfOfficial(repo: Repository) {
  return isCommunityRepo(repo) && officialRepoKeys.value.has(repoKey(repo))
}


/** 多语言取值：优先当前语言 → 中文 → 首个 → 原字段。 */
function pickI18n(map: Record<string, string> | undefined, fallback: string): string {
  if (!map) return fallback
  return map[locale.value] || map['zh-CN'] || Object.values(map)[0] || fallback
}
function repoName(repo: Repository) {
  return pickI18n(repo.name_i18n, repo.name)
}
function repoGroup(repo: Repository) {
  return pickI18n(repo.group_i18n, repo.group || '') || t('未分组')
}
/**
 * 卡片简介：note_i18n 取当前语言，用统一的 Markdown 渲染取纯文本摘要
 * （卡片是摘要，完整 Markdown 在详情抽屉里渲染）。
 */
function repoNote(repo: Repository) {
  return markdownSummary(pickI18n(repo.note_i18n, repo.note || ''))
}

/* ===== 搜索 ===== */
const query = computed(() => search.value.trim().toLowerCase())
function matches(repo: Repository) {
  if (!query.value) return true
  const haystack = [
    repoName(repo),
    repo.name,
    repo.note,
    ...Object.values(repo.note_i18n ?? {}),
    repoGroup(repo),
    repo.group ?? '',
    repo.username,
    repo.repository,
    repo.provider,
  ].join(' ').toLowerCase()
  return haystack.includes(query.value)
}

const visibleCount = computed(() => repositories.value.filter(matches).length)

/* ===== 分组树 ===== */
interface GroupNode { key: string; label: string; repos: Repository[] }
interface SourceNode {
  source: RepositorySource
  label: string
  groups: GroupNode[]
  count: number
  community: boolean
  /** 社区源只有一组且不显示分组标题。 */
  flat: boolean
}

const openSources = ref<string[]>([])
const openGroups = ref<string[]>([])

const tree = computed<SourceNode[]>(() => {
  const bySource = new Map<string, Repository[]>()
  for (const repo of repositories.value) {
    if (!matches(repo)) continue
    // 官方源已收录的仓库不在社区源里重复展示
    if (isDuplicateOfOfficial(repo)) continue
    const key = repo.source_id ?? '__none__'
    const list = bySource.get(key)
    if (list) list.push(repo)
    else bySource.set(key, [repo])
  }

  const nodes: SourceNode[] = []
  const push = (source: RepositorySource, repos: Repository[]) => {
    if (COMMUNITY_IDS.has(source.id)) {
      // 社区仓库统一视为未分组：单组、无标题，直接平铺卡片。
      nodes.push({
        source,
        label: pickI18n(source.nickname_i18n, source.nickname),
        groups: [{ key: `${source.id}::flat`, label: '', repos }],
        count: repos.length,
        community: true,
        flat: true,
      })
      return
    }
    const buckets = new Map<string, Repository[]>()
    for (const repo of repos) {
      const label = repoGroup(repo)
      const list = buckets.get(label)
      if (list) list.push(repo)
      else buckets.set(label, [repo])
    }
    const groups: GroupNode[] = [...buckets.entries()]
      .sort(([a], [b]) => (a === t('未分组') ? 1 : b === t('未分组') ? -1 : a.localeCompare(b)))
      .map(([label, list]) => ({ key: `${source.id}::${label}`, label, repos: list }))
    nodes.push({
      source,
      label: pickI18n(source.nickname_i18n, source.nickname),
      groups,
      count: repos.length,
      community: false,
      flat: false,
    })
  }

  // 按 config.repository_sources 的持久化顺序输出（读取顺序 = 展示顺序）
  for (const source of sources.value) {
    const repos = bySource.get(source.id)
    if (!repos?.length) continue
    push(source, repos)
    bySource.delete(source.id)
  }
  const orphan = bySource.get('__none__')
  if (orphan?.length) push({ id: '__none__', nickname: t('未归类仓库'), url: '', builtin: false }, orphan)

  // 社区源固定放最下方（内部仍保持原相对顺序）
  const normal = nodes.filter((node) => !node.community)
  const community = nodes.filter((node) => node.community)
  return [...normal, ...community]
})

const hasCommunity = computed(() => tree.value.some((node) => node.community))
const normalTree = computed(() => tree.value.filter((node) => !node.community))
const communityTree = computed(() => tree.value.filter((node) => node.community))

// 默认全部展开（源与分类），搜索时也保持展开以便直接看到结果。
const allSourceKeys = computed(() => tree.value.map((node) => node.source.id))
const allGroupKeys = computed(() => tree.value.flatMap((node) => (node.flat ? [] : node.groups.map((group) => group.key))))
const sourcesExpanded = computed(() => (query.value ? allSourceKeys.value : (openSources.value.length ? openSources.value : allSourceKeys.value)))
const groupsExpanded = computed(() => (query.value ? allGroupKeys.value : (openGroups.value.length ? openGroups.value : allGroupKeys.value)))

/* ===== 动作 ===== */

function latest(repo: Repository) {
  return repo.cached_releases[0]
}

/**
 * 该 Release 是否按预发布对待：元数据标记 **或** 版本号带 `-` 后缀。
 * `0.4.0-beta`、`0.4.0.5-beta` 即使作者没勾 prerelease 也算。
 */
function releaseIsPrerelease(release: { prerelease: boolean; version?: string | null; tag_name?: string }) {
  return isPrereleaseVersion(release.version || release.tag_name, release.prerelease)
}

/**
 * 卡片版本号：统一规范化后再显示（`0.4.0` / `0.4.0.0` 都显示成 `0.4`）。
 * 预发布判定取「元数据标记」或「版本号带 - 后缀」的或关系。
 */
function versionLabel(repo: Repository) {
  const release = latest(repo)
  if (!release) return '—'
  const value = formatVersion(release.version || release.tag_name) || '—'
  return releaseIsPrerelease(release) ? `${value} · ${t('预发布')}` : value
}

/** 下载量：优先最新 Release 首个匹配资产的 download_count。 */
function downloadCount(repo: Repository) {
  const release = latest(repo)
  if (!release) return null
  const total = release.assets.reduce((sum, asset) => sum + (asset.download_count ?? 0), 0)
  return total || null
}

/**
 * 卡片状态标签：统一走共享的 statusTags 模块（与 MOD 管理页同一套文案与配色）。
 * 已安装状态依据「已安装 MOD 的版本」与「仓库最新 Release 版本」比较。
 */
interface RepoStatus { key: string; tone: StatusTone; text: string }
function repoStatus(repo: Repository): RepoStatus {
  const status = repositoryStatus(repo, mods.value)
  return { key: status.key, tone: status.tone, text: t(status.text) }
}

/** 每个仓库只算一次状态，模板与筛选共用。 */
const statusByRepo = computed(() => {
  const map = new Map<string, RepoStatus>()
  for (const repo of repositories.value) map.set(repo.id, repoStatus(repo))
  return map
})
function statusOf(repo: Repository): RepoStatus {
  return statusByRepo.value.get(repo.id) ?? { key: 'not_refreshed', tone: 'idle', text: t('未刷新') }
}

/**
 * 单个源的刷新按钮：先重拉该源文档（父源会连带刷新它声明的子源），
 * 再并行刷新刷新后属于这条源链的仓库。
 */
async function refreshSource(node: SourceNode) {
  // refreshSourceChain 内部已按租用完 reloadConfig，这里不再全量 load
  const errors = await engine.refreshSourceChain(node.source.id)
  errors.forEach((error) => message.error(error))
  const ids = new Set<string>()
  const collect = (sourceId: string) => {
    ids.add(sourceId)
    for (const child of sources.value.filter((entry) => entry.parent_id === sourceId)) collect(child.id)
  }
  collect(node.source.id)
  const scoped = repositories.value.filter((repo) => repo.enabled && repo.source_id && ids.has(repo.source_id))
  const releaseErrors = await engine.refreshMany(scoped)
  releaseErrors.forEach((error) => message.error(error))
}

/** 顶栏「刷新全部仓库源」与 FAB 共用入口。 */
async function refreshAll() {
  const errors = await engine.refreshEverything()
  errors.forEach((error) => message.error(error))
}

/**
 * 分组刷新：只刷新本组内的仓库 Release，不重拉源文档、不影响其它组。
 * 源文档（分组 / 仓库列表的来源）不在这里动，避免只点一个分组就把整棵源树清空重载。
 */
async function refreshGroup(group: GroupNode) {
  const errors = await engine.refreshMany(group.repos.filter((repo) => repo.enabled))
  errors.forEach((error) => message.error(error))
}

/** 该组是否有仓库正在刷新（决定分组按钮的转圈）。 */
function groupRefreshing(group: GroupNode) {
  return group.repos.some((repo) => refreshing.value[repo.id])
}

function gameBlocked() {
  if (game.value?.kitchen_running) {
    message.warning(t('游戏运行中，已禁止安装。请先关闭游戏。'))
    return true
  }
  return false
}

/**
 * 仓库卡片的「安装 / 重装 / 更新」。
 *
 * 流程与手动安装统一：先让后端**只下载并解析**（preview_only），
 * 把解析结果交给 ModInstallDialog 展示，用户点「立即安装」才真正写入。
 * 下载与预发布确认都在 useRepositoryPreview 里，两个页面共用。
 */
function install(repo: Repository, tag?: string) {
  return previewRepository(repo, tag)
}

defineExpose({ refreshAll, visibleCount })
</script>

<template>
    <!-- 页头 h1 右侧：当前下载渠道 -->
    <Teleport to="#page-header-title-extra" defer>
      <n-tag size="small" :bordered="false" type="info">{{ regionLabel }}</n-tag>
    </Teleport>

    <!--
      页头右侧，从左到右：计数 → 搜索 → 刷新全部仓库源 → 安装 MOD。
      「安装 MOD」固定在最右：它是本页最主要的动作入口，
      和 MOD 管理页原来的位置一致，只是搬到了本页。
    -->
    <Teleport to="#page-header-actions" defer>
      <span class="header-count muted">{{ t('共 {count} 个仓库', { count: visibleCount }) }}</span>
      <n-input v-model:value="search" clearable size="small" class="header-search"
        :placeholder="t('搜索')">
        <template #prefix><SearchOutline width="14" height="14" /></template>
      </n-input>
      <n-button size="small" secondary :loading="anyRefreshing" :disabled="anyRefreshing" @click="refreshAll">
        <template #icon><RefreshOutline width="14" height="14" /></template>
        {{ t('刷新全部仓库源') }}
      </n-button>
      <n-button size="small" type="primary" @click="openInstall">
        <LinkOutline width="14" height="14" />{{ t('安装 MOD') }}
      </n-button>
    </Teleport>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !visibleCount" :description="t('没有匹配的仓库')" class="empty" />

    <template v-else>
      <n-collapse :expanded-names="sourcesExpanded" class="source-collapse"
        @update:expanded-names="(names: string[]) => (openSources = names)">
        <n-collapse-item v-for="node in normalTree" :key="node.source.id" :name="node.source.id">
          <template #header>
            <div class="source-head">
              <b>{{ node.label }}</b>
              <n-tag size="tiny" :bordered="false">{{ node.count }}</n-tag>
              <n-button size="tiny" quaternary
                :loading="sourceRefreshing[node.source.id] || node.groups.some((g) => g.repos.some((r) => refreshing[r.id]))"
                :disabled="anyRefreshing" @click.stop="refreshSource(node)">
                <template #icon><RefreshOutline width="13" height="13" /></template>
              </n-button>
            </div>
          </template>

          <n-collapse :expanded-names="groupsExpanded" class="group-collapse"
            @update:expanded-names="(names: string[]) => (openGroups = names)">
            <n-collapse-item v-for="group in node.groups" :key="group.key" :name="group.key">
              <template #header>
                <div class="group-head">
                  <span>{{ group.label }}</span>
                  <n-tag size="tiny" :bordered="false">{{ group.repos.length }}</n-tag>
                  <!-- 分组刷新：只刷新本组内的仓库，不影响其它组 -->
                  <n-button size="tiny" quaternary class="group-refresh"
                    :loading="groupRefreshing(group)"
                    :disabled="anyRefreshing" @click.stop="refreshGroup(group)">
                    <template #icon><RefreshOutline width="12" height="12" /></template>
                  </n-button>
                </div>
              </template>

              <div class="repo-grid">
                <article v-for="repo in group.repos" :key="repo.id" class="repo-card" :data-focus-id="repo.id"
                  :class="{ active: detailId === repo.id, 'focus-flash': highlightId === repo.id }"
                  @click="detailId = repo.id">
                  <div class="card-head">
                    <ProviderIcon :provider="repo.provider" :size="30" />
                    <span class="card-title">{{ repoName(repo) }}</span>
                    <span class="stat stat-status">
                      <!--
                        标签配色：未安装类一律灰色；「已安装且可更新」用主题色对比色，
                        与「重装 / 更新」按钮同色系；其余状态沿用状态语义色。
                      -->
                      <!--
                        标签配色：未安装类灰色；最新版/已安装用主题色；
                        可更新用对比色。全部由 theme.ts 的 statusToneColors 计算，
                        通过 n-tag 原生 :color 下发，页面里不再写颜色。
                      -->
                      <n-tag size="tiny" :bordered="false" class="state-tag"
                        :color="tagColorForAny(statusOf(repo).tone)"
                        :class="{ 'state-tag--link': hasLocalInstall(repo) }"
                        :title="hasLocalInstall(repo) ? t('在 MOD 管理中查看') : undefined"
                        @click.stop="openInLocalMods(repo)">{{ statusOf(repo).text }}</n-tag>
                    </span>
                  </div>

                  <p class="card-desc">{{ repoNote(repo) || repo.repository }}</p>

                  <div class="card-foot">
                    <span class="stat stat-version">
                      <span class="muted">{{ t('最新版本') }}</span>
                      <span class="version-line">
                        <b class="mono" :class="{ prerelease: !!latest(repo) && releaseIsPrerelease(latest(repo)!) }">{{ versionLabel(repo) }}</b>
                        <!-- 刷新状态：转圈即表示该仓库正在拉取 Release -->
                        <RefreshOutline v-if="refreshing[repo.id]" class="spin-icon" width="12" height="12" />
                      </span>
                    </span>
                    <span class="stat">
                      <span class="muted">{{ t('下载量') }}</span>
                      <b>{{ downloadCount(repo)?.toLocaleString() ?? '—' }}</b>
                    </span>
                    <!--
                      主按钮：安装（主题色）/ 重装、更新（对比色）/ 刷新（刷新失败的警告色）。
                      文案、图标与动作都由 installActionOf 决定，不在模板里各自判断。
                    -->
                    <n-button size="tiny" class="card-install"
                      :style="installButtonStyle(repo)"
                      :loading="!!installing[repo.id]"
                      :disabled="!!game?.kitchen_running || !!refreshing[repo.id]" @click.stop="runCardAction(repo)">
                      <!--
                        图标必须放在 #icon 具名槽：Naive 只用 IconSwitchTransition
                        切换这个槽位的内容（图标 ↔ spinner）。放在默认插槽里会被
                        渲染两份 —— 默认插槽的 children 既进图标槽、又作为文字前缀
                        再渲染一次，loading 时就出现「spinner + SVG 并存」。
                      -->
                      <template #icon>
                        <RefreshOutline v-if="installActionOf(repo) === 'refresh'" width="13" height="13" />
                        <RefreshOutline v-else-if="installActionOf(repo) === 'reinstall'" width="13" height="13" />
                        <ArrowUpCircleOutline v-else-if="installActionOf(repo) === 'update'" width="13" height="13" />
                        <DownloadOutline v-else width="13" height="13" />
                      </template>
                      {{ installLabel(repo) }}
                    </n-button>
                  </div>

                  <div v-if="repo.last_error" class="err muted">{{ repo.last_error }}</div>
                </article>
              </div>
            </n-collapse-item>
          </n-collapse>
        </n-collapse-item>
      </n-collapse>

      <!-- 社区仓库：与上方仓库源用分割线隔开，固定在最下方 -->
      <template v-if="hasCommunity">
        <div class="community-divider">
          <span>{{ t('社区仓库') }}</span>
        </div>
        <n-collapse :expanded-names="sourcesExpanded" class="source-collapse"
          @update:expanded-names="(names: string[]) => (openSources = names)">
          <n-collapse-item v-for="node in communityTree" :key="node.source.id" :name="node.source.id">
            <template #header>
              <div class="source-head">
                <b>{{ node.label }}</b>
                <!-- 社区来源未经官方索引校验，用严重级红色与「重复」同级提示 -->
                <n-tag size="tiny" :bordered="false" class="state-tag"
                  :color="tagColorFor('critical')">{{ t('来源未知') }}</n-tag>
                <n-tag size="tiny" :bordered="false" class="state-tag"
                  :color="tagColorFor('muted')">{{ node.count }}</n-tag>
              </div>
            </template>
            <!-- 社区仓库不分组：直接平铺卡片，没有分类标题 -->
            <div class="repo-grid community-grid">
              <article v-for="repo in node.groups[0]?.repos ?? []" :key="repo.id" class="repo-card" :data-focus-id="repo.id"
                :class="{ active: detailId === repo.id, 'focus-flash': highlightId === repo.id }"
                @click="detailId = repo.id">
                <div class="card-head">
                  <ProviderIcon :provider="repo.provider" :size="30" />
                  <span class="card-title">{{ repoName(repo) }}</span>
                  <span class="stat stat-status">
                      <!--
                        标签配色：未安装类灰色；最新版/已安装用主题色；
                        可更新用对比色。全部由 theme.ts 的 statusToneColors 计算，
                        通过 n-tag 原生 :color 下发，页面里不再写颜色。
                      -->
                      <n-tag size="tiny" :bordered="false" class="state-tag"
                        :color="tagColorForAny(statusOf(repo).tone)"
                        :class="{ 'state-tag--link': hasLocalInstall(repo) }"
                        :title="hasLocalInstall(repo) ? t('在 MOD 管理中查看') : undefined"
                        @click.stop="openInLocalMods(repo)">{{ statusOf(repo).text }}</n-tag>
                  </span>
                </div>
                <p class="card-desc">{{ repoNote(repo) || repo.repository }}</p>
                <div class="card-foot">
                  <span class="stat stat-version">
                    <span class="muted">{{ t('最新版本') }}</span>
                    <span class="version-line">
                      <b class="mono" :class="{ prerelease: !!latest(repo) && releaseIsPrerelease(latest(repo)!) }">{{ versionLabel(repo) }}</b>
                      <RefreshOutline v-if="refreshing[repo.id]" class="spin-icon" width="12" height="12" />
                    </span>
                  </span>
                  <span class="stat">
                    <span class="muted">{{ t('下载量') }}</span>
                    <b>{{ downloadCount(repo)?.toLocaleString() ?? '—' }}</b>
                  </span>
                  <n-button size="tiny" class="card-install"
                    :style="installButtonStyle(repo)"
                    :loading="!!installing[repo.id]"
                    :disabled="!!game?.kitchen_running || !!refreshing[repo.id]" @click.stop="runCardAction(repo)">
                    <!-- 图标放 #icon 槽，才会被 Naive 的 IconSwitchTransition 与 spinner 互换 -->
                    <template #icon>
                      <RefreshOutline v-if="installActionOf(repo) === 'refresh'" width="13" height="13" />
                      <RefreshOutline v-else-if="installActionOf(repo) === 'reinstall'" width="13" height="13" />
                      <ArrowUpCircleOutline v-else-if="installActionOf(repo) === 'update'" width="13" height="13" />
                      <DownloadOutline v-else width="13" height="13" />
                    </template>
                    {{ installLabel(repo) }}
                  </n-button>
                </div>
                <div v-if="repo.last_error" class="err muted">{{ repo.last_error }}</div>
              </article>
            </div>
            <div v-if="!node.count" class="community-empty muted">{{ t('当前渠道没有社区仓库') }}</div>
          </n-collapse-item>
        </n-collapse>
      </template>
    </template>

    <n-float-button position="fixed" bottom="28" right="32" :disabled="anyRefreshing"
      :tooltip="t('先刷新源文档，再并行刷新各仓库')" @click="refreshAll">
      <RefreshOutline width="17" height="17" />
    </n-float-button>
  </n-spin>

  <!-- 点击卡片：从右侧滑出仓库详情（指标 / 链接 / 备注 / README） -->
  <n-drawer :show="!!detailRepo" :width="720" placement="right" @update:show="(value: boolean) => { if (!value) detailId = null }">
    <n-drawer-content v-if="detailRepo" :native-scrollbar="false" closable>
      <RepositoryDetail :repo="detailRepo" :installing="!!installing[detailRepo.id]" :game-blocked="!!game?.kitchen_running"
        @close="detailId = null" @install="(tag?: string) => detailRepo && install(detailRepo, tag)" />
    </n-drawer-content>
  </n-drawer>

  <!--
    安装流程三段：来源选择 → 详情预览 → 确认安装。
    「安装 MOD」按钮走来源选择；仓库卡片的「安装 / 重装 / 更新」
    先用 preview_only 下载解析，然后直接把预览交给同一个详情弹窗。
  -->
  <ModInstallSourceDialog v-model:show="installSourceOpen"
    @preview="openInstallPreview" />
  <ModInstallDialog v-model:show="installPreviewOpen" :preview="installPreview"
    @installed="handleInstallInstalled()"
    @discarded="discardInstallPreview" />
</template>

<style scoped>
.source-collapse { width: 100%; }

/* 三级固定缩进：源 0、分类 14px、卡片 28px */
.source-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.source-head b { font-size: 14px; color: var(--oc2-text-1); }
.group-collapse { margin: -4px 0 0 14px; }
.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--oc2-accent);
}
/* 分组刷新按钮：紧跟分组名与数量，不占额外行高 */
.group-refresh {
  flex: 0 0 auto;
  color: var(--oc2-text-3);
}
.group-refresh:hover { color: var(--oc2-accent); }
/*
 * 仓库卡片网格。
 *
 * 左侧 28px 是为与上方源标题的缩进对齐；右侧同样留 28px，
 * 否则卡片会顶到 .page-body 的 padding 之外（截图里超出边距的那块）。
 * minmax(0, 1fr) 而不是 minmax(310px, 1fr)：后者在容器窄于 310px 时
 * 会让整列撑破容器。
 */
.repo-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(310px, 100%), 1fr));
  gap: 12px;
  margin: 2px 28px 6px 28px;
}

/* 卡片：圆角边缘 + 内部分区（标题行 / 描述 / 指标行） */
.repo-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  transition: border-color .16s ease, background-color .16s ease, box-shadow .16s ease;
}
.repo-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
  box-shadow: 0 4px 16px rgba(0, 0, 0, .16);
}
.repo-card { cursor: pointer; }
.repo-card.active {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
/* 跨页跳转高亮：3 秒后自动褪去（时长由 crossLink.ts 控制） */
.repo-card.focus-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 16%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
}
/* 有本地安装的状态标签可点击跳到 MOD 管理 */
.state-tag--link { cursor: pointer; }
.state-tag--link:hover { filter: brightness(1.25); }
.card-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.card-title {
  flex: 1;
  min-width: 0;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.card-install {
  flex: 0 0 auto;
  /* 把剩余空白全部推到左边：版本/下载量靠左紧挨，按钮贴右 */
  margin-left: auto;
}
/*
 * 主按钮配色由 installButtonStyle() 通过 :style 内联下发，
 * 取值来自 theme.ts 的 buttonColorFor（与标签共用同一套 tone 刻度）。
 * 这里不再手写 --n-color 覆盖 —— 那样每加一种动作就要复制一整块变量。
 */
.card-desc {
  margin: 0;
  font-size: 12px;
  line-height: 1.55;
  color: var(--oc2-text-3);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  min-height: 37px;
}
.card-foot {
  /*
   * 版本 / 下载量：各自按内容宽度（尽量窄）靠左紧挨，中间不留固定间隔；
   * 安装按钮靠右。空白全部落在「下载量」与「安装按钮」之间。
   */
  display: flex;
  align-items: center;
  justify-content: flex-start;
  gap: 14px;
  padding-top: 8px;
  border-top: 1px solid var(--oc2-row-line);
}
.stat {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1px;
  /* 宽度跟随内容，不再固定列宽 */
  flex: 0 0 auto;
  min-width: 0;
  width: auto;
  text-align: center;
}
/* 版本号可能带「· 预发布」后缀，允许比下载量宽松一点但仍随内容 */
.stat-version { max-width: 180px; }
/* 版本号 + 刷新转圈图标同排 */
.version-line {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
  max-width: 100%;
}
.spin-icon { flex: 0 0 auto; color: var(--oc2-accent); animation: oc2-spin .8s linear infinite; }
@keyframes oc2-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .spin-icon { animation: none; }
}
.stat b { font-size: 12.5px; color: var(--oc2-text-1); max-width: 100%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
/*
 * 预发布版本：跟随「中度」严重度色（与「可更新」「预发布」标签同一档），
 * 不再用固定的琥珀色 —— 否则换主题色时版本号颜色和标签对不上。
 */
.stat b.prerelease { color: var(--oc2-accent-contrast); }
/*
 * 标题行的状态标签靠右，与下方安装按钮同列对齐。
 * 标签本身跟随文字宽度（不拉伸），外观与 MOD 管理页的 .state-tag 完全一致：
 * 圆角、内边距、字号都来自 global.css 的 .state-tag，这里只负责容器定位。
 */
.card-head .stat-status {
  margin-left: auto;
  flex: 0 0 auto;
  /* 覆盖 .stat 的固定列宽：标签宽度跟随文字，与 MOD 管理页一致 */
  width: auto;
  min-width: 0;
  align-items: center;
  justify-self: auto;
}
.err { color: #ee8585; }
.releases {
  padding-top: 8px;
  border-top: 1px dashed var(--oc2-row-line);
  display: grid;
  gap: 6px;
}
.release-row { display: flex; align-items: center; gap: 10px; }
.release-row .n-button { margin-left: auto; }

.community-divider {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 20px 0 12px;
  width: 100%;
  color: var(--oc2-text-3);
  font-size: 12px;
}
.community-divider::before,
.community-divider::after {
  content: '';
  flex: 1;
  height: 1px;
  background: var(--oc2-row-line);
}
.header-count { white-space: nowrap; flex: 0 0 auto; }
/*
 * 搜索框是操作区里唯一能收缩的项：窗口变窄时它先变短，
 * 计数与「刷新全部仓库源」保持完整，避免整行折行。
 */
.header-search { width: 240px; flex: 0 1 240px; min-width: 120px; }
</style>
