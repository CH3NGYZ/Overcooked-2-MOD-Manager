<script setup lang="ts">
import { t, locale } from '../i18n'
import { buttonColorFor, tagColorFor, tagColorForAny } from '../theme'
import { PULSE_DURATION } from '../motion'
import { collapseCardWithScroll, scrollExpandedCardIntoView } from '../expandScroll'
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useDialog } from 'naive-ui'
import { useNotify } from '../useNotify'
import {
  NButton, NCollapse, NCollapseItem, NCollapseTransition, NEmpty, NFloatButton,
  NSwitch, NTabPane, NTabs, NTag,
} from 'naive-ui'
import {
  ArrowUpCircleOutline, DocumentTextOutline, DownloadOutline, FolderOpenOutline, RefreshOutline,
  SettingsOutline, TrashOutline,
} from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { formatVersion, repositoryForMod } from '../modStatus'
import type { RelationStatus } from '../modRelations'
import { repositoryIsAvailable } from '../repositoryAvailability'
import {
  installedModTags, summarizeUnits, packageStatuses, type PackageStatus, type StatusSummary,
} from '../statusTags'
import { statusActionFor, type StatusActionSpec } from '../statusActions'
import { installedModDependencyRows } from '../dependencyRows'
import type { DependencyRow } from '../dependencyRows'
import { statusPulseClass, useFocusHighlight, useRepositoryLink, useStatusHighlight } from '../crossLink'
import { runtimeModStatus, type RuntimeRefreshState } from '../modRuntimeStatus'
import { tagTargetTitle } from '../tagTargets'
import { describeRefreshOutcome } from '../refreshOutcomeText'
import { markDialogBusy } from '../dialogBusy'
import { cardSwitchOn, modSwitchKey, publishCardSwitches } from '../cardSwitchState'
import { useDashboard } from '../composables/useDashboardStore'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import DependencyTable from '../components/DependencyTable.vue'
import LocalItemCard from '../components/LocalItemCard.vue'
import ModConfigEditor from '../components/ModConfigEditor.vue'
import ModDetailGrid from '../components/ModDetailGrid.vue'
import ModStatusTags from '../components/ModStatusTags.vue'
import ModInstallDialog from '../components/ModInstallDialog.vue'
import InstallCheckDialog from '../components/InstallCheckDialog.vue'
import InstallPlanDialog from '../components/InstallPlanDialog.vue'
import InstallSourceCard from '../components/InstallSourceCard.vue'
import PageLoading from '../components/PageLoading.vue'
import RefreshResultTag from '../components/RefreshResultTag.vue'
import { useModInstallFlow } from '../composables/useModInstallFlow'
import { useFileDrop } from '../composables/useFileDrop'
import { takePendingDrop, useDropRouting } from '../composables/useDropRouting'
import { useRepositoryPreview } from '../composables/useRepositoryPreview'
import { useInstallPlanGate } from '../composables/useInstallPlanGate'
import { useModRemoval } from '../composables/useModRemoval'
import { useCardExpansion } from '../composables/useCardExpansion'
import type { InstalledMod, Repository } from '../types'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'

const message = useNotify()
const dialog = useDialog()
const { load, ensureLoaded, loading, mods, game, config } = useDashboard()

/**
 * 删除是否经过回收站。
 *
 * 文案必须与真实行为一致：用户关掉「删除 MOD 时优先放入回收站」后，
 * 删除就是直接抹掉文件，再提示「已移入回收站」会让人以为还能找回。
 * 默认按开启处理（后端 default 也是 true）。
 */
const recyclesOnDelete = computed(() => config.value?.recycle_deleted_mods !== false)

/** 删除前的确认文案与按钮：随开关切换。 */
const deleteConfirm = computed(() => recyclesOnDelete.value
  ? {
      body: (name: string) => t('modsLocal.willBeMovedToTheRecycleBinAndCanBeRestoredContinue', { name }),
      action: t('modsLocal.moveToRecycleBin'),
      done: t('modsLocal.movedToRecycleBin'),
    }
  : {
      body: (name: string) => t('modsLocal.willBeDeletedPermanentlyThisCannotBeUndoneContinue', { name }),
      action: t('modsLocal.deletePermanently'),
      done: t('modsLocal.deleted'),
    })

/** 整包删除的确认文案与动作名：同样随开关切换。 */
const deletePackageConfirm = computed(() => recyclesOnDelete.value
  ? {
      body: (name: string, count: number) =>
        t('tools.allFilesOfWillBeMovedToTheRecycleBinAndCanBeRestoredContinue', { name, count }),
      action: t('modsOnline.movePackageToRecycleBin'),
      done: t('modsLocal.movedToRecycleBin'),
    }
  : {
      body: (name: string, count: number) =>
        t('tools.allFilesOfWillBeDeletedPermanentlyThisCannotBeUndoneContinue', { name, count }),
      action: t('modsLocal.deletePackage'),
      done: t('modsLocal.deleted'),
    })

/**
 * 卸载单个 MOD：确认 → 后端 → （可能）整包确认 → 刷新。
 *
 * 与 MOD 安装页的「卸载冲突」共用同一份实现 —— 那段流程里
 * 「后端要求二次确认」的分支很容易漏，漏了就会让包内 DLL 删不掉。
 */
const { remove } = useModRemoval({
  deleteCopy: () => deleteConfirm.value,
  titleOf: (mod) => modTitle(mod),
  onBusy: (mod, busyState) => {
    if (busyState) busy.value[mod.dll_file] = true
    else delete busy.value[mod.dll_file]
  },
  onDone: () => load(),
})
/**
 * 刷新状态：与 MOD 安装页共用同一份模块级状态。
 *
 * `refreshing` 按仓库、`sourceRefreshing` 按仓库源、`refreshingAll` 是整批
 * 「刷新全部」（门控用，不参与卡片判定）、`entryStateUnavailable` 是仓库列表
 * 正在清空重建（卡片判定唯一需要的批次兜底）、`checkingBound` 是本页「检查更新」
 * 的批次态。谁发起刷新都改同一份状态，所以切页往返不丢，两页的标签会同时显示「刷新中」。
 */
const {
  refreshing, queued, sourceRefreshing, refreshingAll, entryStateUnavailable,
  checkingBound, outcome,
  refreshOne, refreshBoundRepositories,
} = useRepositoryRefresh()

/**
 * 检查更新：**只刷新与已安装 MOD 绑定的仓库**。
 *
 * 与 MOD 安装页的「刷新全部」不是一回事：
 *   · 那边是「浏览可用 MOD」，要重新拉源文档、刷新所有仓库；
 *   · 这里是「我这几个 MOD 有没有新版」，只关心它们各自绑定的仓库。
 *
 * 走引擎的 `refreshBoundRepositories`，不在页面里自己拼一批仓库去刷：
 * 它负责强制绕过 5 分钟 Release 缓存、维护共享的批次态 `checkingBound`，
 * 并在批量结束后统一重扫本地 MOD。页面自己拼批次，就会出现「这个页在转、
 * 那个页不转」—— 谁发起请求决定了别的页面看到什么。
 *
 * 顺带的好处：装 4 个 MOD、来自 3 个仓库时只发 3 个 Release 请求，
 * 而不是二十多个 —— 更快，也不浪费匿名 API 额度。
 */
async function checkUpdates() {
  if (checkingBound.value || refreshingAll.value) return
  /*
   * 收集「当前地区可见、且确实有本地 MOD 绑定」的仓库。
   *
   * 三点约束：
   *   · 用 `repositoryIsAvailable` 而不是只看 enabled —— 地区不适用等于
   *     该仓库不存在，不能给它发请求（否则会报「不属于当前下载渠道」）；
   *   · 跳过 RAW 模式：它没有 Release 可查（引擎入口内部也会跳过，
   *     这里先滤掉避免无意义的排队）；
   *   · 同一仓库只刷一次：多个 MOD 常来自同一个仓库（同一个包里的多个 DLL）。
   */
  const region = config.value?.region
  const platform = config.value?.platform
  const sources = config.value?.repository_sources
  const targets = new Map<string, Repository>()
  for (const mod of effectiveMods.value) {
    if (mod.is_dependency) continue
    const repo = repositoryForMod(repositories.value, mod)
    if (!repo) continue
    if (!repositoryIsAvailable(repo, sources, region, platform)) continue
    if (repo.download_mode === 'repository_file') continue
    targets.set(repo.id, repo)
  }

  if (!targets.size) {
    message.warning(t('overview.noInstalledModIsBoundToAnAvailableOnlineRepository'))
    return
  }
  void api.writeFrontendLog('info',
    `检查更新：匹配到 ${targets.size} 个仓库（已安装 MOD 共 ${effectiveMods.value.length} 个）`)

  try {
    // 批次态（按钮转不转、可否再点）与批次结果都由引擎维护，页面不另存一份
    const summary = await refreshBoundRepositories([...targets.values()])
    summary.errors.forEach((error) => message.error(error))
    /*
     * 结果文案与页头那枚「上次检查」标签共用 `describeRefreshOutcome`：
     * 同一批数据在两处永远说同一句话。
     *
     * 只有真的刷到了仓库才报数量：这些仓库可能刚好都在刷新中（例如刚在
     * 安装页点过单仓刷新），此时队列为空、既没成功也没失败，
     * 弹一句「已检查 0 个仓库」只会让人以为哪里坏了。
     */
    if (!summary.errors.length && summary.refreshed > 0 && outcome.value)
      message.success(describeRefreshOutcome(outcome.value, 'repositories'))
  } catch (reason) {
    fail(reason)
  }
}

/** 状态标签跳转：点标签跳到 MOD 安装页，并高亮对应仓库卡片 3 秒。 */
const { linkToRepository } = useRepositoryLink()
/** 从 MOD 安装页跳回来时，高亮绑定到该仓库的 MOD 卡片 3 秒并滚动到它。 */
const { highlightId } = useFocusHighlight()
/**
 * 页内定位：点标题后的聚合标签，高亮该状态的全部 MOD 并把第一张居中。
 * 与跨页高亮同一套时长与滚动规则，只是不涉及路由。
 */
const { activeStatus, focusStatus, pulseTarget, pulseSeq } = useStatusHighlight()

/*
 * 固定按名称排序。
 *
 * 原来的「排序」切换入口是右下角浮动按钮，现在该位置改成了「重新扫描
 * 本地 MOD」，排序已无切换入口，所以把配置一起去掉 —— 留一个永远不变的
 * sortBy 只会误导后续维护者。
 */
const busy = ref<Record<string, boolean>>({})
/*
 * 卡片展开状态有两套，规则都在 `useCardExpansion` 里：
 *   - `cardExpansion`：独立 MOD 卡片（外层，可多开）
 *   - `packageExpansion`：包外壳（外层）+ 包内子 MOD / 随包依赖（内层）
 * 拆成两套是因为两层的键空间不同：独立卡的键就是 `dll_file`，包内子卡的键是
 * 「包 key + 文件名」—— 合成一个 record 时同名的 dll_file 会互相顶掉。
 */
const cardExpansion = useCardExpansion()
const packageExpansion = useCardExpansion()

// 切页面复用已有 Dashboard；开关/删除后仍用 load() 取最新扫描结果
onMounted(() => {
  void ensureLoaded()
  // 从地图页「去安装」跳过来：路径已暂存在模块里，直接接着走预览
  const pending = takePendingDrop('mod')
  if (pending) void startDropInstall(pending)
})

/**
 * 本地刷新：重新读取 Dashboard，重扫 BepInEx/plugins 并重算标签。
 *
 * 只碰本地磁盘，不刷新任何仓库 Release —— 仓库刷新在 MOD 安装页。
 * 用户在游戏外手动增删或改名插件后，点这个就能看到最新结果。
 */
const refreshingLocal = ref(false)
async function refreshLocal() {
  if (refreshingLocal.value) return
  refreshingLocal.value = true
  try {
    await load()
    message.success(t('tools.localModsRescanned'))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    refreshingLocal.value = false
  }
}

/**
 * 刷新状态快照：判定函数吃普通对象，避免把响应式细节带进纯逻辑模块。
 */
const refreshSnapshot = computed<RuntimeRefreshState>(() => ({
  refreshing: refreshing.value,
  queued: queued.value,
  sourceRefreshing: sourceRefreshing.value,
  entryStateUnavailable: entryStateUnavailable.value,
}))

/**
 * 每个 MOD 的运行期状态（合成后端扫描结果 + 当前仓库缓存 + 刷新状态）。
 *
 * 判定逻辑在 `modRuntimeStatus`：卡片标签组件（`ModStatusTags`）调的是**同一个**
 * 函数，所以页头聚合与每张卡永远不会一个说「刷新中」、一个说「最新版」。
 */
