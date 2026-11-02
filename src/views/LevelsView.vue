<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, provide, reactive, ref, watch } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { NButton, NCollapse, NCollapseItem, NEmpty, NFloatButton, NTabPane, NTabs, NTag, useDialog } from 'naive-ui'
import { FolderOpenOutline, RefreshOutline } from '@vicons/ionicons5'
import { t, locale, pickI18n } from '../i18n'
import { api, watchLevelSetScan } from '../services/tauri'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { tagColorFor } from '../theme'
import { useNotify } from '../useNotify'
import { useDashboard } from '../composables/useDashboardStore'
import { useMapSources } from '../composables/useMapSources'
import { useFocusHighlight, useMapLink, useRepositoryLink, useStatusHighlight } from '../crossLink'
import { levelSetDependencyRows } from '../dependencyRows'
import { repositoryIsAvailable } from '../repositoryAvailability'
import { describeRefreshOutcome } from '../refreshOutcomeText'
import { markDialogBusy } from '../dialogBusy'
import { collapseCardWithScroll, naturalContentHeight, scrollExpandedCardIntoView } from '../expandScroll'
import PageLoading from '../components/PageLoading.vue'
import RefreshResultTag from '../components/RefreshResultTag.vue'
import type { DependencyRow } from '../components/DependencyTable.vue'
import InstallSourceCard from '../components/InstallSourceCard.vue'
import InstallDetailDialog from '../components/InstallDetailDialog.vue'
import InstallCheckDialog from '../components/InstallCheckDialog.vue'
import BusyOverlay from '../components/BusyOverlay.vue'
import { useFileDrop } from '../composables/useFileDrop'
import { useDropRouting, takePendingDrop } from '../composables/useDropRouting'
import { useLevelSetImport } from '../composables/useLevelSetImport'
import { useCardExpansion } from '../composables/useCardExpansion'
import LevelSetGroupBody from '../components/LevelSetGroupBody.vue'
import LevelSetGroupHeader from '../components/LevelSetGroupHeader.vue'
import {
  LEVEL_SET_GROUP_CTX,
  TOOL_BUTTON_WIDTH,
  TOOL_GAP,
  type LevelSetGroupContext,
} from '../components/levelSetGroupContext'
import {
  levelSetAuthorNames,
  levelSetCardTags,
  levelSetDisplayName,
  levelSetGroup,
  levelSetRelationStatuses,
  levelSetState,
  mapForLevelSet,
  mapVersionForLevelSet,
  mapVersionRefs,
  type LevelSetCardTag,
  type LevelSetRefreshingHint,
  type LevelSetTone,
} from '../levelSetStatus.ts'
import type { InstallSource, LevelSetInfo, LevelSetPlugin, LevelSetScanProgress, MapInfo, Repository } from '../types'

/**
 * 地图管理页 —— 只做「管理已经在本地的地图」。
 *
 * ## 安装入口：本页只收本地包
 *
 * 线上 Release 的安装入口在地图下载页（`MapsOnlineView.vue`）—— 地图源与
 * 版本只在那里。本页收的是「手上已经有一个 ZIP」的情况：把 ZIP 拖进窗口，
 * 来源卡片会打开、识别出里面的关卡集并请用户确认，与下载页共用
 * `useLevelSetImport` 那一份实现。
 *
 * ## 与 MOD 管理页的关系
 *
 * 卡片外壳直接复用 `LocalItemCard`（与 MOD 管理页同一份），页头动作的顺序
 * 与语义也照它排：批量开关 → 检测更新 → 打开目录；右下角浮动按钮是「重新扫描
 * 本地地图」。批量开关做成二选一分段控件而不是两个按钮：它回答的是「全体现在
 * 处于哪一档」，部分开启时两边都不选中才是真实状态。
 *
 * 数据模型仍然完全不同：MOD 是 BepInEx 插件（身份是 GUID），地图是
 * `levels/` 下的一个目录（身份是 `levelSetName + levelSetNameZH + author`），所以没有把地图
 * 塞进 `InstalledMod`。
 *
 * ## 数据来源与刷新状态
 *
 * 地图列表与刷新状态都来自 `useMapSources` 的模块级单例，与地图下载页是
 * **同一份**：同一张地图不能在两页得出相反的版本结论，刷新态也必须熬得过
 * 一次切页（挂在页面上的 ref 会随组件卸载一起消失）。
 *
 * 进入页面只读一次 `api.levelSets()`，地图列表交给 `loadMaps()`。开关、删除、
 * 批量开关只影响本地关卡集，之后**只重读关卡集**；地图列表带 Release 缓存，
 * 与这些操作无关。「检测更新」走 `refreshBoundMaps`（并发 4、强制绕过缓存），
 * 是唯一重拉已装地图 Release 的入口；浮动刷新是唯一「两端都重读」的入口。
 */

const message = useNotify()
const dialog = useDialog()
const { config, game, mods, ensureLoaded } = useDashboard()
const { linkToMap } = useMapLink()
/**
 * 「缺依赖」的安装入口与 MOD 管理页同一个落点：跳到 MOD 安装页并高亮那张
 * 能提供这个依赖的仓库卡片。地图的前置（DIYLevel、Web Runtime）本身就是
 * 仓库里的 MOD，所以这条跳转对地图同样成立。
 */
const { linkToRepository } = useRepositoryLink()
/** 跨页跳转落点（从地图下载页点状态标签跳过来时带 `focus=<地图 id>`）。 */
const { highlightId } = useFocusHighlight()
/**
 * 页内定位：页头的聚合标签点一下就跳到命中的卡片并高亮。
 *
 * 与 MOD 管理页共用同一套 `useStatusHighlight`（高亮时长、滚动居中、
 * 展开组后等高度动画结束再滚），页面只负责算出「哪些卡片命中这个标签」。
 */
const { activeStatus, focusStatus, pulseTarget, pulseSeq } = useStatusHighlight()

/** 工具栏：两个按钮（打开、删除），列宽与间距与 MOD 管理页同源。 */
/** 工具栏：两个按钮（打开、删除），列宽与间距与 MOD 管理页同源。 */
const toolsMinWidth = computed(() => TOOL_BUTTON_WIDTH * 2 + TOOL_GAP)

const sets = ref<LevelSetInfo[]>([])
/**
 * 地图源声明的地图与刷新状态，全部来自模块级单例（`useMapSources`）。
 *
 * 地图列表不能在本页再存一份副本：版本对照（`versionRefs`）、依赖判定与
 * 卡片标签都读它，而地图下载页刷新 Release 后写的也是同一份数据 —— 两份副本
 * 会让同一张图在两页得出相反的结论，且切页回来读到的是自己那份旧快照。
 * 刷新态同理：`checkingBound` 记的是共享引擎的批次，页面局部 ref 熬不过切页。
 */
const {
  maps, refreshing, queued, refreshingSources, entryStateUnavailable, checkingBound, outcome,
  loadMaps, refreshBoundMaps,
} = useMapSources()
const loading = ref(true)
/**
 * 扫描进度（后端 `level-set-scan-progress` 逐份上报）。
 *
 * 只用来填转圈条下面的那两行；`null` 表示「还不知道总数」（刚发出请求、
 * 后端还没列完目录），这时只显示不确定动画，不编一个假百分比。
 */
