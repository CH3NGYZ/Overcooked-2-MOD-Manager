<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { NButton, NCollapse, NCollapseItem, NDrawer, NDrawerContent, NEmpty, NFloatButton, NInput, NSpin, NTag } from 'naive-ui'
import { LinkOutline, RefreshOutline, SearchOutline } from '@vicons/ionicons5'
import { useDashboard } from '../composables/useDashboardStore'
import { prefetchAvatars } from '../avatars'
import { useNotify } from '../useNotify'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import { isPrereleaseVersion, modsForRepository, versionDisplay } from '../modStatus'
import { repositoryInstallAction, repositoryStatus, type InstallAction, type StatusTone } from '../statusTags'
import { useFocusHighlight, useLocalModLink } from '../crossLink'
import { primaryRelation, relationCardTag, relationJumpRepositoryId, resolveRelations, type RelationStatus } from '../modRelations'
import { pickI18n, t } from '../i18n'
import { tagColorFor, type TagTone } from '../theme'
import { repositoryRegionLabel, sourceMatchesRegion } from '../repositorySources'
import { repositoryIsAvailable } from '../repositoryAvailability'
import { markdownSummary } from '../markdown'
import RepositoryDetail from '../components/RepositoryDetail.vue'
import ModInstallDialog from '../components/ModInstallDialog.vue'
import InstallPlanDialog from '../components/InstallPlanDialog.vue'
import ModInstallSourceDialog from '../components/ModInstallSourceDialog.vue'
import { useModInstallFlow } from '../composables/useModInstallFlow'
import { useRepositoryPreview } from '../composables/useRepositoryPreview'
import { useInstallPlanGate } from '../composables/useInstallPlanGate'
import OnlineCard, { type CardAction, type CardDependencyTag } from '../components/OnlineCard.vue'
import type { Repository, RepositorySource } from '../types'

const message = useNotify()
const { load, ensureLoaded, loading, config, game, mods } = useDashboard()

/** 刷新引擎是模块级单例：启动刷新与页面挂载解耦，教程门控也在那里。 */
const engine = useRepositoryRefresh()
/**
 * `anyRefreshing`：禁用按钮用（源文档阶段也算忙，避免重复触发）。
 * `repositoriesLoading`：只决定图标转不转 —— 只有仓库真的在加载时才转。
 */
const { refreshing, sourceRefreshing, anyRefreshing, repositoriesLoading } = engine
/** 状态标签跳转：点标签跳到 MOD 管理页，并高亮绑定该仓库的 MOD 卡片 3 秒。 */
const { linkToLocalMods } = useLocalModLink()
/**
 * 从 MOD 管理页跳回来时，高亮对应仓库卡片 3 秒并滚动到它。
 * `focus` 也用于**页内**跳转：点卡头的「缺依赖」标签时直接高亮依赖仓库，
 * 不需要绕一圈路由。
 */
const { highlightId, focus: focusRepository } = useFocusHighlight()
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

/** 仓库卡片 / 详情抽屉的「安装 / 重装 / 更新」：先检查关系，再下载预览。 */
const { previewRepository } = useRepositoryPreview(openInstallPreview, {
  gameBlocked,
  onBusyChange: (repositoryId, busy) => {
    if (busy) installing.value[repositoryId] = true
    else delete installing.value[repositoryId]
  },
  /*
   * 安装前把依赖 / 冲突 / 兼容一次处理完。
   *
   * 放在这里而不是「装完再提示」：用户点安装的意图是「让它能用」，
   * 缺依赖时它装上也跑不起来；等装完再弹窗，用户已经离开这个页面了。
   * 冲突同理 —— 装上之后才发现冲突，等于先制造一次故障再解释。
   */
  prepareInstall: (repo, tag) => runInstallPlan(repo, tag),
})

/* ===== 安装前检查（依赖 / 冲突 / 兼容） ===== */

/**
 * 生成计划、弹窗、等结果 —— 与 MOD 管理页的「更新」共用同一份实现。
 *
 * 两个页面都要做这件事，写两份迟早分叉（管理页就曾因此漏掉整个检查）。
 */