const runtimeStatuses = computed(() =>
  mods.value.map((mod) => runtimeModStatus(
    mod, mods.value, repositories.value, refreshSnapshot.value,
  )))

/** 列表与卡片实际用的 MOD（`labels` / `update_available` 已按运行期状态重算）。 */
const effectiveMods = computed(() => runtimeStatuses.value.map((entry) => entry.mod))

/**
 * 每个本地 MOD 的关系问题，按 dll_file 索引。
 *
 * 模板、动作处理与卡片标签组件都要读它（判断标签、决定卸载哪个 MOD、
 * 找可安装仓库），关系判定已经算在 `runtimeStatuses` 里，这里只做索引。
 */
const relationIssueByDll = computed<Record<string, RelationStatus>>(() => {
  const map: Record<string, RelationStatus> = {}
  for (const mod of mods.value) {
    const entry = runtimeStatuses.value.find((item) => item.mod.dll_file === mod.dll_file)
    if (entry?.issue) map[mod.dll_file] = entry.issue
  }
  return map
})

/** 列表固定按标题排序（排序切换入口已移除，见上方说明）。 */
const sortedMods = computed(() =>
  [...effectiveMods.value].sort((a, b) => modTitle(a).localeCompare(modTitle(b))),
)

/* ===== 开关快照 =====
 *
 * 页面把当前开关状态发布到 `cardSwitchState`，卡片在折叠插槽里按稳定键读它。
 *
 * 为什么不能只靠 `load()` 换数据：卡片挂在 `n-collapse-item` 的内容区里，
 * 那块内容由 naive-ui 的 `CollapseItemContent` 拿稳定插槽渲染，父级重渲染时
 * 插槽闭包不会重新求值 —— 只有插槽体里读到的响应式值变了才会重渲染。
 * 单卡开关一直正常，是因为插槽读了 `busy[...]`；批量启停只改 `mods`，
 * 插槽里没有任何依赖它，所以开关和详情停在旧状态（实测 25 个开关只有 21 个
 * 更新，包内子卡不动）。早先的做法是换整棵列表的 key 强制重建，
 * 代价是开关被重新挂载：新节点一上来就是目标状态，Naive 的值变化过渡
 * 没有播的机会，用户看到的就是「一瞬间全变了」。
 *
 * 发布快照后，插槽里每张卡的 `cardSwitchOn(...)` 都变了，折叠过渡自己重渲染
 * → 插槽整片重建 → 卡片以同一个 DOM 节点收到新 `value`，过渡照常播。
 *
 * `flush: 'post'` 不能省：要等页面 render 把新数据写进折叠项插槽之后再发布，
 * 否则过渡重渲染拿到的还是上一轮的插槽闭包（旧列表）。
 */
watch(mods, (list) => {
  publishCardSwitches(list.map((mod) => [modSwitchKey(mod.dll_file), mod.enabled] as const))
}, { immediate: true, flush: 'post' })

/* ===== 全部启用 / 全部禁用 =====
 *
 * 这两个动作服务「一次性切换所有 MOD」的场景，入口只能有一个：
 * MOD 管理页的页头。放在总览页是错的 —— 它管理的是 MOD，
 * 离 MOD 列表越近越容易理解后果（页面上每张卡的开关会一起变）。
 *
 * 页头上它是一个二选一分段控件（与发布详情面板「说明 / 下载安装」同款），
 * 不再是一左一右两个各管一个方向的按钮：同一时刻只有「全开 / 全关」
 * 两个互斥状态，分段的选中态天然表达「现在在哪一边、中间是混合」。
 */
const bulkWorking = ref(false)
const enabledModCount = computed(() => effectiveMods.value.filter((mod) => mod.enabled).length)
const allModsEnabled = computed(() =>
  effectiveMods.value.length > 0 && enabledModCount.value === effectiveMods.value.length)

/**
 * 分段控件的选中值：全部启用 → `enable`，全部禁用 → `disable`，
 * 部分启用 → 空串（没有选中项）。
 *
 * 由聚合状态推导，而不是另存「用户最后点了哪个」：
 * 控件表达的是页面当前状态，存一份点击意图会在操作失败、确认框取消
 * 或外部刷新之后和真实状态脱节。
 *
 * 混合态用空串哨兵而不是 `null` / `undefined`：Naive 的 `value` 只收
 * `string | number`，传 `undefined` 会让它退回**非受控**模式 —— 点一下
 * 就自己记住选中项，取消确认框后控件仍停在「全部禁用」上，正好是要避免
 * 的那种假状态。空串则始终是受控的「没有匹配项」。
 */
type BulkSwitchValue = 'enable' | 'disable' | ''
const bulkSwitchValue = computed<BulkSwitchValue>(() => {
  if (!effectiveMods.value.length) return ''
  if (allModsEnabled.value) return 'enable'
  if (!enabledModCount.value) return 'disable'
  return ''
})

/**
 * 分段控件点击：只发起动作，不先改选中态。
 *
 * 禁用方向要过确认框：用户点「取消」时状态没变，控件也不能先跳到
 * 「全部禁用」，否则会让人以为已经生效。
 */
function onBulkSwitch(value: string | number) {
  if (value === 'enable') void setAllEnabled(true)
  else if (value === 'disable') confirmDisableAll()
}

/**
 * 批量切换。
 *
 * 后端整体成功或整体回滚，不会留下半开半关的状态；
 * 禁用方向会弹确认（游戏将不加载任何 MOD，但可随时恢复）。
 */
async function setAllEnabled(enabled: boolean) {
  if (bulkWorking.value || !effectiveMods.value.length) return
  bulkWorking.value = true
  try {
    const result = await api.setAllLocalModsEnabled(enabled)
    if (result.ok) {
      message.success(translateBackendMessage(result.message))
      await load()
    } else {
      message.error(translateBackendMessage(result.message))
    }
  } catch (reason) { fail(reason) } finally { bulkWorking.value = false }
}

function confirmDisableAll() {
  dialog.warning({
    title: t('overview.disableAllMods'),
    content: t('overview.allModsWillBeDisabledTheGameWillLoadNoneOfThemYouCanReEnableThemAnytime',
      { count: effectiveMods.value.length }),
    positiveText: t('overview.disableAll'),
    negativeText: t('recycle.cancel'),
    onPositiveClick() {
      void setAllEnabled(false)
    },
  })
}

/**
 * 按「已安装时绑定的仓库分类」分组。
 * MOD 通过 repository_id / repository_binding 找到对应仓库，取其 group_i18n；
 * 找不到绑定的 MOD 归入「未绑定」分组。
 */
/**
 * 三层结构：分类（仓库分组）→ MOD 包 → 包内 DLL。
 *
 * 包身份用后端给的 `package_id`（等于安装清单的 repository_id），
 * 所以同一个仓库安装出来的多个 DLL 天然属于同一个包。
 * 只有 1 个 DLL 的包不额外套一层，直接把那张 DLL 卡片当作包展示；
 * 2 个及以上才折叠成包，避免无意义的嵌套。
 */
interface ModPackageNode {
  /** 包的稳定键：有 package_id 用它，否则退回单 DLL 的 dll_file。 */
  key: string
  /** 包在 UI 上的标题（单 DLL 包用该 DLL 的标题）。 */
  label: string
  mods: InstalledMod[]
  /** 真正的多 DLL 包才会折叠；单 DLL 包直接渲染卡片。 */
  grouped: boolean
  enabled: boolean
  partial: boolean
  /**
   * 包级状态：以整个包为主体，版本结论只保留一条，不带数量。
   * 见 `statusTags.ts` 的 `packageStatuses`。
   */
  statuses: PackageStatus[]
  /** 包标题的快捷动作（更新 / 刷新），由包状态决定，见 statusActions.ts。 */
  action?: StatusActionSpec
  /** 包内用于跳转的仓库 id（点标签跳到 MOD 安装页对应仓库）。 */
  repositoryId?: string
}

interface ModGroupNode {
  key: string
  label: string
  /** 该分类下的包（已按包聚合）。 */
  packages: ModPackageNode[]
}

const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])

/**
 * 状态变化脉冲：某个 MOD 的标签从「刷新中」变成「最新版 / 可更新」时，
 * 给它的标签组播一次短暂高亮，让异步结果被看见。
 *
 * 只对「变化」触发：首次渲染不播（那是初始状态，不是变化）。
 *
 * 实现上刻意分开两件事：
 *   - watcher 只读 `effectiveMods` 的 dll_file + labels（不读其它 computed），
 *     避免在 Dashboard 尚未就绪时触发更深的依赖链；
 *   - 用 Map 直接存 key，而不是把 dll_file 和 key 拼成一个字符串再切分 ——
 *     Windows 路径可能含冒号，拼串会导致解析错位。
 */
const lastStatusKey = new Map<string, string>()
const pulsingDlls = ref<Record<string, boolean>>({})
const pulseTimers = new Map<string, number>()

/** 供模板调用：该 MOD 的标签是否刚变化过（用于加 pulse 类）。 */
function isStatusPulsing(mod: InstalledMod) {
  return !!pulsingDlls.value[mod.dll_file]
}

function startPulse(dllFile: string) {
  pulsingDlls.value = { ...pulsingDlls.value, [dllFile]: true }
  const existing = pulseTimers.get(dllFile)
  if (existing !== undefined) window.clearTimeout(existing)
  pulseTimers.set(dllFile, window.setTimeout(() => {
    const next = { ...pulsingDlls.value }
    delete next[dllFile]
    pulsingDlls.value = next
    pulseTimers.delete(dllFile)
  }, PULSE_DURATION))
}

watch(
  () => effectiveMods.value.map((mod) => ({
    dll: mod.dll_file,
    key: (mod.labels ?? []).join('|'),
  })),
  (entries) => {
    for (const { dll, key } of entries) {
      const previous = lastStatusKey.get(dll)
      lastStatusKey.set(dll, key)
      // 首次见到这个 MOD，或状态没变：不播
      if (previous === undefined || previous === key) continue
      startPulse(dll)
    }
  },
  { immediate: true },
)

onBeforeUnmount(() => {
  for (const timer of pulseTimers.values()) window.clearTimeout(timer)
  pulseTimers.clear()
})

function pickI18n(map: Record<string, string> | undefined, fallback: string): string {
  if (!map) return fallback
  return map[locale.value] || map['zh-CN'] || Object.values(map)[0] || fallback
}

/**
 * MOD 的分类：优先用安装清单里的分组快照（后端 `group_i18n`）。
 *
 * 分组语义是「安装时绑定的仓库分类」，快照在安装那一刻固化，
 * 所以刚启动、仓库源还没刷新完（config.repositories 可能为空或仍旧）时也不会乱。
 * 快照里没有分组才回退到当前仓库配置；找不到仓库归入「未绑定」。
 */
