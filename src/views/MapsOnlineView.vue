<script setup lang="ts">
import { useGameOperationGate } from '../composables/useGameOperationGate'
import { computed, onMounted, ref, watch } from 'vue'
import {
  NButton,
  NCollapse,
  NCollapseItem,
  NDrawer,
  NDrawerContent,
  NFloatButton,
  NTag,
} from 'naive-ui'
import ContentState from '../components/ContentState.vue'
import { LinkOutline, RefreshOutline } from '@vicons/ionicons5'
import { t, pickI18n } from '../i18n'
import { api } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { tagColorForAny, type TagTone } from '../theme'
import { useNotify } from '../useNotify'
import PageLoading from '../components/PageLoading.vue'
import HeaderIconAction from '../components/HeaderIconAction.vue'
import HeaderSearch from '../components/HeaderSearch.vue'
import { prefetchAvatars } from '../avatars'
import { useDashboard } from '../composables/useDashboardStore'
import { useInstallPlanGate } from '../composables/useInstallPlanGate'
import { versionDisplay } from '../modStatus'
import { markdownSummary } from '../markdown'
import { formatVersionSpec } from '../versionSpec'
import { modRepositoryIsAvailable } from '../repositoryAvailability'
import { useFocusHighlight, useLevelSetLink, useModRepositoryLink } from '../crossLink'
import { dependencyTagFor } from '../modRelations'
import {
  frameworkStates,
  levelSetForMap,
  mapRelationSubject,
  mapRelationStatuses,
  type FrameworkState,
} from '../levelSetStatus'
import { mapCardAction, mapCardStatus, type MapCardState } from '../mapCardStatus'
import OnlineCard, { type CardAction, type CardDependencyTag } from '../components/OnlineCard.vue'
import SourceGroupSlot from '../components/SourceGroupSlot.vue'
import SourceDirectory from '../components/SourceDirectory.vue'
import {
  useSourceGroupTree,
  type SourceGroupEntry,
  type SourceGroupLeaf,
  type SourceGroupSource,
} from '../composables/useSourceGroupTree'
import ReleaseDetailPanel from '../components/ReleaseDetailPanel.vue'
import type { DependencyRow } from '../components/DependencyTable.vue'
import InstallPlanDialog from '../components/InstallPlanDialog.vue'
import InstallCheckDialog from '../components/InstallCheckDialog.vue'
import InstallSourceCard from '../components/InstallSourceCard.vue'
import InstallDetailDialog from '../components/InstallDetailDialog.vue'
import BusyOverlay from '../components/BusyOverlay.vue'
import { useFileDrop } from '../composables/useFileDrop'
import { takePendingDrop, useDropRouting } from '../composables/useDropRouting'
import { useLevelSetImport } from '../composables/useLevelSetImport'
import { useLevelSetScan } from '../composables/useLevelSetScan'
import type { InstallSource, LevelSetInfo, MapInfo, ModRepository } from '../types'
import { demoActive, withDemoConfig, withDemoGame, withDemoLevelSets, withDemoMaps, withDemoMapSourceNodes, withDemoMods } from '../tutorial/demo'
import { useMapReleaseDetail } from '../composables/useMapReleaseDetail'
import { cachedInstallRelease } from '../repositoryAssets'

/**
 * 地图安装页。
 *
 * ## 与 MOD 安装页的关系
 *
 * 这一页的结构是**照着 MOD 安装页复刻**的：同一个卡片组件（`OnlineCard.vue`）、
 * 同一套三层折叠树（源 → 分类 → 卡片，含每级计数与刷新）、同一套页头
 * （计数 → 搜索 → 安装）、同一套网格与按钮文案规则。用户在同一件事
 * （「把线上这张图装到本地」）上不该看到两套长相。
 *
 * 树本身的规则（分桶键、顺序、展开）只有一份实现：
 * `composables/useSourceGroupTree.ts`，本页只把自己特有的数据投影进去。
 *
 * | 卡片字段 | MOD 安装页来源 | 地图来源 |
 * |---|---|---|
 * | 状态 | 已装插件的 GUID + 版本（`repositoryStatus`） | 本地关卡集（`mapCardStatus`） |
 * | 依赖提示 | 插件依赖关系 | `required_frameworks` + Dashboard 的本地 MOD 列表 |
 * | 主按钮 | 安装 / 重装 / 更新 / 刷新 | 同一张动作表（`mapCardAction`） |
 *
 * ## 为什么状态要读本地关卡集
 *
 * 早先这里只按「有没有 Release」显示「可安装 / 未刷新」，与 MOD 安装页的
 * 「未安装 / 已安装 / 有更新 / 最新版」不是一回事，用户因此觉得两页不像。
 * 现在进页面补一次 `scanLevelSets()`（后端对 `info` 有解析缓存，这一次调用
 * 换来的是每张卡都能回答「我装没装、要不要更新」）。
 *
 * ## 依赖判定为什么在前端
 *
 * 后端 `get_maps` 不填 `unmet_frameworks`（契约冻结）：依赖是否满足要用
 * 当前 Dashboard 的 MOD 列表判，而这份列表前端本来就有，交给后端等于每次
 * 打开页面再扫一遍 `BepInEx/plugins`。判定本身复用 MOD 侧同一条管线
 * （`mapRelationSubject` 仅提供关系上下文，再走共享 `resolveRelations`）。
 *
 * ## 安装入口在安装页
 *
 * 和「安装 MOD」一样，「安装地图」属于安装页：线上地图的卡片按钮在卡片上，
 * 本地 ZIP / 拖入 / 直链走页头的「安装地图」来源卡片（与 MOD 两页是同一个
 * `InstallSourceCard`）。地图管理页只做管理，不放安装入口。
 *
 * ## 跳转是双向的（与 MOD 两页一致）
 *
 * - 「地图管理 → 地图下载」：管理页点状态标签跳到本页并高亮那张卡（锚点是
 *   地图 id，本页卡片带 `data-focus-id`，由 `useFocusHighlight` 定位）。
 * - 「地图下载 → 地图管理」：本页状态标签的语义是**本地已装状态**（已安装 /
 *   有更新 / 最新版），点它就该去看本地的实物 —— MOD 安装页的「最新版」标签
 *   同样是跳到 MOD 管理页（`modsOnline.viewInModManager`）。未安装的地图没有
 *   本地卡片可去，标签因此不可点（`status-clickable` 只在有绑定时为真）。
 *   目标页与锚点：`#/levels` + 对应关卡集卡片的 `focus-flash`。
 */