const scanProgress = ref<LevelSetScanProgress | null>(null)
/** 每个集自己的开关请求状态（互不阻塞）。 */
const busy = ref<Record<string, boolean>>({})
/** 每个集自己的删除请求状态。 */
const deleting = ref<Record<string, boolean>>({})
/*
 * 卡片展开状态（规则在 `useCardExpansion` 里）：外层是关卡集卡片，内层是集详情
 * 里的随包插件。`singleOuter` 表示一次只展开一张集卡片，点另一张就换过去；
 * 集卡片收起时它名下的插件展开状态一起清掉，下次展开是全新的集。
 */
const levelExpansion = useCardExpansion({ singleOuter: true })

const enabledCount = computed(() => sets.value.filter((set) => set.enabled).length)
/** 游戏是否探测到 —— 没探测到时本地 MOD 列表必然为空，不能因此判「缺依赖」。 */
const gameProbed = computed(() => !!game.value?.path)
const kitchenRunning = computed(() => !!game.value?.kitchen_running)
/** 删除走回收站还是直接删（与设置页同一开关，文案与后端行为都跟它走）。 */
const recyclesOnDelete = computed(() => config.value?.recycle_deleted_mods !== false)

/**
 * 已排序：只按当前语言的展示名排。
 *
 * 开关状态不参与比较：批量启用 / 停用会同时翻转整列，若 `enabled` 参与排序，
 * 点一下整列次序就重排；单卡开关也会让那张卡当场跳到列表另一头。排序描述的是
 * 「浏览顺序」，状态是卡片上的事实 —— 混在一起会让用户刚点过的卡片换位置，
 * 看起来像点错了。
 */
const sortedSets = computed(() =>
  [...sets.value].sort((left, right) =>
    levelSetDisplayName(left, locale.value).localeCompare(
      levelSetDisplayName(right, locale.value),
      locale.value,
    ),
  ),
)

/**
 * 从地图源算出「本地关卡集 → 在线版本」的对照表。
 *
 * 只有刷新过 Release 的地图才有 tag —— 没刷新的地图不该让本地关卡集显示
 * 「最新版」，那是无根据的结论（见 `levelSetStatus.ts` 的 `mapVersionRefs`）。
 */
const versionRefs = computed(() => mapVersionRefs(maps.value))

/** 分组节点：一个折叠项 = 一个作者（地图的分组语义就是作者）。 */
interface LevelSetGroupNode {
  key: string
  label: string
  sets: LevelSetInfo[]
}

/**
 * 本地关卡集按作者分组。
 *
 * 组顺序与组内卡片用同一把尺子：当前语言的作者展示名。组的先后若跟着数据
 * 排列（关卡集的返回顺序）走，换语言、重新扫描或改卡片排序规则时组的位置
 * 会漂移，用户记住的位置就没了；作者名排序与卡片名排序同源，整页才是
 * 「按名字排」的一致观感。
 *
 * 分组口径全部由 `levelSetStatus.ts` 的 `levelSetGroup` 决定（线上声明 →
 * 本地 author → 未分组），页面只负责取展示名与折叠。
 */
const groups = computed<LevelSetGroupNode[]>(() => {
  const buckets = new Map<string, { label: string; sets: LevelSetInfo[] }>()
  for (const set of sortedSets.value) {
    const group = levelSetGroup(set, maps.value)
    let bucket = buckets.get(group.key)
    if (!bucket) {
      // 展示名按当前语言取；作者名没有多语言表时退回规范名本身，
      // 两者都空才是「未分组」（`levelSetGroup` 已经把这种情况归到同一个键）
      bucket = {
        label: pickI18n(group.labelI18n, group.canonical) || t('dialogs.ungrouped'),
        sets: [],
      }
      buckets.set(group.key, bucket)
    }
    bucket.sets.push(set)
  }
  return [...buckets.entries()]
    .map(([key, bucket]) => ({ key, label: bucket.label, sets: bucket.sets }))
    .sort((left, right) => left.label.localeCompare(right.label, locale.value))
})

const allGroupKeys = computed(() => groups.value.map((group) => group.key))
/** 默认全部展开：新出现的分组自动展开，用户的折叠选择单独记住。 */
const collapsedGroups = ref<string[]>([])
const groupsExpanded = computed(() =>
  allGroupKeys.value.filter((key) => !collapsedGroups.value.includes(key)),
)
function onGroupsExpanded(names: string[]) {
  const open = new Set(names)
  collapsedGroups.value = allGroupKeys.value.filter((key) => !open.has(key))
}

/**
 * 跨页跳转的落点如果落在收起的组里，先把那个组展开。
 *
 * `highlightId` 来自地图下载页的 `focus=<地图 id>`；折叠组的卡片根本不会
 * 渲染，`useFocusHighlight` 靠 `data-focus-id` 找元素就一直落空（它最多重试
 * 3 秒），高亮与滚动会静默失效。监听 `groups` 而不是只在挂载时判断一次：
 * 路由参数比 `sets` / `maps` 先到是常态，等数据到了分组才有内容可查。
 */
function expandGroupForHighlight() {
  const id = highlightId.value
  if (!id) return
  const hit = sets.value.find((set) => focusIdOf(set) === id)
  if (!hit) return
  const key = levelSetGroup(hit, maps.value).key
  if (!collapsedGroups.value.includes(key)) return
  collapsedGroups.value = collapsedGroups.value.filter((value) => value !== key)
}
watch([highlightId, groups], expandGroupForHighlight, { immediate: true })

/**
 * 在线 MOD 仓库，只留「在当前地区真实存在」的那些。
 *
 * 依赖表的名称与判定都要用它：地图的依赖是框架 MOD（DIYLevel / Web Runtime），
 * 它们本身也是在线 MOD 仓库，作者在源文档里给它们起了名字。缺了这份列表，
 * 名称会退化成 GUID、状态也会退化成「缺依赖」。
 *
 * 过滤口径与地图下载页、MOD 安装页完全一致 —— `repositoryIsAvailable` 是
 * 地区 / 平台 / `enabled` 的唯一前端入口，页面不再另写一遍条件。
 */
const repositories = computed<Repository[]>(() =>
  (config.value?.repositories ?? []).filter((repo) =>
    repositoryIsAvailable(
      repo,
      config.value?.repository_sources,
      config.value?.region,
      config.value?.platform,
    )),
)

/**
 * 首次进入：关卡集与地图列表一起读。
 *
 * 两次读取互不依赖，所以不串行。地图列表交给 `loadMaps()`：它把结果写进
 * 单例、失败也自己提示，而且整源刷新进行中会**主动跳过** —— 切页回来时
 * 不会用旧缓存把正在重建的列表盖掉。地图列表失败不该让本地关卡集消失，
 * 那是最基础的信息，用户至少要看得到自己装了哪些图。
 */