interface ModCategory { key: string; label: string }
function modCategory(mod: InstalledMod): ModCategory {
  const repo = repositoryForMod(repositories.value, mod)
  /*
   * 分类优先级：
   *   1. 能匹配到在线仓库 → 归该仓库的分组（手动安装的 MOD 只要 GUID/产品名
   *      对得上某个在线仓库，也走这条，和从 MOD 安装页装进来的表现一致）；
   *   2. 匹配不到，但清单是手动安装创建的（`manual-<uuid>`）→「本地安装」；
   *   3. 其余（直接丢进 plugins、没有清单）→「未绑定」。
   * 安装清单里的 group 快照只在「有快照且能匹配仓库」时才使用，
   * 否则会把本地安装错误地显示成空分组。
   *
   * 快照现在只有 i18n 形态（后端已删掉单语 `group` 字段）。
   * 分组键必须与语言无关，所以取 `zh-CN`（或任一可用值）作为规范名，
   * 展示名才用 `pickI18n` 按当前语言取 —— 否则切语言会重建折叠键。
   */
  const snapshot = mod.group_i18n
  const hasSnapshot = !!snapshot && Object.keys(snapshot).length > 0
  if (repo) {
    if (hasSnapshot) {
      const canonical = (snapshot['zh-CN'] || pickI18n(snapshot, '')).trim()
      const label = pickI18n(snapshot, '') || t('dialogs.ungrouped')
      return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
    }
    // key 用语言无关的原始分组名，切换语言时折叠状态不丢
    const canonical = (repo.group || repo.group_i18n?.['zh-CN'] || pickI18n(repo.group_i18n, '')).trim()
    const label = pickI18n(repo.group_i18n, repo.group || '') || t('dialogs.ungrouped')
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  if (mod.manual_install) return { key: '__local_install__', label: t('tools.localInstall') }
  if (hasSnapshot) {
    const canonical = (snapshot!['zh-CN'] || pickI18n(snapshot, '')).trim()
    const label = pickI18n(snapshot, '') || t('dialogs.ungrouped')
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  return { key: '__unbound__', label: t('modsLocal.unbound') }
}

const groups = computed<ModGroupNode[]>(() => {
  /*
   * 先按 package_id 聚成包，再决定整包归属哪个分类。
   *
   * 之前的顺序反了（先按分类分桶、再在桶内聚包）：包内的纯依赖 DLL
   * 没有 BepInPlugin GUID，匹配不到在线仓库，于是进「本地安装」分组；
   * 同包的主 MOD 能匹配到仓库，进在线分组 —— 同一个包被拆到两个分类，
   * 也就永远不会折叠成一个 MOD 包，还会互判「重复」。
   *
   * 一个包只属于一个分类，取包内**最能代表仓库**的成员：
   * 优先能绑定在线仓库的，其次主 MOD，最后才用依赖自身。
   */
  const packagesByKey = new Map<string, InstalledMod[]>()
  for (const mod of sortedMods.value) {
    const packageKey = mod.package_id ? `pkg:${mod.package_id}` : `dll:${mod.dll_file}`
    const list = packagesByKey.get(packageKey)
    if (list) list.push(mod)
    else packagesByKey.set(packageKey, [mod])
  }

  /** 包的分组代表：能匹配仓库的成员优先，主 MOD 其次，依赖最后。 */
  const representative = (members: InstalledMod[]): InstalledMod => {
    const bound = members.find((mod) => repositoryForMod(repositories.value, mod))
    if (bound) return bound
    const primary = members.find((mod) => !mod.is_dependency)
    return primary ?? members[0]
  }

  const buckets = new Map<string, { label: string; packages: Map<string, InstalledMod[]> }>()
  for (const [packageKey, members] of packagesByKey) {
    const category = modCategory(representative(members))
    let bucket = buckets.get(category.key)
    if (!bucket) {
      bucket = { label: category.label, packages: new Map() }
      buckets.set(category.key, bucket)
    }
    bucket.packages.set(packageKey, members)
  }

  const toPackage = (key: string, list: InstalledMod[]): ModPackageNode => {
    /*
     * 主 MOD 在前、依赖 DLL 在后：包内顺序要能一眼看出「哪个是本体、
     * 哪些是它带的库」。依赖表格紧跟在整批子卡片之后（见模板），
     * 于是整段读下来是「本体 → 它带的依赖 → 这个包还要求什么」。
     */
    const ordered = [...list].sort(
      (a, b) => Number(!!a.is_dependency) - Number(!!b.is_dependency),
    )
    const enabledCount = ordered.filter((mod) => mod.enabled).length
    /*
     * 包标题：能认出绑定仓库就用仓库名，让「装的是哪个包」一眼可读；
     * 否则退回安装包名与插件名。整包成员共享一个仓库，所以同一包内
     * 各 DLL 仍由行内插件名区分，不会因为标题统一而重名。
     */
    const label = ordered[0] ? displayTitle(ordered[0]) : ''
    /*
     * 包级状态：以整个包为主体，版本结论只留一条且不带数量
     * （包里只有一个 Release tag，说「1 最新版」会暗示还有别的东西）。
     * 实现见 statusTags.ts 的 packageStatuses。
     */
    const statuses = packageStatuses(ordered)
    /*
     * 包的仓库身份与快捷动作：整包成员共享同一个仓库，
     * 所以直接用「包内能绑定仓库的那一个」——与包标题、分组用的是同一代表。
     * 动作由包对外公开的状态集合解析（与单 MOD 共用 statusActions 的规则）。
     */
    const repo = repositoryForMod(repositories.value, representative(ordered))
    const representativeMod = representative(ordered)
    const action = actionContext(statuses.map((entry) => entry.key), representativeMod)?.action
    return {
      key,
      label,
      mods: ordered,
      grouped: ordered.length >= 2,
      enabled: enabledCount === ordered.length,
      partial: enabledCount > 0 && enabledCount < ordered.length,
      statuses,
      action,
      repositoryId: repo?.id,
    }
  }

  return [...buckets.entries()]
    .sort(([aKey, a], [bKey, b]) => {
      // 「未绑定」「未分组」排最后
      const tails = new Set(['__unbound__', '__ungrouped__'])
      if (tails.has(aKey) !== tails.has(bKey)) return tails.has(aKey) ? 1 : -1
      return a.label.localeCompare(b.label)
    })
    .map(([key, { label, packages }]) => ({
      key,
      label,
      packages: [...packages.entries()].map(([pkgKey, list]) => toPackage(pkgKey, list)),
    }))
})

const allGroupKeys = computed(() => groups.value.map((node) => node.key))
/** 默认全部展开：新出现的分类自动展开，用户的折叠选择单独记住。 */
const collapsedGroups = ref<string[]>([])
const groupsExpanded = computed(() => allGroupKeys.value.filter((key) => !collapsedGroups.value.includes(key)))
function onGroupsExpanded(names: string[]) {
  const open = new Set(names)
  collapsedGroups.value = allGroupKeys.value.filter((key) => !open.has(key))
}

/**
 * 页头聚合标签：按状态汇总整页的**展示单元**数量。
 *
 * 单元 = 页面上能独立点选的那个东西：单 MOD 一张卡片，多 DLL 包是整包
 * 一个单元。包里子 DLL 的状态不对外公开 —— 一个包只有一个版本结论
 * （`packageStatuses`），所以两个 DLL 都最新时这里计 1，而不是 2。
 *
 * 这也让聚合数量与页内定位一致：点「N 最新版」高亮的是 N 张卡片
 * （单 MOD 卡片 + 包外壳），不会把包内某一行单独高亮出来。
 */
const statusSummaryTags = computed<StatusSummary[]>(() =>
  summarizeUnits(statusUnitNodes.value.map((unit) => ({ keys: unit.keys }))),
)

/**
 * 开关状态的 key：页头把它们单独成组，与结论类标签分开。
 *
 * 「已启用 / 已禁用」回答的是「现在开没开」，「有更新 / 缺依赖 / 最新版」
 * 回答的是「这个版本怎么样」。排在同一排里读起来像同一类结论。
 */
const TOGGLE_STATUS_KEYS = ['enabled', 'disabled'] as const

const isToggleStatus = (key: string) =>
  (TOGGLE_STATUS_KEYS as readonly string[]).includes(key)

/** 结论类标签（有更新 / 缺依赖 / 最新版…）。 */
const statusTagsPrimary = computed(() =>
  statusSummaryTags.value.filter((entry) => !isToggleStatus(entry.key)),
)

/** 开关状态标签：固定「已启用 → 已禁用」的顺序，数量为 0 的不显示。 */
const statusTagsToggle = computed(() =>
  TOGGLE_STATUS_KEYS.map((key) => statusSummaryTags.value.find((entry) => entry.key === key)).filter(
    (entry): entry is StatusSummary => !!entry,
  ),
)

/** 卡片标题：优先 MOD 包名（同一包内的子 MOD 显示同一个包标题），没有包则用插件名。 */
/**
 * 绑定仓库的本地化名称；未绑定返回 undefined。
 *
 * 用户认识的是「装的是哪个包」，而不是 DLL 文件名 ——
 * 本地直装、手动安装或仓库改名后，DLL 名往往还是一串技术命名，
 * 所以只要认出对应仓库，标题就以仓库名为准。
 */
function boundRepositoryName(mod: InstalledMod): string | undefined {
  const repo = repositoryForMod(repositories.value, mod)
  if (!repo) return undefined
  return pickI18n(repo.name_i18n, repo.name).trim() || undefined
}

/**
 * 包内 DLL 的标题：插件名优先（让同包的不同 DLL 能区分开）。
 * 包名由外层「包标题行」负责展示，这里不再重复；
 * 同一个包里的 DLL 都绑同一个仓库，若这里也换成仓库名就全重名了。
 */
function modTitle(mod: InstalledMod) {
  return mod.plugin_name?.trim() || mod.title || mod.product_name || mod.name
}

/**
 * 单 DLL 包 / 包标题行的名称：绑定仓库名优先，
 * 再退回安装包名（多语言快照）与实际插件名。
 *
 * 包名快照带 `{仓库名} - {平台}` 后缀，所以它排在纯仓库名之后：
 * 有当前仓库配置时用干净的仓库名，仓库已下线时才用安装时的快照。
 */
function displayTitle(mod: InstalledMod) {
  return boundRepositoryName(mod)
    || pickI18n(mod.package_name_i18n, mod.package_name ?? '').trim()
    || modTitle(mod)
}

/**
 * 单 DLL 包的标题：那张卡片既是包本身也是唯一成员，
 * 所以优先用包名（与多 DLL 包的标题行保持同一语义）。
 */
function singleModTitle(mod: InstalledMod) {
  return displayTitle(mod)
}

function fail(reason: unknown) {
  message.error(backendMessageOf(reason))
}

async function toggle(mod: InstalledMod, enabled: boolean) {
  busy.value[mod.dll_file] = true
  try {
    const result = await api.toggleLocalMod(mod.dll_file, enabled)
    result.ok
      ? message.success(result.message ? translateBackendMessage(result.message) : (enabled ? t('modsLocal.enabled2') : t('modsLocal.disabled')))
      : message.error(translateBackendMessage(result.message))
    await load()
  } catch (reason) { fail(reason) } finally { busy.value[mod.dll_file] = false }
}

/**
 * 冲突的「去卸载」：确认后卸掉关系里指名的那个 MOD。
 *
 * 只卸冲突对方，不动声明方 —— 声明方是用户想留下的那个（它是当前看板
 * 卡片的归属），卸错了就要重新装一遍。多个 MOD 同时冲突时也只处理
 * 当前这条关系，逐个解决比一次卸一堆更可控。
 */
function uninstallConflict(mod: InstalledMod, target: InstalledMod) {
  remove(target, {
    title: t('modsOnline.uninstallConflictingMod'),
    body: (name) => t('modsOnline.isIncompatibleWithAndMustBeUninstalledContinue',
      { name, owner: modTitle(mod) }),
  })
}

/**
 * 展开后的滚动（策略见 `expandScroll.ts`）：顺手记下这次滚动的起止，
 * 收起时按它原样倒回去。
 *
 * 只有「用户自己点的这一张」滚：这里没有监听 `expanded`，所以批量收起、
 * 重新渲染、跨页定位都不会顺带触发一次滚动。
 */
function scrollAfterExpand(card: HTMLElement | null | undefined, anchorSelector: string, bodySelector: string) {
  if (!card) return
  const anchor = card.querySelector(anchorSelector)
  if (!anchor) return
  void nextTick().then(() => {
    scrollExpandedCardIntoView({ card, anchor, body: card.querySelector(bodySelector) })
  })
}

/**
 * 收起一张卡片：把状态改动交给 `collapseCardWithScroll` 里的回调。
 *
 * 顺序由它保证：先同步量折叠前的位置，再改状态让详情开始折，最后把展开那次滚动
 * 倒回去（两者同时发生）。调用方自己先改状态的话，量到的就是折叠后的位置。
 */
function collapseWithScroll(card: HTMLElement | null | undefined, bodySelector: string, collapse: () => void) {
  if (!card) {
    collapse()
    return
  }
  collapseCardWithScroll({ card, body: card.querySelector(bodySelector) }, collapse)
}

/**
 * 点击卡片顶栏展开/收起详情。
 *
 * 详情区不再阻止冒泡（点它就是「点空白」，由 onDocumentClick 收起），
 * 所以这里必须排除来自 `.card-detail` 的事件，否则点详情会先被
 * onCardClick 切换一次、再被文档监听收起，表现为「闪一下就没了」。
 *
 * 同时要告诉 onDocumentClick「这次点击已处理」：否则同一次点击里
 * 本函数先把该项 toggle 开，文档监听紧接着把所有展开项置回 false，
 * 内容只出现一帧就消失（折叠过渡动画还没播完就被打断）。
 */
let cardClickHandled = false

function onCardClick(event: MouseEvent, mod: InstalledMod, packageKey?: string) {
  const target = event.target
  if (target instanceof Element && target.closest('.card-detail')) return
  cardClickHandled = true
  const card = event.currentTarget instanceof HTMLElement ? event.currentTarget : null
  /*
   * 独立 MOD 卡片与包内子卡片分属两套状态（键空间不同，见 `cardExpansion` 的
   * 注释），但交互必须完全一致：展开时滚到可见，收起时把展开那次滚动倒回去。
   * 传了包 key 说明这是包内子卡，否则是独立卡。
   */
  const open = packageKey === undefined
    ? cardExpansion.isOuterOpen(mod.dll_file)
    : packageExpansion.isInnerOpen(packageKey, mod.dll_file)
  const toggle = () => {
    if (packageKey === undefined) cardExpansion.toggleOuter(mod.dll_file)
    else packageExpansion.toggleInner(packageKey, mod.dll_file)
  }
  if (open) {
    collapseWithScroll(card, '.card-detail', toggle)
    return
  }
  toggle()
  scrollAfterExpand(card, '.card-row', '.card-detail')
}

/**
 * 点击详情之外的空白收起已展开的 MOD。
 *
 * 「空白」的范围包含详情区自身：用户展开卡片后，点详情里的文字/标签/值
 * 也算点空白，应当收起（详情内容是只读的，没有需要保留点击的元素）。
 * 判定规则是「点击目标是否属于本行顶栏」：
 *   - `.card-row`（单 MOD 行的开关/标题/标签/工具按钮）
 *   - `.package-head`（包标题行）
 *   - `.tool-group` / `.card-switch`（按钮与开关自身）
 * 落在这些里面说明用户是在操作这一行，交给各自的 @click.stop / 处理函数，
 * 不在这里收起，避免一次点击既展开又收起。
 *
 * 收起后只有「同时只展开了这一张」时把展开那次滚动倒回去：一次收掉多张时
 * 没有哪一张是用户正在看的那张，滚动只会把视图甩到别处。
 */
function onDocumentClick(event: MouseEvent) {
  /*
   * 卡片自身的点击已由 onCardClick 处理（它会 toggle），
   * 这里必须让路，否则同一次点击会「先展开、再全部收起」。
   * 标记在 document 监听执行后清除，保证只影响当前这一次点击。
   */
  if (cardClickHandled) {
    cardClickHandled = false
    return
  }
  /*
   * 展开中的卡片来自两套状态：独立 MOD 卡（外层）与包内子卡（内层）。
   * 包外壳本身不参与「点空白收起」—— 点包标题行以外的地方只收子卡，包保持
   * 展开（与改动前一致）；所以这里用 `closeAllInner` 而不是 `closeAll`。
   */
  const openCards = [
    ...cardExpansion.outerKeys.value,
    ...packageExpansion.innerKeys.value.map((key) => packageExpansion.childOf(key)),
  ]
  if (!openCards.length) return
  const target = event.target
  if (target instanceof Element && target.closest('.card-row, .package-head')) return
  const collapseAll = () => {
    cardExpansion.closeAll()
    packageExpansion.closeAllInner()
  }
  if (openCards.length !== 1) {
    collapseAll()
    return
  }
  const card = document.querySelector<HTMLElement>(`[data-dll-file="${CSS.escape(openCards[0])}"]`)
  collapseWithScroll(card, '.card-detail', collapseAll)
}
onMounted(() => {
  document.addEventListener('click', onDocumentClick)
})
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClick)
})

