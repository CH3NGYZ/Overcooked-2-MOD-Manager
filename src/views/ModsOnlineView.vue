<script setup lang="ts">
import { useGameOperationGate } from '../composables/useGameOperationGate'
import { computed, onMounted, ref, watch } from 'vue'
import { NButton, NCollapse, NCollapseItem, NDrawer, NDrawerContent, NEmpty, NFloatButton, NTag } from 'naive-ui'
import { LinkOutline, RefreshOutline } from '@vicons/ionicons5'
import { useDashboard } from '../composables/useDashboardStore'
import { prefetchAvatars } from '../avatars'
import { useNotify } from '../useNotify'
import { useModRepositoryRefresh } from '../composables/useModRepositoryRefresh'
import { isPrereleaseVersion, modsForRepository, versionDisplay } from '../modStatus'
import { repositoryInstallAction, repositoryStatus, type InstallAction, type StatusTone } from '../statusTags'
import { useFocusHighlight, useLocalModLink } from '../crossLink'
import { dependencyTagFor, primaryRelation, relationJumpRepositoryId, resolveRelations, type RelationStatus } from '../modRelations'
import { pickI18n, t } from '../i18n'
import { tagColorFor, type TagTone } from '../theme'
import { repositoryRegionLabel, sourceMatchesRegion } from '../repositorySources'
import { modRepositoryIsAvailable } from '../repositoryAvailability'
import { markdownSummary } from '../markdown'
import ModRepositoryDetail from '../components/ModRepositoryDetail.vue'
import PageLoading from '../components/PageLoading.vue'
import HeaderIconAction from '../components/HeaderIconAction.vue'
import HeaderSearch from '../components/HeaderSearch.vue'
import ModInstallDialog from '../components/ModInstallDialog.vue'
import InstallCheckDialog from '../components/InstallCheckDialog.vue'
import InstallPlanDialog from '../components/InstallPlanDialog.vue'
import InstallSourceCard from '../components/InstallSourceCard.vue'
import { useModInstallFlow } from '../composables/useModInstallFlow'
import { useFileDrop } from '../composables/useFileDrop'
import { takePendingDrop, useDropRouting } from '../composables/useDropRouting'
import { useModRepositoryPreview } from '../composables/useModRepositoryPreview'
import { useInstallPlanGate } from '../composables/useInstallPlanGate'
import OnlineCard, { type CardAction, type CardDependencyTag } from '../components/OnlineCard.vue'
import SourceGroupSlot from '../components/SourceGroupSlot.vue'
import SourceTitle from '../components/SourceTitle.vue'
import SourceDirectory from '../components/SourceDirectory.vue'
import {
  useSourceGroupTree,
  type SourceGroupEntry,
  type SourceGroupLeaf,
  type SourceGroupOrder,
} from '../composables/useSourceGroupTree'
import type { ModRepository, RepositorySource } from '../types'
import { withDemoRepositories, withDemoSources } from '../tutorial/demo'

const message = useNotify()
const { blockModOperation: gameBlocked } = useGameOperationGate()
const { load, ensureLoaded, loading, config, mods } = useDashboard()

/** 刷新引擎是模块级单例：启动刷新与页面挂载解耦（教程门控脚手架仍在引擎里，当前无调用方）。 */
const engine = useModRepositoryRefresh()
/**
 * `anyRefreshing`：禁用按钮用（源文档阶段也算忙，避免重复触发）。
 * `repositoriesLoading`：只决定图标转不转 —— 只有仓库真的在加载时才转。
 */
const { refreshing, queued, preparingSources, sourceRefreshing, anyRefreshing, repositoriesLoading } = engine
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
  sourceBusy: installSourceBusy,
  previewOpen: installPreviewOpen,
  preview: installPreview,
  openSource: openInstallSource,
  openPreview: openInstallPreview,
  handleSourceSelect: handleInstallSourceSelect,
  pickSourceFile: pickInstallSourceFile,
  discardPreview: discardInstallPreview,
  handleInstalled: handleInstallInstalled,
  checkOpen: installCheckOpen,
  checkModel: installCheckModel,
  executeCheck: executeInstallCheck,
  finishCheck: finishInstallCheck,
} = useModInstallFlow(() => load())