async function load() {
  loading.value = sets.value.length === 0
  // 每次重新扫描都清掉上一次的进度，避免转圈条下面挂着过期的「5/7」
  scanProgress.value = null
  try {
    /*
     * 两读并发，但**都要等**：地图列表没回来时 `boundMapOf` 找不到绑定，
     * 卡片会先显示一帧「未绑定」再跳成「有更新 / 最新版」—— 用户看到的是状态闪变。
     * `loadMaps` 内部自己吞掉失败（只提示、不 reject），所以这里不会连累关卡集。
     */
    const levelSets = await Promise.all([api.levelSets(), loadMaps()]).then(([list]) => list)
    sets.value = levelSets
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    loading.value = false
  }
}

/**
 * 只重读关卡集。
 *
 * 开关 / 删除 / 导入改的都是 `levels/` 与 `BepInEx/cache/modmanager_disabled_maps/` 下的目录，
 * 地图源列表（`useMapSources` 的 `maps`）不会因此变化，重拉纯属浪费。
 */
async function reloadSets() {
  try {
    scanProgress.value = null
    sets.value = await api.levelSets()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

/**
 * 「安装地图」来源卡片：本地 ZIP / 整窗拖入 / 在线直链。
 *
 * 管理页也有本地安装入口（与 MOD 管理页一致）：用户手上已经是一个包时，不该
 * 先切到下载页。暂存 → 确认 → 导入由 `useLevelSetImport` 承担，与地图下载页
 * 共用同一份，本页只负责装完重读关卡集。
 */
const installSourceOpen = ref(false)
const {
  importing, importSource,
  previewOpen: importPreviewOpen, detail: importDetail,
  confirmImport, discardImport,
  importCheck,
  zipProgress, zipProgressText,
} = useLevelSetImport(() => reloadSets())

/**
 * 整窗进度遮罩：来源卡片没打开时才用。
 *
 * 卡片打开时（本地拖入 / 直链）进度显示在卡片里，不叠两个转圈；卡片刻意不接
 * 这个遮罩的判据 —— 在线 Release 安装与确认后的写盘阶段没有卡片可显示，
 * 那时遮罩是唯一能看到「到第几个了」的地方。
 */
const showZipOverlay = computed(
  () => importing.value && !!zipProgress.value && !installSourceOpen.value,
)

async function handleInstallSourceSelect(source: InstallSource) {
  await importSource(source, {
    // 来源已经可用：先收起卡片，再交给确认弹窗
    onStaged: () => { installSourceOpen.value = false },
  })
}

/** 拖入 / 跨页跳转过来的文件：确认属于地图域之后，走与来源卡片同一条链路。 */
async function startDropInstall(path: string) {
  installSourceOpen.value = true
  await handleInstallSourceSelect({ kind: 'local', value: path })
}

const { acceptDrop } = useDropRouting()

/**
 * 整窗拖放：拖一个 ZIP 进来等价于点「安装地图」。
 *
 * 拖放以前挂在来源卡片里，于是「能不能拖」取决于「这页碰巧挂没挂卡片」——
 * 本页当时连安装入口都没有，拖进来毫无反应。现在拖放是窗口能力
 * （`useFileDrop`），四个 MOD / 地图页面各自声明一次。
 *
 * 先过 `acceptDrop`：纯 DLL 的包不是地图包，要引导到 MOD 安装页，而不是
 * 在这里报「ZIP 里没有 info」。
 */
const { dragActive } = useFileDrop({
  accept: ['zip', 'dll'],
  busy: () => importing.value,
  onDrop: (path) => {
    void (async () => {
      if (await acceptDrop(path, 'map')) await startDropInstall(path)
    })()
  },
})

/** 扫描进度事件的解绑函数（卸载时调用）。 */
let unlistenScan: UnlistenFn | null = null

onMounted(() => {
  /*
   * 点击卡片行之外收起详情：`LocalItemCard` 只在卡片行 emit `activate`，
   * 详情区的点击被它排除在外，「点详情收起」只能由页面在文档级实现
   * （与 MOD 管理页同一套做法）。
   */
  document.addEventListener('click', onDocumentClick)
  /*
   * 依赖缺失判定与「删除是否进回收站」都要读 Dashboard（mods / config），
   * 而 Dashboard 由页面自己按需加载。直接进本页时它可能还是空的：那时
   * 「依赖缺失」标签会消失、删除确认会退回默认文案 —— 都是无声的错误结论。
   * 已有数据时 `ensureLoaded` 直接返回，不会重复扫描。
   */
  void ensureLoaded()
  /*
   * 扫描进度：本页只要在扫就会收到，解绑放在卸载里 —— 事件监听器不随页面
   * 卸载自动消失，漏解绑会随每次进出累积（与文档级 click 同理）。
   */
  void watchLevelSetScan((progress) => {
    scanProgress.value = progress
  }).then((unlisten) => {
    unlistenScan = unlisten
  })
  void load()
  // 从 MOD 页「去安装」跳过来：路径已暂存在模块里，直接接着走导入
  const pending = takePendingDrop('map')
  if (pending) void startDropInstall(pending)
})

/** 文档级监听挂在页面之外，页面卸载必须解绑，否则会随每次进出累积。 */
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClick)
  unlistenScan?.()
  unlistenScan = null
})

/** 本地关卡集绑定的地图源条目（身份匹配规则与「有更新」标签同源）。 */
function boundMapOf(set: LevelSetInfo): MapInfo | undefined {
  return mapForLevelSet(set, maps.value)
}

/** 跨页定位锚点：绑定的地图 id（未绑定时没有锚点）。 */
function focusIdOf(set: LevelSetInfo): string | undefined {
  return boundMapOf(set)?.id
}

/**
 * 绑定地图的展示信息（详情行用）。
 *
 * 名称按当前语言取；tag 可能还没刷新到 Release（空串表示待检查）。
 */
function bindingOf(set: LevelSetInfo) {
  const map = boundMapOf(set)
  if (!map) return undefined
  return {
    map,
    name: pickI18n(map.name_i18n, map.name) || map.id,
    tag: map.latest_tag ?? '',
  }
}

function stateOf(set: LevelSetInfo) {
  return levelSetState(set, mapVersionForLevelSet(set, versionRefs.value))
}

/**
 * 作者的中英展示名（详情里两行）。
 *
 * 取值全部交给 `levelSetAuthorNames`：线上声明的作者译名优先、本地 info 的
 * `author` 兜底 —— 与分组标题（`levelSetGroup`）同一条回退链，页面不再自己
 * 判断该读哪一级。
 */
function authorNamesOf(set: LevelSetInfo) {
  return levelSetAuthorNames(set, maps.value)
}

/**
 * 正在重抓 Release 的地图 id 与正在重拉文档的地图源 id。
 *
 * 刷新期间版本结论标签要换成「刷新中」（`levelSetCardTags` 的第 6 个参数）：
 * 刷新完成前「有更新 / 最新版」都是马上会被推翻的结论，用户点完「检查更新」
 * 却还看到刷新前的标签会以为按钮没生效。
 *
 * `entryStateUnavailable` 是列表清空/重建（下载页「刷新全部」的源树阶段）
 * 那一档：它会把共享的 `maps` 清空再重建，这段时间**所有**关卡集都找不到
 * 绑定，只传 id 集合的话卡片会集体退化成「未绑定」——那比过期结论更误导。
 * 列表填回来之后就必须关掉它，否则整源标志会把每张卡按住，逐个返回的结果
 * 被压成整批一起返回。
 *
 * 每次重算都新建两个 Set：一次刷新最多几十个 id，且只在 `refreshing` /
 * `refreshingSources` / `entryStateUnavailable` 变化（刷新的开始与结束）时重算，成本可忽略。
 */
