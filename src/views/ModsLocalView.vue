<script setup lang="ts">
import { requestToggleChanges, requestAllToggleChanges } from '../modChanges/requests'
import { useGameOperationGate } from '../composables/useGameOperationGate'
import { pickI18n, t } from '../i18n'
import { tagColorForAny } from '../theme'
import { PULSE_DURATION } from '../motion'
import { computed, onBeforeUnmount, onMounted, provide, reactive, ref, watch } from 'vue'
import { useDialog } from 'naive-ui'
import { useNotify } from '../useNotify'
import {
  NCollapse, NCollapseItem, NFloatButton, NTag,
} from 'naive-ui'
import ContentState from '../components/ContentState.vue'
import {
  FolderOpenOutline, RefreshOutline,
} from '@vicons/ionicons5'
import { matchesLocalSearch, modSearchText } from '../localItemSearch'
import { api } from '../services/tauri'
import { repositoryForMod } from '../modStatus'
import { conflictRemovalTarget, dependencyRepairStatus, dependencyTagFor, isRelationCardKey, relationJumpTarget, type RelationStatus } from '../modRelations'
import { modRepositoryIsAvailable } from '../repositoryAvailability'
import { installedModTags } from '../statusTags'
import {
  buildLocalModGroups,
  filterLocalModGroups,
  localModStatusUnits,
  packageKeysMatchingStatus,
  summarizeLocalModStatuses,
} from '../localModGroups'
import { statusActionFor, type StatusActionSpec } from '../statusActions'
import { dependencyRowFromStatus, dependencyTagTitle, installedModDependencyRows, mergedModDependencyRows } from '../dependencyRows'
import { useDependencyNavigation } from '../composables/useDependencyNavigation'
import type { DependencyRow } from '../dependencyRows'
import { useFocusHighlight, useModRepositoryLink } from '../crossLink'
import { useListVisibility } from '../composables/useListVisibility'
import { bulkSwitchValue as bulkSwitchValueOf } from '../listVisibility'
import { runtimeModStatus } from '../modRuntimeStatus'
import { describeRefreshOutcome } from '../refreshOutcomeText'
import { markDialogBusy } from '../dialogBusy'
import { useDashboard } from '../composables/useDashboardStore'
import { useModRepositoryRefresh } from '../composables/useModRepositoryRefresh'
import { demoActive, withDemoConfig, withDemoGame, withDemoMods } from '../tutorial/demo'
import HeaderIconAction from '../components/HeaderIconAction.vue'
import HeaderSearch from '../components/HeaderSearch.vue'
import HeaderBulkSwitch from '../components/HeaderBulkSwitch.vue'
import ModGroupBody from '../components/ModGroupBody.vue'
import ModGroupHeader from '../components/ModGroupHeader.vue'
import {
  MOD_GROUP_CTX,
  TOOL_BUTTON_WIDTH,
  TOOL_GAP,
  type ModGroupContext,
  type ModGroupNode,
  type ModPackageNode,
} from '../components/levelSetGroupContext'
import ModConfigEditor from '../components/ModConfigEditor.vue'
import InstallSourceCard from '../components/InstallSourceCard.vue'
import PageLoading from '../components/PageLoading.vue'
import { useModInstallFlow } from '../composables/useModInstallFlow'
import { useFileDrop } from '../composables/useFileDrop'
import { takePendingDrop, useDropRouting } from '../composables/useDropRouting'
import { useModRepositoryPreview } from '../composables/useModRepositoryPreview'
import { useModRemoval } from '../composables/useModRemoval'
import { useModsLocalCardExpansion } from '../composables/useModsLocalCardExpansion'
import { useLocalGroupExpansion } from '../composables/useLocalGroupExpansion'
import type { InstalledMod, ModRepository } from '../types'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'

const message = useNotify()
const dialog = useDialog()
const { load, ensureLoaded, modsLoading: realLoading, mods: scannedMods, game: realGame, config: realConfig } = useDashboard()
const loading = computed(() => !demoActive.value && realLoading.value)
const game = computed(() => withDemoGame(realGame.value))
const config = computed(() => withDemoConfig(realConfig.value))

const { blockModOperation } = useGameOperationGate()

