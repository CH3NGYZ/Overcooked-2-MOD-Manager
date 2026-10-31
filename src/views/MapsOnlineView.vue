<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import {
  NButton,
  NCollapse,
  NCollapseItem,
  NDrawer,
  NDrawerContent,
  NEmpty,
  NFloatButton,
  NInput,
  NSpin,
  NTag,
  useDialog,
} from 'naive-ui'
import { LinkOutline, RefreshOutline, SearchOutline } from '@vicons/ionicons5'
import { t, locale, pickI18n } from '../i18n'
import { api } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { tagColorForAny, type TagTone } from '../theme'
import { useNotify } from '../useNotify'
import { useDashboard } from '../composables/useDashboardStore'
import { useInstallPlanGate } from '../composables/useInstallPlanGate'
import { useRepositoryReadme } from '../composables/useRepositoryReadme'
import { formatVersion, isPrereleaseVersion, versionDisplay } from '../modStatus'
import { markdownSummary } from '../markdown'
import { mapDependencyRows } from '../dependencyRows'
import { matchedReleaseAssets } from '../repositoryAssets'
import { repositoryIsAvailable } from '../repositoryAvailability'
import { useFocusHighlight, useLevelSetLink } from '../crossLink'
import {
  frameworkStates,
  levelSetForMap,
  mapCardAction,
  mapCardStatus,
  mapRelationRepository,
  type FrameworkState,
  type MapCardState,
} from '../levelSetStatus'
import OnlineCard, { type CardAction, type CardDependencyTag } from '../components/OnlineCard.vue'
import ReleaseDetailPanel from '../components/ReleaseDetailPanel.vue'
import type { ReleaseDetailLink } from '../components/ReleaseDetailPanel.vue'
import type { DependencyRow } from '../components/DependencyTable.vue'
import InstallPlanDialog from '../components/InstallPlanDialog.vue'
import type { LevelSetInfo, MapInfo, ReleaseInfo, Repository } from '../types'

/**
 * 地图下载页。
 *
 * ## 与 MOD 安装页的关系
 *
 * 这一页的结构是**照着 MOD 安装页复刻**的：同一个卡片组件（`OnlineCard.vue`）、
 * 同一套折叠树（分类标题 + 计数 + 每级刷新）、同一套页头（计数 → 搜索 → 安装）、
 * 同一套网格与按钮文案规则。用户在同一件事（「把线上这张图装到本地」）上不该
 * 看到两套长相。
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
 * 现在进页面补一次 `api.levelSets()`（后端对 `info` 有解析缓存，这一次调用
 * 换来的是每张卡都能回答「我装没装、要不要更新」）。
 *
 * ## 依赖判定为什么在前端
 *
 * 后端 `get_maps` 不填 `unmet_frameworks`（契约冻结）：依赖是否满足要用
 * 当前 Dashboard 的 MOD 列表判，而这份列表前端本来就有，交给后端等于每次
 * 打开页面再扫一遍 `BepInEx/plugins`。判定本身复用 MOD 侧同一条管线
 * （`mapRelationRepository` 把地图投影成 `Repository` 再走 `resolveRelations`）。
 *
 * ## 安装入口在安装页
 *
 * 和「安装 MOD」一样，「安装地图」属于安装页：线上地图的卡片按钮在卡片上，
 * 本地 ZIP 走页头的「安装地图」。地图管理页只做管理，不放安装入口。
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
const dialog = useDialog()
const { mods, game, config, load, ensureLoaded } = useDashboard()
const { highlightId } = useFocusHighlight()
const { linkToLevelSets } = useLevelSetLink()

const maps = ref<MapInfo[]>([])
const sets = ref<LevelSetInfo[]>([])
const loading = ref(true)
const refreshing = ref<Record<string, boolean>>({})
const installing = ref<Record<string, boolean>>({})
const refreshingAll = ref(false)
/** 「安装地图」按钮的忙碌状态（选 ZIP → 识别 → 确认 → 落盘）。 */
const importing = ref(false)
/** 详情抽屉里当前打开的地图 id（抽屉挂在页面级，卡片只负责打开）。 */
const detailId = ref<string | null>(null)
/** 展开的分类 key；空数组表示「用默认值（全展开）」。 */
const openGroups = ref<string[]>([])

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
 * 安装前检查。
 *
 * 地图的 `relations` 与 MOD 仓库的 `relations` 是同一套模型，所以判定逻辑也
 * 共用 `useInstallPlanGate`：它需要一个 `Repository` 作为关系来源。地图不是
 * `Repository`，这里走 `mapRelationRepository` 的**投影**把声明搬过去，
 * 与「地图管理」页算依赖缺失用的是同一份实现。
 *
 * 弹窗、执行顺序与「装依赖」步骤都来自 MOD 那一套（`InstallPlanDialog` +
 * `buildInstallPlan`）：地图安装与 MOD 安装在用户眼里是同一个动作 ——
 * 「把线上这个东西装到本地，缺的前置先补齐」。
 */