const refreshingHint = computed<LevelSetRefreshingHint>(() => ({
  entryStateUnavailable: entryStateUnavailable.value,
  maps: new Set(Object.keys(refreshing.value)),
  queued: new Set(Object.keys(queued.value)),
  sources: new Set(Object.keys(refreshingSources.value)),
}))

/** 卡片状态标签：先依赖提醒，再版本结论（由 `levelSetStatus` 统一排序）。 */
function tagsOf(set: LevelSetInfo): LevelSetCardTag[] {
  return levelSetCardTags(
    set, maps.value, mods.value, gameProbed.value, repositories.value, refreshingHint.value,
  )
}

/** 页头聚合标签的一项。 */
interface LevelStatusSummary {
  key: string
  text: string
  tone: LevelSetTone
  count: number
}

/**
 * 标签 key 带上看 `kind`。
 *
 * 同一段文案在「依赖提醒」与「版本结论」里说的是两件事，共用一个 key 会让
 * 页头定位把不相干的卡片也算命中。
 */
function tagKeyOf(tag: LevelSetCardTag) {
  return `tag:${tag.kind}:${tag.text}`
}

/** 开关状态的两个合成 key（页头单独成组，与结论类标签分开）。 */
const TOGGLE_KEYS = ['enabled', 'disabled'] as const

/** 一张卡是否命中某个聚合标签（含开关状态的两个合成 key）。 */
function matchesStatus(set: LevelSetInfo, key: string) {
  if (key === 'enabled') return set.enabled
  if (key === 'disabled') return !set.enabled
  return tagsOf(set).some((tag) => tagKeyOf(tag) === key)
}

/**
 * 页头「结论」组：卡片上真实存在的标签汇总（依赖提醒 + 版本结论）。
 *
 * 只汇总卡片上会出现的标签：页头每个标签都必须点得出落点，凑出来的分类
 * 点下去没有任何卡片会亮，比不放更糟。
 *
 * 停用状态的卡片自己会显示「已禁用」，但它回答的是「开没开」而不是「版本
 * 怎么样」，所以这里把它剔出去，交给下面的开关状态组 —— 否则同一件事会在
 * 页头出现两次（「1 已禁用」并排两条）。
 */
const statusSummary = computed<LevelStatusSummary[]>(() => {
  const buckets = new Map<string, LevelStatusSummary>()
  const add = (key: string, text: string, tone: LevelSetTone) => {
    const existing = buckets.get(key)
    if (existing) existing.count += 1
    else buckets.set(key, { key, text, tone, count: 1 })
  }
  for (const set of sortedSets.value) {
    for (const tag of tagsOf(set)) {
      if (tag.kind === 'state' && tag.text === 'modsLocal.disabled') continue
      add(tagKeyOf(tag), tag.text, tag.tone)
    }
  }
  /*
   * 顺序：关系提醒 → 版本结论。同类里按数量降序，最常点的一类不用往右找。
   * 判定顺序与卡片标签的排序同源（`levelSetCardTags` 也是先依赖后版本），
   * 页头与卡片的阅读顺序因此一致。
   */
  const rank = (entry: LevelStatusSummary) =>
    entry.key.startsWith('tag:dependency:') ? 0 : 1
  return [...buckets.values()].sort(
    (left, right) => rank(left) - rank(right) || right.count - left.count,
  )
})

/** 页头「开关状态」组：固定「已启用 → 已禁用」，数量为 0 的不显示。 */
const toggleSummary = computed<LevelStatusSummary[]>(() => {
  const entries: LevelStatusSummary[] = []
  for (const key of TOGGLE_KEYS) {
    const count = sortedSets.value.filter((set) => matchesStatus(set, key)).length
    if (!count) continue
    entries.push({
      key,
      text: key === 'enabled' ? 'modsLocal.enabled2' : 'modsLocal.disabled',
      tone: key === 'enabled' ? 'accent' : 'muted',
      count,
    })
  }
  return entries
})

/** 关卡集 → 所属折叠组：定位落在收起的组里得先展开，否则卡片根本没渲染。 */
const groupKeyOfSet = computed(() => {
  const index = new Map<string, string>()
  for (const group of groups.value) {
    for (const set of group.sets) index.set(set.dir_name, group.key)
  }
  return index
})

/** 同一个聚合标签连点：在命中的卡片之间循环，而不是一直停在第一张。 */
const statusCursor = ref<{ key: string; index: number }>({ key: '', index: 0 })

function locateStatus(key: string) {
  const matches = sortedSets.value.filter((set) => matchesStatus(set, key))
  if (!matches.length) return
  const repeat = statusCursor.value.key === key
  const start = repeat ? (statusCursor.value.index + 1) % matches.length : 0
  statusCursor.value = { key, index: start }
  const target = matches[start]
  const groupKey = groupKeyOfSet.value.get(target.dir_name)
  const needsExpand = !!groupKey && collapsedGroups.value.includes(groupKey)
  if (groupKey && needsExpand) {
    collapsedGroups.value = collapsedGroups.value.filter((value) => value !== groupKey)
  }
  focusStatus(
    key,
    () => document.querySelector<HTMLElement>(`[data-level-dir="${CSS.escape(target.dir_name)}"]`),
    needsExpand,
    target.dir_name,
  )
}

/** 卡片高亮条件：页头正在定位的这个聚合标签是否命中它（可能同时命中多张）。 */
function matchesActiveStatus(set: LevelSetInfo) {
  const key = activeStatus.value
  return !!key && matchesStatus(set, key)
}

/**
 * 「缺依赖」标签的安装去处：能提供满足区间版本的仓库卡片 id。
 *
 * 与 MOD 管理页的「去安装」是同一条规则（`dependencyTagFor` 给的目标），
 * 没有可安装来源时返回 `undefined` —— 此时既不出按钮，标签也不可点。
 */
function dependencyInstallTarget(set: LevelSetInfo): string | undefined {
  return tagsOf(set).find((tag) => tag.kind === 'dependency' && tag.repositoryId)?.repositoryId
}

/** 点「去安装」：跳到 MOD 安装页并高亮那张能补上依赖的仓库卡片。 */
function installDependency(set: LevelSetInfo) {
  const target = dependencyInstallTarget(set)
  if (target) linkToRepository(target)
}

/**
 * 标签悬停提示：标签本身很短，把「为什么」放在 title 里。
 *
 * - 缺依赖：列出**这条声明链**里没满足的 GUID 与版本要求（绑定的在线地图优先，
 *   没有就退回本地快照 —— 与依赖表、标签本身同一条来源）；
 * - 有更新（可点）：说明点下去会去哪儿，避免用户以为是就地更新。
 */