const message = useNotify()
const { blockModOperation: gameBlocked } = useGameOperationGate()
import { useMapSources } from '../composables/useMapSources'

const { mods: realMods, game: realGame, config: realConfig, load, ensureLoaded } = useDashboard()
const mods = computed(() => withDemoMods(realMods.value))
const game = computed(() => withDemoGame(realGame.value))
const config = computed(() => withDemoConfig(realConfig.value))
const { highlightId } = useFocusHighlight()
const { linkToLevelSets } = useLevelSetLink()
/** 缺依赖的落点：MOD 安装页里那张能补上依赖的仓库卡片（与 MOD 管理页一致）。 */
const { linkToModRepository } = useModRepositoryLink()

/*
 * 地图源数据与刷新状态都来自模块级单例（`useMapSources`）。
 * 放在模块作用域是为了「刷新中切走再切回来」时状态还在 —— 页面实例会被
 * 卸载重建，挂在页面上的 ref 熬不过一次切页。理由详见该 composable 的注释。
 *
 * `anyRefreshing` 也读单例而不是本页自己拼：地图管理页点「检查更新」时，
 * 本页的整源 / 分组刷新按钮必须一起禁用 —— 两页共用同一个刷新引擎，
 * 忙碌状态也只能有一个真相。
 */
const {
  maps: realMaps, sourceNodes: realSourceNodes, loading, refreshing, queued, preparingSources, refreshingSources, installing, refreshingAll, anyRefreshing,
  loadMaps, refreshAll, refreshSource, refreshReleases, setFlag, syncDependencySnapshots,
} = useMapSources()
/**
 * 教程期间地图列表整体换成内置示例数据（与 MOD 安装页同一套 `withDemo*`，
 * 理由见 `tutorial/demo.ts` 的文件头）：地图源要联网才有条目，首启连不上源
 * 时教程那几步没有卡片可指。
 *
 * 用 computed 包一层而不是改名到每个使用点：计数、搜索、分组树与详情抽屉
 * 全都读 `maps.value`，这里换掉数据源即可，展示逻辑一行不用动。
 */
const maps = computed<MapInfo[]>(() => withDemoMaps(realMaps.value))
const sourceNodes = computed(() => withDemoMapSourceNodes(realSourceNodes.value))
const realSets = ref<LevelSetInfo[]>([])
const sets = computed(() => withDemoLevelSets(realSets.value))
/**
 * 本地关卡集扫描的唯一入口。
 *
 * 状态与进度是 `useLevelSetScan` 的模块级单例：别的页面正在读时这里也在读，
 * 总览页的卡片因此看得到这次扫描（反过来同理）。
 */
const { scanLevelSets } = useLevelSetScan()
/**
 * 「安装地图」的暂存 → 确认 → 导入交给共享实现（地图管理页的拖放用的是同一份）。
 * 本页只负责装完重读本地关卡集：卡片状态与按钮文案都由它算出来。
 * 确认界面与 MOD 安装共用 `InstallDetailDialog`（一个地图集当一个 MOD 展示）。
 */
const {
  importing, importSource, installRelease,
  previewOpen: importPreviewOpen, detail: importDetail,
  confirmImport, discardImport,
  importCheck,
  zipProgress, zipProgressText,
} = useLevelSetImport(() => loadLevelSets())
/** 「安装地图」来源卡片是否打开。 */
const installSourceOpen = ref(false)

/**
 * 整窗进度遮罩：来源卡片没打开时才用。
 *
 * 本页在线安装是主路径，它没有来源卡片 —— 下载由 `DownloadStrip` 显示，
 * 下载完的识别与写盘只能靠这里告诉用户「读到第几个 info / 写到第几张」；
 * 卡片打开时进度显示在卡片里，不叠两个转圈。
 */
const showZipOverlay = computed(
  () => importing.value && !!zipProgress.value && !installSourceOpen.value,
)
/** 详情抽屉里当前打开的地图 id（抽屉挂在页面级，卡片只负责打开）。 */
const detailId = ref<string | null>(null)