const planGate = useInstallPlanGate({
  mods: () => mods.value,
  repositories: () => repositories.value,
  gameBlocked,
  onApplied: () => load(),
})

function gameBlocked() {
  // 地图安装允许在游戏运行时进行（与本地 ZIP 导入同一条策略），所以这里不拦
  return false
}

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

/** 一个折叠分类：key 取原始 group 值，label 取多语言展示名。 */
interface MapGroup {
  key: string
  label: string
  items: MapInfo[]
}

/**
 * 按 `group` 分桶。
 *
 * key 用原始值而不是翻译后的 label：切语言时不应把用户的展开状态清空。
 * 没有 group 的条目归到「其他」，与 MOD 安装页的「未分类仓库」同一处理。
 */
const groups = computed<MapGroup[]>(() => {
  const byKey = new Map<string, MapGroup>()
  for (const map of maps.value) {
    if (!matches(map)) continue
    const key = map.group?.trim() || '__ungrouped__'
    const bucket = byKey.get(key)
    if (bucket) bucket.items.push(map)
    else {
      byKey.set(key, {
        key,
        label: pickI18n(map.group_i18n, map.group ?? '') || t('common.other'),
        items: [map],
      })
    }
  }
  return [...byKey.values()].sort((left, right) =>
    left.label.localeCompare(right.label, locale.value),
  )
})

const allGroupKeys = computed(() => groups.value.map((group) => group.key))
/**
 * 展开的分类：默认全展开，与 MOD 安装页的源树一致。
 *
 * 搜索时强制全展开：命中的卡片被折叠在别的分类里，等于搜索没生效。
 */
const expandedGroups = computed(() =>
  query.value ? allGroupKeys.value : (openGroups.value.length ? openGroups.value : allGroupKeys.value),
)

const anyRefreshing = computed(
  () => refreshingAll.value || Object.keys(refreshing.value).length > 0,
)

async function loadMaps() {
  loading.value = maps.value.length === 0
  try {
    maps.value = await api.maps()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    loading.value = false
  }
}

/**
 * 读一次本地关卡集。
 *
 * 卡片状态与按钮文案都依赖它（「已安装 / 有更新 / 重装」），所以进页面必须
 * 拿到。失败不影响列表展示：最坏是状态停在「无法检测更新」那一档。
 */
async function loadLevelSets() {
  try {
    sets.value = await api.levelSets()
  } catch (reason) {
    message.warning(backendMessageOf(reason))
  }
}

onMounted(() => {
  /*
   * 依赖提示与安装前检查都读 Dashboard 的 MOD 列表，而 Dashboard 由页面
   * 自己按需加载。缺少这一步时直接进本页会判不出「依赖缺失」，安装前置检查
   * 也会漏掉已装的框架 —— 结论错在「看起来一切正常」这一侧。
   */
  void ensureLoaded()
  void loadMaps()
  void loadLevelSets()
})

/** 展示名：多语言表 → 单语言镜像 → 仓库坐标。 */
function mapName(map: MapInfo): string {
  return pickI18n(map.name_i18n, map.name) || `${map.username}/${map.repository}`
}

/** 简介：下载页要留足说明，用户才知道这张图是什么。 */
function mapNote(map: MapInfo): string {
  return markdownSummary(pickI18n(map.note_i18n, map.note ?? '')) || map.repository
}

function isRefreshing(map: MapInfo): boolean {
  return !!refreshing.value[map.id]
}