/**
 * MOD 的状态标签。
 *
 * 传入的 mod 若来自 `effectiveMods`，其 labels 已是「后端扫描 + 当前仓库缓存」
 * 合成后的结果，所以刷新期间也会立刻反映最新版本判断。
 */
function modTags(mod: InstalledMod) {
  return installedModTags(mod).map((tag) => ({ ...tag, text: t(tag.text) }))
}
function statusSummary(mod: InstalledMod) {
  return modTags(mod).map((tag) => tag.text).join(' · ')
}

/** 该 MOD 绑定仓库的 id（优先安装清单，其次身份绑定），用于卡片的跨页定位。 */
function bindingRepositoryId(mod: InstalledMod): string | undefined {
  return repositoryForMod(repositories.value, mod)?.id
}

/*
 * 标签点下去跳哪个仓库的逻辑已经下沉到 `tagTargets.ts`：
 * 卡片标签由 `ModStatusTags` 组件自己渲染并处理点击（原因见该组件注释），
 * 页面不再需要自己算一遍跳转目标。
 */

/**
 * 某个状态对应的按钮能不能真的执行。
 *
 * 动作种类由 `statusActionFor` 决定（可更新 → 更新、刷新失败 → 刷新…），
 * 但「能不能做」还要看实际条件，这些条件各状态不同，所以留在视图层：
 *   - update：必须有可安装的仓库，且不是仓库文件模式（无版本可比）；
 *   - refresh：必须有绑定的仓库 id（否则不知道该刷哪一条）；
 *   - uninstall-conflict：必须解析出要卸载的那个 MOD。
 *
 * 返回 undefined 表示这个状态不适合给按钮（例如未绑定、依赖 DLL），
 * 卡片就不留按钮位，避免出现点了没反应的死按钮。
 */
function actionContext(labels: readonly string[] | undefined, mod: InstalledMod): {
  action: StatusActionSpec
  repository?: Repository
  /** 冲突要卸载的目标：可能是别的 MOD，不一定是当前这张卡片。 */
  conflictMod?: InstalledMod
} | undefined {
  const action = statusActionFor(labels)
  if (!action) return undefined
  // 纯依赖 DLL 没有版本语义，任何快捷动作都没有意义
  if (mod.is_dependency) return undefined

  const repository = repositoryForMod(repositories.value, mod)
  switch (action.kind) {
    case 'update':
      // 没有仓库或 RAW 仓库：点下去只会下载一份和本地一样的文件
      if (!repository || repository.download_mode === 'repository_file') return undefined
      return { action, repository }
    case 'refresh':
      // 刷新针对仓库而不是某个 DLL，解析不出仓库就不给按钮
      if (!repository) return undefined
      return { action, repository }
    case 'install-dependency': {
      /*
       * 「去安装」要跳到**依赖的**仓库，不是这个 MOD 自己的仓库。
       *
       * 之前这里用的是 `repositoryForMod(mod)` —— 当前卡片的仓库，
       * 于是点了「去安装」，安装页高亮的却是这张本身有问题的 MOD，
       * 用户在那里找不到要装的东西。
       *
       * 关系解析不出可安装来源时不给按钮：那说明线上没有这个 GUID，
       * 跳过去也无处可去，只留「缺依赖」标签说明情况。
       */
      const dependency = relationIssueByDll.value[mod.dll_file]
      if (!dependency?.repository) return undefined
      return { action, repository: dependency.repository }
    }
    case 'uninstall-conflict': {
      /*
       * 冲突要卸的是**对方**，不一定是当前卡片这一项：
       * A 声明「与 B 1.x 冲突」时，用户看的是 A 的卡片，要卸的是 B。
       * 解析不出具体目标就不给按钮 —— 一个不知道该卸什么的按钮没意义。
       */
      const conflictMod = relationIssueByDll.value[mod.dll_file]?.installedMod
      if (!conflictMod) return undefined
      return { action, repository, conflictMod }
    }
  }
}

/** 单 MOD 卡片的快捷动作。 */
function cardAction(mod: InstalledMod) {
  return actionContext(mod.labels as string[] | undefined, mod)
}

/**
 * 按 DLL 预计算动作：模板里要读 4 次（判断、配色、文案、图标），
 * 每次都重跑仓库匹配太浪费，派生一次即可。键是 dll_file。
 */
const actionByDll = computed<Record<string, StatusActionSpec>>(() => {
  const map: Record<string, StatusActionSpec> = {}
  for (const mod of effectiveMods.value) {
    const context = cardAction(mod)
    if (context) map[mod.dll_file] = context.action
  }
  return map
})

/**
 * 包内携带依赖关系声明的成员（可能不存在）。
 *
 * 依赖关系挂在仓库上，包内只有绑了仓库的那个 DLL 才解析得出 `relationIssue`。
 * 缺依赖 / 冲突的标签、按钮和跳转都必须按这个成员来，否则代表成员恰好是
 * 那个没有仓库身份的 DLL 时，标签在、动作和跳转却解析不出来。
 */
function relationOwnerMod(pkg: ModPackageNode): InstalledMod | undefined {
  return pkg.mods.find((mod) => relationIssueByDll.value[mod.dll_file])
}

/** 包标题的快捷动作：用包对外公开的状态集合判断。 */
function packageAction(pkg: ModPackageNode) {
  const keys = pkg.statuses.map((entry) => entry.key)
  const base = pkg.mods.find((mod) => !mod.is_dependency) ?? pkg.mods[0]
  if (!base) return undefined
  // 依赖类动作按「真正部署了关系的那一个成员」解析
  const needsRelation = keys.some(
    (key) => key === 'missing_dependency' || key === 'dependency_conflict',
  )
  return actionContext(keys, (needsRelation ? relationOwnerMod(pkg) : undefined) ?? base)
}

/**
 * 执行快捷动作。
 *
 * 三个动作走的是不同链路，这是它们唯一的分歧点：
 *   - update：预下载 → 安装详情弹窗 → 用户确认（与仓库卡片安装同一套）
 *   - refresh：只重拉这一条仓库的 Release，不下载任何文件
 *   - install-dependency：跳到 MOD 安装页并定位到该仓库卡片
 *   - uninstall-conflict：确认后卸载关系里指名的那个 MOD
 */
function runCardAction(mod: InstalledMod) {
  const context = cardAction(mod)
  if (!context) return
  if (context.action.kind === 'update' && context.repository) {
    void previewRepository(context.repository)
  } else if (context.action.kind === 'install-dependency' && context.repository) {
    // 缺依赖：跳到 MOD 安装页并定位到该仓库卡片，由那边给出「安装依赖」
    linkToRepository(context.repository.id)
  } else if (context.action.kind === 'uninstall-conflict' && context.conflictMod) {
    uninstallConflict(mod, context.conflictMod)
  } else if (context.repository) {
    void refreshRepository(context.repository)
  }
}

function runPackageAction(pkg: ModPackageNode) {
  const context = packageAction(pkg)
  if (!context) return
  const first = pkg.mods.find((mod) => !mod.is_dependency) ?? pkg.mods[0]
  if (context.action.kind === 'uninstall-conflict' && context.conflictMod) {
    // 要卸的是「对方」，但当前这一方必须是真正声明关系的成员
    const owner = relationOwnerMod(pkg) ?? first
    if (owner) uninstallConflict(owner, context.conflictMod)
    return
  }
  if (!context.repository) return
  if (context.action.kind === 'update') void previewRepository(context.repository)
  else if (context.action.kind === 'install-dependency') linkToRepository(context.repository.id)
  else void refreshRepository(context.repository)
}

/** 单仓刷新：复用刷新引擎，只更新这一条仓库的缓存。 */
async function refreshRepository(repo: Repository) {
  // 用户主动点刷新 → 强制请求，不吃 5 分钟 Release 缓存
  const error = await refreshOne(repo, true)
  if (error) message.error(error)
}

/* ===== 页内定位：点标题后的聚合标签 ===== */

/** 卡片锚点：聚合标签按状态定位时用它找 DOM，与跨页高亮的锚点同一套命名。 */
function modAnchor(mod: InstalledMod) {
  return mod.dll_file
}

/**
 * 按状态定位时的「单元」：单 MOD 一个，多 DLL 包整包一个。
 *
 * 包是页面上的一个整体（一个外壳、一条标题、一个版本结论），
 * 点它的状态标签时应当高亮这个外壳，而不是把包内某个 DLL 行单独
 * 描出来 —— 那样看起来像「这个 DLL 有问题」，而实际结论属于整个包。
 */
interface StatusUnitNode {
  /** 稳定的 DOM 锚点（单 MOD 用 dll_file，包用包键）。 */
  anchor: string
  /** 命中时的状态集合，与 `packageStatuses` / 单 MOD 标签同源。 */
  keys: string[]
  /** 该单元所在的分类与包键，用于定位前展开折叠容器。 */
  groupKey: string
  packageKey: string
  /** 单元内的成员（单 MOD 或包内全部 DLL），供分类/包展开使用。 */
  mods: InstalledMod[]
}

/**
 * 一个单元的开关状态 key（`enabled` / `disabled`）。
 *
 * 页头把「N 已启用 / N 已禁用」单独成组，与结论类标签分开；这两个 key 也
 * 参与定位，所以直接并进单元的 keys —— `unitsWithStatus` 与聚合汇总共用
 * 同一份数据，不必另写一套命中判定。
 *
 * 半开半关的包两边都不算：说它「已启用」会掩盖还有 DLL 被停用的事实。
 */
function enableKeysOf(mods: readonly InstalledMod[]): string[] {
  if (mods.every((mod) => mod.enabled)) return ['enabled']
  if (mods.every((mod) => !mod.enabled)) return ['disabled']
  return []
}

const statusUnitNodes = computed<StatusUnitNode[]>(() => {
  const nodes: StatusUnitNode[] = []
  for (const group of groups.value) {
    for (const pkg of group.packages) {
      if (pkg.grouped) {
        nodes.push({
          anchor: `unit:${pkg.key}`,
          keys: [...pkg.statuses.map((entry) => entry.key), ...enableKeysOf(pkg.mods)],
          groupKey: group.key,
          packageKey: pkg.key,
          mods: pkg.mods,
        })
        continue
      }
      // 单 DLL 包：这一张卡片自己就是单元，标签按成员自己的 labels 判断
      for (const mod of pkg.mods) {
        if (mod.is_dependency) continue
        nodes.push({
          anchor: modAnchor(mod),
          /*
           * 取「卡片实际渲染的标签键」，而不是裸 `mod.labels`。
           *
           * 两者不等价：`installedModTags()` 会**动态补上** `prerelease`
           * （版本号带 -beta / 四段序号时），它不在后端 labels 里。
           * 用 labels 作命中键时，页头聚合会少一个「预发布」，
           * 就算它出现了，点了也定位不到任何卡片。
           */
          keys: [...installedModTags(mod).map((tag) => tag.key), ...enableKeysOf([mod])],
          groupKey: group.key,
          packageKey: pkg.key,
          mods: [mod],
        })
      }
    }
  }
  return nodes
})