/**
 * 游戏是否已探测。
 *
 * 没探测到游戏时 Dashboard 的 MOD 列表必然是空的，此时「缺框架」只是把
 * 「不知道」说成「缺」，所以在探测到游戏之前不显示依赖标签。
 */
const gameProbed = computed(() => !!game.value?.path)

/**
 * 可安装来源：与 MOD 安装页同一套「在当前地区真实存在」过滤。
 *
 * 地图的前置是框架（DIYLevel / Web Runtime），它们本身也是**在线 MOD 仓库**，
 * 所以缺框架时能像 MOD 的依赖一样被装上。传空数组会让「安装前检查」只显示
 * 一句「缺依赖」而没有可执行的步骤，用户还得自己去设置页找框架。
 */
const repositories = computed<ModRepository[]>(() =>
  (config.value?.repositories ?? []).filter((repo) =>
    modRepositoryIsAvailable(
      repo,
      config.value?.repository_sources,
      config.value?.region,
      config.value?.platform,
    )),
)

/**
 * 安装前检查。
 *
 * 地图的 `relations` 与 MOD 仓库的 `relations` 是同一套模型，所以判定逻辑也
 * 共用 useInstallPlanGate 的最小 RelationSubject。地图只提供身份、显示名、
 * Release 和关系上下文；依赖候选仍来自真实 MOD 仓库，不构造虚拟 MOD。
 */
const planGate = useInstallPlanGate({
  mods: () => mods.value,
  repositories: () => repositories.value,
  gameBlocked,
  onApplied: () => load(),
})


/* ===== 搜索 =====
 *
 * 与 MOD 安装页同一套：纯前端过滤，不触发任何网络请求，也不影响刷新状态。
 * 匹配范围覆盖「用户可能记得的任何一个名字」—— 卡片展示名、原始名、五个
 * 语言的本地化名、分类、作者、绑定身份（关卡集名）与仓库坐标、备注。
 */
const search = ref('')
const query = computed(() => search.value.trim().toLowerCase())

function matches(map: MapInfo) {
  if (!query.value) return true
  const haystack = [
    mapName(map),
    map.name,
    ...Object.values(map.name_i18n ?? {}),
    map.group ?? '',
    ...Object.values(map.group_i18n ?? {}),
    map.author,
    map.level_set_name,
    map.username,
    map.repository,
    map.provider,
    map.note ?? '',
    ...Object.values(map.note_i18n ?? {}),
  ].join(' ').toLowerCase()
  return haystack.includes(query.value)
}

/** 页头计数：有搜索词时是过滤后的数量（与 MOD 安装页同一口径）。 */
const visibleCount = computed(() => maps.value.filter(matches).length)

/**
 * 空状态文案。
 *
 * 两种「空」要分开说：没有地图源是配置问题（去设置里看源），搜不到是本次
 * 输入没命中（改搜索词），把后者说成前者会把用户引到错误的地方去。
 */
const emptyDescription = computed(() =>
  maps.value.length && query.value ? t('levelSet.noMatchMaps') : t('levelSet.noMapSources'),
)

/* ===== 源 → 分类 → 地图 =====
 *
 * 层级结构与 MOD 安装页完全对齐（同一个 `useSourceGroupTree`）：分桶键用
 * 原文不用翻译名、顺序按源/条目的声明顺序不排序、搜索时强制全展开。
 *
 * 本页特有的两件事由这里投影进去：
 *
 * - **源名挂在条目上**。地图源树在后端不落盘（源只是归类方式），所以每条
 *   地图自带 `source_id` + `source_name(_i18n)`（加载源树时写入并持久化），
 *   源顺序也就只能按条目首次出现的顺序读 —— 那正是源文档的声明顺序。
 * - 没有「社区源」这一层特例，所有源都正常显示分类标题。
 */
const treeEntries = computed<SourceGroupEntry<MapInfo>[]>(() =>
  maps.value
    .filter(matches)
    .map((map) => ({
      sourceId: map.source_id ?? '',
      sourceLabel: pickI18n(map.source_name_i18n, map.source_name ?? ''),
      group: map.group ?? '',
      groupLabel: pickI18n(map.group_i18n, map.group ?? '') || t('common.other'),
      item: map,
    })),
)

const { tree, setOpenSources, setOpenGroups, sourcesExpanded, groupsExpanded } = useSourceGroupTree(
  () => treeEntries.value,
  () => ({
    order: sourceNodes.value.map(node => ({ id: node.id, parentId: node.parent_id,
      label: pickI18n(node.label_i18n, node.label), note_i18n: node.note_i18n, url: node.url, status: node.status, error: node.error })),
    preserveEmpty: !query.value,
    // Document declarations include pending and failed sources.
    ungrouped: { id: '__none__', label: t('levelSet.uncategorizedMapSource') },
  }),
  () => !!query.value,
)

/**
 * 读一次本地关卡集。
 *
 * 卡片状态与按钮文案都依赖它（「已安装 / 有更新 / 重装」），所以进页面必须
 * 拿到。失败不影响列表展示：最坏是状态停在「无法检测更新」那一档。
 */
async function loadLevelSets() {
  if (demoActive.value) return
  try {
    realSets.value = await scanLevelSets()
  } catch (reason) {
    message.warning(backendMessageOf(reason))
  }
}