function isInstalling(map: MapInfo): boolean {
  return !!installing.value[map.id]
}

/**
 * 卡片状态：本地已装状态（与 MOD 安装页同款结论）。
 *
 * 判定细节在 `levelSetStatus.ts` 的纯函数里（Node 测试直接覆盖），这里只做
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
 * author`。只有存在本地副本时，卡片状态才会落到「已安装 / 有更新 / 最新版」，
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

const detailMap = computed(() => maps.value.find((map) => map.id === detailId.value) ?? null)
const detailStatus = computed(() =>
  detailMap.value
    ? statusOf(detailMap.value)
    : { key: 'not_refreshed' as MapCardState, text: '', tone: 'muted' as TagTone },
)

/**
 * 详情抽屉的 README。
 *
 * 后端 `get_repository_readme` 同时认 MOD 仓库 id 与地图仓库 id，所以地图
 * 详情和 MOD 详情用同一个 composable：拉不到（仓库没有 README.md）时
 * `exists = false`，面板整块收起，不会留一段空白。
 */
const { readme: detailReadme, loading: detailReadmeLoading, error: detailReadmeError } =
  useRepositoryReadme(() => detailMap.value?.id ?? null)

/**
 * 依赖框架表（说明 tab 里、备注上方）。
 *
 * 行由 `dependencyRows.ts` 统一构造，和 MOD 安装页详情、地图管理页详情是同
 * 一份口径：名称按「在线 MOD 仓库卡片名 → 本地已装 MOD 的包标题 → GUID」
 * 解析（依赖是框架 MOD，它们本身也是在线仓库，所以有作者起的名字可显示），
 * 满足时只写「已满足」——版本写在「要求」列，状态列只回答「现在行不行」。
 *
 * 满足情况仍由 Dashboard 已扫好的本地 MOD 判定，与卡片上的「缺依赖」标签、
 * 安装前检查弹窗共用同一份结论。地图的安装入口在设置页，因此行里不带跳转
 * （`mapDependencyRows` 固定 `mode: 'map'`、`jumpToRepository: false`）。
 */
const detailDependencies = computed<DependencyRow[]>(() =>
  mapDependencyRows(detailMap.value, {
    mods: mods.value,
    repositories: repositories.value,
    translate: t,
    pickName: pickI18n,
  }),
)

/* ===== 详情抽屉：版本选择与面板展示值 ===== */

/** 抽屉里选中的 tag；`null` 表示还没选（默认落到最新）。 */
const detailSelectedTag = ref<string | null>(null)
/** 缓存 Release：后端保证最新在前，未刷新时是空数组。 */
const detailReleases = computed<ReleaseInfo[]>(() => detailMap.value?.cached_releases ?? [])
const detailLatestTag = computed(() => detailReleases.value[0]?.tag_name ?? null)

/**
 * 当前选中的 Release。
 *
 * 页面刷新完 Release 后旧选择可能已不在列表里，此时静默回落到最新——
 * 下拉值由 `detailSelectedTag` 同步（见下面的 watch），不会出现「选中的
 * 版本文件列表已经换了」这种错位。
 */
const detailRelease = computed<ReleaseInfo | null>(() => {
  const tag = detailSelectedTag.value
  const found = detailReleases.value.find((release) => release.tag_name === tag)
  return found ?? detailReleases.value[0] ?? null
})

/** 版本下拉选项：label 用原始 tag（含第 4 位），value 是 tag。 */
const detailVersions = computed(() =>
  detailReleases.value.map((release) => ({
    label: formatVersion(release.tag_name || release.version || ''),
    value: release.tag_name,
  })),
)

/**
 * 资产下拉的候选：命中地图源 `asset_rules` 的文件（与 MOD 侧同一套规则镜像）。
 *
 * 规则没写或没命中时退化成「该 Release 里所有可下载文件」：这时后端也不会
 * 按规则选出东西，把列表清空只会得到一个不可用的安装按钮；列全反而让用户
 * 能自己指定要装哪个，且选中项会真的传给后端。
 */
const detailAssets = computed(() => {
  const release = detailRelease.value
  if (!release) return []
  const matched = matchedReleaseAssets(release, detailMap.value?.asset_rules ?? [])
  return matched.length ? matched : release.assets.filter((asset) => !!asset.download_url)
})