/** 命中该状态的全部单元（按页面展示顺序）。 */
function unitsWithStatus(statusKey: string) {
  return statusUnitNodes.value.filter((unit) => unit.keys.includes(statusKey))
}

/**
 * 当前页内定位的状态是否命中该单元。
 *
 * 单 MOD 卡片传自己的锚点，包外壳传 `unit:<包键>`；因此包内子 DLL 行
 * 不会因为「包命中了」而被逐个高亮。命中集合按状态缓存成 Set，
 * 避免每张卡片渲染时都重新扫一遍全部单元。
 */
const activeStatusAnchors = computed(() => {
  const status = activeStatus.value
  if (!status) return new Set<string>()
  return new Set(unitsWithStatus(status).map((unit) => unit.anchor))
})

function matchesActiveStatus(anchor: string) {
  return activeStatusAnchors.value.has(anchor)
}

/** 当前轮到的命中项序号：重复点击同一标签时逐张轮换，而不是每次回到第一张。 */
const statusCursor = ref<{ key: string; index: number }>({ key: '', index: 0 })

/**
 * 点聚合标签：高亮该状态的全部单元，并把下一张命中项滚到视口中间。
 *
 * 命中项可能分布在收起的分类里：先把它们全部展开，
 * 展开带 0.3s 高度动画，所以滚动要等动画结束（由 focusStatus 负责）。
 */
function locateStatus(statusKey: string) {
  const matches = unitsWithStatus(statusKey)
  if (!matches.length) return

  const sameTag = statusCursor.value.key === statusKey
  const start = sameTag ? (statusCursor.value.index + 1) % matches.length : 0
  statusCursor.value = { key: statusKey, index: start }
  const anchor = matches[start].anchor

  const groupsOpened = expandGroupsFor(matches)
  focusStatus(
    statusKey,
    () => document.querySelector<HTMLElement>(`[data-mod-anchor="${CSS.escape(anchor)}"]`),
    groupsOpened,
    anchor,
  )
}

/** 展开命中单元所在的分类；返回是否真的展开了收起的分类。 */
function expandGroupsFor(matches: StatusUnitNode[]) {
  const keys = new Set(matches.map((unit) => unit.groupKey))
  const before = collapsedGroups.value.length
  collapsedGroups.value = collapsedGroups.value.filter((key) => !keys.has(key))
  return collapsedGroups.value.length !== before
}

/**
 * 点击状态标签：跳到 MOD 安装页并高亮**该标签指向的**仓库卡片。
 *
 * 目标是 `tagTargetRepositoryId` 算出来的那个 id，所以「缺依赖」去的是
 * 依赖的仓库，其余标签去的是这个 MOD 自己的仓库。
 */
/**
 * 包标签点下去该跳到哪个仓库。
 *
 * 与单 MOD 同一套规则，只是「依赖类标签」要取**包内任一成员**解析出的
 * 依赖仓库 —— 包是多个 DLL 的聚合，缺依赖的可能是其中某一个。
 */
function packageTagTargetId(pkg: ModPackageNode, tagKey: string): string | undefined {
  if (tagKey === 'missing_dependency' || tagKey === 'dependency_conflict') {
    for (const mod of pkg.mods) {
      const repository = relationIssueByDll.value[mod.dll_file]?.repository
      if (repository) return repository.id
    }
    return undefined
  }
  return pkg.repositoryId
}

function openPackageTag(pkg: ModPackageNode, tagKey: string) {
  const id = packageTagTargetId(pkg, tagKey)
  if (id) linkToRepository(id)
}

/** 包标签的悬停提示：与单 MOD 卡片同一套文案（依赖类单独说明）。 */
function packageTagTitle(pkg: ModPackageNode, tagKey: string): string | undefined {
  return tagTargetTitle(tagKey, packageTagTargetId(pkg, tagKey))
}

/* ===== 依赖表 ===== */

/**
 * 单 MOD 卡片的依赖表行。
 *
 * 「用哪份关系声明、目标叫什么、满没满足」全部由 `installedModDependencyRows`
 * 决定（四个页面共用一份口径，见 `dependencyRows.ts`）：绑定仓库还在配置里
 * 就以线上声明为准，仓库下线了才退回安装清单快照。页面不重算这些，否则同一条
 * 声明在安装页和管理页会给出两种结论。
 *
 * 刻意不做缓存：行依赖 `mods` / `repositories` 两个 Dashboard ref，函数里读
 * `.value` 本身就是响应式的；另立一份按 dll_file 的缓存，就得在每次刷新后
 * 额外失效一次，漏一处就会把上一次的满足情况留在界面上。一次解析只有几条关系。
 */
function modDependencyRows(mod: InstalledMod): DependencyRow[] {
  // 纯依赖 DLL 不是声明方：关系挂在仓库/包上，给它单独画一张表只是噪音
  if (mod.is_dependency) return []
  return installedModDependencyRows(mod, {
    mods: mods.value,
    repositories: repositories.value,
    translate: t,
    pickName: pickI18n,
  })
}

/**
 * 多 DLL 包外壳的依赖表行。
 *
 * 包的关系声明挂在仓库上、整包共用，所以整包只出一张表，放在整批子卡片**下面**
 * （先看包里有什么，再看这个包对外要求什么）；子卡片不再各自显示，同一段声明
 * 重复 N 次只是噪音。
 *
 * 代表成员取「非依赖且绑定了仓库」的那个：纯依赖 DLL 的清单快照里没有声明，
 * 直接拿 `pkg.mods[0]` 当代表时，若它恰好是依赖 DLL（DIYLevel 这类包很常见），
 * 整包的表会凭空消失。口径与 `packageAction` 取 base 一致。
 */
function packageDependencyRows(pkg: ModPackageNode): DependencyRow[] {
  const owner = pkg.mods.find((mod) => !mod.is_dependency && !!repositoryForMod(repositories.value, mod))
    ?? pkg.mods.find((mod) => !mod.is_dependency)
    ?? pkg.mods[0]
  return owner ? modDependencyRows(owner) : []
}

/**
 * 依赖表状态标签的跳转：可安装 / 可更新的行落到 MOD 安装页那张仓库卡片。
 *
 * 与卡片上「缺依赖」标签走同一个入口（`linkToRepository` →
 * `#/mods-online?focus=<id>`）；缺依赖、冲突这类线上没有解法的行不带
 * `jump`，标签本来就不是可点样式，这里也就不可能被调用。
 */
function openDependencyJump(row: DependencyRow) {
  const id = row.jump?.id
  if (id) linkToRepository(id)
}

/** 绑定仓库展示：provider + owner/repo（@tag）。 */
function bindingText(mod: InstalledMod) {
  const binding = mod.repository_binding
  if (!binding) return mod.repository_id ? `#${mod.repository_id}` : '—'
  const tag = binding.tag ? `@${binding.tag}` : ''
  return `${binding.provider}: ${binding.owner}/${binding.repository}${tag}`
}

/** 包成员数文案。 */
function packageCountText(mod: InstalledMod) {
  return t('rename.text', { count: mod.package_mod_count ?? 0 })
}

/* ===== 工具栏：定位目录 / 配置编辑 ===== */

/** 配置编辑器弹窗（同一时刻只编辑一个 MOD）。 */
const configOpen = ref(false)
const configTarget = ref<InstalledMod | null>(null)

/**
 * 工具栏列宽：四个按钮位（日志、配置、定位、删除）等间距排布。
 *
 * 之前用 `.tool-spacer { flex: 1 1 auto }` 把「可选项」和「固定按钮」推成
 * 两端对齐，导致配置与定位之间比其它相邻按钮宽一截，四个图标看起来疏密
 * 不匀。现在去掉弹性占位，全部靠统一的 gap 分配，四个按钮等距。
 *
 * 缺失的可选项仍用同宽占位补上，所以「打开位置」「删除」在任何行都落在
 * 同一条竖线上。
 */
const TOOL_BUTTON_WIDTH = 30
const TOOL_GAP = 8
/** 4 个 30px 按钮 + 3 个 gap（相邻按钮之间的空档，两端不加）。 */
const toolColumnWidth = computed(
  () => 4 * TOOL_BUTTON_WIDTH + 3 * TOOL_GAP,
)
/**
 * 把按钮宽度与间距以 CSS 变量下发，让样式和上面的列宽计算共用同一组数字。
 * 写死在两处（JS 算列宽、CSS 排间距）迟早会漂移。
 */
const toolGroupStyle = computed(() => ({
  minWidth: `${toolColumnWidth.value}px`,
  '--tool-gap': `${TOOL_GAP}px`,
  '--tool-button-width': `${TOOL_BUTTON_WIDTH}px`,
}))

/** 打开该 MOD 所在文件夹并选中文件（后端只允许当前游戏的 BepInEx/plugins）。 */
async function openLocation(mod: InstalledMod) {
  try {
    const result = await api.openModFileLocation(mod.dll_file)
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) { fail(reason) }
}

/** 打开配置编辑器（仅对存在配置文件的 MOD 可用）。 */
function editConfig(mod: InstalledMod) {
  configTarget.value = mod
  configOpen.value = true
}

/** 打开该 MOD 在 BepInEx/logs/<guid> 下的日志目录（仅 has_logs 的 MOD 显示按钮）。 */
async function openLogs(mod: InstalledMod) {
  try {
    const result = await api.openModLogsDirectory(mod.dll_file)
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) { fail(reason) }
}

/* ===== 页头操作：打开 MOD 目录 / 安装 MOD ===== */

/** 打开当前厨房的 BepInEx/plugins 目录（只读操作，运行中也可用）。 */
async function openModsDirectory() {
  try {
    const result = await api.openModDirectory()
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) { fail(reason) }
}

/**
 * 安装流程：来源选择 → 预览 → 确认安装。
 *
 * 三段都由共用组件承担（InstallSourceCard / ModInstallDialog），
 * 本页只持有流程状态；暂存清理在流程 composable 里、拖放监听在页面
 * （`useFileDrop`：拖放是窗口能力，不归来源卡片）。
 */
const {
  sourceOpen: installSourceOpen,
  sourceBusy: installSourceBusy,
  previewOpen: installPreviewOpen,
  preview: installPreview,
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
  installSourceOpen.value = true
  await handleInstallSourceSelect({ kind: 'local', value: path })
}

const { acceptDrop } = useDropRouting()

/**
 * 整窗拖放：拖 DLL / ZIP 进来等价于点「安装 MOD」。
 *
 * 与 MOD 安装页同一份实现、同一份扩展名规则：本页没有「安装」按钮，拖放是
 * 本页的安装入口，所以它必须由页面注册而不是躲在来源卡片里。
 *
 * 先过 `acceptDrop`：含 `info` 的 ZIP 是地图包（哪怕它也带 DLL），在这里装
 * 只会得到一个空预览 + 一张没装上的地图，必须引导到地图安装页。
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

/**
 * MOD 卡片的「更新」按钮：和仓库卡片的安装共用同一条预览链路
 * （preview_only 下载 → 安装详情弹窗 → 用户确认后才写入）。
 */
const previewing = ref<Record<string, boolean>>({})
const { previewRepository } = useRepositoryPreview(openInstallPreview, {
  gameBlocked: () => {
    if (!game.value?.kitchen_running) return false
    message.warning(t('modsOnline.theGameIsRunningCloseItBeforeInstalling'))
    return true
  },
  onBusyChange: (repositoryId, busy) => {
    if (busy) previewing.value[repositoryId] = true
    else delete previewing.value[repositoryId]
  },
  /*
   * 更新也要走安装前检查。
   *
   * 之前这里不传 `prepareInstall`，理由是「依赖在首次安装时就该齐了」。
   * 这个假设不成立：依赖可能被用户单独删掉、被别的包顶掉，或者新版本
   * 增删了依赖 —— 更新一个新增了依赖的版本，装完照样跑不起来。
   * 与安装页共用同一份门控（`useInstallPlanGate`），所以判定与弹窗一致。
   */
  prepareInstall: (repo, tag) => runInstallPlan(repo, tag),
})

/* ===== 安装前检查（更新时的依赖 / 冲突 / 兼容） ===== */