/**
 * 预取 owner 头像 —— 与 MOD 安装页同一条链路、同一份实现。
 *
 * 卡片与详情抽屉的图标（`ProviderIcon`）只读 `avatars.ts` 的内存缓存，缓存
 * 由这里批量填充：`prefetchAvatars` 按 `provider:owner` 去重，后端再按 owner
 * 单飞 + 磁盘缓存（12 小时），所以同一 owner 只真正下载一次。地图的
 * `provider` / `username` 与仓库同形，不需要为地图另写一份预取。
 *
 * `immediate` + `onMounted` 双触发与 MOD 安装页同因：首次进页面时 `maps`
 * 还是空的，watch 不会触发；数据到了再触发一次，已有在途请求会复用。
 */
watch(maps, (list) => { void prefetchAvatars(list) }, { immediate: true })

onMounted(() => {
  if (demoActive.value) return
  /*
   * 依赖提示与安装前检查都读 Dashboard 的 MOD 列表，而 Dashboard 由页面
   * 自己按需加载。缺少这一步时直接进本页会判不出「依赖缺失」，安装前置检查
   * 也会漏掉已装的框架 —— 结论错在「看起来一切正常」这一侧。
   */
  void ensureLoaded()
  void loadMaps()
  void loadLevelSets()
  void prefetchAvatars(maps.value)
  // 从 MOD 页「去安装」跳过来：路径已暂存在模块里，直接接着走导入
  const pending = takePendingDrop('map')
  if (pending) void startDropInstall(pending)
})

watch(demoActive, active => {
  if (active) return
  void ensureLoaded()
  void loadMaps()
  void loadLevelSets()
})

/** 展示名：多语言表 → 单语言镜像 → 仓库坐标。 */
function mapName(map: MapInfo): string {
  return pickI18n(map.name_i18n, map.name) || `${map.username}/${map.repository}`
}

/**
 * 简介：下载页要留足说明，用户才知道这张图是什么。
 *
 * 没有说明时不再退回仓库名 —— 一个仓库多张图时，仓库名会把每张卡写成一个样，
 * 用户看不出这是哪张图；直接说明「暂无说明」更诚实。
 */
function mapNote(map: MapInfo): string {
  return (
    markdownSummary(pickI18n(map.note_i18n, map.note ?? '')) || t('levelSet.mapNoNote')
  )
}

function isRefreshing(map: MapInfo): boolean {
  return preparingSources.value || !!refreshing.value[map.id] || !!queued.value[map.id]
}

function isInstalling(map: MapInfo): boolean {
  return !!installing.value[map.id]
}

/**
 * 卡片状态：本地已装状态（与 MOD 安装页同款结论）。
 *
 * 判定细节在 `mapCardStatus.ts` 的纯函数里（Node 测试直接覆盖），这里只做
 * 翻译与着色 —— 页面不再自己拼状态字符串，避免两处规则漂移。
 */
function statusOf(map: MapInfo): { key: MapCardState; text: string; tone: TagTone } {
  const status = mapCardStatus(map, sets.value)
  return { key: status.key, text: t(status.text), tone: status.tone }
}

/**
 * 本地是否已装这张图。
 *
 * 判据是「地图声明的关卡集身份能在本地找到」——同一个 `level_set_name +
 * level_set_name_zh + author`。只有存在本地副本时，卡片状态才会落到
 * 「已安装 / 有更新 / 最新版」，
 * 也才有本地卡片可以跳过去。
 */
function hasLocalMap(map: MapInfo): boolean {
  return !!levelSetForMap(map, sets.value)
}

/**
 * 状态标签点击 → 地图管理页并高亮那张关卡集卡。
 *
 * 与 MOD 安装页完全对齐：那边「最新版」标签点下去是 MOD 管理页
 * （`modsOnline.viewInModManager`），这里就是地图管理页。未安装的地图没有
 * 本地卡片可去，标签因此不可点（模板用 `status-clickable` 同步视觉）。
 */
function openInLevelSets(map: MapInfo) {
  if (!hasLocalMap(map)) return
  linkToLevelSets(map.id)
}

const {
  detailMap,
  detailRelease,
  detailVersions,
  detailDependencies,
  detailAssets,
  detailAsset,
  detailSelectedTag,
  detailSelectedAsset,
  detailReleaseTag,
  detailPrerelease,
  detailLinks,
  detailNote,
  detailName,
  detailProvider,
  detailOwner,
  detailRepository,
  detailBindingValue,
  detailPrimaryAction,
} = useMapReleaseDetail({
  maps,
  detailId,
  mods,
  repositories,
  mapName,
})
const detailStatus = computed(() =>
  detailMap.value
    ? statusOf(detailMap.value)
    : { key: 'not_refreshed' as MapCardState, text: '', tone: 'muted' as TagTone },
)

/** 版本号展示：与 MOD 卡片同一套两段式规则。 */
function versionOf(map: MapInfo) {
  const release = map.latest_release
  if (!release) return versionDisplay(null)
  return versionDisplay(release.tag_name || release.version, release.prerelease, t('modsOnline.t'))
}

/** 下载量：与 MOD 卡片同一算法（最新 Release 全部资产之和）。 */
function downloadCount(map: MapInfo): number | null {
  const total =
    map.latest_release?.assets?.reduce((sum, asset) => sum + (asset.download_count ?? 0), 0) ?? 0
  return total || null
}