/**
 * 教程期间整批展示示例 MOD，真实扫描结果只保留在 store。
 *
 * 只改展示层、不改 store：教程一结束 `withDemoMods` 立刻返回原始数组，
 * 页面不需要任何清理；真实列表非空与在飞结果均不影响示例。
 */
const mods = computed(() => withDemoMods(scannedMods.value))

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
  gameBlocked: blockModOperation,
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
  refreshing, refreshingAll, runtimeState,
  checkingBound, outcome,
  refreshOne, refreshBoundRepositories,
} = useModRepositoryRefresh()

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
   *   · 用 `modRepositoryIsAvailable` 而不是只看 enabled —— 地区不适用等于
   *     该仓库不存在，不能给它发请求（否则会报「不属于当前下载渠道」）；
   *   · 跳过 RAW 模式：它没有 Release 可查（引擎入口内部也会跳过，
   *     这里先滤掉避免无意义的排队）；
   *   · 同一仓库只刷一次：多个 MOD 常来自同一个仓库（同一个包里的多个 DLL）。
   */
  const region = config.value?.region
  const platform = config.value?.platform
  const sources = config.value?.repository_sources
  const targets = new Map<string, ModRepository>()
  for (const mod of effectiveMods.value) {
    if (mod.is_dependency) continue
    const repo = repositoryForMod(repositories.value, mod)
    if (!repo) continue
    if (!modRepositoryIsAvailable(repo, sources, region, platform)) continue
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
     * 检查结果通过共享文案汇报：
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
const { linkToModRepository } = useModRepositoryLink()
/**
 * 从 MOD 安装页跳回来时，高亮绑定到该仓库的 MOD 卡片 3 秒并滚动到它。
 *
 * 只高亮 + 滚到视口中间，**不展开卡片**：卡片根节点的 `[data-focus-id]` 在收起
 * 状态下也在 DOM 里，展开与否该由用户自己决定 —— 自动摊开第一张会把其余命中
 * 挤出视口，也违背「跳过来只是让我看到它」的预期。
 */
const { highlightId } = useFocusHighlight()
/* 页头聚合标签现在切换列表可见性（见下方 `visibility`）；跨页定位只保留 `useFocusHighlight`。 */

/*
 * 固定按名称排序。
 *
 * 原来的「排序」切换入口是右下角浮动按钮，现在该位置改成了「重新扫描
 * 本地 MOD」，排序已无切换入口，所以把配置一起去掉 —— 留一个永远不变的
 * sortBy 只会误导后续维护者。
 */
const busy = ref<Record<string, boolean>>({})
/*
 * 卡片展开状态只有一套，规则都在 `useCardExpansion` 里：
 *   - 外层：独立 MOD 卡（键 `mod:<dll_file>`）与包外壳（键 `pkg:<package_id>`）
 *     算**同一级**，`singleOuter` 表示一次只展开其中一张。
 *   - 内层：包内子 MOD / 随包依赖，父键是包 key、子键是 `dll_file`。
 * 独立卡与包外壳必须落在同一个键空间里，互斥才覆盖得到两者；拆成两个实例时
 * 一张独立卡与一个包可以同时摊开，用户分不清自己点开的是哪一个。
 */
const { expansion, modCardKey, onCardClick, togglePackage } = useModsLocalCardExpansion()
// 切页面复用已有 Dashboard；开关/删除后仍用 load() 取最新扫描结果
onMounted(() => {
  if (demoActive.value) return
  void ensureLoaded()
  // 从地图页「去安装」跳过来：路径已暂存在模块里，直接接着走预览
  const pending = takePendingDrop('mod')
  if (pending) void startDropInstall(pending)
})
watch(demoActive, active => { if (!active) void ensureLoaded() })

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
 * 每个 MOD 的运行期状态（合成后端扫描结果 + 当前仓库缓存 + 刷新状态）。
 *
 * 判定逻辑在 `modRuntimeStatus`：卡片标签组件（`ModStatusTags`）调的是**同一个**
 * 函数，所以页头聚合与每张卡永远不会一个说「刷新中」、一个说「最新版」。
 */
const runtimeStatuses = computed(() =>
  mods.value.map((mod) => runtimeModStatus(
    mod, mods.value, repositories.value, runtimeState.value,
  )))

/** 列表与卡片实际用的 MOD（`labels` / `update_available` 已按运行期状态重算）。 */
const effectiveMods = computed(() => runtimeStatuses.value.map((entry) => entry.mod))

/** 按 DLL 索引完整关系，快捷动作不能只看卡头汇总。 */
const relationsByDll = computed<Record<string, RelationStatus[]>>(() =>
  Object.fromEntries(runtimeStatuses.value.map(({ mod, relations }) => [mod.dll_file, relations])))

/* ===== 搜索 =====
 *
 * 纯前端过滤，只收窄本页的展示集合：刷新、依赖快照与批量开关仍按全部本地
 * MOD 走。命中范围见 `localItemSearch` 的字段白名单 —— 卡片上看得见的名称、
 * 作者、分类、版本号、来源都算，路径、GUID、状态枚举这些内部数据不算。
 */
const search = ref('')
const query = computed(() => search.value.trim().toLowerCase())
/** 搜索词 + 状态标签共用一份可见性规则（与地图管理页同一实现）。 */
const visibility = useListVisibility(query, {
  noData: () => t('modsLocal.noModsInstalled'),
  noQueryMatch: () => t('modsLocal.noMatchMods'),
  noStatusMatch: () => t('common.noStatusMatch'),
})

/** 卡片标题可能来自绑定仓库名或包名快照，纯模块看不到，作为额外文字拼进来。 */
function searchExtra(mod: InstalledMod) {
  return [displayTitle(mod), singleModTitle(mod)]
}

/** 列表固定按标题排序（排序切换入口已移除，见上方说明）。 */
const sortedMods = computed(() =>
  [...effectiveMods.value]
    .sort((a, b) => modTitle(a).localeCompare(modTitle(b))),
)

const searchMatchedDllFiles = computed(() => {
  if (!query.value) return null
  const keys = new Set<string>()
  for (const mod of effectiveMods.value) {
    if (matchesLocalSearch(query.value, modSearchText(mod), ...searchExtra(mod))) {
      keys.add(mod.dll_file)
    }
  }
  return keys
})

/** 三种空态的判定在共享的可见性规则里。 */
const emptyDescription = computed(() =>
  visibility.emptyText(effectiveMods.value.length > 0, visibleGroups.value.length),
)

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

/** 页头分段开关的纯状态投影由两页共用；本页只提供完整 MOD 列表的计数。 */
const bulkSwitchValue = computed(() =>
  bulkSwitchValueOf(enabledModCount.value, effectiveMods.value.length))

/**
 * 分段控件点击：只发起动作，不先改选中态。
 *
 * 禁用方向要过确认框：用户点「取消」时状态没变，控件也不能先跳到
 * 「全部禁用」，否则会让人以为已经生效。
 */
function onBulkSwitch(value: string | number) {
  if (blockModOperation()) return
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
  if (blockModOperation()) return
  if (bulkWorking.value || !effectiveMods.value.length) return
  bulkWorking.value = true
  try {
    const result = await requestAllToggleChanges('mod', enabled)
    if (!result) return
    if (result.ok) {
      message.success(translateBackendMessage(result.message))
      await load()
    } else {
      message.error(translateBackendMessage(result.message))
    }
  } catch (reason) { fail(reason) } finally { bulkWorking.value = false }
}

function confirmDisableAll() { void setAllEnabled(false) }

/**
 * 按「已安装时绑定的仓库分类」分组。
 * MOD 通过 repository_id / repository_binding 找到对应仓库，取其 group_i18n；
 * 找不到绑定的 MOD 归入「未绑定」分组。
 */
const repositories = computed<ModRepository[]>(() => config.value?.repositories ?? [])

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

const groups = computed<ModGroupNode[]>(() => buildLocalModGroups({
  mods: sortedMods.value,
  repositories: repositories.value,
  matchedDllFiles: searchMatchedDllFiles.value,
  categoryTexts: {
    ungrouped: t('dialogs.ungrouped'),
    localInstall: t('tools.localInstall'),
    unbound: t('modsLocal.unbound'),
  },
  pickI18n,
  displayTitle,
}))

const allGroupKeys = computed(() => visibleGroups.value.map((node) => node.key))
const {
  expandedKeys: groupsExpanded,
  onExpanded: onGroupsExpanded,
} = useLocalGroupExpansion(allGroupKeys, visibility.forceExpand, visibility.ignoringCollapse)

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
const statusHeaderSummary = computed(() => summarizeLocalModStatuses(statusUnitNodes.value))
const statusTagsPrimary = computed(() => statusHeaderSummary.value.primary)
const statusTagsToggle = computed(() => statusHeaderSummary.value.toggle)

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
  if (blockModOperation()) return
  busy.value[mod.dll_file] = true
  try {
    const result = await requestToggleChanges({ dllFiles: [mod.dll_file], enabled })
    if (!result) return
    result.ok
      ? message.success(result.message ? translateBackendMessage(result.message) : (enabled ? t('modsLocal.enabled2') : t('modsLocal.disabled')))
      : message.error(translateBackendMessage(result.message))
    await load()
  } catch (reason) { fail(reason) } finally { busy.value[mod.dll_file] = false }
}

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
function actionContext(labels: readonly string[] | undefined, mod: InstalledMod, conflictMod?: InstalledMod): {
  action: StatusActionSpec
  repository?: ModRepository
  /** 冲突要卸载的目标：可能是别的 MOD，不一定是当前这张卡片。 */
  conflictMod?: InstalledMod
} | undefined {
  // 纯依赖 DLL 没有版本语义，任何快捷动作都没有意义
  if (mod.is_dependency) return undefined
  const action = statusActionFor(labels, !!conflictMod)
  if (!action) return undefined

  const repository = repositoryForMod(repositories.value, mod)
  switch (action.kind) {
    case 'uninstall-duplicate':
      // A package's aggregated label must not target a different member.
      return mod.duplicate_cleanup_available ? { action } : undefined
    case 'update':
      // 没有仓库或 RAW 仓库：点下去只会下载一份和本地一样的文件
      if (!repository || repository.download_mode === 'repository_file') return undefined
      return { action, repository }
    case 'refresh':
      // 刷新针对仓库而不是某个 DLL，解析不出仓库就不给按钮
      if (!repository) return undefined
      return { action, repository }
    case 'uninstall-conflict': return conflictMod ? { action, conflictMod } : undefined
  }
}