const {
  open: installPlanOpen,
  plan: installPlan,
  target: installPlanTarget,
  run: runInstallPlan,
  finish: finishInstallPlan,
} = useInstallPlanGate({
  mods: () => mods.value,
  repositories: () => repositories.value,
  gameBlocked: () => {
    if (!game.value?.kitchen_running) return false
    message.warning(t('modsOnline.theGameIsRunningCloseItBeforeInstalling'))
    return true
  },
  onApplied: () => load(),
})

/* ===== 包级操作 ===== */

/*
 * 包的折叠状态在 `packageExpansion` 里（与包内子卡同一份状态，收起时一起清）。
 * 只记「用户展开过的包」而不是「收起过的包」：默认收起时，新扫出来的包不在
 * 集合里，本身就是默认收起，不需要额外登记，也就不会漏。
 */

/**
 * 包外壳的展开 / 收起。
 *
 * 滚动策略与单 MOD 卡片同一套（`expandScroll.ts`）：包 body 撑开后优先让整包可见，
 * 收起时把展开那次滚动倒回去。收起必须交给 `collapseWithScroll`：它先量折叠前的
 * 位置再改状态，自己先改就量不到了。
 */
function togglePackage(node: ModPackageNode, event: MouseEvent) {
  const opening = !packageExpansion.isOuterOpen(node.key)
  const head = event.currentTarget
  const shell = head instanceof Element ? head.closest('.package-shell') : null
  const card = shell instanceof HTMLElement ? shell : null
  if (!opening) {
    // 收起连带清掉包内子卡的展开状态（`useCardExpansion` 的规则）：
    // 下次展开是一个全新的包，子卡要重新点开才展开。
    collapseWithScroll(card, '.package-body', () => packageExpansion.closeOuter(node.key))
    return
  }
  packageExpansion.openOuter(node.key)
  scrollAfterExpand(card, '.package-head', '.package-body')
}

/**
 * 跨页 focus 落点：目标是一个多 DLL 包时先把它展开。
 *
 * 包默认收起，而 `?focus=` 只带一个仓库 id（总览页「本地 MOD 可更新 → 去更新」、
 * MOD 安装页的状态标签都走这条路径）。不展开的话用户跳过来只看到一个收起的壳，
 * 看不到真正要更新的那几行 —— 深链必须落到可见内容上。
 *
 * 只对包外壳生效：命中单 MOD 卡片时它的内容本来就直接可见，这里不改任何包的
 * 折叠状态（用户手动收起过的包不该因为一次跳转被重新摊开）。
 *
 * 同时监听 `groups`：focus 在页面挂载时就已经写进 `highlightId`，而 Dashboard
 * 可能还没加载完；只听 highlightId 会错过「数据到齐」那一刻。
 */
watch([highlightId, groups], () => {
  const id = highlightId.value
  if (!id) return
  const hits = groups.value
    .flatMap((group) => group.packages)
    .filter((pkg) => pkg.grouped && pkg.repositoryId === id)
    .map((pkg) => pkg.key)
  // `openOuter` 对已展开的包是空操作，不必先算差集
  for (const key of hits) packageExpansion.openOuter(key)
}, { immediate: true })

/** 包级开关：走后端已有的 toggle_local_mod_package，一次切换整包。 */
async function togglePackageEnabled(node: ModPackageNode, enabled: boolean) {
  const packageId = node.mods[0]?.package_id
  if (!packageId) return
  for (const mod of node.mods) busy.value[mod.dll_file] = true
  try {
    const result = await api.toggleLocalModPackage(packageId, enabled)
    if (result.ok) {
      /*
       * 提示带上子 MOD 数量：整包开关一次改掉包内每个 DLL。
       * 只说「MOD 包已启用」时，用户容易以为只是换了个标记位，
       * 不知道自己顺手改了四个子 MOD —— 所以把数量说出来。
       */
      const count = node.mods.length
      message.success(enabled
        ? t('modsLocal.packageEnabledToast', { count })
        : t('modsLocal.packageDisabledToast', { count }))
    } else {
      message.error(translateBackendMessage(result.message))
    }
    await load()
  } catch (reason) {
    fail(reason)
  } finally {
    for (const mod of node.mods) delete busy.value[mod.dll_file]
  }
}

/** 包级删除：整包删除（文案随「是否经过回收站」切换）。 */
function removePackage(node: ModPackageNode) {
  const first = node.mods[0]
  if (!first) return
  const confirm = deletePackageConfirm.value
  const instance = dialog.warning({
    title: t('tools.deleteModPackage'),
    content: confirm.body(node.label, node.mods.length),
    positiveText: confirm.action,
    negativeText: t('recycle.cancel'),
    async onPositiveClick() {
      markDialogBusy(instance)
      for (const mod of node.mods) busy.value[mod.dll_file] = true
      try {
        const result = await api.uninstallLocalMod(first.dll_file, true)
        result.ok
          ? message.success(result.message ? translateBackendMessage(result.message) : confirm.done)
          : message.error(translateBackendMessage(result.message))
        await load()
      } catch (reason) {
        fail(reason)
      } finally {
        for (const mod of node.mods) delete busy.value[mod.dll_file]
      }
    },
  })
}
</script>