function tagTitle(set: LevelSetInfo, tag: LevelSetCardTag): string | undefined {
  if (tag.kind === 'dependency') {
    const list = levelSetRelationStatuses(set, maps.value, mods.value, repositories.value)
      .filter((status) => status.state !== 'ok' && (status.relation.kind ?? 'requires') === 'requires')
      .map((status) => {
        const min = status.relation.min_version?.trim()
        return `${status.relation.guid}${min ? ` ≥ ${min}` : ''}`
      })
      .join(' · ')
    return t('levelSet.frameworksNotMet', { list: list || t('modsOnline.missingDep') })
  }
  if (tag.mapId) return t('levelSet.viewInMapDownload')
  return undefined
}

function onTagClick(tag: LevelSetCardTag) {
  // 缺依赖：跳到**依赖的**仓库（与 MOD 管理页同一个落点）；没有来源时
  // 标签不带 repositoryId，也就不会走到这里。
  if (tag.repositoryId) linkToRepository(tag.repositoryId)
  else if (tag.mapId) linkToMap(tag.mapId)
}

/**
 * 状态说明：为什么是「未绑定」、为什么能更新。没有额外信息时返回空串。
 */
function hintOf(set: LevelSetInfo): string {
  const state = stateOf(set)
  if (state === 'unbound') return t('levelSet.unboundHint')
  if (state === 'update_available') {
    const tag = boundMapOf(set)?.latest_tag
    return tag ? t('levelSet.updateAvailableHint', { version: tag }) : ''
  }
  return ''
}

/**
 * 依赖表行。
 *
 * 名称、状态文案与色调全部由 `dependencyRows.ts` 统一构造（四个页面共用
 * 一份口径）：名称按「在线 MOD 仓库卡片名 → 本地已装 MOD 的包标题 → GUID」
 * 解析，满足时只写「已满足」——版本写在「要求」列，状态列只回答「现在
 * 行不行」。声明来源也由它决定：绑定的地图还在配置里就用它的在线声明
 * （`relations`），地图源下线后才退回安装清单里的快照。
 *
 * 未满足时与 MOD 侧同一套结论：线上有满足区间的版本写「可安装 / 可更新」
 * 并带跳转，没有来源才写「缺依赖」。
 */
function dependencyRows(set: LevelSetInfo): DependencyRow[] {
  return levelSetDependencyRows(set, {
    maps: maps.value,
    mods: mods.value,
    repositories: repositories.value,
    translate: t,
    pickName: pickI18n,
  })
}

/** 依赖表里可跳的行：跳到能提供它的仓库卡片（与 MOD 管理页同一个落点）。 */
function openDependencyJump(row: DependencyRow) {
  if (row.jump) linkToRepository(row.jump.id)
}

/**
 * 卡片行点击：展开自己，或收起已展开的那张。
 *
 * 详情区不阻止冒泡（点详情里的文字/值等于「点空白」，由 `onDocumentClick`
 * 收起），所以必须让文档监听知道这一次点击已经处理过：否则同一次点击里
 * 这里先把该项 toggle 开，文档监听紧接着把它置回 `null`，内容只出现一帧
 * ——折叠过渡动画还没播完就被打断，看起来像「点了没反应」。
 *
 * 展开 / 收起的滚动策略见 `expandScroll.ts`：这一张是用户点的，滚；
 * 因展开 B 而被动收起的 A 不滚（单展开语义下 A 只是让位）。
 */
let cardClickHandled = false

/** 卡片元素锚点：批量/展开状态都由 ref 驱动，找 DOM 只能靠页面自己打的 `data-level-dir`。 */
function cardOf(dirName: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`[data-level-dir="${CSS.escape(dirName)}"]`)
}

function activate(set: LevelSetInfo, card?: HTMLElement) {
  cardClickHandled = true
  const previous = levelExpansion.outerKeys.value[0] ?? null
  if (levelExpansion.isOuterOpen(set.dir_name)) {
    // 收起：量位置、改状态都交给 collapseCardWithScroll —— 它必须在折叠前量，
    // 再把展开那次滚动倒回去，折叠与回滚同时发生，看起来才是「原路收回去」。
    const target = card ?? cardOf(set.dir_name)
    const collapse = () => levelExpansion.closeOuter(set.dir_name)
    if (!target) {
      collapse()
      return
    }
    collapseCardWithScroll({ card: target, body: target.querySelector('.card-detail') }, collapse)
    return
  }
  // 单展开语义：展开 B 会让 A 收起。A 在 B 上面时，收起动画会把 B 整体往上带，
  // 量 B 的 top 时必须减掉 A 会缩掉的高度，否则会多滚一截。
  const previousCard = previous ? cardOf(previous) : null
  const previousBody = previousCard?.querySelector('.card-detail') ?? null
  /*
   * A 名下展开过的随包插件会被一起清掉（`useCardExpansion` 的规则），A 的详情
   * 内容因此在改状态的同一步就变矮 —— 收掉的高度必须在改状态之前量，之后量会
   * 低估，滚动就冲过头。
   */
  const shrink = previousCard && previousBody ? naturalContentHeight(previousBody) : 0
  levelExpansion.openOuter(set.dir_name)
  if (!card) return
  const anchor = card.querySelector('.card-row')
  if (!anchor) return
  void nextTick().then(() => {
    scrollExpandedCardIntoView({
      card,
      anchor,
      body: card.querySelector('.card-detail'),
      collapsing: previousCard
        ? { card: previousCard, body: previousBody, shrink }
        : null,
    })
  })
}

/*
 * 随包插件的展开状态在 `levelExpansion` 的内层里（键由 `useCardExpansion` 拼）。
 * 与集卡片不同：集是单展开（同时只看一张），插件是「查看某个 DLL 的信息」，
 * 用户可能想对比两个，所以按 `集目录 + 文件名` 记多份。
 */

/**
 * 随包插件卡片的展开 / 收起。
 *
 * 二级卡片与集卡片走同一套滚动策略（`expandScroll.ts`）：展开时先让整卡可见、
 * 放不下才把卡片外框钉到页头下沿，收起时把折叠后的卡片带到可视区正中。
 * 不滚的话，插件卡片展开后底部会被视口切掉，用户得自己往下找。
 *
 * 只滚「用户点的这一张」：别的插件卡片被被动影响时没有调用点。
 */
function togglePlugin(set: LevelSetInfo, plugin: LevelSetPlugin, card?: HTMLElement) {
  // 这次点击已经由插件卡片处理；不设标记的话，document 上「点空白收起」
  // 会顺手把外层的集卡片一起收掉，用户就看不到刚展开的插件详情了。
  cardClickHandled = true
  const close = () => levelExpansion.closeInner(set.dir_name, plugin.file_name)
  if (levelExpansion.isInnerOpen(set.dir_name, plugin.file_name)) {
    if (!card) {
      close()
      return
    }
    collapseCardWithScroll({ card, body: card.querySelector('.card-detail') }, close)
    return
  }
  levelExpansion.openInner(set.dir_name, plugin.file_name)
  if (!card) return
  const anchor = card.querySelector('.card-row')
  if (!anchor) return
  void nextTick().then(() => {
    scrollExpandedCardIntoView({ card, anchor, body: card.querySelector('.card-detail') })
  })
}