/** 单 MOD 卡片的快捷动作。 */
function cardAction(mod: InstalledMod) {
  return actionContext(mod.labels, mod, conflictRemovalTarget(relationsByDll.value[mod.dll_file] ?? []))
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

/** 包标题的快捷动作：用包对外公开的状态集合判断。 */
function packageAction(pkg: ModPackageNode) {
  const base = pkg.coreMods.find(mod => repositoryForMod(repositories.value, mod)) ?? pkg.coreMods[0]
  if (!base) return undefined
  // Only member cards can unambiguously identify the duplicate DLL to remove.
  const keys = pkg.statuses.map(entry => entry.key).filter(key => key !== 'duplicate_lower_version')
  return actionContext(keys, base, conflictRemovalTarget(packageRelations(pkg)))
}

/**
 * 执行快捷动作。
 *
 * 三个动作走的是不同链路，这是它们唯一的分歧点：
 *   - update：预下载 → 安装详情弹窗 → 用户确认（与仓库卡片安装同一套）
 *   - refresh：只重拉这一条仓库的 Release，不下载任何文件
 *   - uninstall-conflict：把关系里指名的那个 MOD 交给统一卸载确认链
 */
function runCardAction(mod: InstalledMod) {
  const context = cardAction(mod)
  if (!context) return
  if (context.action.kind === 'update' && blockModOperation()) return
  if (context.action.kind === 'uninstall-duplicate') {
    removeOlderCopy(mod)
  } else if (context.action.kind === 'update' && context.repository) {
    void previewModRepository(context.repository)
  } else if (context.action.kind === 'uninstall-conflict' && context.conflictMod) {
    // 冲突目标是对方 MOD；复用右侧垃圾桶的完整确认、整包确认、忙碌与刷新流程。
    remove(context.conflictMod)
  } else if (context.repository) {
    void refreshModRepository(context.repository)
  }
}

function removeOlderCopy(mod: InstalledMod) {
  remove(mod, {
    title: t('modsLocal.uninstallOlderCopy'),
    body: (name) => `${deleteConfirm.value.body(mod.version ? `${name} ${mod.version}` : name)} ${t('modsLocal.keepNewerCopy')}\n${mod.dll_file}`,
    duplicateOnly: true,
  })
}

function removeCardMod(mod: InstalledMod) {
  if (mod.duplicate_cleanup_available) removeOlderCopy(mod)
  else remove(mod)
}

function runPackageAction(pkg: ModPackageNode) {
  const context = packageAction(pkg)
  if (!context) return
  if (context.action.kind === 'update' && blockModOperation()) return
  if (context.action.kind === 'uninstall-conflict' && context.conflictMod) {
    // 包级冲突同样直接卸载关系目标，不能误删当前包或只做页面定位。
    remove(context.conflictMod)
    return
  }
  if (context.repository && context.action.kind === 'update') void previewModRepository(context.repository)
  else if (context.repository) void refreshModRepository(context.repository)
}

/** 单仓刷新：复用刷新引擎，只更新这一条仓库的缓存。 */
async function refreshModRepository(repo: ModRepository) {
  // 用户主动点刷新 → 强制请求，不吃 5 分钟 Release 缓存
  const error = await refreshOne(repo)
  if (error) message.error(error)
}

/* ===== 页内定位：点标题后的聚合标签 ===== */

/** 卡片锚点：聚合标签按状态定位时用它找 DOM，与跨页高亮的锚点同一套命名。 */
function modAnchor(mod: InstalledMod) {
  return mod.dll_file
}

/**
 * 状态筛选的「单元」：单 MOD 一个，多 DLL 包整包一个。
 *
 * 包是页面上的一个整体（一个外壳、一条标题、一个版本结论），状态结论也属于
 * 整包；筛选时按整包一起显示或一起隐藏，不能把包内 DLL 拆开判断。
 */
const statusUnitNodes = computed(() => localModStatusUnits(groups.value))

/**
 * 状态筛选按「包」收窄：包内多个 DLL 共享一个状态结论，整包一起显示或一起
 * 隐藏。页头计数仍读 `statusUnitNodes`（筛选前的全量），否则筛完之后其它
 * 标签会全部归零、没法再点回来。
 */
const visiblePackageKeys = computed(() => packageKeysMatchingStatus(
  statusUnitNodes.value,
  (keys) => visibility.statusMatches(keys),
))
const visibleGroups = computed(() => filterLocalModGroups(groups.value, visiblePackageKeys.value))
const visibleUnitCount = computed(() => visiblePackageKeys.value.size)

function packageRelations(pkg: ModPackageNode): RelationStatus[] {
  return pkg.mods.flatMap(mod => relationsByDll.value[mod.dll_file] ?? [])
}

function packageRelationTag(pkg: ModPackageNode, tagKey: string): RelationStatus | undefined {
  const tag = dependencyTagFor(packageRelations(pkg), true)
  return tag?.key === tagKey ? tag.dependency : undefined
}

function packageTagTargetId(pkg: ModPackageNode, tagKey: string): string | undefined {
  return isRelationCardKey(tagKey) ? relationJumpTarget(packageRelationTag(pkg, tagKey))?.id : pkg.repositoryId
}

function currentPackage(key: string): ModPackageNode | undefined {
  return groups.value.flatMap(group => group.packages).find(entry => entry.key === key)
}

function modRepairStatus(mod: InstalledMod) {
  return dependencyRepairStatus(relationsByDll.value[mod.dll_file] ?? [])
}

function packageRepairStatus(pkg: ModPackageNode) {
  return dependencyRepairStatus(packageRelations(pkg))
}

function repairModDependencies(mod: InstalledMod) {
  dependencyNavigation.request(() => {
    const current = effectiveMods.value.find(entry => entry.dll_file === mod.dll_file)
    const issue = current && modRepairStatus(current)
    return issue && dependencyRow(issue)
  })
}

function repairPackageDependencies(pkg: ModPackageNode) {
  dependencyNavigation.request(() => {
    const current = currentPackage(pkg.key)
    const issue = current && packageRepairStatus(current)
    return issue && dependencyRow(issue)
  })
}

function openPackageTag(pkg: ModPackageNode, tagKey: string) {
  if (isRelationCardKey(tagKey)) {
    dependencyNavigation.request(() => {
      const current = currentPackage(pkg.key)
      const issue = current && packageRelationTag(current, tagKey)
      return issue ? dependencyRow(issue) : undefined
    })
    return
  }
  const id = packageTagTargetId(pkg, tagKey)
  if (id) linkToModRepository(id)
}

function packageTagTitle(pkg: ModPackageNode, tagKey: string): string | undefined {
  if (isRelationCardKey(tagKey)) {
    const status = packageRelationTag(pkg, tagKey)
    return status && dependencyTagTitle(dependencyRow(status))
  }
  return pkg.repositoryId ? t('dialogs.viewInModInstall') : undefined
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
 * 多 DLL 包外壳的依赖表行：合并包内每个成员自己的关系表。
 *
 * 包的关系声明挂在仓库上、整包共用，所以整包只出一张表，放在整批子卡片**下面**
 * （先看包里有什么，再看这个包对外要求什么）；子卡片不再各自显示，同一段声明
 * 重复 N 次只是噪音。
 *
 * 成员可以有不同的版本要求或具体原因。合并口径见 `mergedModDependencyRows`。
 */
function packageDependencyRows(pkg: ModPackageNode): DependencyRow[] {
  return mergedModDependencyRows(pkg.mods.filter((mod) => !mod.is_dependency), {
    mods: mods.value,
    repositories: repositories.value,
    translate: t,
    pickName: pickI18n,
  })
}

/**
 * 依赖表状态标签的跳转：可安装 / 可更新的行落到 MOD 安装页那张仓库卡片。
 *
 * 权威修复目标由共享导航交给整链；仅仓库导航进入这里并定位到指定版本。
 */
function openDependencyJump(row: DependencyRow) {
  const id = row.jump?.id
  if (id) linkToModRepository(id, row.jump?.targetVersion)
}

const dependencyNavigation = useDependencyNavigation(openDependencyJump)
function dependencyRow(status: RelationStatus): DependencyRow {
  return dependencyRowFromStatus(status, { mods: mods.value, repositories: repositories.value,
    translate: t, pickName: pickI18n, jumpToRepository: true })
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
  if (blockModOperation()) return
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

/** 手动来源、拖放与在线更新共用全局整链方案；清理暂存由流程持有。 */
const {
  sourceOpen: installSourceOpen,
  sourceBusy: installSourceBusy,
  handleSourceSelect: handleInstallSourceSelect,
  pickSourceFile: pickInstallSourceFile,
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
  busy: () => demoActive.value || installSourceBusy.value,
  onDrop: (path) => {
    void (async () => {
      if (await acceptDrop(path, 'mod')) await startDropInstall(path)
    })()
  },
})

/** 更新锁定选定根版本，下载验证后整批提交。 */
const previewing = ref<Record<string, boolean>>({})
const { previewModRepository } = useModRepositoryPreview({
  gameBlocked: blockModOperation,
  onBusyChange: (repositoryId, busy) => {
    if (busy) previewing.value[repositoryId] = true
    else delete previewing.value[repositoryId]
  },
  onApplied: () => load(),
})

/* ===== 包级操作 ===== */

/*
 * 包的折叠状态与独立卡是同一份外层状态（`expansion`）：两者互斥，收起包时
 * 包内子卡的展开状态也一起清。只记「用户展开过的包」而不是「收起过的包」：
 * 默认收起时，新扫出来的包不在集合里，本身就是默认收起，不需要额外登记。
 */

/** 包级开关：提交整包根到共享计划，一次确认切换整包。 */
async function togglePackageEnabled(node: ModPackageNode, enabled: boolean) {
  if (!node.grouped || blockModOperation()) return
  for (const mod of node.mods) busy.value[mod.dll_file] = true
  try {
    const result = await requestToggleChanges({ dllFiles: node.mods.map(mod => mod.dll_file), enabled, wholeNodes: true })
    if (!result) return
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
  if (!node.canRemovePackage || blockModOperation()) return
  const first = node.mods[0]
  if (!first) return
  const confirm = deletePackageConfirm.value
  const instance = dialog.warning({
    title: t('tools.deleteModPackage'),
    content: confirm.body(node.label, node.mods.length),
    positiveText: confirm.action,
    negativeText: t('recycle.cancel'),
    async onPositiveClick() {
      if (blockModOperation()) return false
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

/** 游戏运行中禁改盘：开关、删除、卸载都拦（与页面其它按钮同一判据）。 */
const kitchenRunning = computed(() => !!game.value?.kitchen_running)

/**
 * 折叠槽里的卡片由 `ModGroupBody` 渲染（原因见 `levelSetGroupContext.ts`）。
 * 这里把分类数据与页面操作一次性交出去；`reactive()` 负责解包 ref/computed。
 */
provide(
  MOD_GROUP_CTX,
  reactive<ModGroupContext>({
    groups: visibleGroups,
    busy,
    expansion,
    highlightId,
    toolsMinWidth: toolColumnWidth,
    toolGroupStyle,
    kitchenRunning,
    actionByDll,
    previewing,
    refreshing,
    modCardKey,
    modAnchor,
    bindingRepositoryId,
    singleModTitle,
    modTitle,
    isStatusPulsing,
    toggle,
    onCardClick,
    runCardAction,
    runPackageAction,
    packageAction: (pkg) => packageAction(pkg)?.action,
    canRepairMod: (mod) => !!modRepairStatus(mod),
    canRepairPackage: (pkg) => !!packageRepairStatus(pkg),
    repairModDependencies,
    repairPackageDependencies,
    openLogs,
    editConfig,
    openLocation,
    remove: removeCardMod,
    bindingText,
    statusSummary,
    packageCountText,
    modDependencyRows,
    packageDependencyRows,
    openDependencyJump,
    togglePackage,
    togglePackageEnabled,
    removePackage,
    packageTagTargetId,
    packageTagTitle,
    openPackageTag,
    linkToModRepository,
  }),
)
// Register eager watchers only after all computed dependencies are initialized.
visibility.reconcileStatuses(computed(() => [...statusTagsPrimary.value, ...statusTagsToggle.value].map(entry => entry.key)))
</script>

<template>
  <!--
    页头标题右侧：整页状态聚合，分两组 —— 左「结论」（有更新 / 缺依赖 / 最新版…），
    右「开关状态」（N 已启用 / N 已禁用），中间一条竖线断开。
    两组共用同一套 tone 与配色表，颜色永远一致；数量为 0 的状态不显示。
  -->
  <Teleport to="#page-header-title-extra" defer>
    <!-- 教程只认这一组：带数字的状态标签是「整页统计 + 点击定位」的入口 -->
    <span class="header-tag-group" data-tutorial="mods-local-tags">
      <n-tag v-for="entry in statusTagsPrimary" :key="entry.key" size="small" :bordered="false"
        class="state-tag state-tag--link" :class="{ 'state-tag--active': visibility.isStatusActive(entry.key) }"
        :color="tagColorForAny(entry.tone)" role="button" tabindex="0"
        :title="t('common.filterByStatus')" @click="visibility.toggleStatus(entry.key)"
        @keydown.enter.prevent="visibility.toggleStatus(entry.key)"
        @keydown.space.prevent="visibility.toggleStatus(entry.key)">
        {{ entry.count }} {{ t(entry.text) }}
      </n-tag>
    </span>
    <span v-if="statusTagsPrimary.length && statusTagsToggle.length" class="header-tag-divider" />
    <span class="header-tag-group">
      <n-tag v-for="entry in statusTagsToggle" :key="entry.key" size="small" :bordered="false"
        class="state-tag state-tag--link" :class="{ 'state-tag--active': visibility.isStatusActive(entry.key) }"
        :color="tagColorForAny(entry.tone)" role="button" tabindex="0"
        :title="t('common.filterByStatus')" @click="visibility.toggleStatus(entry.key)"
        @keydown.enter.prevent="visibility.toggleStatus(entry.key)"
        @keydown.space.prevent="visibility.toggleStatus(entry.key)">
        {{ entry.count }} {{ t(entry.text) }}
      </n-tag>
    </span>
  </Teleport>

  <!--
    页头右侧，从左到右：批量开关 → 检查更新 → 打开目录。
    「检查更新」只刷绑定仓库（`refreshBoundRepositories`），状态与 MOD 安装页
    是同一份：那边整体刷新或单源刷新时这里也会转，反过来也一样。放在 MOD 管理页
    是因为用户在这里看到「可更新」后，最自然的下一步就是「再查一次」。
  -->
  <Teleport to="#page-header-actions" defer>
    <!--
      搜索框与在线页同一位置。有词时显示「命中 / 全部」；批量开关走后端
      一次操作锁、作用于全部本地 MOD，搜索期间先收起，避免把筛选结果
      误当成它的作用范围。
    -->
    <span v-if="visibility.filtering" class="header-count muted">{{ visibility.countLabel(visibleUnitCount, statusUnitNodes.length) }}</span>
    <HeaderSearch v-model:value="search" />
    <!--
      批量开关：一个二选一分段控件（与发布详情面板「说明 / 下载安装」同款），
      替代原来的两个按钮 —— 同一时刻只有全开 / 全关两种状态，是互斥选择。
      · 全部启用 → 选中左侧；全部禁用 → 选中右侧；
      · 部分启用 → 两个都不选中（空串哨兵），由 .is-mixed 隐掉选中胶囊，
        否则胶囊会停在上一次的选中项上，看起来像已经全开 / 全关；
      · 禁用方向仍走 confirmDisableAll()：整页 MOD 全部停用不是随手试的操作；
      · 批量进行中两个分段都禁用，避免连点再发一批命令。
    -->
    <HeaderBulkSwitch v-if="visibility.showBulkSwitch(effectiveMods.length > 0)"
      data-tutorial="mods-local-bulk" :value="bulkSwitchValue" :disabled="bulkWorking" @change="onBulkSwitch" />
    <HeaderIconAction secondary :label="t('overview.check')" :loading="checkingBound" :disabled="checkingBound || refreshingAll"
      data-tutorial="mods-local-actions"
      :description="t('overview.refreshOnlyTheRepositoriesYourInstalledModsAreBoundToAndCheckForNewVersions')" @click="checkUpdates">
      <RefreshOutline width="16" height="16" aria-hidden="true" />
    </HeaderIconAction>
    <HeaderIconAction secondary :label="t('modsLocal.openFolder')" data-tutorial="mods-local-open-dir" @click="openModsDirectory">
      <FolderOpenOutline width="16" height="16" aria-hidden="true" />
    </HeaderIconAction>
  </Teleport>

  <!--
    安装流程四段：来源选择 → 同身份冲突检查 → 详情预览 → 确认安装。
    与 MOD 安装页、仓库卡片共用同一套组件与流程，信息完全一致；
    解析出来若本地已有同身份的旧装项，先在检查弹窗里卸干净再进详情。
  -->
  <InstallSourceCard v-model:show="installSourceOpen" :busy="installSourceBusy"
    :drag-active="dragActive" :pick-file="pickInstallSourceFile"
    @select="handleInstallSourceSelect" />

  <!--
    安装前检查：更新一个已装 MOD 时，若它的新版本还缺依赖 / 有冲突，
    在这里一次处理完再继续安装。与 MOD 安装页共用同一个弹窗与判定。
  -->


  <!--
    页头标题后的聚合标签已经承载了状态统计（数量、启用、可更新都在标签里），
    原来的 .toolbar 只重复这些数字，已无存在必要，移除。
  -->
  <PageLoading :show="loading">
    <template #status>{{ t('common.loading') }}</template>
    <ContentState v-if="!loading && !visibleGroups.length" :description="emptyDescription" />

    <!-- 按安装时绑定仓库的分类折叠：同一分类的 MOD 归在一起 -->
    <n-collapse v-else :expanded-names="groupsExpanded" class="group-collapse"
      @update:expanded-names="onGroupsExpanded">
      <!--
        折叠项里只挂组件、不写卡片：`n-collapse` 的动态 prop 只有
        `expanded-names`，同分类增删卡时它可能拿到同一个数组引用，整棵子树
        会被跳过。分类标题与分类内容各自有自己的渲染 effect，才跟得上数据。
      -->
      <n-collapse-item v-for="node in visibleGroups" :key="node.key" :name="node.key">
        <template #header>
          <ModGroupHeader :group-key="node.key" />
        </template>
        <ModGroupBody :group-key="node.key" />
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
.header-count { white-space: nowrap; flex: 0 0 auto; }

/* 分类折叠：分类标题行 + 内部单列 MOD 卡片 */
.group-collapse { width: 100%; }

/* 可点标签的手型与悬停反馈由 global.css 的 .state-tag--link 统一提供 */

/* 本地刷新进行中：图标自转（类由刷新按钮绑定） */
.spin-icon { animation: oc2-local-spin .8s linear infinite; }
@keyframes oc2-local-spin {
  from { transform: rotate(0deg); }
  to { transform: rotate(360deg); }
}
@media (prefers-reduced-motion: reduce) {
  .spin-icon { animation: none; }
}

/* 页头批量分段控件的样式在 src/styles/global.css 的 `.bulk-switch`。 */
</style>