/** 一条框架依赖的展示文案：GUID 太抽象，补上「已装多少」。 */
function describeFramework(state: FrameworkState): string {
  const spec = formatVersionSpec(state.version)
  const required = spec ? `${state.guid} ${spec}` : state.guid
  if (!state.installedVersion) return required
  return `${required}（${t('levelSet.installedVersion', { version: state.installedVersion })}）`
}

/** 详情面板头部链接：交给后端用系统浏览器打开（与 MOD 详情同一入口）。 */
async function openLink(url: string) {
  if (!url) return
  await api.openReleasePage(url).catch(() => undefined)
}

/**
 * 依赖提示标签。
 *
 * 与 MOD 安装页共用 `dependencyTagFor` 一条规则：前置没满足就写「缺依赖」；
 * 线上有能补上的仓库卡片时标签可点，落点是**那张仓库卡**（MOD 安装页）——
 * 地图的前置（DIYLevel、Web Runtime）本身就是仓库里的 MOD，所以这条跳转
 * 与 MOD 管理页的「缺依赖 → 去安装」是同一个去处。
 */
function dependencyOf(map: MapInfo): CardDependencyTag | null {
  if (!gameProbed.value) return null
  const tag = dependencyTagFor(
    mapRelationStatuses(map, mods.value, repositories.value),
    hasLocalMap(map),
  )
  if (!tag) return null
  const missing = frameworkStates(map, mods.value, repositories.value)
    .filter((state) => !state.satisfied)
  return {
    text: tag.text,
    tone: tag.tone,
    title: missing.length
      ? t('levelSet.frameworksNotMet', { list: missing.map(describeFramework).join(' · ') })
      : undefined,
    clickable: !!tag.repositoryId,
  }
}

/** 点依赖标签：跳到能提供它的那张仓库卡片（与 MOD 安装页同一个高亮机制）。 */
function jumpToDependency(map: MapInfo) {
  const tag = dependencyTagFor(
    mapRelationStatuses(map, mods.value, repositories.value),
    hasLocalMap(map),
  )
  if (tag?.repositoryId) linkToModRepository(tag.repositoryId)
}

/** 依赖表里可跳的行：同样落到那张仓库卡片上。 */
function openDependencyJump(row: DependencyRow) {
  if (row.jump) linkToModRepository(row.jump.id)
}

/** 主按钮文案：与 MOD 安装页共用同一批 key。 */
const ACTION_LABELS: Record<CardAction['kind'], string> = {
  install: 'modsOnline.install',
  reinstall: 'overview.reinstall',
  update: 'dialogs.update',
  refresh: 'modsOnline.refresh',
}

/**
 * 主按钮。
 *
 * 规则完全交给 `mapCardAction`（纯函数、有测试）：没有 Release 只能刷新，
 * 本地没装是安装，本地版本低是更新，已经装了是重装 —— 与 MOD 安装页一致。
 */
function actionOf(map: MapInfo): CardAction {
  const kind = mapCardAction(map, sets.value)
  return {
    kind,
    label: t(ACTION_LABELS[kind]),
    loading: kind === 'refresh' ? isRefreshing(map) : isInstalling(map),
    disabled: isRefreshing(map) || isInstalling(map),
  }
}



/** 刷新一张地图的 Release（用户主动点刷新 → 绕过缓存）。 */
async function refresh(map: MapInfo) {
  const result = await refreshReleases([map], { trigger: 'single' })
  if (result.errors.length) message.error(result.errors[0])
}

/**
 * 安装。
 *
 * 顺序与 MOD 卡片完全一致：
 *   1. 安装前检查（依赖框架缺失 / 版本不够 → 能补就先补，不能补只提醒）
 *   2. 下载 + 校验 + 识别（后端暂存资产，进度由全局 DownloadStrip 显示）
 *   3. 确认详情（与本次安装身份相同的旧关卡集先卸载，失败即中止）
 *   4. 落盘导入
 *
 * `asset` 是详情页资产下拉里选中的文件名：不传时后端按 `asset_rules` 自动选，
 * 与 MOD 安装同一条规则。
 *
 * 落盘后补读一次本地关卡集：状态标签与按钮要立刻从「未安装」翻成「已安装」，
 * 否则用户装完看到还是「安装」，会以为没生效。
 */
async function install(map: MapInfo, tag?: string, asset?: string) {
  if (isInstalling(map) || isRefreshing(map)) return
  const release = cachedInstallRelease(map.cached_releases, tag ?? map.latest_tag ?? undefined)
  if (!release) {
    message.error(t('mapSource.releaseMissing'))
    return
  }
  const targetTag = release.tag_name
  // 前置检查：地图缺框架时装上也跑不起来，必须先让用户看到
  const ready = await planGate.run(mapRelationSubject(map, targetTag), targetTag)
  if (!ready) return

  setFlag(installing, map.id, true)
  try {
    /*
     * 后端只负责「下载资产 + 识别关卡集」的暂存，确认、冲突卸载与导入全部
     * 走 `useLevelSetImport.installRelease`（与本地 ZIP / 直链同一份实现）。
     * 安装后的依赖快照同步必须早于关卡集重读，否则卡片状态会按旧快照算，
     * 所以这里把「快照 → 重读」作为 afterImport 传给共享实现；成功提示仍按
     * 地图名说「安装完成」，与本地 ZIP 导入的「已导入 N 个」区分开。
     */
    await installRelease(map.id, targetTag, asset, {
      afterImport: async () => {
        // 安装清单里的依赖快照按线上声明重写，紧接着的状态重读才不会用到旧快照
        await syncDependencySnapshots()
        await loadLevelSets()
      },
      successText: () => t('levelSet.installDone', { name: mapName(map) }),
    })
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    setFlag(installing, map.id, false)
  }
}