/**
 * 点击落点所在的**二级卡片**（集详情里的随包插件卡）。
 *
 * 一级卡与二级卡渲染出来都是 `.mod-card`，靠「是否长在某个 `.card-detail` 里」
 * 区分：随包插件卡片只可能出现在集详情内部，一级卡自己不在任何详情区里。
 */
function nestedPluginCard(target: Element): HTMLElement | null {
  const card = target.closest('.mod-card')
  if (!(card instanceof HTMLElement)) return null
  return card.closest('.card-detail') ? card : null
}

/**
 * 收起某张随包插件卡片。
 *
 * 卡片只带 `data-plugin-of` / `data-plugin-file` 两个原始字段（内层键里的 NUL
 * 写不进 HTML 属性），拼键规则仍只有 `useCardExpansion` 一处实现。
 */
function collapsePluginCard(card: HTMLElement) {
  const dirName = card.dataset.pluginOf
  const fileName = card.dataset.pluginFile
  if (!dirName || !fileName) return
  if (!levelExpansion.isInnerOpen(dirName, fileName)) return
  collapseCardWithScroll({ card, body: card.querySelector('.card-detail') }, () => {
    levelExpansion.closeInner(dirName, fileName)
  })
}

/** 插件卡片标题：BepInPlugin 名优先，读不到就用文件名去掉 `.dll` / `.dll.disabled`。 */
function pluginTitle(plugin: LevelSetPlugin) {
  return plugin.name?.trim() || plugin.file_name.replace(/\.dll(\.disabled)?$/i, '')
}

/**
 * 点击卡片行之外的地方收起已展开的详情。
 *
 * 「空白」包含详情区自身：展开后点详情里的任意文字、标签或值都算点空白，
 * 应当收起（详情内容是只读的，没有需要保留点击的元素）。判定规则是「点击
 * 目标是否属于这一行的顶栏」——落在 `.card-row`（开关 / 标题 / 状态标签 /
 * 工具按钮）里说明用户在操作这一行，交给各自的 `@click.stop` 与处理函数，
 * 不在这里收起，避免一次点击既展开又收起。
 *
 * 二级卡片（集详情里的随包插件）要先认领：它的详情区同时落在集卡片的
 * `.card-detail` 里，不先判它就会被当成「点空白」，把外层集卡片一起收掉
 * （用户看到的就是「点插件详情，地图包整个收起来了」）。
 *
 * 收起后把展开那次滚动倒回去（单展开语义下它是唯一展开过的那张卡）。
 */
function onDocumentClick(event: MouseEvent) {
  if (cardClickHandled) {
    cardClickHandled = false
    return
  }
  const target = event.target
  const nested = target instanceof Element ? nestedPluginCard(target) : null
  if (nested) {
    collapsePluginCard(nested)
    return
  }
  const dirName = levelExpansion.outerKeys.value[0] ?? null
  if (dirName === null) return
  if (target instanceof Element && target.closest('.card-row')) return
  const card = cardOf(dirName)
  const collapse = () => levelExpansion.closeOuter(dirName)
  if (!card) {
    collapse()
    return
  }
  collapseCardWithScroll({ card, body: card.querySelector('.card-detail') }, collapse)
}

/**
 * 切换开关。
 *
 * 后端在游戏运行时会返回 `levelSet.gameRunning` —— 移动 `levels/` 里的目录
 * 可能与 DIYLevel 正在做的枚举冲突。前端不自己判断游戏状态，直接按后端
 * 结果展示，避免两处口径不一致。
 */
async function toggle(set: LevelSetInfo, enabled: boolean) {
  if (busy.value[set.dir_name]) return
  busy.value = { ...busy.value, [set.dir_name]: true }
  try {
    const result = await api.setLevelSetEnabled(set.dir_name, enabled)
    if (!result.ok) {
      message.error(translateBackendMessage(result.message))
      return
    }
    message.success(translateBackendMessage(result.message))
    await reloadSets()
  } catch (reason) {
    // 开关失败时列表保持原状：下一次重读会拿到真实磁盘状态
    message.error(backendMessageOf(reason))
  } finally {
    const next = { ...busy.value }
    delete next[set.dir_name]
    busy.value = next
  }
}