/** 抽屉里选中的资产名；`null` 表示还没选（默认落到第一个候选）。 */
const detailSelectedAsset = ref<string | null>(null)
const detailAsset = computed(() =>
  detailAssets.value.find((asset) => asset.name === detailSelectedAsset.value)
  ?? detailAssets.value[0]
  ?? null,
)
/** 换地图或换版本时清掉已选资产：旧选择在新 Release 里通常不存在。 */
watch(
  [() => detailMap.value?.id, () => detailRelease.value?.tag_name],
  () => { detailSelectedAsset.value = null },
)

const detailReleaseTag = computed(
  () => detailRelease.value?.tag_name || detailRelease.value?.name || '—',
)
const detailPrerelease = computed(() =>
  detailRelease.value
    ? isPrereleaseVersion(detailRelease.value.tag_name || detailRelease.value.version)
    : false,
)

/** 头部链接：仓库主页 + 最新 Release 页（与 MOD 详情同一种「点开去看源」）。 */
const detailLinks = computed<ReleaseDetailLink[]>(() => {
  const map = detailMap.value
  if (!map) return []
  const host = map.provider === 'gitee' ? 'https://gitee.com' : 'https://github.com'
  const sourceUrl = `${host}/${map.username}/${map.repository}`
  return [
    { label: t('levelSet.sourcePage'), url: sourceUrl },
    { label: t('levelSet.releasePage'), url: `${sourceUrl}/releases/latest` },
  ]
})

/** 简介（面板自己渲染 Markdown，这里给原文）。 */
const detailNote = computed(() =>
  detailMap.value ? pickI18n(detailMap.value.note_i18n, detailMap.value.note ?? '') : '',
)

/**
 * 面板的必填头部字段。
 *
 * 模板里的 `v-if` 收窄不会穿透到插槽内容（vue-tsc 把插槽编译成独立函数），
 * 所以这些值一律走 computed 兜底，面板只在抽屉打开时渲染。
 */
const detailName = computed(() => (detailMap.value ? mapName(detailMap.value) : ''))
const detailProvider = computed<'github' | 'gitee'>(() => detailMap.value?.provider ?? 'github')
const detailOwner = computed(() => detailMap.value?.username ?? '')
const detailRepository = computed(() => detailMap.value?.repository ?? '')
const detailBindingName = computed(() => detailMap.value?.level_set_name ?? '')
const detailAuthor = computed(() => detailMap.value?.author ?? '')

/**
 * 面板「版本绑定」位显示的身份。
 *
 * MOD 那一列放的是仓库坐标（`owner/repository`），地图没有「绑定到哪个仓库」
 * 这回事，对应的身份就是关卡集的 `levelSetName · author` —— 管理处卡片、
 * 展开详情和这里用同一份身份，位置也与 MOD 的版本绑定列对齐。
 */
const detailBindingValue = computed(() => {
  const name = detailBindingName.value
  if (!name) return ''
  return detailAuthor.value ? `${name} · ${detailAuthor.value}` : name
})

/** 没有缓存 Release 时直接安装会命中后端 `releaseMissing`，主按钮换成刷新。 */
const detailPrimaryAction = computed<'install' | 'refresh'>(() =>
  detailReleases.value.length ? 'install' : 'refresh',
)

