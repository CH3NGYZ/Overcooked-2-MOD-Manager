<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { NButton, NCollapse, NCollapseItem, NEmpty, NFloatButton, NTabPane, NTabs, NTag, useDialog } from 'naive-ui'
import { DownloadOutline, FolderOpenOutline, RefreshOutline, TrashOutline } from '@vicons/ionicons5'
import { t, locale, pickI18n } from '../i18n'
import { api, watchLevelSetScan } from '../services/tauri'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { buttonColorFor, tagColorFor, tagColorForAny } from '../theme'
import { useNotify } from '../useNotify'
import { useDashboard } from '../composables/useDashboardStore'
import { useMapSources } from '../composables/useMapSources'
import { statusPulseClass, useFocusHighlight, useMapLink, useRepositoryLink, useStatusHighlight } from '../crossLink'
import { levelSetDependencyRows } from '../dependencyRows'
import { repositoryIsAvailable } from '../repositoryAvailability'
import { describeRefreshOutcome } from '../refreshOutcomeText'
import { collapseCardWithScroll, scrollExpandedCardIntoView } from '../expandScroll'
import LocalItemCard from '../components/LocalItemCard.vue'
import PageLoading from '../components/PageLoading.vue'
import RefreshResultTag from '../components/RefreshResultTag.vue'
import DependencyTable from '../components/DependencyTable.vue'
import type { DependencyRow } from '../components/DependencyTable.vue'
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
import type { LevelSetInfo, LevelSetScanProgress, MapInfo, Repository } from '../types'

/**
 * 地图管理页 —— 只做「管理已经在本地的地图」。
 *
 * ## 安装入口在下载页
 *
 * 「安装地图」（线上 Release + 本地 ZIP）和「安装 MOD」一样属于安装页的
 * 职责，入口因此在地图下载页（`MapsOnlineView.vue`）。本页只保留管理动作：
 * 启用/禁用（含全部启用 / 全部禁用）、检测更新、打开目录、删除。
 *
 * ## 与 MOD 管理页的关系
 *
 * 卡片外壳直接复用 `LocalItemCard`（与 MOD 管理页同一份），页头动作的顺序
 * 与语义也照它排：批量开关 → 检测更新 → 打开目录；右下角浮动按钮是「重新扫描
 * 本地地图」。批量开关做成二选一分段控件而不是两个按钮：它回答的是「全体现在
 * 处于哪一档」，部分开启时两边都不选中才是真实状态。
 *
 * 数据模型仍然完全不同：MOD 是 BepInEx 插件（身份是 GUID），地图是
 * `levels/` 下的一个目录（身份是 `levelSetName + author`），所以没有把地图
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
const TOOL_BUTTON_WIDTH = 30
const TOOL_GAP = 8
const toolsMinWidth = TOOL_BUTTON_WIDTH * 2 + TOOL_GAP

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
  maps, refreshing, refreshingSources, refreshingAll, checkingBound, outcome, loadMaps, refreshBoundMaps,
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
/** 当前展开详情的目录名：一次只展开一个，点另一张卡就换过去。 */
const expandedDir = ref<string | null>(null)

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
 * 开关 / 删除 / 导入改的都是 `levels/` 与 `disabled/` 下的目录，
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
 * `all` 是整源刷新（下载页的「刷新全部」）那一档：它会把共享的 `maps` 清空
 * 再重建，这段时间**所有**关卡集都找不到绑定，只传 id 集合的话卡片会集体
 * 退化成「未绑定」——那比过期结论更误导。整批刷期间一律显示「刷新中」。
 *
 * 每次重算都新建两个 Set：一次刷新最多几十个 id，且只在 `refreshing` /
 * `refreshingSources` / `refreshingAll` 变化（刷新的开始与结束）时重算，成本可忽略。
 */