/** 在资源管理器里打开这一集的真实目录（只读，游戏运行时也允许）。 */
async function openDirectory(set: LevelSetInfo) {
  try {
    const result = await api.openLevelSetDirectory(set.dir_name, set.enabled)
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

/**
 * 删除一集：确认框文案跟「删除是否进回收站」走。
 *
 * 与 MOD 管理页同一套 key：用户看到的后果（可恢复 / 不可恢复）必须一致，
 * 不能在地图页凭空换一套说法。
 */
function remove(set: LevelSetInfo) {
  if (deleting.value[set.dir_name] || kitchenRunning.value) return
  const name = levelSetDisplayName(set, locale.value)
  const recycle = recyclesOnDelete.value
  const instance = dialog.warning({
    title: t('levelSet.deleteTitle'),
    content: recycle
      ? t('modsLocal.willBeMovedToTheRecycleBinAndCanBeRestoredContinue', { name })
      : t('modsLocal.willBeDeletedPermanentlyThisCannotBeUndoneContinue', { name }),
    positiveText: recycle ? t('modsLocal.moveToRecycleBin') : t('modsLocal.deletePermanently'),
    negativeText: t('recycle.cancel'),
    onPositiveClick: async () => {
      markDialogBusy(instance)
      deleting.value = { ...deleting.value, [set.dir_name]: true }
      try {
        const result = await api.deleteLevelSet(set.dir_name, set.enabled)
        if (!result.ok) {
          message.error(t('levelSet.deleteFailed', { reason: translateBackendMessage(result.message) }))
          return
        }
        message.success(translateBackendMessage(result.message))
        levelExpansion.closeOuter(set.dir_name)
        await reloadSets()
      } catch (reason) {
        message.error(t('levelSet.deleteFailed', { reason: backendMessageOf(reason) }))
      } finally {
        const next = { ...deleting.value }
        delete next[set.dir_name]
        deleting.value = next
      }
    },
  })
}

/** 打开 DIYLevel 的地图目录（页头按钮，不针对某一集）。 */
async function openLevelsDirectory() {
  try {
    const result = await api.openDiyLevelDirectory()
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

/* ===== 批量开关 / 检测更新（与 MOD 管理页同职责） =====
 *
 * 安装入口只收本地 ZIP（拖进窗口，见页面顶部注释）；线上 Release 在地图下载页。
 * 这里只关心「已经在本地的地图」——启用/禁用、看有没有新版本、打开目录、删除。
 */
const bulkWorking = ref(false)
/**
 * 分段控件的选中值：全部启用 → `enable`，全部停用 → `disable`，
 * 部分启用 → 空串（没有选中项）。
 *
 * 由聚合状态推导，而不是另存「用户最后点了哪个」：控件表达的是列表当前
 * 状态，存一份点击意图会在操作失败、确认框取消或外部重读之后和真实状态脱节。
 *
 * 混合态用空串哨兵而不是 `null` / `undefined`：Naive 的 `value` 只收
 * `string | number`，传 `undefined` 会让它退回**非受控**模式 —— 点一下
 * 就自己记住选中项，取消确认框后控件仍停在「全部停用」上，正好是要避免的
 * 那种假状态。空串则始终是受控的「没有匹配项」。
 */
type BulkSwitchValue = 'enable' | 'disable' | ''
const bulkSwitchValue = computed<BulkSwitchValue>(() => {
  if (!sets.value.length) return ''
  if (enabledCount.value === sets.value.length) return 'enable'
  if (!enabledCount.value) return 'disable'
  return ''
})

/**
 * 批量切换全部关卡集。
 *
 * 走后端一次操作锁：前端逐条切换会让每个目录都重新探测一次游戏目录，
 * 地图多时纯属浪费；后端还会在中途失败时回滚，不会留下半开半关的列表。
 */
async function setAllEnabled(enabled: boolean) {
  if (bulkWorking.value || !sets.value.length) return
  bulkWorking.value = true
  try {
    const result = await api.setAllLevelSetsEnabled(enabled)
    if (result.ok) {
      message.success(translateBackendMessage(result.message))
      await reloadSets()
    } else {
      message.error(translateBackendMessage(result.message))
    }
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    bulkWorking.value = false
  }
}

/** 全部停用要确认：停用后游戏内不会加载任何自定义地图（可随时恢复）。 */
function confirmDisableAll() {
  dialog.warning({
    title: t('levelSet.disableAllTitle'),
    content: t('levelSet.disableAllConfirm', { count: enabledCount.value }),
    positiveText: t('overview.disableAll'),
    negativeText: t('recycle.cancel'),
    onPositiveClick() {
      void setAllEnabled(false)
    },
  })
}

/**
 * 分段控件点击：只发起动作，不先改选中态。
 *
 * 停用方向要过确认框：用户点「取消」时状态没变，控件也不能先跳到
 * 「全部停用」，否则会让人以为已经生效。
 */
function onBulkSwitch(value: string | number) {
  if (value === 'enable') void setAllEnabled(true)
  else if (value === 'disable') confirmDisableAll()
}

/**
 * 检查更新：只刷新「本地已装地图绑定的地图」的 Release，与 MOD 管理页同一规则。
 *
 * 走共享引擎的 `refreshBoundMaps`（并发 4 组、强制绕过缓存），不在本页自己
 * 循环调后端命令：
 *   · 刷新状态记在 `useMapSources` 的单例里，地图下载页的按钮与卡片会同时
 *     进入刷新态，切页往返也不丢；
 *   · 引擎收尾时自己同步依赖快照并重读地图列表，`versionRefs` 随之重算，
 *     卡片标签（`stateOf` / `tagsOf`）当场变成新结论，页面不用手动再读一次。
 */
async function checkUpdates() {
  // 同一时刻只跑一批：按钮的禁用态与共享单例同步，这里只是防住同一帧内的连点
  if (checkingBound.value) return
  const targets = new Map<string, MapInfo>()
  for (const set of sets.value) {
    const map = boundMapOf(set)
    if (map) targets.set(map.id, map)
  }
  if (!targets.size) {
    message.warning(t('levelSet.noBoundMaps'))
    return
  }
  try {
    /*
     * `countUpdated` 由页面给：引擎只认识「地图源里的条目」，要判断「我这几张
     * 本地地图里几个有新版本」必须读关卡集，那是本页的数据。
     */
    const summary = await refreshBoundMaps([...targets.values()], {
      countUpdated: () => sets.value.filter((set) => stateOf(set) === 'update_available').length,
    })
    // 逐条弹错：一次刷几十张图时合并成一句会把「哪张失败了」吞掉
    summary.errors.forEach((error) => message.error(error))
    /*
     * 结果文案与页头那枚「上次检查」标签共用 `describeRefreshOutcome`。
     * 只有真的刷到了地图才报数量：这些图可能刚好都在刷新中（例如下载页
     * 正在刷同一批），此时队列为空、既没成功也没失败，
     * 弹一句「已检查 0 张地图」只会让人以为哪里坏了。
     */
    if (!summary.errors.length && summary.refreshed > 0 && outcome.value) {
      message.success(describeRefreshOutcome(outcome.value, 'maps'))
    }
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

/**
 * 折叠槽里的卡片由 `LevelSetGroupBody` 渲染（原因见 `levelSetGroupContext.ts`）。
 * 这里把组数据与页面操作一次性交出去；`reactive()` 负责解包 ref/computed，
 * 组件模板里读 `ctx.groups` / `ctx.busy[...]` 即等价于读本页状态。
 */
provide(
  LEVEL_SET_GROUP_CTX,
  reactive<LevelSetGroupContext>({
    groups,
    busy,
    deleting,
    expansion: levelExpansion,
    highlightId,
    pulseTarget,
    pulseSeq,
    toolsMinWidth,
    kitchenRunning,
    toggle,
    activate,
    tagsOf,
    tagTitle,
    onTagClick,
    authorNamesOf,
    bindingOf,
    hintOf,
    focusIdOf,
    matchesActiveStatus,
    dependencyInstallTarget,
    installDependency,
    removeSet: remove,
    openDir: openDirectory,
    pluginTitle,
    togglePlugin,
    dependencyRows,
    openDependencyJump,
    linkToMap,
  }),
)
</script>

<template>
  <!-- 页头标题右侧：状态聚合，与 MOD 管理页同一套标签外观 -->
  <!--
    页头状态区与 MOD 管理页同一套交互（点一下定位并高亮，再点同一标签在命中
    卡片之间循环），同一套分组：左「结论」（缺依赖 / 可更新 / 最新版…），
    右「开关状态」（N 已启用 / N 已禁用），中间一条竖线断开。
  -->
  <Teleport to="#page-header-title-extra" defer>
    <span class="header-tag-group">
      <n-tag v-for="entry in statusSummary" :key="entry.key" size="small" :bordered="false"
        class="state-tag state-tag--link" :color="tagColorFor(entry.tone)" role="button" tabindex="0"
        :title="t('modsOnline.locateOnThisPage')" @click="locateStatus(entry.key)"
        @keydown.enter.prevent="locateStatus(entry.key)"
        @keydown.space.prevent="locateStatus(entry.key)">
        {{ entry.count }} {{ t(entry.text) }}
      </n-tag>
    </span>
    <span v-if="statusSummary.length && toggleSummary.length" class="header-tag-divider" />
    <span class="header-tag-group">
      <n-tag v-for="entry in toggleSummary" :key="entry.key" size="small" :bordered="false"
        class="state-tag state-tag--link" :color="tagColorFor(entry.tone)" role="button" tabindex="0"
        :title="t('modsOnline.locateOnThisPage')" @click="locateStatus(entry.key)"
        @keydown.enter.prevent="locateStatus(entry.key)"
        @keydown.space.prevent="locateStatus(entry.key)">
        {{ entry.count }} {{ t(entry.text) }}
      </n-tag>
    </span>
    <!-- 「上次检查」摘要：与 MOD 管理页同一枚组件、同一套文案 -->
    <template v-if="outcome">
      <span class="header-tag-divider" />
      <span class="header-tag-group">
        <RefreshResultTag :outcome="outcome" domain="maps" />
      </span>
    </template>
  </Teleport>

  <!--
    页头右侧，从左到右：批量开关 → 检测更新 → 打开目录（与 MOD 管理页同序）。
    安装入口在本页只走拖放：线上 Release 与「选文件 / 直链」都在地图下载页。
    「检测更新」的忙碌态读共享单例的 `checkingBound`，而不是页面局部的 ref：
    地图下载页也有一份刷新入口，同一批任务必须只有一个忙碌真相。
  -->
  <Teleport to="#page-header-actions" defer>
    <!--
      批量开关：一个二选一分段控件（与 MOD 管理页、发布详情面板同款），
      替代原来的两个按钮 —— 同一时刻只有全开 / 全关两种状态，是互斥选择。
      · 全部启用 → 选中左侧；全部停用 → 选中右侧；
      · 部分启用 → 两个都不选中（空串），由 .is-mixed 隐掉选中胶囊，
        否则胶囊会停在上一次的选中项上，看起来像已经全开 / 全关；
      · 停用方向仍走 confirmDisableAll()：整页地图一起停用不是随手试的操作；
      · 批量进行中两个分段都禁用，避免连点再发一批命令。
    -->
    <n-tabs v-if="sets.length" type="segment" animated class="bulk-switch"
      :class="{ 'is-mixed': !bulkSwitchValue }" :value="bulkSwitchValue"
      @update:value="onBulkSwitch">
      <!--
        文案走 `#tab` 槽而不是 `:tab` 属性：pane 没有任何子节点时，Naive 的
        Tab 会把空占位当作默认槽渲染，`:tab` 的文案因此不显示（实测只剩一个
        空胶囊）。给每个 pane 一个 `#tab` 槽就绕开了那条渲染路径。
      -->
      <n-tab-pane name="enable" :disabled="bulkWorking">
        <template #tab>{{ t('overview.enableAll') }}</template>
      </n-tab-pane>
      <n-tab-pane name="disable" :disabled="bulkWorking">
        <template #tab>{{ t('overview.disableAll') }}</template>
      </n-tab-pane>
    </n-tabs>
    <n-button size="small" secondary :loading="checkingBound" :disabled="checkingBound || loading"
      :title="t('levelSet.checkUpdatesHint')" @click="checkUpdates">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('overview.check') }}
    </n-button>
    <n-button size="small" secondary @click="openLevelsDirectory">
      <template #icon><FolderOpenOutline width="14" height="14" /></template>
      {{ t('modsLocal.openFolder') }}
    </n-button>
  </Teleport>

  <!--
    本地 ZIP 的安装入口：拖入窗口即打开本卡片，识别出关卡集后请用户确认。
    与地图下载页同一份组件与同一份导入实现，差别只是本页没有「选文件」按钮 ——
    它是本页唯一的安装入口，平时不占页头位置。
  -->
  <InstallSourceCard v-model:show="installSourceOpen" :accept="['zip']"
    :title="t('levelSet.installMap')" :busy="importing" :drag-active="dragActive"
    :read-progress-text="importing ? zipProgressText : ''"
    :pick-file="api.chooseLevelSetZip" @select="handleInstallSourceSelect" />

  <!--
    来源卡片没打开时的进度（在线 Release 暂存、确认后的写盘阶段）：与卡片里
    那行文案同一份数据，避免两处各写一套「到第几个了」。
  -->
  <BusyOverlay :show="showZipOverlay" :status="zipProgressText" />

  <!--
    同身份冲突的检查：在「安装详情」之前挡一道（已有同一张地图时先卸载）。
    与 MOD 安装、地图下载页共用同一个弹窗与同一套执行顺序。
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

  <PageLoading :show="loading">
    <!--
      转圈下方显示真实进度与当前读取的文件夹。
      `done/total` 来自后端逐份上报，没拿到总数时只显示不确定文案，不编百分比。
    -->
    <template #status>
      <span v-if="scanProgress">{{ t('levelSet.scanningProgress', { done: scanProgress.done, total: scanProgress.total }) }}</span>
      <span v-else>{{ t('levelSet.scanningPreparing') }}</span>
      <span v-if="scanProgress?.current" class="scan-folder" :title="scanProgress.current">{{ t('levelSet.scanningFolder', { name: scanProgress.current }) }}</span>
    </template>
    <n-empty v-if="!loading && !sortedSets.length" class="empty"
      :description="t('levelSet.noLevelSets')" />

    <!-- 按作者分组折叠：同一位作者的地图归在一张折叠项里 -->
    <n-collapse v-else :expanded-names="groupsExpanded" class="group-collapse"
      @update:expanded-names="onGroupsExpanded">
      <!--
        折叠项里只挂组件、不写卡片：`n-collapse` 的动态 prop 只有
        `expanded-names`，同组增删卡片时它可能拿到同一个数组引用，整棵子树
        会被跳过。组标题与组内容各自有自己的渲染 effect，才跟得上数据。
      -->
      <n-collapse-item v-for="node in groups" :key="node.key" :name="node.key">
        <template #header>
          <LevelSetGroupHeader :group-key="node.key" />
        </template>
        <LevelSetGroupBody :group-key="node.key" />
      </n-collapse-item>
    </n-collapse>
  </PageLoading>

  <!--
    浮动按钮：重新扫描本地地图（只读磁盘，不碰地图源 Release）。
    「检测更新」在页头，两者职责分开：一个读本地，一个看线上。
  -->
  <n-float-button position="fixed" bottom="28" right="32" :tooltip="t('levelSet.rescanLocal')"
    :disabled="loading" @click="load">
    <RefreshOutline width="17" height="17" :class="{ 'spin-icon': loading }" />
  </n-float-button>
</template>

<style scoped>
/* 页头批量分段控件的样式在 src/styles/global.css 的 `.bulk-switch`。 */

.group-collapse {
  width: 100%;
}

/* 「有更新」可点：手型 + 悬停提亮，无点击目标的标签保持默认光标 */
.state-tag--link {
  cursor: pointer;
}

.state-tag--link:hover {
  filter: brightness(1.15);
}

/* 浮动刷新按钮转圈：与 MOD 两页同一套观感（各自页面作用域） */
.spin-icon {
  flex: 0 0 auto;
  color: var(--oc2-accent);
  animation: oc2-map-spin .8s linear infinite;
}

@keyframes oc2-map-spin {
  to { transform: rotate(360deg); }
}

@media (prefers-reduced-motion: reduce) {
  .spin-icon { animation: none; }
}

.empty {
  padding: 48px 0;
}

/*
 * 加载遮罩里「正在读取的文件夹」那一行。
 * 两行的排布与间距由 PageLoading 的 .page-loading-text 统一给，这里只补
 * 内容差异：文件夹名可能很长，单行省略 + title 给全文 —— 进度数字才是
 * 这一块的主信息。
 */
.scan-folder {
  max-width: 56vw;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--oc2-text-3);
}
</style>