/**
 * 「安装地图」来源卡片：本地文件 / 拖入窗口 / 在线直链三个入口共用。
 *
 * 与「安装 MOD」同一职责定位 —— 线上装不进来的东西（作者发的离线包、自己
 * 整理的图、挂在别处的直链）走这里。暂存 → 确认 → 导入的实现下沉在
 * `useLevelSetImport`：地图管理页的整窗拖放走的是同一份，两页的确认文案、
 * 覆盖提示、暂存清理与「装完重读」因此不会各漂各的。
 */
async function handleInstallSourceSelect(source: InstallSource) {
  await importSource(source, {
    // 来源已经可用：先收起卡片，再交给确认弹窗
    onStaged: () => { installSourceOpen.value = false },
  })
}

/**
 * 拖入 / 跨页跳转过来的文件：先打开来源卡片再交给同一个处理函数。
 *
 * 卡片里的转圈是「正在暂存与识别」的唯一反馈 —— 识别一份上百 MB 的地图合集
 * 要几十秒，没有反馈用户会以为没反应。
 */
async function startDropInstall(path: string) {
  installSourceOpen.value = true
  await handleInstallSourceSelect({ kind: 'local', value: path })
}

const { acceptDrop } = useDropRouting()

/**
 * 整窗拖放：拖一个 ZIP 进来等价于在本页点「安装地图」。
 *
 * 拖放是窗口能力而不是某张卡片的属性（见 `useFileDrop`），所以地图管理页也
 * 能用同一条链路。先过 `acceptDrop`：纯 DLL 的包属于 MOD 域，要引导到 MOD
 * 安装页，不能在这里报「ZIP 里没有 info」。
 */
const { dragActive } = useFileDrop({
  accept: ['zip', 'dll'],
  busy: () => demoActive.value || importing.value,
  onDrop: (path) => {
    void (async () => {
      if (await acceptDrop(path, 'map')) await startDropInstall(path)
    })()
  },
})

/** 详情抽屉里的安装：把当前选中的 tag 与资产一起传下去。 */
function installFromDetail(tag?: string, asset?: string) {
  const map = detailMap.value
  if (map) void install(map, tag, asset)
}

function refreshDetail() {
  const map = detailMap.value
  if (map) void refresh(map)
}

/** 主按钮点击：刷新就走刷新，其余动作都落到安装入口。 */
function runAction(map: MapInfo) {
  if (mapCardAction(map, sets.value) === 'refresh') {
    void refresh(map)
    return
  }
  void install(map)
}

/** 分类里是否有卡片正在刷新/安装（决定分类刷新按钮的转圈与禁用）。 */
function groupRefreshing(group: SourceGroupLeaf<MapInfo>): boolean {
  return group.items.some((map) => isRefreshing(map) || isInstalling(map))
}

/** 源里是否有卡片正在忙（决定源刷新按钮的转圈）。 */
function sourceRefreshing(node: SourceGroupSource<MapInfo>): boolean {
  if (refreshingSources.value[node.id]) return true
  return node.groups.some((group) => group.items.some((map) => isRefreshing(map) || isInstalling(map)))
}


/**
 * 刷新一个分类下的全部 Release。
 *
 * 只刷本分类内的 Release，不重拉源树：源文档是分类与地图的来源，只点一个
 * 分类就把整棵源树清空重载没有意义。
 */
async function refreshGroup(group: SourceGroupLeaf<MapInfo>) {
  if (refreshingAll.value || groupRefreshing(group)) return
  const result = await refreshReleases(group.items)
  if (result.errors.length) message.error(result.errors[0])
}


/**
 * 整页遮罩的显示条件，与 MOD 安装页同一条判据：
 * 「正在刷新 **且** 列表是空的」。
 *
 * 刷新一开始就把列表清空（见 `refreshAll`），所以这条在这里是有意义的；
 * 反过来，源树回来、第一张卡片能渲染时 `visibleCount` 立刻大于 0，遮罩当场
 * 消失 —— 不需要在几个中间状态之间切换，也就不会有「遮罩闪两下」的抖动。
 */
const pageLoading = computed(() => loading.value && tree.value.length === 0)

</script>