/** 拖入 / 跨页跳转过来的文件：打开来源卡片后走同一条预览链路。 */
async function startDropInstall(path: string) {
  openInstallSource()
  await handleInstallSourceSelect({ kind: 'local', value: path })
}

const { acceptDrop } = useDropRouting()

/**
 * 整窗拖放：拖 DLL / ZIP 进来等价于点「安装 MOD」。
 *
 * 拖放是窗口能力（见 `useFileDrop`），四个 MOD / 地图页面各自声明一次；
 * 来源卡片只负责显示高亮，不再自己挂监听 —— 以前监听挂在卡片里，能不能拖
 * 取决于「这页碰巧挂没挂那张卡片」。
 *
 * 先过 `acceptDrop`：含 `info` 的 ZIP 是地图包（哪怕它也带 DLL），装进来
 * 只会得到一个空预览，必须引导到地图安装页。
 */
const { dragActive } = useFileDrop({
  accept: ['dll', 'zip'],
  busy: () => installSourceBusy.value,
  onDrop: (path) => {
    void (async () => {
      if (await acceptDrop(path, 'mod')) await startDropInstall(path)
    })()
  },
})

/** 仓库卡片 / 详情抽屉的「安装 / 重装 / 更新」：先检查关系，再下载预览。 */
const { previewModRepository } = useModRepositoryPreview(openInstallPreview, {
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

const COMMUNITY_IDS = new Set(['github-community', 'gitee-community', 'gitcode-community'])

// 进入页面只复用现有数据，不触发刷新也不重新扫描；
// 刷新由手动按钮或安装器启动后的刷新链路触发。
onMounted(() => {
  void ensureLoaded()
  // 从地图页「去安装」跳过来：路径已暂存在模块里，直接接着走预览
  const pending = takePendingDrop('mod')
  if (pending) void startDropInstall(pending)
})

/**
 * 主按钮动作：刷新失败 → 刷新，否则安装 / 重装 / 更新。
 *
 * 刷新失败时仓库没有可用 Release，此时「安装」点下去只会提示先刷新，
 * 用户还得再找一个刷新入口。把同一个按钮换成「刷新」，
 * 用户对着同一个位置点一次就能重试。
 */
function installActionOf(repo: ModRepository): InstallAction | 'refresh' {
  if (statusOf(repo).key === 'refresh_failed') return 'refresh'
  return repositoryInstallAction(statusOf(repo), repo)
}

const INSTALL_LABELS: Record<InstallAction | 'refresh', string> = {
  install: 'modsOnline.install',
  reinstall: 'overview.reinstall',
  update: 'dialogs.update',
  refresh: 'modsOnline.refresh',
}
function installLabel(repo: ModRepository) {
  return t(INSTALL_LABELS[installActionOf(repo)])
}



/** 主按钮：刷新失败时重试该仓库的 Release，其余走安装预览链路。 */
function runCardAction(repo: ModRepository) {
  if (installActionOf(repo) === 'refresh') void refreshModRepository(repo)
  else void install(repo)
}

/**
 * 卡片的依赖提示（给共享卡片组件用）。
 *
 * 组件只管展示，判定仍在这里 —— 「缺依赖 / 冲突」的语义来自
 * `modRelations.dependencyTagFor`，四个页面共用同一份实现。
 */
function cardDependency(repo: ModRepository): CardDependencyTag | null {
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
function cardAction(repo: ModRepository): CardAction {
  return {
    kind: installActionOf(repo),
    label: installLabel(repo),
    loading: !!installing.value[repo.id],
    disabled: preparingSources.value || !!refreshing.value[repo.id] || !!queued.value[repo.id],
  }
}

/** 单仓刷新：与批量刷新共用引擎，只更新这一条仓库的缓存。 */
async function refreshModRepository(repo: ModRepository) {
  // 用户主动点刷新 → 强制请求，不吃 5 分钟 Release 缓存
  const error = await engine.refreshOne(repo)
  if (error) message.error(error)
}

/** 该仓库是否有已安装的本地 MOD（决定状态标签能否跳到 MOD 管理）。 */
function hasLocalInstall(repo: ModRepository) {
  return modsForRepository(mods.value, repo).length > 0
}

/** 点击状态标签：跳到 MOD 管理页并高亮绑定该仓库的 MOD 卡片。 */
function openInLocalMods(repo: ModRepository) {
  if (hasLocalInstall(repo)) linkToLocalMods(repo.id)
}

const currentRegion = computed(() => config.value?.region)
const regionLabel = computed(() => repositoryRegionLabel(currentRegion.value))

/** 仅保留当前下载渠道适用的仓库。 */
/**
 * 安装页展示的仓库：只保留「在当前地区真实存在」的那些。
 *
 * 判定统一走 `modRepositoryIsAvailable`（enabled + 仓库地区 + 所属源地区），
 * 与刷新排队、MOD 绑定、后端更新状态用同一套规则。
 * 地区不匹配的仓库不只是不显示：它也不会被刷新，
 * 所以不会出现「看不见的仓库刷新失败」这种无处处理的报错。
 */
/*
 * 教程期间这两个列表整体换成内置示例源（`withDemo*`）：教程讲的是固定的
 * 「源分组 → 卡片 → 详情」路径，真实配置为空、或首启连不上源时那几步没有
 * 东西可指。合并只发生在展示层（见 `tutorial/demo.ts` 的边界说明），教程
 * 一结束这两个 computed 当场回到真实数据。
 *
 * 源必须先换：`modRepositoryIsAvailable` 会查「所属源是否存在」，只换仓库不换源，
 * 示例卡片会被全部滤掉。
 */
const sources = computed<RepositorySource[]>(() =>
  withDemoSources(
    (config.value?.repository_sources ?? []).filter((source) => sourceMatchesRegion(source, currentRegion.value)),
  ),
)
const repositories = computed<ModRepository[]>(() =>
  withDemoRepositories(
    (config.value?.repositories ?? []).filter((repo) =>
      modRepositoryIsAvailable(repo, sources.value, currentRegion.value, config.value?.platform),
    ),
  ),
)
const detailRepo = computed(() => repositories.value.find((repo) => repo.id === detailId.value) ?? null)

/*
 * 预取 owner 头像。
 *
 * 放在这里而不是每个卡片自己请求：一次刷新会同时刷出几十个仓库，很多属于
 * 同一个 owner（所有 BepInEx 插件都是 BepInEx）。`prefetchAvatars` 按
 * `provider:owner` 去重，后端再按 owner 单飞 + 磁盘缓存，所以同一 owner
 * 只会真正下载一次。地图安装页用的是**同一个** `prefetchAvatars`（入参只要
 * `{ provider, username }`，两个模型都满足），不要在那边再写一份预取。
 *
 * `immediate` + `onMounted` 双触发是有意的：首次进入页面时 `repositories`
 * 可能还没数据（Dashboard 还在读），watch 不会触发；等数据到了再触发一次。
 * 两次都调用没有副作用 —— 已有在途请求时会复用。
 */
watch(repositories, (list) => { void prefetchAvatars(list) }, { immediate: true })
onMounted(() => { void prefetchAvatars(repositories.value) })

/** 社区仓库（github-community / gitee-community）不展示分组标题，全部按未分组处理。 */
function isCommunityRepo(repo: ModRepository) {
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
function repoKey(repo: ModRepository) {
  return `${repo.provider}:${repo.username.toLowerCase()}/${repo.repository.toLowerCase()}`
}
function isDuplicateOfOfficial(repo: ModRepository) {
  return isCommunityRepo(repo) && officialRepoKeys.value.has(repoKey(repo))
}


/* 多语言取值统一走 i18n 的 pickI18n（回收站、MOD 管理页也用它）。 */
function repoName(repo: ModRepository) {
  return pickI18n(repo.name_i18n, repo.name)
}
function repoGroup(repo: ModRepository) {
  return pickI18n(repo.group_i18n, repo.group || '') || t('dialogs.ungrouped')
}
/**
 * 卡片简介：note_i18n 取当前语言，用统一的 Markdown 渲染取纯文本摘要
 * （卡片是摘要，完整 Markdown 在详情抽屉里渲染）。
 */
function repoNote(repo: ModRepository) {
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
function unmetRelation(repo: ModRepository): RelationStatus | undefined {
  return primaryRelation(resolveRelations(repo, mods.value, repositories.value))
}

/**
 * 卡头那个关系标签该显示什么（`null` 表示不显示）。
 *
 * 判定与文案全部来自 `modRelations.dependencyTagFor` —— 与 MOD 管理页、
 * 地图下载页、地图管理页共用同一份实现，四个页面不会各写一套「缺依赖」。
 */
function relationTag(repo: ModRepository): { text: string; tone: TagTone } | null {
  const status = unmetRelation(repo)
  const tag = dependencyTagFor(status ? [status] : [], hasLocalInstall(repo))
  return tag ? { text: tag.text, tone: tag.tone } : null
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
function relationJumpId(repo: ModRepository): string | undefined {
  return relationJumpRepositoryId(unmetRelation(repo))
}

/**
 * 关系标签的悬停提示：说清「跟谁、为什么」，以及点了会去哪。
 *
 * 没有可跳转目标时不假装可点，直接说明原因（没有可安装来源）。
 */
function relationTagTitle(repo: ModRepository) {
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
function jumpToRelationRepository(repo: ModRepository) {
  const id = relationJumpId(repo)
  if (id) focusRepository(id)
}

/* ===== 搜索 ===== */
const query = computed(() => search.value.trim().toLowerCase())
function matches(repo: ModRepository) {
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

/*
 * 源 → 分类两级折叠（卡片是第三层，不是折叠节点）。
 *
 * 规则全部在 `useSourceGroupTree`：分桶键用原文而不是翻译名、顺序按源文档的
 * 声明顺序不排序、搜索时强制全展开。地图下载页用**同一份实现** —— 两页各写
 * 一套的结果已经出现过一次：一页按显示名排序，切个语言分组顺序就变了。
 *
 * 本页只负责把自己特有的两件事投影进去：源名来自 `repository_sources`
 * （地图页的源名挂在条目上），社区源不分类。
 */
const treeEntries = computed<SourceGroupEntry<ModRepository>[]>(() =>
  repositories.value
    // 搜索过滤与「官方源已收录的不在社区源里重复展示」都在进树前完成
    .filter((repo) => matches(repo) && !isDuplicateOfOfficial(repo))
    .map((repo) => ({
      sourceId: repo.source_id ?? '',
      // 源名在源列表上，交给下面的 order；条目这一级不重复翻译
      sourceLabel: '',
      group: repo.group ?? '',
      groupLabel: repoGroup(repo),
      item: repo,
    })),
)

/** 源顺序 = `config.repository_sources` 的持久化顺序（读取顺序 = 展示顺序）。 */
const sourceOrder = computed<SourceGroupOrder[]>(() =>
  sources.value.filter(source => !COMMUNITY_IDS.has(source.id)).map((source) => ({
    id: source.id,
    label: pickI18n(source.nickname_i18n, source.nickname),
    parentId: source.parent_id, url: source.url,
    status: engine.sourceNodes.value.find(node => node.id === source.id)?.status,
    error: engine.sourceNodes.value.find(node => node.id === source.id)?.error,
  })),
)

const {
  tree,
  setOpenSources,
  setOpenGroups,
  sourcesExpanded,
  groupsExpanded,
} = useSourceGroupTree(
  () => treeEntries.value,
  () => ({
    order: sourceOrder.value,
    preserveEmpty: !query.value,
    ungrouped: { id: '__none__', label: t('dialogs.uncategorizedRepositories') },
    /* 社区仓库统一视为未分组：单组、无分类标题，直接平铺卡片。 */
    flat: (sourceId: string) => COMMUNITY_IDS.has(sourceId),
  }),
  () => !!query.value,
)

/*
 * 社区源固定放最下方（内部仍保持原有相对顺序）：它们未经官方索引校验，
 * 混在官方源中间等于把「作者声明的仓库」和「搜索结果」当成同一件事。
 */
const normalTree = computed(() => tree.value.filter((node) => !COMMUNITY_IDS.has(node.id)))
const communityTree = computed(() => tree.value.filter((node) => COMMUNITY_IDS.has(node.id)))
const hasCommunity = computed(() => communityTree.value.length > 0)

/*
 * 内置教程要指「第一张仓库卡」，但卡片是按分组 v-for 出来的，不能给每张都
 * 加同名的 `data-tutorial`：重复 key 时 `querySelector` 落到哪一张取决于
 * DOM 顺序，教程就变成了随机指一张。只认整棵树里的第一个单条。
 */
const tutorialCardId = computed(() => tree.value.flatMap(node => node.groups.flatMap(group => group.items))[0]?.id ?? null)

/* ===== 动作 ===== */

function latest(repo: ModRepository) {
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
function versionParts(repo: ModRepository) {
  const release = latest(repo)
  if (!release) return { base: '—', marker: null as string | null, markerIsNumeric: false }
  return versionDisplay(
    release.tag_name || release.version,
    releaseIsPrerelease(release),
    t('modsOnline.t'),
  )
}

/** 下载量：优先最新 Release 首个匹配资产的 download_count。 */
function downloadCount(repo: ModRepository) {
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
function repoStatus(repo: ModRepository): RepoStatus {
  const status = repositoryStatus(repo, mods.value)
  return { key: status.key, tone: status.tone, text: t(status.text) }
}

/** 每个仓库只算一次状态，模板与筛选共用。 */
const statusByRepo = computed(() => {
  const map = new Map<string, RepoStatus>()
  for (const repo of repositories.value) map.set(repo.id, repoStatus(repo))
  return map
})
function statusOf(repo: ModRepository): RepoStatus {
  return statusByRepo.value.get(repo.id) ?? { key: 'not_refreshed', tone: 'idle', text: t('modsOnline.notRefreshed') }
}

/**
 * 单个源的刷新按钮：先重拉该源文档（父源会连带刷新它声明的子源），
 * 再并行刷新刷新后属于这条源链的仓库。
 */
async function refreshSource(node: { id: string }) {
  const errors = await engine.refreshSource(node.id)
  errors.forEach((error) => message.error(error))
}

/**
 * 整页遮罩的显示条件，与地图安装页同一条判据：
 * 「正在整源刷新 **且** 列表是空的」。
 *
 * 首次加载：Dashboard 还在读（`loading`）。
 * 整源刷新：`refreshEverything()` 会先清空源缓存让列表当场变空，这段时间没有
 * 任何内容可看，必须显示加载态；第一张卡回来立刻收起。
 *
 * 为什么用引擎的 `refreshingAll` 而不是页面自己的标志：它在
 * `useModRepositoryRefresh` 里是模块级单例，刷新中切走再切回来时仍然是 true ——
 * 页面级 ref 熬不过一次切页，那样用户切回来只会看到一个空列表加「没有仓库」，
 * 完全不知道后台那次刷新还在跑。
 *
 * 为什么不能只看 `loading`：它只在应用第一次读 Dashboard 时为真，之后整源
 * 刷新走的是 `backgroundLoading`（不显示整页遮罩）。实测点刷新后 852 帧采样
 * 里遮罩一次都没出现。
 */
const pageLoading = computed(() => loading.value && tree.value.length === 0)

/** 整源刷新入口（仅浮动按钮调用；页头按钮已移除）。 */
async function refreshAll() {
  const errors = await engine.refreshEverything({ force: true })
  errors.forEach((error) => message.error(error))
}

/**
 * 分组刷新：只刷新本组内的仓库 Release，不重拉源文档、不影响其它组。
 * 源文档（分组 / 仓库列表的来源）不在这里动，避免只点一个分组就把整棵源树清空重载。
 */
async function refreshGroup(group: SourceGroupLeaf<ModRepository>) {
  // 组内不可用的仓库同样跳过刷新（分组由已筛选列表派生，这里再校验一次）
  const errors = await engine.refreshMany(group.items.filter(
    (repo) => modRepositoryIsAvailable(repo, config.value?.repository_sources, currentRegion.value, config.value?.platform),
  ))
  errors.forEach((error) => message.error(error))
}

/** 该组是否有仓库正在刷新（决定分组按钮的转圈）。 */
function groupRefreshing(group: SourceGroupLeaf<ModRepository>) {
  return group.items.some((repo) => refreshing.value[repo.id] || queued.value[repo.id])
}


/**
 * 仓库卡片的「安装 / 重装 / 更新」。
 *
 * 流程与手动安装统一：先让后端**只下载并解析**（preview_only），
 * 把解析结果交给 ModInstallDialog 展示，用户点「立即安装」才真正写入。
 * 下载与预发布确认都在 useModRepositoryPreview 里，两个页面共用。
 */
function install(repo: ModRepository, tag?: string, asset?: string) {
  return previewModRepository(repo, tag, asset)
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
function installFromDetail(tag?: string, asset?: string) {
  const repo = detailRepo.value
  if (!repo) return
  detailId.value = null
  return install(repo, tag, asset)
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
      <HeaderSearch v-model:value="search" data-tutorial="mods-online-search" />
      <HeaderIconAction type="primary" :label="t('tools.installMod')" @click="openInstall">
        <LinkOutline width="16" height="16" aria-hidden="true" />
      </HeaderIconAction>
    </Teleport>

  <PageLoading :show="pageLoading">
    <template #status>{{ t('modsOnline.readingRepositorySources') }}</template>
    <n-empty v-if="!pageLoading && !tree.length" :description="t('tools.noMatchingRepositories')" class="empty" />

    <template v-else>
      <SourceDirectory :nodes="normalTree" :expanded="sourcesExpanded" :disabled="anyRefreshing"
        :busy="node => !!sourceRefreshing[node.id] || node.groups.some(group => group.items.some(repo => refreshing[repo.id]))"
        @refresh="refreshSource" @update:expanded="setOpenSources" v-slot="{ node }">
          <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" v-slot="{ node }">
            <n-collapse :expanded-names="groupsExpanded" class="group-collapse"
              @update:expanded-names="setOpenGroups">
              <n-collapse-item v-for="group in node.groups" :key="group.key" :name="group.key">
                <template #header>
                  <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" :group-key="group.key" v-slot="{ group }">
                    <div class="group-head">
                      <span>{{ group.label }}</span>
                      <n-tag size="tiny" :bordered="false">{{ group.items.length }}</n-tag>
                      <!-- 分组刷新：只刷新本组内的仓库，不影响其它组 -->
                      <n-button size="tiny" quaternary class="group-refresh"
                        :loading="groupRefreshing(group)"
                        :disabled="anyRefreshing" @click.stop="refreshGroup(group)">
                        <template #icon><RefreshOutline width="12" height="12" /></template>
                      </n-button>
                    </div>
                  </SourceGroupSlot>
                </template>
                <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" :group-key="group.key" v-slot="{ group }">
                  <div class="repo-grid">
                    <OnlineCard v-for="repo in group.items" :key="repo.id" :data-focus-id="repo.id"
                      :data-tutorial="repo.id === tutorialCardId ? 'mods-online-card' : undefined"
                      :provider="repo.provider" :owner="repo.username" :title="repoName(repo)"
                      :note="repoNote(repo) || repo.repository"
                      :status-text="statusOf(repo).text" :status-tone="statusOf(repo).tone"
                      :status-clickable="hasLocalInstall(repo)"
                      :status-title="hasLocalInstall(repo) ? t('modsOnline.viewInModManager') : undefined"
                      :dependency="cardDependency(repo)"
                      :version="versionParts(repo)" :refreshing="preparingSources || !!refreshing[repo.id] || !!queued[repo.id]"
                      :downloads="downloadCount(repo)"
                      :action="cardAction(repo)"
                      :error="repo.last_error" :active="detailId === repo.id"
                      :focus-flash="highlightId === repo.id"
                      @open="detailId = repo.id"
                      @status-click="openInLocalMods(repo)"
                      @dependency-click="jumpToRelationRepository(repo)"
                      @action="runCardAction(repo)" />
                  </div>
                </SourceGroupSlot>
              </n-collapse-item>
            </n-collapse>
          </SourceGroupSlot>
      </SourceDirectory>

      <!-- 社区仓库：与上方仓库源用分割线隔开，固定在最下方 -->
      <template v-if="hasCommunity">
        <div class="community-divider">
          <span>{{ t('dialogs.communityRepositories') }}</span>
        </div>
        <n-collapse :expanded-names="sourcesExpanded" class="source-collapse"
          @update:expanded-names="setOpenSources">
          <n-collapse-item v-for="node in communityTree" :key="node.id" :name="node.id">
            <template #header>
              <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" v-slot="{ node }">
                <div class="source-head">
                  <SourceTitle :label="node.label" :items="repositories.filter(item => item.source_id === node.id)" :source-url="sources.find(source => source.id === node.id)?.url" />
                  <!-- 社区来源未经官方索引校验，用严重级红色与「重复」同级提示 -->
                  <n-tag size="tiny" :bordered="false" class="state-tag"
                    :color="tagColorFor('critical')">{{ t('dialogs.unknownOrigin') }}</n-tag>
                  <n-tag size="tiny" :bordered="false" class="state-tag"
                    :color="tagColorFor('muted')">{{ node.count }}</n-tag>
                </div>
              </SourceGroupSlot>
            </template>
            <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" v-slot="{ node }">
              <!-- 社区仓库不分组：直接平铺卡片，没有分类标题 -->
              <div class="repo-grid community-grid">
                <OnlineCard v-for="repo in node.groups[0]?.items ?? []" :key="repo.id"
                  :data-focus-id="repo.id"
                  :data-tutorial="repo.id === tutorialCardId ? 'mods-online-card' : undefined"
                  :provider="repo.provider" :owner="repo.username" :title="repoName(repo)"
                  :note="repoNote(repo) || repo.repository"
                  :status-text="statusOf(repo).text" :status-tone="statusOf(repo).tone"
                  :status-clickable="hasLocalInstall(repo)"
                  :status-title="hasLocalInstall(repo) ? t('modsOnline.viewInModManager') : undefined"
                  :dependency="cardDependency(repo)"
                  :version="versionParts(repo)" :refreshing="preparingSources || !!refreshing[repo.id] || !!queued[repo.id]"
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
            </SourceGroupSlot>
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
  </PageLoading>

  <!-- 点击卡片：从右侧滑出仓库详情（指标 / 链接 / 备注 / README） -->
  <n-drawer :show="!!detailRepo" :width="720" placement="right" @update:show="(value: boolean) => { if (!value) detailId = null }">
    <n-drawer-content v-if="detailRepo" :native-scrollbar="false" body-content-class="drawer-detail-body">
      <ModRepositoryDetail :repo="detailRepo" :installing="!!installing[detailRepo.id]" :game-blocked="false"
        :mods="mods" :repositories="repositories"
        @close="detailId = null" @install="(tag?: string, asset?: string) => installFromDetail(tag, asset)" />
    </n-drawer-content>
  </n-drawer>

  <!--
    安装流程四段：来源选择 → 同身份冲突检查 → 详情预览 → 确认安装。
    「安装 MOD」按钮走来源选择；仓库卡片的「安装 / 重装 / 更新」
    先用 preview_only 下载解析，然后直接把预览交给同一个详情弹窗。
    解析出来若本地已有同身份的旧装项，先在检查弹窗里卸干净再进详情。
  -->
  <InstallSourceCard v-model:show="installSourceOpen" :busy="installSourceBusy"
    :drag-active="dragActive" :pick-file="pickInstallSourceFile"
    @select="handleInstallSourceSelect" />
  <InstallCheckDialog v-model:show="installCheckOpen" :model="installCheckModel"
    :execute="executeInstallCheck" @finished="finishInstallCheck" />
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

</style>