<template>
  <!--
    页头标题右侧：整页状态聚合，分两组 —— 左「结论」（有更新 / 缺依赖 / 最新版…），
    右「开关状态」（N 已启用 / N 已禁用），中间一条竖线断开。
    两组共用同一套 tone 与配色表，颜色永远一致；数量为 0 的状态不显示。
  -->
  <Teleport to="#page-header-title-extra" defer>
    <span class="header-tag-group">
      <n-tag v-for="entry in statusTagsPrimary" :key="entry.key" size="small" :bordered="false"
        class="state-tag state-tag--link" :color="tagColorForAny(entry.tone)" role="button" tabindex="0"
        :title="t('modsOnline.locateOnThisPage')" @click="locateStatus(entry.key)"
        @keydown.enter.prevent="locateStatus(entry.key)"
        @keydown.space.prevent="locateStatus(entry.key)">
        {{ entry.count }} {{ t(entry.text) }}
      </n-tag>
    </span>
    <span v-if="statusTagsPrimary.length && statusTagsToggle.length" class="header-tag-divider" />
    <span class="header-tag-group">
      <n-tag v-for="entry in statusTagsToggle" :key="entry.key" size="small" :bordered="false"
        class="state-tag state-tag--link" :color="tagColorForAny(entry.tone)" role="button" tabindex="0"
        :title="t('modsOnline.locateOnThisPage')" @click="locateStatus(entry.key)"
        @keydown.enter.prevent="locateStatus(entry.key)"
        @keydown.space.prevent="locateStatus(entry.key)">
        {{ entry.count }} {{ t(entry.text) }}
      </n-tag>
    </span>
    <!--
      「上次检查」摘要：刷新结果原来只以 toast 出现，一闪就没了，
      用户过几分钟再看列表分不清「这些结论是刚刷的还是昨晚的」。
      这里把它常驻，悬停能看到完成时间与失败原因。
    -->
    <template v-if="outcome">
      <span class="header-tag-divider" />
      <span class="header-tag-group">
        <RefreshResultTag :outcome="outcome" domain="repositories" />
      </span>
    </template>
  </Teleport>

  <!--
    页头右侧，从左到右：批量开关 → 检查更新 → 打开目录。
    「检查更新」只刷绑定仓库（`refreshBoundRepositories`），状态与 MOD 安装页
    是同一份：那边整体刷新或单源刷新时这里也会转，反过来也一样。放在 MOD 管理页
    是因为用户在这里看到「可更新」后，最自然的下一步就是「再查一次」。
  -->
  <Teleport to="#page-header-actions" defer>
    <!--
      批量开关：一个二选一分段控件（与发布详情面板「说明 / 下载安装」同款），
      替代原来的两个按钮 —— 同一时刻只有全开 / 全关两种状态，是互斥选择。
      · 全部启用 → 选中左侧；全部禁用 → 选中右侧；
      · 部分启用 → 两个都不选中（空串哨兵），由 .is-mixed 隐掉选中胶囊，
        否则胶囊会停在上一次的选中项上，看起来像已经全开 / 全关；
      · 禁用方向仍走 confirmDisableAll()：整页 MOD 全部停用不是随手试的操作；
      · 批量进行中两个分段都禁用，避免连点再发一批命令。
    -->
    <n-tabs v-if="effectiveMods.length" type="segment" animated class="bulk-switch"
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
    <n-button size="small" secondary :loading="checkingBound" :disabled="checkingBound || refreshingAll"
      :title="t('overview.refreshOnlyTheRepositoriesYourInstalledModsAreBoundToAndCheckForNewVersions')" @click="checkUpdates">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('overview.check') }}
    </n-button>
    <n-button size="small" secondary @click="openModsDirectory">
      <FolderOpenOutline width="14" height="14" />{{ t('modsLocal.openFolder') }}
    </n-button>
  </Teleport>

  <!--
    安装流程四段：来源选择 → 同身份冲突检查 → 详情预览 → 确认安装。
    与 MOD 安装页、仓库卡片共用同一套组件与流程，信息完全一致；
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
  <!--
    安装前检查：更新一个已装 MOD 时，若它的新版本还缺依赖 / 有冲突，
    在这里一次处理完再继续安装。与 MOD 安装页共用同一个弹窗与判定。
  -->
  <InstallPlanDialog v-model:show="installPlanOpen" :plan="installPlan"
    :target-label="installPlanTarget"
    @finished="(ready: boolean) => { void finishInstallPlan(ready) }" />

  <!--
    页头标题后的聚合标签已经承载了状态统计（数量、启用、可更新都在标签里），
    原来的 .toolbar 只重复这些数字，已无存在必要，移除。
  -->
  <PageLoading :show="loading">
    <template #status>{{ t('common.loading') }}</template>
    <n-empty v-if="!loading && !sortedMods.length" :description="t('modsLocal.noModsInstalled')" class="empty" />

    <!-- 按安装时绑定仓库的分类折叠：同一分类的 MOD 归在一起 -->
    <n-collapse v-else :expanded-names="groupsExpanded" class="group-collapse"
      @update:expanded-names="onGroupsExpanded">
      <n-collapse-item v-for="node in groups" :key="node.key" :name="node.key">
        <template #header>
          <!--
            分组标题只显示分类名。
            这里曾经还挂着「N 个」「X 个可更新」「已禁用 / 部分启用」几个标签，
            但页头已经有一排覆盖全页的状态聚合（点它能按状态定位），
            分组标题再重复一遍只是把同一信息说两次，还把标题挤窄。
          -->
          <div class="group-head">
            <b>{{ node.label }}</b>
          </div>
        </template>

        <div class="mod-grid">
          <template v-for="pkg in node.packages" :key="pkg.key">
            <!--
              单 DLL 包：不套外壳，直接就是那张卡片。
              外壳（开关 + 标题 + 标签区 + 工具栏 + 展开）由 LocalItemCard 负责，
              这里只提供数据、状态插槽与动作；原生 click 仍由页面处理 ——
              onCardClick 要给 document 级的「点空白收起」打标记。

              `switchable` 只对随包依赖关掉：没有 BepInPlugin GUID 的库 DLL
              不是独立 MOD，启停跟着所属包走，单独关掉只会让依赖它的主 MOD
              加载失败（随包插件同理，见地图管理页的详情）。
            -->
            <LocalItemCard v-if="!pkg.grouped" v-for="mod in pkg.mods" :key="mod.dll_file"
              :data-dll-file="mod.dll_file"
              :data-focus-id="bindingRepositoryId(mod)"
              :data-mod-anchor="modAnchor(mod)"
              :switchable="!mod.is_dependency"
              :switch-key="modSwitchKey(mod.dll_file)"
              :enabled="cardSwitchOn(modSwitchKey(mod.dll_file), mod.enabled)"
              :toggle-loading="!!busy[mod.dll_file]"
              :title="singleModTitle(mod)"
              :version="formatVersion(mod.version) || undefined"
              :title-tag="mod.is_dependency ? t('modsLocal.bundledDependency') : undefined"
              :disabled="!mod.enabled"
              :expanded="cardExpansion.isOuterOpen(mod.dll_file)"
              :focus-flash="highlightId === bindingRepositoryId(mod)"
              :status-flash="matchesActiveStatus(modAnchor(mod))"
              :pulse-class="statusPulseClass(modAnchor(mod), pulseTarget, pulseSeq)"
              :tools-min-width="toolColumnWidth"
              :tool-gap="TOOL_GAP"
              :tool-button-width="TOOL_BUTTON_WIDTH"
              @toggle="(value: boolean) => toggle(mod, value)"
              @click="onCardClick($event, mod)">
              <template #status>
                <!--
                  状态快捷动作：按钮由当前最需要注意的状态决定
                  （可更新 → 更新、仓库刷新失败 → 刷新），
                  规则集中在 statusActions.ts。与标签之间用竖线隔开，
                  视觉上属于标签区的一部分（不是右侧工具栏的常规动作）。
                -->
                <template v-if="actionByDll[mod.dll_file]">
                  <!-- 配色跟随触发它的状态档位，来自 theme.ts 的统一色阶 -->
                  <n-button size="tiny" :style="buttonColorFor(actionByDll[mod.dll_file].tone, 'soft')"
                    :title="t(actionByDll[mod.dll_file].label)"
                    :loading="!!previewing[bindingRepositoryId(mod) ?? ''] || !!refreshing[bindingRepositoryId(mod) ?? '']"
                    :disabled="!!game?.kitchen_running" @click.stop="runCardAction(mod)">
                    <ArrowUpCircleOutline v-if="actionByDll[mod.dll_file].kind === 'update'" width="14" height="14" />
                    <DownloadOutline v-else-if="actionByDll[mod.dll_file].kind === 'install-dependency'" width="14" height="14" />
                    <TrashOutline v-else-if="actionByDll[mod.dll_file].kind === 'uninstall-conflict'" width="14" height="14" />
                    <RefreshOutline v-else width="14" height="14" />
                    {{ t(actionByDll[mod.dll_file].label) }}
                  </n-button>
                  <span class="tool-divider" />
                </template>

                <!--
                  状态标签组：宽度按内容自适应，整体贴住分割线左侧。
                  status-pulse：刷新完成后标签变化时播一次短暂高亮（见 isStatusPulsing）。
                -->
                <span class="tag-group" :class="{ 'status-pulse': isStatusPulsing(mod) }">
                  <!--
                    配色走 Naive n-tag 原生 :color，由 theme.ts 统一计算。
                    「缺依赖」/「冲突」跳到**依赖的**仓库；解析不出可安装来源时
                    没有跳转目标，此时不给链接样式也不可点 —— 一个点了没反应、
                    却长得像能点的标签比纯文字更糟。
                  -->
                  <ModStatusTags :dll-file="mod.dll_file" />
                </span>
              </template>

              <!-- 工具栏：可选按钮在最左（用占位保持列宽），固定按钮贴右 -->
              <template #tools>
                <!-- 查看日志：只有 BepInEx/logs/<guid> 存在时才显示；缺失时保留同宽占位 -->
                <n-button v-if="mod.has_logs" size="tiny" quaternary :title="t('modsLocal.viewLogs')"
                  class="tool-optional" @click.stop="openLogs(mod)">
                  <DocumentTextOutline width="14" height="14" />
                </n-button>
                <span v-else class="tool-placeholder" />
                <!-- 配置编辑：只有存在配置文件的 MOD 才有；缺失时保留同宽占位，
                     这样「打开位置」「删除」始终落在最右侧同一列 -->
                <n-button v-if="mod.has_config" size="tiny" quaternary :title="t('tools.editConfig')"
                  class="tool-optional" @click.stop="editConfig(mod)">
                  <SettingsOutline width="14" height="14" />
                </n-button>
                <span v-else class="tool-placeholder" />
                <n-button size="tiny" quaternary :title="t('modsLocal.revealInFolder')"
                  :disabled="game?.kitchen_running" @click.stop="openLocation(mod)">
                  <FolderOpenOutline width="14" height="14" />
                </n-button>
                <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running" :title="t('modsLocal.delete')"
                  @click.stop="remove(mod)">
                  <TrashOutline width="14" height="14" />
                </n-button>
              </template>

              <!-- 不阻止冒泡：点详情里的任意位置都算「点空白」，由 onDocumentClick 收起 -->
              <template #detail>
                <ModDetailGrid :mod="mod" :binding-text="bindingText(mod)"
                  :status-summary="statusSummary(mod)" :package-count-text="packageCountText(mod)" />
                <!--
                  依赖声明：和地图管理页详情一样，表格直接贴着上面的安装信息
                  （`.detail-section--table` 去掉虚线与额外留白，只留 8px 节奏），
                  因为表格自己就有一圈边框和圆角。没有声明时整块都不渲染。
                -->
                <div v-if="modDependencyRows(mod).length" class="detail-section detail-section--table">
                  <DependencyTable :rows="modDependencyRows(mod)" @jump="openDependencyJump" />
                </div>
              </template>
            </LocalItemCard>

            <!--
              多 DLL 包：折叠外壳 + 包级操作，内部仍是同样的卡片。
              跨页跳转（从 MOD 安装页点标签过来）要落在**包外壳**上：
              包是页面上的一个展示单元，跳转进来高亮整包才符合语义；
              包内子 DLL 不是单元，所以它们不带 data-focus-id。
            -->
            <section v-else class="package-shell"
              :data-focus-id="pkg.repositoryId"
              :data-mod-anchor="`unit:${pkg.key}`"
              :class="[
                {
                  expanded: packageExpansion.isOuterOpen(pkg.key),
                  'status-flash': matchesActiveStatus(`unit:${pkg.key}`),
                  'focus-flash': !!pkg.repositoryId && highlightId === pkg.repositoryId,
                },
                statusPulseClass(`unit:${pkg.key}`, pulseTarget, pulseSeq),
              ]">
              <header class="package-head"
                @click="togglePackage(pkg, $event)">
                <!-- 左：开关 + 包名 + 子 MOD 数量（数量紧跟包名；「N 个」已表明这是包） -->
                <div class="package-left">
                  <!--
                    包级开关三态：全开 / 全关 / 部分。
                    Naive 的 indeterminate 会画一个加载圈，不像「开关中间态」，
                    所以这里仍用布尔值，靠 is-partial 类把滑块和轨道做半透明表达。
                    title 说明它会一起切换包内每个子 MOD —— 只写「N 个」时
                    用户未必意识到这一个开关等于四个。
                  -->
                  <n-switch class="package-switch" size="small" :value="pkg.enabled"
                    :loading="pkg.mods.some((m) => busy[m.dll_file])"
                    :title="t('modsLocal.packageSwitchHint', { count: pkg.mods.length })"
                    :class="{ 'is-partial': pkg.partial }"
                    @click.stop @update:value="(value: boolean) => togglePackageEnabled(pkg, value)" />
                  <b class="package-title" :title="pkg.label">{{ pkg.label }}</b>
                  <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">
                    {{ t('rename.text', { count: pkg.mods.length }) }}
                  </n-tag>
                </div>

                <!-- 右：状态标签 + 分割线 + 工具列（与单 MOD 卡片同一套列宽预留） -->
                <div class="package-right">
                  <!--
                    包级快捷动作：与单 MOD 卡片共用 statusActions 的规则。
                    包是一个整体，动作针对的是这个包绑定的仓库，
                    不是包内某个 DLL。
                  -->
                  <template v-if="pkg.action">
                    <n-button size="tiny" :style="buttonColorFor(pkg.action.tone, 'soft')"
                      :title="t(pkg.action.label)"
                      :loading="!!previewing[pkg.repositoryId ?? ''] || !!refreshing[pkg.repositoryId ?? '']"
                      :disabled="!!game?.kitchen_running"
                      @click.stop="runPackageAction(pkg)">
                      <ArrowUpCircleOutline v-if="pkg.action.kind === 'update'" width="14" height="14" />
                      <DownloadOutline v-else-if="pkg.action.kind === 'install-dependency'" width="14" height="14" />
                      <TrashOutline v-else-if="pkg.action.kind === 'uninstall-conflict'" width="14" height="14" />
                      <RefreshOutline v-else width="14" height="14" />
                      {{ t(pkg.action.label) }}
                    </n-button>
                    <span class="package-divider" />
                  </template>

                  <!--
                    包状态标签：以包为主体。版本结论（最新版 / 可更新 / 刷新中…）
                    整包只有一条、不带数量；成员级告警（重复 / 低版本 / 加载警告）
                    才带数量，因为确实可能只命中包内某几个 DLL。
                    配色与单 MOD 行同源，点击跳到 MOD 安装页对应仓库。
                  -->
                  <span class="tag-group">
                    <n-tag v-for="entry in pkg.statuses" :key="entry.key" size="tiny" :bordered="false"
                      class="state-tag" :class="{ 'state-tag--link': !!packageTagTargetId(pkg, entry.key) }"
                      :color="tagColorForAny(entry.tone)"
                      :title="packageTagTitle(pkg, entry.key)"
                      @click.stop="openPackageTag(pkg, entry.key)">
                      {{ entry.count ? `${entry.count} ${t(entry.text)}` : t(entry.text) }}
                    </n-tag>
                    <n-tag v-if="!pkg.enabled && !pkg.partial" size="tiny" :bordered="false"
                      class="state-tag" :color="tagColorFor('muted')">{{ t('modsLocal.disabled') }}</n-tag>
                    <n-tag v-else-if="pkg.partial" size="tiny" :bordered="false"
                      class="state-tag" :color="tagColorFor('accent')">{{ t('tools.partiallyEnabled') }}</n-tag>
                  </span>

                  <span class="package-divider" />

                  <!--
                    包标题与单 MOD 卡片是同一层级，工具栏必须用同一列宽，
                    这样删除按钮落在同一竖线；只有一个按钮时其余位置留空。
                  -->
                  <span class="package-tool-group" :style="toolGroupStyle" @click.stop>
                    <n-button size="tiny" quaternary type="error" :title="t('modsLocal.deletePackage')"
                      :disabled="game?.kitchen_running" @click="removePackage(pkg)">
                      <TrashOutline width="14" height="14" />
                    </n-button>
                  </span>
                </div>
              </header>

              <!-- 包展开/收起：同样用 Naive 的折叠过渡 -->
              <n-collapse-transition :show="packageExpansion.isOuterOpen(pkg.key)" display-directive="show">
              <div class="package-body">
                <!--
                  包内子 DLL 行不是独立单元：不加定位锚点，也不参与状态高亮。
                  状态结论属于整个包，高亮只画在外壳上（见 package-shell）。
                  外壳仍由 LocalItemCard 提供；这些行没有状态标签，
                  所以不传 #status，组件会自己补 1px 隐形占位保持按钮列对齐。
                  顺序是「主 MOD → 它带的依赖 DLL」：标题后带版本号，依赖项多一个
                  灰色「随包依赖」标签（排序见 toPackage）。
                  依赖行同样不给开关（理由见上面单 DLL 包那段注释）。
                -->
                <LocalItemCard v-for="mod in pkg.mods" :key="mod.dll_file"
                  :data-dll-file="mod.dll_file"
                  :switchable="!mod.is_dependency"
                  :switch-key="modSwitchKey(mod.dll_file)"
                  :enabled="cardSwitchOn(modSwitchKey(mod.dll_file), mod.enabled)"
                  :toggle-loading="!!busy[mod.dll_file]"
                  :title="modTitle(mod)"
                  :version="formatVersion(mod.version) || undefined"
                  :title-tag="mod.is_dependency ? t('modsLocal.bundledDependency') : undefined"
                  :disabled="!mod.enabled"
                  :expanded="packageExpansion.isInnerOpen(pkg.key, mod.dll_file)"
                  :tools-min-width="toolColumnWidth"
                  :tool-gap="TOOL_GAP"
                  :tool-button-width="TOOL_BUTTON_WIDTH"
                  @toggle="(value: boolean) => toggle(mod, value)"
                  @click="onCardClick($event, mod, pkg.key)">
                  <!--
                    包内子 MOD 只保留三个操作（标签统一在包标题行）。
                  -->
                  <template #tools>
                    <n-button v-if="mod.has_logs" size="tiny" quaternary :title="t('modsLocal.viewLogs')"
                      class="tool-optional" @click.stop="openLogs(mod)">
                      <DocumentTextOutline width="14" height="14" />
                    </n-button>
                    <span v-else class="tool-placeholder" />
                    <n-button v-if="mod.has_config" size="tiny" quaternary :title="t('tools.editConfig')"
                      class="tool-optional" @click.stop="editConfig(mod)">
                      <SettingsOutline width="14" height="14" />
                    </n-button>
                    <span v-else class="tool-placeholder" />
                    <n-button size="tiny" quaternary :title="t('modsLocal.revealInFolder')"
                      :disabled="game?.kitchen_running" @click.stop="openLocation(mod)">
                      <FolderOpenOutline width="14" height="14" />
                    </n-button>
                    <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running"
                      :title="t('modsLocal.delete')" @click.stop="remove(mod)">
                      <TrashOutline width="14" height="14" />
                    </n-button>
                  </template>

                  <!-- 同上：详情区不拦冒泡，点任意位置都能收起 -->
                  <template #detail>
                    <ModDetailGrid :mod="mod" :binding-text="bindingText(mod)"
                      :status-summary="statusSummary(mod)" :package-count-text="packageCountText(mod)" />
                  </template>
                </LocalItemCard>
                <!--
                  整包共用一份依赖声明（关系挂在仓库上），所以只出一张表。
                  放整批子卡片**之后**：先看包里有什么（本体与它带的依赖），
                  再看这个包对外要求什么；和地图管理页「信息 → 随包插件 →
                  依赖声明」同一顺序，两页不用记两套版式。
                -->
                <div v-if="packageDependencyRows(pkg).length" class="detail-section detail-section--table">
                  <DependencyTable :rows="packageDependencyRows(pkg)" @jump="openDependencyJump" />
                </div>
              </div>
              </n-collapse-transition>
            </section>
          </template>
        </div>
      </n-collapse-item>
    </n-collapse>
  </PageLoading>

  <!-- 只保留排序切换（卡片已是唯一布局） -->
  <!--
    浮动按钮：重新扫描本地 MOD（只读磁盘，不碰仓库 Release）。
    「重新读取 Dashboard」会重扫 BepInEx/plugins 并重算标签，等同用户手动
    改过文件后的一次本地刷新；仓库刷新在 MOD 安装页，两者职责分开。
  -->
  <n-float-button position="fixed" bottom="28" right="32" :tooltip="t('tools.rescanLocalMods')"
    :disabled="refreshingLocal" @click="refreshLocal">
    <RefreshOutline width="17" height="17" :class="{ 'spin-icon': refreshingLocal }" />
  </n-float-button>

  <!-- 配置编辑器：编辑该 MOD 在 BepInEx/config 下的 .cfg（Naive 文本域） -->
  <ModConfigEditor v-if="configTarget" v-model:show="configOpen"
    :dll-file="configTarget.dll_file" :title="modTitle(configTarget)" />