const refreshingHint = computed<LevelSetRefreshingHint>(() => ({
  all: refreshingAll.value,
  maps: new Set(Object.keys(refreshing.value)),
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
  const previous = expandedDir.value
  const opening = previous !== set.dir_name
  if (!opening) {
    // 收起：量位置、改状态都交给 collapseCardWithScroll —— 它必须在折叠前量，
    // 再把展开那次滚动倒回去，折叠与回滚同时发生，看起来才是「原路收回去」。
    const target = card ?? cardOf(set.dir_name)
    const collapse = () => {
      expandedDir.value = null
    }
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
  expandedDir.value = set.dir_name
  if (!card) return
  const anchor = card.querySelector('.card-row')
  if (!anchor) return
  void nextTick().then(() => {
    scrollExpandedCardIntoView({
      card,
      anchor,
      body: card.querySelector('.card-detail'),
      collapsing: previousCard
        ? { card: previousCard, body: previousCard.querySelector('.card-detail') }
        : null,
    })
  })
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
 * 收起后把展开那次滚动倒回去（单展开语义下它是唯一展开过的那张卡）。
 */
function onDocumentClick(event: MouseEvent) {
  if (cardClickHandled) {
    cardClickHandled = false
    return
  }
  const dirName = expandedDir.value
  if (dirName === null) return
  const target = event.target
  if (target instanceof Element && target.closest('.card-row')) return
  const card = cardOf(dirName)
  const collapse = () => {
    if (expandedDir.value === dirName) expandedDir.value = null
  }
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
  dialog.warning({
    title: t('levelSet.deleteTitle'),
    content: recycle
      ? t('modsLocal.willBeMovedToTheRecycleBinAndCanBeRestoredContinue', { name })
      : t('modsLocal.willBeDeletedPermanentlyThisCannotBeUndoneContinue', { name }),
    positiveText: recycle ? t('modsLocal.moveToRecycleBin') : t('modsLocal.deletePermanently'),
    negativeText: t('recycle.cancel'),
    onPositiveClick: async () => {
      deleting.value = { ...deleting.value, [set.dir_name]: true }
      try {
        const result = await api.deleteLevelSet(set.dir_name, set.enabled)
        if (!result.ok) {
          message.error(t('levelSet.deleteFailed', { reason: translateBackendMessage(result.message) }))
          return
        }
        message.success(translateBackendMessage(result.message))
        if (expandedDir.value === set.dir_name) expandedDir.value = null
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
 * 安装入口不在这页：线上 Release 与本地 ZIP 都在地图下载页。这里只关心
 * 「已经在本地的地图」——启用/禁用、看有没有新版本、打开目录、删除。
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
 * 列表重建令牌，只在批量切换成功后自增。
 *
 * 批量启停后 `reloadSets()` 已经把新状态写进 `sets`，但折叠项里的卡片不会
 * 重渲染：`n-collapse-item` 的内容区由 naive-ui 的 `CollapseItemContent` 拿着
 * 稳定插槽（`_: 1`）去渲染内部的 `FadeInExpandTransition`，那个组件 vnode 的
 * patchFlag 是 0，父级重渲染时 Vue 的 `shouldUpdateComponent` 直接跳过它，
 * 插槽闭包不再求值，卡片停在批量切换前的开关状态。
 *
 * 单卡开关不会踩这个坑：它会把 `busy` 置位，而 `busy` 正好被卡片插槽读取，
 * 折叠过渡因此自己收到依赖变化、重渲染并拿到页面最新的插槽；批量路径只改
 * `sets`（页面 render 时读）与页头的 `bulkWorking`，卡片插槽里没有它们的依赖。
 *
 * 换掉整棵列表的 key 强制重建（与 MOD 管理页 `listEpoch` 同一套做法）：
 * 展开状态都在页面级 ref（`expandedDir` / `collapsedGroups`）里，重建不丢。
 */
const listEpoch = ref(0)

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
      // 见 listEpoch：批量后强制重建折叠列表，让卡片重新读 enabled
      listEpoch.value += 1
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
    安装入口不在这页：它属于安装页的职责，见地图下载页的「安装地图」。
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
    <!--
      :key="listEpoch" 只由批量切换推进（见 script 里的 listEpoch）：
      折叠项内容挂在折叠过渡的稳定插槽里，批量改数据后不会重渲染，
      换 key 强制重建整棵列表。
    -->
    <n-collapse v-else :key="listEpoch" :expanded-names="groupsExpanded" class="group-collapse"
      @update:expanded-names="onGroupsExpanded">
      <n-collapse-item v-for="node in groups" :key="node.key" :name="node.key">
        <template #header>
          <!--
            分组标题只显示分组名。
            计数（启用 / 停用）由页头聚合标签承担，组标题再挂一遍标签只会
            把标题挤窄，并把同一信息说两次。
          -->
          <div class="group-head">
            <b>{{ node.label }}</b>
          </div>
        </template>

        <div class="map-grid">
          <!--
            卡片 key 只用目录名。把 enabled 写进 key 会连整张卡片一起重建，
            开关每次都是「新挂载」，Naive 的切换过渡动画因此整个消失（现象：点下去
            没有滑动，状态已经是新的）。

            但详情确实要跟着开关刷新：停用会把目录搬进 disabled/，详情里的「路径」
            跟着变；而详情挂在 `n-collapse-transition` 的稳定插槽里，父级换了数据它
            也不重新求值。这份刷新交给 `detail-key`：外壳与开关保持同一个 DOM 节点，
            只让详情子树按状态重建。
          -->
          <LocalItemCard v-for="set in node.sets" :key="set.dir_name"
            :detail-key="`${set.dir_name}:${set.enabled}`"
            :data-level-dir="set.dir_name"
            :data-focus-id="focusIdOf(set)"
            :enabled="set.enabled"
            :toggle-loading="!!busy[set.dir_name]"
            :title="levelSetDisplayName(set, locale)"
            :title-tooltip="set.error ? translateBackendMessage(set.error) : levelSetDisplayName(set, locale)"
            :disabled="!set.enabled"
            :expanded="expandedDir === set.dir_name"
            :focus-flash="!!highlightId && highlightId === focusIdOf(set)"
            :status-flash="matchesActiveStatus(set)"
            :pulse-class="statusPulseClass(set.dir_name, pulseTarget, pulseSeq)"
            :tools-min-width="toolsMinWidth"
            :tool-gap="TOOL_GAP"
            :tool-button-width="TOOL_BUTTON_WIDTH"
            @toggle="(value: boolean) => toggle(set, value)"
            @activate="(card: HTMLElement) => activate(set, card)">
            <!--
              状态区：快捷动作 + 标签组。
              「去安装」与 MOD 管理页同一规则（`dependencyTagFor`）：缺依赖且
              线上有能补上的仓库卡片时才出现，点了跳到那张卡；没有来源就不出
              按钮 —— 一个点了没反应的按钮比没有更糟。
            -->
            <template #status>
              <template v-if="dependencyInstallTarget(set)">
                <n-button size="tiny" :style="buttonColorFor('critical', 'soft')"
                  :title="t('overview.browse')" @click.stop="installDependency(set)">
                  <DownloadOutline width="14" height="14" />
                  {{ t('overview.browse') }}
                </n-button>
                <span class="tool-divider" />
              </template>
              <span class="tag-group">
                <n-tag v-for="tag in tagsOf(set)" :key="tag.kind + tag.text" size="tiny"
                  :bordered="false" class="state-tag"
                  :class="{ 'state-tag--link': !!(tag.repositoryId || tag.mapId) }"
                  :color="tagColorForAny(tag.tone)" :title="tagTitle(set, tag)"
                  @click.stop="onTagClick(tag)">
                  {{ t(tag.text) }}
                </n-tag>
              </span>
            </template>

            <!-- 工具栏：打开目录、删除（游戏运行时禁止删除） -->
            <template #tools>
              <n-button size="tiny" quaternary :title="t('modsLocal.revealInFolder')"
                @click.stop="openDirectory(set)">
                <FolderOpenOutline width="14" height="14" />
              </n-button>
              <n-button size="tiny" quaternary type="error" :title="t('modsLocal.delete')"
                :disabled="kitchenRunning || !!deleting[set.dir_name]"
                :loading="!!deleting[set.dir_name]" @click.stop="remove(set)">
                <TrashOutline width="14" height="14" />
              </n-button>
            </template>

            <!-- 展开：元数据 + 绑定地图 + 框架依赖 -->
            <template #detail>
              <div class="detail-grid">
                <!--
                  固定四行版式（爸爸定的）：中文名+作者中文名 / 英文名+作者英文名 /
                  版本+关卡数 / 路径。两列网格按 DOM 顺序两两成行，所以这里必须两个
                  一对地交错写，不能按「先列四个名字」的语义顺序排。
                  info 里本来就存了中/英两个集名，作者的译名则来自绑定地图源的
                  `group_i18n`；缺哪个都显示 `—`，而不是留空让行消失。
                -->
                <div class="detail-row">
                  <span class="detail-label">{{ t('levelSet.nameZh') }}</span>
                  <span class="detail-value">{{ set.name_zh || '—' }}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">{{ t('levelSet.authorZh') }}</span>
                  <span class="detail-value">{{ authorNamesOf(set).zh || '—' }}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">{{ t('levelSet.nameEn') }}</span>
                  <span class="detail-value">{{ set.name || '—' }}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">{{ t('levelSet.authorEn') }}</span>
                  <span class="detail-value">{{ authorNamesOf(set).en || '—' }}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">{{ t('levelSet.version') }}</span>
                  <span class="detail-value">{{ set.version || '—' }}</span>
                </div>
                <div class="detail-row">
                  <span class="detail-label">{{ t('levelSet.levelCountLabel') }}</span>
                  <span class="detail-value">{{ set.level_count }}</span>
                </div>
                <!-- 绝对路径占满整行：半列宽度只够显示到盘符 -->
                <div class="detail-row detail-row--wide">
                  <span class="detail-label">{{ t('levelSet.path') }}</span>
                  <span class="detail-value mono detail-value--path"
                    :title="set.path || undefined">{{ set.path || '—' }}</span>
                </div>
              </div>

              <!--
                线上绑定与状态单独一节：这几项说的是「这张本地图在线上/游戏侧是
                什么情况」，与上面四行「这张图本身是什么」不是同一类信息，混着排
                会读不清，也会把上面固定的四行版式撑乱。
              -->
              <div class="detail-section">
                <div class="detail-section-title">{{ t('levelSet.bindStatusSection') }}</div>
                <div class="detail-grid">
                  <div class="detail-row detail-row--wide">
                    <span class="detail-label">{{ t('levelSet.boundMap') }}</span>
                    <span class="detail-value">
                      <template v-if="bindingOf(set)">
                        {{ bindingOf(set)?.name }}
                        <span v-if="bindingOf(set)?.tag" class="mono bound-tag">{{ bindingOf(set)?.tag }}</span>
                        <!-- 反方向跳转：从管理页看这张图在下载页的样子 -->
                        <n-button size="tiny" text type="primary" class="bound-link"
                          @click.stop="linkToMap(bindingOf(set)?.map.id)">
                          {{ t('levelSet.viewInMapDownload') }}
                        </n-button>
                      </template>
                      <template v-else>—</template>
                    </span>
                  </div>
                  <!-- 解析失败：原因放在详情里，卡片行只留标签 -->
                  <div v-if="set.error" class="detail-row detail-row--wide">
                    <span class="detail-label">{{ t('levelSet.parseError') }}</span>
                    <span class="detail-value is-error">{{ translateBackendMessage(set.error) }}</span>
                  </div>
                  <div v-if="hintOf(set)" class="detail-row detail-row--wide">
                    <span class="detail-label">{{ t('levelSet.statusHint') }}</span>
                    <span class="detail-value">{{ hintOf(set) }}</span>
                  </div>
                </div>
              </div>

              <!--
                依赖声明：本地缺失的标红，用户重新启用前就能看到；与下载页详情、
                MOD 安装页详情共用同一张依赖表（名称是人类可读的仓库名/包名，
                不再需要等宽字体）。没有声明时整块不渲染，不留空分隔线。
              -->
              <div v-if="dependencyRows(set).length" class="detail-section">
                <DependencyTable :rows="dependencyRows(set)" @jump="openDependencyJump" />
              </div>
            </template>
          </LocalItemCard>
        </div>
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

/*
 * 分组折叠：组标题行 + 组内单列卡片。
 *
 * 尺寸与 `ModsLocalView.vue` 的同名规则逐项对齐（14px 强调色标题、组内
 * 卡片 10px 竖向节奏、左侧 14px 缩进），两页的分组看起来才是同一套。
 * 卡片外壳、悬停、展开动画都由 LocalItemCard 负责，这里只管列表节奏。
 */
.group-collapse {
  width: 100%;
}

.group-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}

.group-head b {
  font-size: 14px;
  color: var(--oc2-accent);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.map-grid {
  /* 每个卡片撑满容器宽度（单列纵向堆叠） */
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 2px 0 6px 14px;
}

/*
 * 状态标签组。
 *
 * 这段样式属于**页面作用域**：标签是挂在 LocalItemCard 的 status 插槽里的，
 * 组件的 scoped 规则命中不到插槽内容。不在这里定义就会丢掉 flex 与间距，
 * 标签会换行、贴不住右侧工具栏。
 *
 * 与 MOD 管理页一致：单行不换行，超出就裁掉。地图最多两个标签
 * （依赖缺失 + 版本结论），换行会让卡片比同列表的其它卡片高出一截。
 */
.tag-group {
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
}

/* 「有更新」可点：手型 + 悬停提亮，无点击目标的标签保持默认光标 */
.state-tag--link {
  cursor: pointer;
}

.state-tag--link:hover {
  filter: brightness(1.15);
}

/* 详情：两列信息网格，值允许换行（作者名 / 状态说明可能很长） */
.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 18px;
}

.detail-row {
  display: flex;
  gap: 10px;
  min-width: 0;
  font-size: 12px;
  line-height: 1.5;
}

/* 绝对路径单独占一行：半列宽度只够显示到盘符 */
.detail-row--wide {
  grid-column: 1 / -1;
}

.detail-label {
  flex: 0 0 auto;
  min-width: 64px;
  color: var(--oc2-text-3);
}

.detail-value {
  min-width: 0;
  color: var(--oc2-text-2);
  overflow-wrap: anywhere;
}

.detail-value.is-error {
  color: var(--oc2-tone-critical-text);
}

/*
 * 路径单行不折行：折成两三行的绝对路径比省略号更难读，全文放在 title 里。
 * 需要 `overflow: hidden` 才轮得到 `text-overflow`，否则只是溢出被裁。
 */
.detail-value--path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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

/* 等宽文本用全局 `.mono`（styles/global.css），这里只补详情行的间距 */
.bound-tag {
  margin-left: 6px;
  color: var(--oc2-text-3);
}

.bound-link {
  margin-left: 8px;
}

.detail-section {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px dashed var(--oc2-row-line);
}

/* 分节小标题：只做分组提示，不抢上面四行主体的视觉重量 */
.detail-section-title {
  margin-bottom: 6px;
  font-size: 12px;
  color: var(--oc2-text-3);
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