<template>

  <!--
    页头右侧，从左到右：计数 → 搜索 → 安装地图（与 MOD 安装页「计数 → 搜索 →
    安装 MOD」同序）。「安装地图」固定在最右：它是本页最主要的本地安装入口。
    整源刷新由右下角浮动按钮承担，页头不再重复放一个按钮。
  -->
  <Teleport to="#page-header-actions" defer>
    <span class="header-count muted">{{ visibleCount }} {{ t('levelSet.mapCount') }}</span>
    <HeaderSearch v-model:value="search" />
    <HeaderIconAction type="primary" :label="t('levelSet.installMap')" :loading="importing" :disabled="importing"
      @click="installSourceOpen = true">
      <LinkOutline width="16" height="16" aria-hidden="true" />
    </HeaderIconAction>
  </Teleport>

  <PageLoading :show="pageLoading">
    <template #status>{{ t('levelSet.readingMapSources') }}</template>
    <ContentState v-if="!pageLoading && !tree.length" :description="emptyDescription" data-tutorial="maps-online-page" />

    <!--
      三层折叠树，与 MOD 安装页同形：源（计数 + 刷新）→ 分类（计数 + 刷新）
      → 卡片网格。层级、缩进与刷新按钮的位置都与 MOD 安装页一致 —— 两页在
      用户眼里是同一件事（「把线上这个东西装到本地」），不该是两套长相。
    -->
    <SourceDirectory v-else data-tutorial="maps-online-page" :nodes="tree" :expanded="sourcesExpanded" :disabled="anyRefreshing"
      :busy="sourceRefreshing" @refresh="refreshSource" @update:expanded="setOpenSources" v-slot="{ node }">
        <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" v-slot="{ node }">
          <n-collapse :expanded-names="groupsExpanded" class="group-collapse"
            @update:expanded-names="setOpenGroups">
            <n-collapse-item v-for="group in node.groups" :key="group.key" :name="group.key">
              <template #header>
                <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" :group-key="group.key" v-slot="{ group }">
                  <div class="group-head">
                    <span>{{ group.label }}</span>
                    <n-tag size="tiny" :bordered="false">{{ group.items.length }}</n-tag>
                    <!-- 分类刷新：只刷新本分类内的地图，不影响其它分类 -->
                    <n-button size="tiny" quaternary class="group-refresh"
                      :loading="groupRefreshing(group)" :disabled="anyRefreshing"
                      :title="t('modsOnline.refresh')" @click.stop="refreshGroup(group)">
                      <template #icon><RefreshOutline width="12" height="12" /></template>
                    </n-button>
                  </div>
                </SourceGroupSlot>
              </template>
              <SourceGroupSlot :get-tree="() => tree" :source-id="node.id" :group-key="group.key" v-slot="{ group }">
                <div class="repo-grid">
                  <!-- 卡片就是 MOD 安装页那张：只把数据换成地图的 -->
                  <OnlineCard v-for="map in group.items" :key="map.id" :data-focus-id="map.id"
                    :provider="map.provider" :owner="map.username" :title="mapName(map)"
                    :note="mapNote(map)"
                    :status-text="statusOf(map).text" :status-tone="statusOf(map).tone"
                    :status-clickable="hasLocalMap(map)"
                    :status-title="hasLocalMap(map) ? t('levelSet.viewInMapManager') : undefined"
                    :dependency="dependencyOf(map)"
                    :version="versionOf(map)" :refreshing="isRefreshing(map)"
                    :downloads="downloadCount(map)"
                    :action="actionOf(map)"
                    :error="map.last_error"
                    :active="detailId === map.id"
                    :focus-flash="highlightId === map.id"
                    @open="detailId = map.id"
                    @status-click="openInLevelSets(map)"
                    @dependency-click="jumpToDependency(map)"
                    @action="runAction(map)" />
                </div>
              </SourceGroupSlot>
            </n-collapse-item>
          </n-collapse>
        </SourceGroupSlot>
    </SourceDirectory>
  </PageLoading>

  <!--
    详情抽屉：与 MOD 安装页是**同一个面板**（`ReleaseDetailPanel.vue` 只做版式）。
    地图只往 `#status` 插槽里补一个「已装状态」标签；依赖框架走面板的依赖表
    （说明 tab 的备注上方），绑定身份走「版本绑定」列 —— 剩下的 README、版本、
    资产下拉与安装按钮全部由面板统一渲染，两页不再各写一套。
    面板不认识 `MapInfo`，所有业务值都在本页算好再传。
  -->
  <n-drawer :show="!!detailMap" :width="720" placement="right"
    @update:show="(value: boolean) => { if (!value) detailId = null }">
    <n-drawer-content v-if="detailMap" :native-scrollbar="false" body-content-class="drawer-detail-body">
      <!--
        `:key` 必须有：面板自己持有「说明 / 下载安装」tab 状态，换地图要重置。
        版本选择的状态留在页面（`detailSelectedTag`），面板只发 `update:version`。
      -->
      <ReleaseDetailPanel :key="detailMap.id" :title="detailName" :provider="detailProvider"
        :owner="detailOwner" :repository="detailRepository" :icon-size="34"
        :meta-links="detailLinks" :note="detailNote"
        :note-empty-text="t('levelSet.mapNoNote')"
        :dependencies="detailDependencies"
        :versions="detailVersions" :version="detailSelectedTag"
        :binding-label="t('levelSet.bindingIdentity')" :binding-value="detailBindingValue"
        :release-tag="detailReleaseTag" :published-at="detailRelease?.published_at ?? null"
        :prerelease="detailPrerelease" :release-notes="detailRelease?.body ?? ''"
        :assets="detailAssets" :asset-value="detailAsset?.name ?? null"
        :primary-action="detailPrimaryAction"
        :installing="isInstalling(detailMap)" :refreshing="isRefreshing(detailMap)"
        @update:version="(value: string) => { detailSelectedTag = value }"
        @update:asset="(value: string) => { detailSelectedAsset = value }"
        @open-link="(url: string) => { void openLink(url) }"
        @dependency-jump="openDependencyJump"
        @install="installFromDetail" @refresh="refreshDetail" @close="detailId = null">
        <!-- 头部状态：与卡片上是同一份结论（`statusOf`） -->
        <template #status>
          <n-tag size="small" :color="tagColorForAny(detailStatus.tone)">
            {{ detailStatus.text }}
          </n-tag>
        </template>
      </ReleaseDetailPanel>
    </n-drawer-content>
  </n-drawer>

  <!--
    安装前检查：缺框架时先补装（与 MOD 安装页同一个弹窗与同一套执行顺序）
  -->
  <InstallPlanDialog v-model:show="planGate.open.value" :plan="planGate.plan.value"
    :target-label="planGate.target.value"
    @finished="(ready: boolean) => { void planGate.finish(ready) }" />

  <!--
    「安装地图」来源卡片：与 MOD 两页同一个组件（本地选择 / 拖放 / 直链），
    只是这里只收 ZIP、标题是「安装地图」。取到来源后统一经后端暂存再确认导入。
  -->
  <InstallSourceCard v-model:show="installSourceOpen" :accept="['zip']"
    :title="t('levelSet.installMap')" :busy="importing" :drag-active="dragActive"
    :read-progress-text="importing ? zipProgressText : ''"
    :pick-file="api.chooseLevelSetZip" @select="handleInstallSourceSelect" />

  <!--
    来源卡片没打开时的进度（本页主路径的在线安装、确认后的写盘阶段）：与卡片里
    那行文案同一份数据，避免两处各写一套「到第几个了」。
  -->
  <BusyOverlay :show="showZipOverlay" :status="zipProgressText" />

  <!--
    同身份冲突的检查：在「安装详情」之前挡一道（已有同一张地图时先卸载）。
    与 MOD 安装、地图管理页共用同一个弹窗与同一套执行顺序。
  -->
  <InstallCheckDialog v-model:show="importCheck.open.value" :model="importCheck.model.value"
    :execute="importCheck.execute"
    @finished="(ready: boolean) => { void importCheck.finish(ready) }" />

  <!--
    导入前的「安装详情」：和 MOD 安装同一张卡片，一个地图集当一个 MOD。
    不接 `busy`：它在「等待确认」期间也为真（整个导入流程都算 busy），
    而那时取消与立即安装恰恰必须能点；写盘期间弹窗已经收起，进度由页面按钮显示。
  -->
  <InstallDetailDialog v-model:show="importPreviewOpen" :model="importDetail"
    @confirm="confirmImport" @discard="discardImport" />

  <!--
    整源刷新入口，与 MOD 安装页同一个位置与观感。
    - 禁用看共享的 `anyRefreshing`（不再只看 `refreshingAll`）：地图管理页正在
      「检查更新」时同一批图也在刷，这时候再点整源刷新会并发两批请求、列表以
      后完成的那批为准 —— 用户看到的是「刷新完又变回去」。
    - 图标只在 `refreshingAll` 时转：单张图或管理页的检查更新不该让这个
      「刷新全部」按钮一直转，那会让人以为整源刷新还没结束。
  -->
  <n-float-button position="fixed" bottom="28" right="32" :disabled="anyRefreshing"
    :tooltip="t('levelSet.refreshAllHint')" @click="refreshAll({ force: true })">
    <RefreshOutline width="17" height="17" :class="{ 'spin-icon': refreshingAll }" />
  </n-float-button>