const {
  open: installPlanOpen,
  plan: installPlan,
  target: installPlanTarget,
  run: runInstallPlan,
  finish: finishInstallPlan,
} = useInstallPlanGate({
  mods: () => mods.value,
  repositories: () => repositories.value,
  gameBlocked,
  onApplied: () => load(),
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
  install: 'modsOnline.install',
  reinstall: 'overview.reinstall',
  update: 'dialogs.update',
  refresh: 'modsOnline.refresh',
}
function installLabel(repo: Repository) {
  return t(INSTALL_LABELS[installActionOf(repo)])
}



/** 主按钮：刷新失败时重试该仓库的 Release，其余走安装预览链路。 */
function runCardAction(repo: Repository) {
  if (installActionOf(repo) === 'refresh') void refreshRepository(repo)
  else void install(repo)
}

/**
 * 卡片的依赖提示（给共享卡片组件用）。
 *
 * 组件只管展示，判定仍在这里 —— 「缺依赖 / 冲突」的语义来自
 * `relationCardTag`，与 MOD 管理页共用同一份实现。
 */
function cardDependency(repo: Repository): CardDependencyTag | null {
  const tag = relationTag(repo)
  if (!tag) return null
  return {
    text: tag.text,
    tone: tag.tone,
    title: relationTagTitle(repo),
    clickable: !!relationJumpId(repo),
  }
}

/** 卡片的主按钮（给共享卡片组件用）。 */
function cardAction(repo: Repository): CardAction {
  return {
    kind: installActionOf(repo),
    label: installLabel(repo),
    loading: !!installing.value[repo.id],
    disabled: !!game.value?.kitchen_running || !!refreshing.value[repo.id],
  }
}

/** 单仓刷新：与批量刷新共用引擎，只更新这一条仓库的缓存。 */
async function refreshRepository(repo: Repository) {
  // 用户主动点刷新 → 强制请求，不吃 10 分钟 Release 缓存
  const error = await engine.refreshOne(repo, true)
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
/**
 * 安装页展示的仓库：只保留「在当前地区真实存在」的那些。
 *
 * 判定统一走 `repositoryIsAvailable`（enabled + 仓库地区 + 所属源地区），
 * 与刷新排队、MOD 绑定、后端更新状态用同一套规则。
 * 地区不匹配的仓库不只是不显示：它也不会被刷新，
 * 所以不会出现「看不见的仓库刷新失败」这种无处处理的报错。
 */
const repositories = computed<Repository[]>(() =>
  (config.value?.repositories ?? []).filter((repo) =>
    repositoryIsAvailable(repo, config.value?.repository_sources, currentRegion.value, config.value?.platform),
  ),
)
const sources = computed<RepositorySource[]>(() =>
  (config.value?.repository_sources ?? []).filter((source) => sourceMatchesRegion(source, currentRegion.value)),
)
const detailRepo = computed(() => repositories.value.find((repo) => repo.id === detailId.value) ?? null)

/*
 * 预取 owner 头像。
 *
 * 放在这里而不是每个卡片自己请求：一次刷新会同时刷出几十个仓库，很多属于
 * 同一个 owner（所有 BepInEx 插件都是 BepInEx）。`prefetchAvatars` 按
 * `provider:owner` 去重，后端再按 owner 单飞 + 磁盘缓存，所以同一 owner
 * 只会真正下载一次。
 *
 * `immediate` + `onMounted` 双触发是有意的：首次进入页面时 `repositories`
 * 可能还没数据（Dashboard 还在读），watch 不会触发；等数据到了再触发一次。
 * 两次都调用没有副作用 —— 已有在途请求时会复用。
 */
watch(repositories, (list) => { void prefetchAvatars(list) }, { immediate: true })
onMounted(() => { void prefetchAvatars(repositories.value) })

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


/* 多语言取值统一走 i18n 的 pickI18n（回收站、MOD 管理页也用它）。 */
function repoName(repo: Repository) {
  return pickI18n(repo.name_i18n, repo.name)
}
function repoGroup(repo: Repository) {
  return pickI18n(repo.group_i18n, repo.group || '') || t('dialogs.ungrouped')
}
/**
 * 分组键：与界面语言无关的稳定标识。
 *
 * 用 `group` 原文（不是翻译后的 label）作键，切语言时同一个分组的键不变，
 * 分桶结果和顺序都不受影响 —— 见 `tree` 里分桶处的说明。
 *
 * 空分组归入 `__ungrouped__`，避免把中文「未分组」这种翻译文案写进键里。
 */
function repoGroupKey(repo: Repository) {
  const raw = (repo.group || '').trim().toLowerCase()
  return raw ? `group:${raw}` : '__ungrouped__'
}
/**
 * 卡片简介：note_i18n 取当前语言，用统一的 Markdown 渲染取纯文本摘要
 * （卡片是摘要，完整 Markdown 在详情抽屉里渲染）。
 */
function repoNote(repo: Repository) {
  return markdownSummary(pickI18n(repo.note_i18n, repo.note || ''))
}

/* ===== 关系提示（依赖 / 冲突） ===== */

/**
 * 一个仓库未满足的第一条关系。
 *
 * 只取「第一条」用于卡片提示：卡头只有一个标签位。优先级由
 * `primaryRelation` 决定 —— 冲突优先（可能已经崩游戏），其次能给安装
 * 入口的依赖，最后是只提示的那些。
 *
 * 计算只依赖当前 Dashboard（已装 MOD + 仓库 Release 缓存），
 * 所以刷新完仓库后标签会自动出现/消失，不需要额外请求。
 */
function unmetRelation(repo: Repository): RelationStatus | undefined {
  return primaryRelation(resolveRelations(repo, mods.value, repositories.value))
}

/**
 * 卡头那个关系标签该显示什么（`null` 表示不显示）。
 *
 * 显示规则本身在 `modRelations.ts` 的 `relationCardTag` 里（纯函数、可测），
 * 这里只负责把「冲突 / 缺依赖」映射成界面文案与色调。
 */
function relationTag(repo: Repository): { text: string; tone: TagTone } | null {
  const tag = relationCardTag(unmetRelation(repo), hasLocalInstall(repo))
  if (!tag) return null
  return tag.kind === 'conflict'
    ? { text: 'modsOnline.conflict', tone: 'critical' }
    : { text: 'modsOnline.missingDep', tone: 'critical' }
}

/** 关系目标的来源名（用于标签提示）。 */
function relationTargetName(status: RelationStatus) {
  if (status.repository) return repoName(status.repository)
  if (status.installedMod) return status.installedMod.name
  return status.relation.guid
}

/**
 * 点关系标签该跳到哪个仓库（`undefined` = 不可点）。
 *
 * 与 MOD 管理页共用 `relationJumpRepositoryId` —— 两处点同一个状态，
 * 落点必须相同：都是**依赖的**仓库，而不是当前这张卡自己的仓库。
 */
function relationJumpId(repo: Repository): string | undefined {
  return relationJumpRepositoryId(unmetRelation(repo))
}

/**
 * 关系标签的悬停提示：说清「跟谁、为什么」，以及点了会去哪。
 *
 * 没有可跳转目标时不假装可点，直接说明原因（没有可安装来源）。
 */
function relationTagTitle(repo: Repository) {
  const status = unmetRelation(repo)
  if (!status) return undefined
  const target = relationTargetName(status)
  const detail = status.state === 'conflict'
    ? t('modsOnline.isIncompatibleWithAndMustBeUninstalled', { owner: repoName(repo), target })
    : t('modsOnline.missingDependency') + '： ' + target
  return relationJumpId(repo)
    ? `${detail} · ${t('modsOnline.clickToLocateThatRepository')}`
    : detail
}

/**
 * 点关系标签：在**本页内**高亮依赖仓库并滚过去。
 *
 * 不需要绕路由：目标卡片就在同一页，走 `focus` 直接高亮 3 秒，
 * 与从 MOD 管理页跳转过来用的是同一套高亮与滚动逻辑。
 */
function jumpToRelationRepository(repo: Repository) {
  const id = relationJumpId(repo)
  if (id) focusRepository(id)
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
    /*
     * 分桶用**分组键**而不是显示名。
     *
     * 显示名是翻译后的字符串，会随界面语言变化；拿它当 Map 键意味着切语言
     * 等于换了一组桶，顺序也随之重排。分组键（`group:` + 规范化后的原文）
     * 与语言无关，语言只影响 label 的展示。
     */
    const buckets = new Map<string, { label: string; repos: Repository[] }>()
    // 首次出现的顺序 = 后端/源文档声明的顺序，分组保持这个顺序，不排序
    const order: string[] = []
    for (const repo of repos) {
      const key = repoGroupKey(repo)
      const existing = buckets.get(key)
      if (existing) {
        existing.repos.push(repo)
      } else {
        buckets.set(key, { label: repoGroup(repo), repos: [repo] })
        order.push(key)
      }
    }
    /*
     * 按首次出现顺序输出：**不按分组名排序**。
     *
     * 原来用 `localeCompare` 按显示名排序，切语言后同一个分组换了名字、
     * 顺序就变了 —— 用户看到的是「改个语言，MOD 安装页的分组全乱了」。
     * 现在顺序完全由源文档里仓库的声明顺序决定，与语言无关。
     * 「未分组」本来就在声明顺序里落在它实际出现的位置，不再强行垫底。
     */
    const groups: GroupNode[] = order.map((key) => {
      const bucket = buckets.get(key)!
      return { key: `${source.id}::${key}`, label: bucket.label, repos: bucket.repos }
    })
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
  if (orphan?.length) push({ id: '__none__', nickname: t('dialogs.uncategorizedRepositories'), url: '', builtin: false }, orphan)

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
 *
 * 版本号一律优先取 `tag_name`：后端的 `version` 字段是 semver 归一结果
 * （`1.8.7.23` → `1.8.7`），用它判断会丢掉第 4 位；`tag_name` 才是原始写法。
 */
function releaseIsPrerelease(release: { prerelease: boolean; version?: string | null; tag_name?: string }) {
  return isPrereleaseVersion(release.tag_name || release.version, release.prerelease)
}

/**
 * 卡片版本号：始终显示 Release tag 的原始写法。
 *
 * 后端 `version` 是 semver 归一后的值（`1.8.7.23` 会变成 `1.8.7`），
 * 直接显示会让用户以为管理器读错了版本，所以优先用 `tag_name`。
 */
/**
 * 版本号的两段式展示：主干（普通色）+ 测试标记（prerelease 等级色）。
 *
 * `v1.9.9-beta.2` 在卡片上占很宽，而且用户真正需要的只有两点：
 * 主干版本是多少、这是第几个测试版。所以拆成 `1.9.9` + 灰底 `2`。
 *
 * 解析不出数字序号时（如 `1.9.9-beta`，或作者只勾了 prerelease 标记），
 * 用「测」这个文字标记提醒这是测试版 —— 数字表达不了就说清楚。
 */
function versionParts(repo: Repository) {
  const release = latest(repo)
  if (!release) return { base: '—', marker: null as string | null, markerIsNumeric: false }
  return versionDisplay(
    release.tag_name || release.version,
    releaseIsPrerelease(release),
    t('modsOnline.t'),
  )
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
  return statusByRepo.value.get(repo.id) ?? { key: 'not_refreshed', tone: 'idle', text: t('modsOnline.notRefreshed') }
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
  /*
   * 源文档刚拉回来的仓库也要按「真实存在」再过滤一次：
   * 与展示同一套判定，避免把地区专属仓库（例如中国大陆下的
   * international 仓库）送进 Release 刷新队列。
   */
  const scoped = repositories.value.filter(
    (repo) => !!repo.source_id && ids.has(repo.source_id)
      && repositoryIsAvailable(repo, config.value?.repository_sources, currentRegion.value, config.value?.platform),
  )
  const releaseErrors = await engine.refreshMany(scoped)
  releaseErrors.forEach((error) => message.error(error))
}

/** 整源刷新入口（仅浮动按钮调用；页头按钮已移除）。 */
async function refreshAll() {
  const errors = await engine.refreshEverything()
  errors.forEach((error) => message.error(error))
}

/**
 * 分组刷新：只刷新本组内的仓库 Release，不重拉源文档、不影响其它组。
 * 源文档（分组 / 仓库列表的来源）不在这里动，避免只点一个分组就把整棵源树清空重载。
 */
async function refreshGroup(group: GroupNode) {
  // 组内不可用的仓库同样跳过刷新（分组由已筛选列表派生，这里再校验一次）
  const errors = await engine.refreshMany(group.repos.filter(
    (repo) => repositoryIsAvailable(repo, config.value?.repository_sources, currentRegion.value, config.value?.platform),
  ))
  errors.forEach((error) => message.error(error))
}

/** 该组是否有仓库正在刷新（决定分组按钮的转圈）。 */
function groupRefreshing(group: GroupNode) {
  return group.repos.some((repo) => refreshing.value[repo.id])
}

function gameBlocked() {
  if (game.value?.kitchen_running) {
    message.warning(t('modsOnline.theGameIsRunningCloseItBeforeInstalling'))
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

/**
 * 从仓库详情抽屉发起安装。
 *
 * 安装前先收起抽屉：接着会弹**安装预览**（ModInstallDialog），两个
 * 右侧浮层叠在一起时，抽屉的滑出动画会和预览弹窗的淡入互相干扰，
 * 看起来像两个东西在抢同一块区域。
 *
 * 收起放在 `install()` 之前而不是之后：`install()` 会 await 整个下载
 * 与解析过程，放后面要等下载完抽屉才动，用户会觉得点了没反应。
 *
 * 先关再装还有个附带好处：下载失败时用户看到的是干净的列表页 +
 * 错误提示，而不是「抽屉里什么都没变」的困惑。
 */
function installFromDetail(tag?: string) {
  const repo = detailRepo.value
  if (!repo) return
  detailId.value = null
  return install(repo, tag)
}

defineExpose({ refreshAll, visibleCount })
</script>

<template>
    <!-- 页头 h1 右侧：当前下载渠道 -->
    <Teleport to="#page-header-title-extra" defer>
      <n-tag size="small" :bordered="false" type="info">{{ regionLabel }}</n-tag>
    </Teleport>

    <!--
      页头右侧，从左到右：计数 → 搜索 → 安装 MOD。
      「安装 MOD」固定在最右：它是本页最主要的动作入口。
      整源刷新由右下角浮动按钮承担，页头不再重复放一个按钮。
    -->
    <Teleport to="#page-header-actions" defer>
      <span class="header-count muted">{{ t('modsOnline.repositories', { count: visibleCount }) }}</span>
      <n-input v-model:value="search" clearable size="small" class="header-search"
        :placeholder="t('dialogs.search')">
        <template #prefix><SearchOutline width="14" height="14" /></template>
      </n-input>
      <n-button size="small" type="primary" @click="openInstall">
        <LinkOutline width="14" height="14" />{{ t('tools.installMod') }}
      </n-button>
    </Teleport>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !visibleCount" :description="t('tools.noMatchingRepositories')" class="empty" />

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
                <OnlineCard v-for="repo in group.repos" :key="repo.id" :data-focus-id="repo.id"
                  :provider="repo.provider" :owner="repo.username" :title="repoName(repo)"
                  :note="repoNote(repo) || repo.repository"
                  :status-text="statusOf(repo).text" :status-tone="statusOf(repo).tone"
                  :status-clickable="hasLocalInstall(repo)"
                  :status-title="hasLocalInstall(repo) ? t('modsOnline.viewInModManager') : undefined"
                  :dependency="cardDependency(repo)"
                  :version="versionParts(repo)" :refreshing="!!refreshing[repo.id]"
                  :downloads="downloadCount(repo)"
                  :action="cardAction(repo)"
                  :error="repo.last_error" :active="detailId === repo.id"
                  :focus-flash="highlightId === repo.id"
                  @open="detailId = repo.id"
                  @status-click="openInLocalMods(repo)"
                  @dependency-click="jumpToRelationRepository(repo)"
                  @action="runCardAction(repo)" />
              </div>
            </n-collapse-item>
          </n-collapse>
        </n-collapse-item>
      </n-collapse>

      <!-- 社区仓库：与上方仓库源用分割线隔开，固定在最下方 -->
      <template v-if="hasCommunity">
        <div class="community-divider">
          <span>{{ t('dialogs.communityRepositories') }}</span>
        </div>
        <n-collapse :expanded-names="sourcesExpanded" class="source-collapse"
          @update:expanded-names="(names: string[]) => (openSources = names)">
          <n-collapse-item v-for="node in communityTree" :key="node.source.id" :name="node.source.id">
            <template #header>
              <div class="source-head">
                <b>{{ node.label }}</b>
                <!-- 社区来源未经官方索引校验，用严重级红色与「重复」同级提示 -->
                <n-tag size="tiny" :bordered="false" class="state-tag"
                  :color="tagColorFor('critical')">{{ t('dialogs.unknownOrigin') }}</n-tag>
                <n-tag size="tiny" :bordered="false" class="state-tag"
                  :color="tagColorFor('muted')">{{ node.count }}</n-tag>
              </div>
            </template>
            <!-- 社区仓库不分组：直接平铺卡片，没有分类标题 -->
            <div class="repo-grid community-grid">
              <OnlineCard v-for="repo in node.groups[0]?.repos ?? []" :key="repo.id"
                :data-focus-id="repo.id"
                :provider="repo.provider" :owner="repo.username" :title="repoName(repo)"
                :note="repoNote(repo) || repo.repository"
                :status-text="statusOf(repo).text" :status-tone="statusOf(repo).tone"
                :status-clickable="hasLocalInstall(repo)"
                :status-title="hasLocalInstall(repo) ? t('modsOnline.viewInModManager') : undefined"
                :dependency="cardDependency(repo)"
                :version="versionParts(repo)" :refreshing="!!refreshing[repo.id]"
                :downloads="downloadCount(repo)"
                :action="cardAction(repo)"
                :error="repo.last_error" :active="detailId === repo.id"
                :focus-flash="highlightId === repo.id"
                @open="detailId = repo.id"
                @status-click="openInLocalMods(repo)"
                @dependency-click="jumpToRelationRepository(repo)"
                @action="runCardAction(repo)" />
            </div>
            <div v-if="!node.count" class="community-empty muted">{{ t('tools.noCommunityRepositoriesInThisChannel') }}</div>
          </n-collapse-item>
        </n-collapse>
      </template>
    </template>

  <!--
    整源刷新入口。
    - 禁用看 `anyRefreshing`：源文档阶段也算忙，防止重复点。
    - 图标旋转看 `repositoriesLoading`：只要有仓库在加载就一直转，
      包括「刚启动切到本页时启动刷新正在跑」和「在 MOD 管理页点了刷新」——
      引擎是模块级单例，两个页面看到的是同一份状态。
  -->
  <n-float-button position="fixed" bottom="28" right="32" :disabled="anyRefreshing"
    :tooltip="t('tools.reloadSourceDocumentsThenRefreshRepositoriesInParallel')" @click="refreshAll">
    <RefreshOutline width="17" height="17" :class="{ 'spin-icon': repositoriesLoading }" />
  </n-float-button>

  <!--
    安装前检查：依赖 / 冲突 / 兼容一次列清并执行。
    `finished` 携带「是否处理完毕」，前置流程据此决定是否继续装主包。
  -->
  <InstallPlanDialog v-model:show="installPlanOpen" :plan="installPlan"
    :target-label="installPlanTarget"
    @finished="(ready: boolean) => { void finishInstallPlan(ready) }" />
  </n-spin>

  <!-- 点击卡片：从右侧滑出仓库详情（指标 / 链接 / 备注 / README） -->
  <n-drawer :show="!!detailRepo" :width="720" placement="right" @update:show="(value: boolean) => { if (!value) detailId = null }">
    <n-drawer-content v-if="detailRepo" :native-scrollbar="false" closable>
      <RepositoryDetail :repo="detailRepo" :installing="!!installing[detailRepo.id]" :game-blocked="!!game?.kitchen_running"
        @close="detailId = null" @install="(tag?: string) => installFromDetail(tag)" />
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

/*
 * 卡片、状态标签、版本号与主按钮的样式全部搬进 `OnlineCard.vue`：
 * 地图下载页用同一个组件，两份样式从这里开始会各自漂移。
 * 本页只保留网格容器与页内专属的浮动按钮转圈。
 */
.spin-icon { flex: 0 0 auto; color: var(--oc2-accent); animation: oc2-spin .8s linear infinite; }
@keyframes oc2-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .spin-icon { animation: none; }
}
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
 * 计数与搜索框保持完整，避免整行折行。
 */
.header-search { width: 240px; flex: 0 1 240px; min-width: 120px; }
</style>