/** 换地图或 Release 列表更新时回到最新版。 */
watch(
  [() => detailMap.value?.id, detailLatestTag],
  () => {
    detailSelectedTag.value = detailLatestTag.value
  },
  { immediate: true },
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
  const required = state.minVersion ? `${state.guid} ≥ ${state.minVersion}` : state.guid
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
 * 地图的前置是框架而不是可安装的仓库，因此标签**不可点击** —— 点了也无处
 * 可去（框架入口在设置页），假装可点只会制造一次无反应的点击。
 */
function dependencyOf(map: MapInfo): CardDependencyTag | null {
  const missing = frameworkStates(map, mods.value).filter((state) => !state.satisfied)
  if (!missing.length || !gameProbed.value) return null
  return {
    text: t('levelSet.dependencyMissing'),
    tone: 'critical',
    title: t('levelSet.frameworksNotMet', { list: missing.map(describeFramework).join(' · ') }),
    clickable: false,
  }
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

function setFlag(flags: typeof refreshing, id: string, value: boolean) {
  const next = { ...flags.value }
  if (value) next[id] = true
  else delete next[id]
  flags.value = next
}

/**
 * 把线上依赖声明同步进本地清单（`sync_dependency_snapshots`）。
 *
 * 依赖只写在源文档里：地图源下线、改版或换仓库后，本地清单里那份安装时
 * 固化的快照就会过期，依赖表会显示成一条过时的结论。刷新完按线上声明重写
 * 一遍，离线时依赖表才不会凭空消失。
 *
 * 失败**不弹错**：快照只是兜底展示信息，同步不上不该让一次成功的刷新
 * 变成一条失败提示。
 */
async function syncDependencySnapshots() {
  await api.syncDependencySnapshots().catch(() => undefined)
}

/** 刷新一张地图的 Release（用户主动点刷新 → 绕过缓存）。 */
async function refresh(map: MapInfo) {
  if (isRefreshing(map) || isInstalling(map)) return
  setFlag(refreshing, map.id, true)
  try {
    await api.refreshMapReleases(map.id, true)
    // 刷新完把线上依赖声明写回本地清单（离线时依赖表的兜底来源）
    await syncDependencySnapshots()
    // Release 缓存变了，卡片与详情都要跟着更新
    await loadMaps()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    setFlag(refreshing, map.id, false)
  }
}

/**
 * 安装。
 *
 * 顺序与 MOD 卡片完全一致：
 *   1. 安装前检查（依赖框架缺失 / 版本不够 → 能补就先补，不能补只提醒）
 *   2. 下载 + 校验（后端走候选竞速，进度由全局 DownloadStrip 显示）
 *   3. 落盘（先回收同名旧关卡集）
 *
 * `asset` 是详情页资产下拉里选中的文件名：不传时后端按 `asset_rules` 自动选，
 * 与 MOD 安装同一条规则。
 *
 * 落盘后补读一次本地关卡集：状态标签与按钮要立刻从「未安装」翻成「已安装」，
 * 否则用户装完看到还是「安装」，会以为没生效。
 */
async function install(map: MapInfo, tag?: string, asset?: string) {
  if (isInstalling(map) || isRefreshing(map)) return
  const targetTag = tag ?? map.latest_tag ?? undefined
  // 前置检查：地图缺框架时装上也跑不起来，必须先让用户看到
  const ready = await planGate.run(mapRelationRepository(map), targetTag)
  if (!ready) return

  setFlag(installing, map.id, true)
  try {
    await api.installMapRelease(map.id, targetTag, asset)
    message.success(t('levelSet.installDone', { name: mapName(map) }))
    // 安装清单里的依赖快照按线上声明重写，紧接着的状态重读才不会用到旧快照
    await syncDependencySnapshots()
    await loadLevelSets()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    setFlag(installing, map.id, false)
  }
}

/**
 * 「安装地图」：从本地 ZIP 导入关卡集。
 *
 * 与「安装 MOD」同一职责定位 —— 线上装不进来的东西（作者发的离线包、
 * 自己整理的一堆图）走这个入口。流程与地图管理页原来那套一致：
 * 选 ZIP → 后端识别里面有哪些关卡集 → 确认 → 写入 `levels/`。
 *
 * 落盘后必须重读本地关卡集：本页的卡片状态由它算出来，否则装完还是
 * 显示「安装」。地图源列表不会因此变化，不重拉。
 */
async function pickZip() {
  if (importing.value) return
  let path: string | null
  try {
    path = await api.chooseLevelSetZip()
  } catch (reason) {
    message.error(backendMessageOf(reason))
    return
  }
  if (!path) return
  importing.value = true
  try {
    const found = await api.inspectLevelSetZip(path)
    if (!found.length) {
      message.warning(t('levelSet.zipWithoutInfo'))
      return
    }
    const selected = found.map((set) => set.dir_name)
    /*
     * 用 Naive 的 dialog 而不是自绘弹窗：项目里所有确认都走它，
     * 视觉与交互（点外部关闭、Esc、焦点）已经统一。
     * 「会覆盖已有版本」时单独说明 —— 旧版本会先进回收站。
     */
    const replacing = found.filter((set) => set.exists)
    const detail = replacing.length
      ? t('levelSet.importConfirmReplace', {
          names: replacing.map((set) => set.name || set.dir_name).join('、'),
        })
      : t('levelSet.importConfirm', { count: found.length })
    const approved = await new Promise<boolean>((resolve) => {
      dialog.warning({
        title: t('levelSet.importTitle'),
        content: `${detail}\n\n${found
          .map(
            (set) =>
              `· ${set.name || set.dir_name}${set.author ? ` — ${set.author}` : ''}${set.version ? ` v${set.version}` : ''}（${t('levelSet.levelCount', { count: set.level_count })}）`,
          )
          .join('\n')}`,
        positiveText: t('modsOnline.install'),
        negativeText: t('recycle.cancel'),
        onPositiveClick: () => resolve(true),
        onNegativeClick: () => resolve(false),
        onClose: () => resolve(false),
      })
    })
    if (!approved) return
    const result = await api.importLevelSets(path, selected)
    message.success(t('levelSet.imported', { count: result.installed.length }))
    await loadLevelSets()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    importing.value = false
  }
}

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
function groupRefreshing(group: MapGroup): boolean {
  return group.items.some((map) => isRefreshing(map) || isInstalling(map))
}

/**
 * 刷新一个分类下的全部 Release。
 *
 * 与 MOD 安装页的分组刷新同一交互：按钮转圈、逐个请求、结束后统一重拉，
 * 单个失败不阻断同组其它条目，只在最后报一次错（逐个弹会让用户被错误淹没）。
 */
async function refreshGroup(group: MapGroup) {
  if (refreshingAll.value || groupRefreshing(group)) return
  let firstError: unknown = null
  for (const map of group.items) {
    if (isRefreshing(map) || isInstalling(map)) continue
    setFlag(refreshing, map.id, true)
    try {
      await api.refreshMapReleases(map.id, true)
    } catch (reason) {
      if (firstError === null) firstError = reason
    } finally {
      setFlag(refreshing, map.id, false)
    }
  }
  if (firstError !== null) message.error(backendMessageOf(firstError))
  await syncDependencySnapshots()
  await loadMaps()
}

/**
 * 刷新全部。
 *
 * 两步：先重拉源树（索引 → 子源，拿到最新的地图清单），再逐个仓库刷 Release。
 * 顺序不能反 —— 源树可能新增/删除了地图，先刷 Release 会给已经不存在的仓库
 * 发请求，新加的地图又拿不到版本。
 */
async function refreshAll() {
  if (refreshingAll.value) return
  refreshingAll.value = true
  try {
    maps.value = await api.refreshMapSources()
  } catch (reason) {
    // 源树失败仍然尝试刷新已有条目的 Release（缓存可能还能用）
    message.error(backendMessageOf(reason))
  }
  try {
    for (const map of maps.value) {
      try {
        await api.refreshMapReleases(map.id, true)
      } catch (reason) {
        message.error(backendMessageOf(reason))
      }
    }
    await syncDependencySnapshots()
    await loadMaps()
  } finally {
    refreshingAll.value = false
  }
}
</script>

<template>

  <!--
    页头右侧，从左到右：计数 → 搜索 → 安装地图（与 MOD 安装页「计数 → 搜索 →
    安装 MOD」同序）。「安装地图」固定在最右：它是本页最主要的本地安装入口。
    整源刷新由右下角浮动按钮承担，页头不再重复放一个按钮。
  -->
  <Teleport to="#page-header-actions" defer>
    <span class="header-count muted">{{ visibleCount }} {{ t('levelSet.mapCount') }}</span>
    <n-input v-model:value="search" clearable size="small" class="header-search"
      :placeholder="t('dialogs.search')">
      <template #prefix><SearchOutline width="14" height="14" /></template>
    </n-input>
    <n-button size="small" type="primary" :loading="importing" :disabled="importing"
      @click="pickZip">
      <LinkOutline width="14" height="14" />{{ t('levelSet.installMap') }}
    </n-button>
  </Teleport>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !visibleCount" class="empty" :description="emptyDescription" />

    <!--
      折叠树与 MOD 安装页同形：分类标题（计数 + 刷新）+ 卡片网格。
      地图源里没有第二层「源」，所以这里只折叠一层。
    -->
    <n-collapse v-else :expanded-names="expandedGroups" class="source-collapse"
      @update:expanded-names="(names: string[]) => { openGroups = names }">
      <n-collapse-item v-for="group in groups" :key="group.key" :name="group.key">
        <template #header>
          <div class="source-head">
            <b>{{ group.label }}</b>
            <n-tag size="tiny" :bordered="false">{{ group.items.length }}</n-tag>
            <n-button size="tiny" quaternary class="group-refresh" :loading="groupRefreshing(group)"
              :disabled="anyRefreshing" :title="t('modsOnline.refresh')" @click.stop="refreshGroup(group)">
              <template #icon><RefreshOutline width="13" height="13" /></template>
            </n-button>
          </div>
        </template>

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
            @action="runAction(map)" />
        </div>
      </n-collapse-item>
    </n-collapse>
  </n-spin>

  <!--
    详情抽屉：与 MOD 安装页是**同一个面板**（`ReleaseDetailPanel.vue` 只做版式）。
    地图只往 `#status` 插槽里补一个「已装状态」标签；依赖框架走面板的依赖表
    （说明 tab 的备注上方），绑定身份走「版本绑定」列 —— 剩下的 README、版本、
    资产下拉与安装按钮全部由面板统一渲染，两页不再各写一套。
    面板不认识 `MapInfo`，所有业务值都在本页算好再传。
  -->
  <n-drawer :show="!!detailMap" :width="720" placement="right"
    @update:show="(value: boolean) => { if (!value) detailId = null }">
    <n-drawer-content v-if="detailMap" :native-scrollbar="false" closable>
      <!--
        `:key` 必须有：面板自己持有「说明 / 下载安装」tab 状态，换地图要重置。
        版本选择的状态留在页面（`detailSelectedTag`），面板只发 `update:version`。
      -->
      <ReleaseDetailPanel :key="detailMap.id" :title="detailName" :provider="detailProvider"
        :owner="detailOwner" :repository="detailRepository" :icon-size="34"
        :meta-links="detailLinks" :note="detailNote"
        :readme-content="detailReadme?.content ?? ''" :readme-exists="!!detailReadme?.exists"
        :readme-loading="detailReadmeLoading" :readme-error="detailReadmeError"
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
        @install="installFromDetail" @refresh="refreshDetail">
        <!-- 头部状态：与卡片上是同一份结论（`statusOf`） -->
        <template #status>
          <n-tag size="small" :color="tagColorForAny(detailStatus.tone)">
            {{ detailStatus.text }}
          </n-tag>
        </template>
      </ReleaseDetailPanel>
    </n-drawer-content>
  </n-drawer>

  <!-- 安装前检查：缺框架时先补装（与 MOD 安装页同一个弹窗与同一套执行顺序） -->
  <InstallPlanDialog v-model:show="planGate.open.value" :plan="planGate.plan.value"
    :target-label="planGate.target.value"
    @finished="(ready: boolean) => { void planGate.finish(ready) }" />

  <!--
    整源刷新入口，与 MOD 安装页同一个位置与观感。
    - 禁用看 `refreshingAll`：源文档阶段也算忙，防止重复点。
    - 图标一直转表示正在刷新（Release 缓存刷新期间用户需要明确反馈）。
  -->
  <n-float-button position="fixed" bottom="28" right="32" :disabled="refreshingAll"
    :tooltip="t('levelSet.refreshAllHint')" @click="refreshAll">
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

/*
 * 搜索框是操作区里唯一能收缩的项（数值与 MOD 安装页一致）：
 * 窗口变窄时它先变短，计数与按钮保持完整。
 */
.header-search {
  width: 240px;
  flex: 0 1 240px;
  min-width: 120px;
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
 * 与 `ModsOnlineView.vue` 的 `.source-collapse` / `.source-head` / `.group-refresh`
 * / `.repo-grid` 保持同一份数值 —— 两页列表要看起来是同一个东西。
 * 卡片内部样式全在 `OnlineCard.vue`，这里只管列表骨架。
 */
.source-collapse {
  width: 100%;
}

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

.empty {
  padding: 48px 0;
}
</style>