</template>

<style scoped>
/* 分类折叠：分类标题行 + 内部单列 MOD 卡片 */
.group-collapse { width: 100%; }
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
.mod-grid {
  /* 每个卡片撑满容器宽度（单列纵向堆叠） */
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 2px 0 6px 14px;
}

/*
 * MOD 包外壳（2 个及以上 DLL 才有）。
 * 视觉上与 DLL 卡片保持同一套语言：同样的圆角、同样的边框与底色，
 * 只是内部多一层缩进和分隔线，让人看出「这是一个包含多个 DLL 的包」。
 */
.package-shell {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  transition: border-color .16s ease, background-color .16s ease;
}
.package-shell:hover { border-color: color-mix(in srgb, var(--oc2-accent) 55%, var(--oc2-row-line)); }
/*
 * 包外壳展开：与单 MOD 卡片展开同一套视觉（accent 边框 + 8% accent 底色）。
 * 不给外壳规则时，收起和展开长得一模一样，用户看不出「这个包点开了」。
 * 底色按 `--oc2-card` 混合（不是 `--card-bg`）：包内子卡片用下沉底色
 * （`.package-body .mod-card { --card-bg: var(--oc2-sunken) }`），外壳若跟着
 * 子卡的基色走，展开后两者同色，包的边界就消失了。
 * 放在 :hover 之后与 `.mod-card.expanded` 顺序一致；后面的 .status-flash /
 * .focus-flash 用 12% 底色，定位高亮仍会盖过展开态。
 */
.package-shell.expanded {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
.package-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  /*
   * 标题行本身就在外壳边框之内，14px 内边距与 .mod-card 一致即可对齐；
   * 包内卡片额外用 margin-right: -1px 抵消外壳边框，两边内容右缘因此相等。
   */
  padding: 10px 14px;
  cursor: pointer;
  user-select: none;
  min-width: 0;
}
/*
 * 包标题行**不吸顶**，与包内子卡片（`LocalItemCard`）一致：展开时由
 * `expandScroll.ts` 的一次性滚动把外壳夹在页头下沿，滚动过程中标题跟着内容走。
 */
/* 左半：开关 + 包名 + 子 MOD 数量（数量紧跟包名） */
.package-left {
  display: flex;
  align-items: center;
  gap: 11px;
  flex: 1;
  min-width: 0;
}
/*
 * 右半：状态标签 + 分割线 + 工具列。
 * 结构与单 MOD 卡片的 .card-right 完全一致，因此标签列与按钮列
 * 会和下方所有卡片对齐在同一竖直线上。
 */
.package-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: nowrap;
  gap: 10px;
  flex: 0 0 auto;
  min-width: 0;
}
/*
 * 包标题按文字宽度显示（不撑满），后面紧跟「N 个」标签，
 * 剩余空间交给左侧容器的 flex 空隙，避免长标题把右侧标签和工具栏推出去。
 */
.package-title {
  flex: 0 1 auto;
  min-width: 0;
  max-width: 60%;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 包级开关：部分启用时轨道半透明，滑块停在中间位置 */
.package-switch.is-partial :deep(.n-switch__rail) {
  background-color: color-mix(in srgb, var(--oc2-accent) 45%, transparent);
}
.package-switch.is-partial :deep(.n-switch__button) {
  /* 滑块水平居中，表达「既非全开也非全关」 */
  transform: translateX(calc(50% - 1px));
}
/*
 * 包标题行的开关 / 分割线 / 工具列。
 *
 * 这几套样式必须留在父页面：卡片里的同名元素由 LocalItemCard 渲染
 * （在组件自己的 scoped 样式里），包标题行则由父页面直接渲染。
 * scoped 是逐元素生效的，两边各留一份，不能指望一条规则同时打两边。
 */
.package-switch { flex: 0 0 auto; }
.package-divider {
  width: 1px;
  height: 16px;
  flex: 0 0 1px;
  background: var(--oc2-row-line);
}
/*
 * 包工具栏：与卡片内 .tool-group 用同一套列宽。
 * gap 与按钮宽度来自 toolGroupStyle 下发的 CSS 变量（与 toolColumnWidth 同源），
 * 所以包标题行与所有卡片的按钮列落在同一条竖线上。
 */
.package-tool-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--tool-gap, 8px);
  flex: 0 0 auto;
}
.package-tool-group :deep(.n-button) {
  width: var(--tool-button-width, 30px);
  min-width: var(--tool-button-width, 30px);
  padding: 0;
}
/*
 * 包内 DLL 列表：外壳留 14px 内边距 + 1px 边框。
 *
 * 子 MOD 行不必和单 MOD 行左对齐——它嵌在外壳里，左侧天然多一层外壳
 * 边框和内边距。约束只在右侧：让「按钮右边缘 → 子卡片边框」与
 * 「子卡片边框 → 外壳边框」两段相等，看起来才是同一套留白。
 */
.package-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 14px;
  border-top: 1px solid var(--oc2-row-line);
}
/*
 * 包内卡片：把卡片自身 1px 右边框也算进「卡片外沿」，让两段留白相等。
 *
 * 目标：按钮右边缘 → 子卡片右边框外沿 = 子卡片右边框外沿 → 外壳边框内沿。
 * 外壳内容区已留 14px，所以子卡片只需再留 `14px - 1px(自身边框) = 13px`，
 * 两段量出来才是 14 / 14 而不是 15 / 14。
 *
 * 底色只改变量不给背景：卡片内部还有 :hover / .expanded 会换底色，
 * 在这里再写 background 就成了两个同优先级的规则抢同一属性，
 * 结果取决于样式表顺序。--card-bg 是卡片自己的默认底色来源。
 */
.package-body .mod-card {
  --card-bg: var(--oc2-sunken);
  padding-right: calc(14px - 1px);
}

/*
 * 详情区的依赖表分隔线：与地图管理页（LevelsView.vue）里那条 `.detail-section`
 * **同名同值**。scoped 样式逐元素生效，两页各自编译一份、互相看不见，所以这不是
 * 重复定义而是必须的两份；改这里必须同时改那边，否则同一张依赖表在两个页面的
 * 上下留白会不一样。
 */
.detail-section {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px dashed var(--oc2-row-line);
}

/*
 * 依赖表专用的那一档：表格自己就有一圈边框和圆角，再叠一条虚线加两段留白，
 * 等于把「这里换内容了」说两遍；包内还会把「子卡片 → 依赖表」推得比子卡片
 * 之间（8px）更松。这里只保留卡片内统一的 8px 节奏，不再画分隔线。
 *
 * 包内 `.package-body` 是 flex + `gap: 8px`，间距已经由 gap 给过，再叠 margin
 * 就是 16px，所以那一处归零。与地图管理页（LevelsView.vue）同名同值。
 */
.detail-section--table {
  margin-top: 8px;
  padding-top: 0;
  border-top: 0;
}
.package-body .detail-section--table { margin-top: 0; }

/*
 * 卡片自身的样式（.mod-card / .card-row / .card-* / .tool-*）已随外壳一起
 * 下沉到 LocalItemCard.vue 的 scoped 样式里。卡片是子组件渲染的 DOM，
 * 父级 scoped 规则只对父级自己的元素生效，留在这里只会变成永不命中的死规则。
 * 父页面只保留包外壳（.package-*）和自己编译的插槽内容（.tag-group 等）。
 *
 * 定位高亮脉冲：卡片用组件自己的 local-item-card-focus-pulse，
 * 这里这份 keyframes 只服务下面的 .package-shell（包高亮画在外壳上）。
 */
@keyframes focus-pulse {
  0%   { box-shadow: 0 0 0 0 color-mix(in srgb, var(--oc2-accent) 70%, transparent); }
  40%  { box-shadow: 0 0 0 5px color-mix(in srgb, var(--oc2-accent) 30%, transparent); }
  100% { box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent); }
}
/*
 * 包外壳的定位高亮：与单 MOD 卡片同一套视觉。
 * 两个来源都落在这里：
 *   · status-flash —— 点包自己的状态标签（页内聚合定位）
 *   · focus-flash  —— 从 MOD 安装页点「最新版」等标签跨页跳过来
 * 包是页面上的一个展示单元，两种情况都该高亮整个外壳，
 * 而不是包内某个 DLL 行 —— 结论属于包，不属于某一个子 MOD。
 */
.package-shell.status-flash,
.package-shell.focus-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 12%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
  animation: focus-pulse 1200ms cubic-bezier(0.4, 0, 0.2, 1);
}
@media (prefers-reduced-motion: reduce) {
  .package-shell.status-flash,
  .package-shell.focus-flash { animation: none; }
}
/* 可点击的状态标签：手型 + hover 反馈，表示能跳到 MOD 安装页 */
.state-tag--link { cursor: pointer; }
.state-tag--link:hover { filter: brightness(1.25); }
/* 本地刷新进行中：图标自转（类由刷新按钮绑定） */
.spin-icon { animation: oc2-local-spin .8s linear infinite; }
@keyframes oc2-local-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
@media (prefers-reduced-motion: reduce) {
  .spin-icon { animation: none; }
}
/*
 * 状态变化脉冲：刷新完成、标签从「刷新中」变成确定状态时，
 * 标签组轻微放大后回落并短暂提亮，让异步结果被看见。
 *
 * 用 scale + filter 而不是改颜色：颜色已由 tone 表达语义，
 * 这里只表达「刚刚变了」这一时间信息，两者不冲突。
 * 时长与 PULSE_DURATION（900ms）对齐 —— 计时器在 script 里，动画在 CSS 里，两处必须一致。
 */
.tag-group.status-pulse {
  animation: status-pulse 900ms cubic-bezier(0.4, 0, 0.2, 1);
}
@keyframes status-pulse {
  0%   { transform: scale(1);    filter: brightness(1); }
  22%  { transform: scale(1.06); filter: brightness(1.45); }
  100% { transform: scale(1);    filter: brightness(1); }
}
@media (prefers-reduced-motion: reduce) {
  /* 减少动效：只保留短暂的亮度变化，不做缩放 */
  .tag-group.status-pulse { animation: status-pulse-fade 900ms ease; }
  @keyframes status-pulse-fade {
    0%   { filter: brightness(1); }
    22%  { filter: brightness(1.45); }
    100% { filter: brightness(1); }
  }
}
/*
 * 状态标签组：宽度跟内容走，不再预留固定最小宽度。
 * 之前用 min-width: 128px 让所有行的标签区起点一致，但单个标签时
 * 右侧会空出一大片（截图里高亮框比「最新版」宽一倍）。
 * 现在靠 flex-end + 固定的分割线/工具栏列宽保证右对齐，
 * 标签少时容器就窄，视觉上没有空白。
 *
 * 这条规则留在父页面：包标题行的标签区直接由父模板渲染，
 * 而单 MOD 卡片里的标签组位于 status 插槽内 —— 插槽内容由父页面编译，
 * 带的是父级 scope id，所以父级规则照样命中。
 */
.tag-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  flex: 0 1 auto;
  flex-wrap: nowrap;
  overflow: hidden;
}
/* 页头批量分段控件的样式在 src/styles/global.css 的 `.bulk-switch`。 */
</style>