</template>

<style scoped>
/* 页头计数：与 MOD 安装页同一档字号/颜色（`muted` 在全局，这里补不换行） */
.header-count {
  flex: 0 0 auto;
  white-space: nowrap;
  font-size: 12.5px;
}


/* 浮动刷新按钮转圈（本页作用域，keyframes 名称与其它页区分） */
.spin-icon {
  flex: 0 0 auto;
  color: var(--oc2-accent);
  animation: oc2-map-download-spin .8s linear infinite;
}

@keyframes oc2-map-download-spin {
  to { transform: rotate(360deg); }
}

@media (prefers-reduced-motion: reduce) {
  .spin-icon { animation: none; }
}

/*
 * 与 `ModsOnlineView.vue` 的 `.source-collapse` / `.source-head` /
 * `.group-collapse` / `.group-head` / `.group-refresh` / `.repo-grid` 保持
 * 同一份数值 —— 两页列表要看起来是同一个东西。卡片内部样式全在
 * `OnlineCard.vue`，这里只管列表骨架。
 */
.source-collapse {
  width: 100%;
}

/* 三级固定缩进：源 0、分类 14px、卡片 28px（与 MOD 安装页同一份数值） */
.source-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}

.source-head b {
  font-size: 14px;
  color: var(--oc2-text-1);
}

.group-collapse {
  margin: -4px 0 0 14px;
}

.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--oc2-accent);
}

.group-refresh {
  flex: 0 0 auto;
  color: var(--oc2-text-3);
}

.group-refresh:hover {
  color: var(--oc2-accent);
}

.repo-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(min(310px, 100%), 1fr));
  gap: 12px;
  margin: 2px 28px 6px 28px;
}

</style>
