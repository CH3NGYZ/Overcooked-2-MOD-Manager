<script setup lang="ts">
import { t, locale } from '../i18n'
import { tagColorForAny } from '../theme'
import { PULSE_DURATION } from '../motion'
import {
  cardByExpandKey,
  collapseCardWithScroll,
  naturalContentHeight,
  scrollExpandedCardIntoView,
} from '../expandScroll'
import { computed, nextTick, onBeforeUnmount, onMounted, provide, reactive, ref, watch } from 'vue'
import { useDialog } from 'naive-ui'
import { useNotify } from '../useNotify'
import {
  NButton, NCollapse, NCollapseItem, NEmpty, NFloatButton, NTabPane, NTabs, NTag,
} from 'naive-ui'
import {
  FolderOpenOutline, RefreshOutline,
} from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { repositoryForMod } from '../modStatus'
import type { RelationStatus } from '../modRelations'
import { repositoryIsAvailable } from '../repositoryAvailability'
import {
  installedModTags, summarizeUnits, packageStatuses, type StatusSummary,
} from '../statusTags'
import { statusActionFor, type StatusActionSpec } from '../statusActions'
import { installedModDependencyRows, mergedModDependencyRows } from '../dependencyRows'
import type { DependencyRow } from '../dependencyRows'
import { useFocusHighlight, useRepositoryLink, useStatusHighlight } from '../crossLink'
import { runtimeModStatus, type RuntimeRefreshState } from '../modRuntimeStatus'
import { isDependencyTag, tagTargetTitle } from '../tagTargets'
import { describeRefreshOutcome } from '../refreshOutcomeText'
import { markDialogBusy } from '../dialogBusy'
import { useDashboard } from '../composables/useDashboardStore'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import { withDemoMods } from '../tutorial/demo'
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
const { load, ensureLoaded, loading, dashboard, game, config } = useDashboard()

/**
 * 本页展示的 MOD 列表 = 真实 Dashboard ∪ 教程示例卡。
 *
 * 只改展示层、不改 store：教程一结束 `withDemoMods` 立刻返回原始数组，
 * 页面不需要任何清理。真实列表非空时也不会混进示例（见 `tutorial/demo.ts`）。
 */
const mods = computed(() => withDemoMods(dashboard.value?.mods ?? []))

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
/**
 * 从 MOD 安装页跳回来时，高亮绑定到该仓库的 MOD 卡片 3 秒并滚动到它。
 *
 * 只高亮 + 滚到视口中间，**不展开卡片**：卡片根节点的 `[data-focus-id]` 在收起
 * 状态下也在 DOM 里，展开与否该由用户自己决定 —— 自动摊开第一张会把其余命中
 * 挤出视口，也违背「跳过来只是让我看到它」的预期。
 */
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
 * 卡片展开状态只有一套，规则都在 `useCardExpansion` 里：
 *   - 外层：独立 MOD 卡（键 `mod:<dll_file>`）与包外壳（键 `pkg:<package_id>`）
 *     算**同一级**，`singleOuter` 表示一次只展开其中一张。
 *   - 内层：包内子 MOD / 随包依赖，父键是包 key、子键是 `dll_file`。
 * 独立卡与包外壳必须落在同一个键空间里，互斥才覆盖得到两者；拆成两个实例时
 * 一张独立卡与一个包可以同时摊开，用户分不清自己点开的是哪一个。
 */
const expansion = useCardExpansion({ singleOuter: true })
/** 独立 MOD 卡的外层键前缀：点空白只收独立卡、不收盘外壳，靠它区分。 */
const MOD_CARD_PREFIX = 'mod:'
function modCardKey(dllFile: string) {
  return `${MOD_CARD_PREFIX}${dllFile}`
}

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
 * 「去卸载」：把关系里点名的那张已安装卡片滚到视口中间并高亮。
 *
 * 冲突的收尾确实是卸载「对方」，但这里不直接弹确认框：用户点的是 A 卡上的
 * 「去卸载」，要卸的却可能是 B —— 直接弹框等于让人对着一张还没见过的卡签字，
 * 卸错了还得重装一遍。先把 B 带到他眼前，删不删由他在那张卡上决定；从 B 的
 * 卡片删也走同一条 `remove` 流程（含后端要求整包确认的分支）。
 *
 * 分类收起时卡片不渲染（折叠体是 `n-collapse` 的内容插槽），`querySelector`
 * 会一直找不到，所以先展开目标所在的分类，并把「刚展开过」交给 `focusStatus`
 * 等高度动画结束再滚。`dependency_conflict` 只是用来点亮同状态的卡片，
 * 真正的落点由 `targetAnchor` 的脉冲指出来。
 */
function locateConflict(target: InstalledMod) {
  const anchor = modAnchor(target)
  const groupKey = groups.value.find((group) =>
    group.packages.some((pkg) => pkg.mods.some((mod) => mod.dll_file === target.dll_file)),
  )?.key
  let opened = false
  if (groupKey && collapsedGroups.value.includes(groupKey)) {
    collapsedGroups.value = collapsedGroups.value.filter((value) => value !== groupKey)
    opened = true
  }
  focusStatus(
    'dependency_conflict',
    () => document.querySelector<HTMLElement>(`[data-mod-anchor="${CSS.escape(anchor)}"]`),
    opened,
    anchor,
  )
}

/**
 * 内层卡片展开后的滚动（策略见 `expandScroll.ts`）。
 *
 * 内层展开不会被动收起别的卡片（外层互斥只发生在外层），所以不需要收缩补偿，
 * 展开后单独量一次就够。
 */
function scrollAfterExpand(card: HTMLElement | null | undefined, anchorSelector: string, bodySelector: string) {
  if (!card) return
  const anchor = card.querySelector(anchorSelector)
  if (!anchor) return
  void nextTick().then(() => {
    scrollExpandedCardIntoView({ card, anchor, body: card.querySelector(bodySelector) })
  })
}

/** 一张外层卡片的内容层：包外壳是 `.package-body`，独立卡是 `.card-detail`。 */
function bodyOfOuterCard(card: HTMLElement | null) {
  if (!card) return null
  return card.classList.contains('package-shell')
    ? card.querySelector('.package-body')
    : card.querySelector('.card-detail')
}

/**
 * 展开一张外层卡片：单展开下上一张会被动收起，先把它的收缩量量好再改状态。
 *
 * `shrink` 必须在改状态之前量：外层收起会连带清掉它的内层展开态，改完再量，
 * 内层已经率先塌掉，收缩量被算少 —— 下方卡片的标题会被上方收起动画一起
 * 拽向视口顶（与 `expandScroll.ts` 里同一个 247px 量级的坑）。
 *
 * 滚动只在这一处发生：调用点不许再补一次 `scrollExpandedCardIntoView`，
 * 那样会把同一次展开滚两遍。
 */
function expandWithScroll(
  card: HTMLElement | null | undefined,
  key: string,
  anchorSelector: string,
  bodySelector: string,
  open: () => void,
) {
  const previousKey = expansion.outerKeys.value.find((openKey) => openKey !== key)
  const previousCard = previousKey ? cardByExpandKey(previousKey) : null
  const previousBody = bodyOfOuterCard(previousCard)
  const shrink = previousBody ? naturalContentHeight(previousBody) : 0
  open()
  void nextTick().then(() => {
    // 跨页 focus 展开时卡片可能刚随数据渲染出来，调用点拿不到元素，这里按键补找
    const target = card ?? cardByExpandKey(key)
    const anchor = target?.querySelector(anchorSelector)
    if (!target || !anchor) return
    scrollExpandedCardIntoView({
      card: target,
      anchor,
      body: target.querySelector(bodySelector),
      collapsing: previousCard ? { card: previousCard, body: previousBody, shrink } : null,
    })
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
   * 独立卡是外层（与包外壳同一级、互斥），包内子卡是内层（父键是包 key）；
   * 传了包 key 说明这是包内子卡，否则是独立卡。两侧交互一致：展开时滚到
   * 可见，收起时把展开那次滚动倒回去。
   */
  if (packageKey === undefined) {
    const key = modCardKey(mod.dll_file)
    if (expansion.isOuterOpen(key)) {
      collapseWithScroll(card, '.card-detail', () => expansion.closeOuter(key))
      return
    }
    expandWithScroll(card, key, '.card-row', '.card-detail', () => expansion.openOuter(key))
    return
  }
  if (expansion.isInnerOpen(packageKey, mod.dll_file)) {
    collapseWithScroll(card, '.card-detail', () => expansion.closeInner(packageKey, mod.dll_file))
    return
  }
  expansion.openInner(packageKey, mod.dll_file)
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
   * 「点空白收起」的范围只到独立卡与全部内层：包外壳的展开 / 收起由标题行
   * 自己的点击控制（`togglePackage`），点包 body 里的空白把整包装收掉会让
   * 用户以为自己点错了位置 —— 这条与改动前一致。外层键用前缀区分，逐个走
   * `closeOuterWhere`（它连带清内层），页面不自己删键。
   */
  const openCards = [
    ...expansion.outerKeys.value
      .filter((key) => key.startsWith(MOD_CARD_PREFIX))
      .map((key) => key.slice(MOD_CARD_PREFIX.length)),
    ...expansion.innerKeys.value.map((key) => expansion.childOf(key)),
  ]
  if (!openCards.length) return
  const target = event.target
  if (target instanceof Element && target.closest('.card-row, .package-head')) return
  const collapseAll = () => {
    expansion.closeOuterWhere((key) => key.startsWith(MOD_CARD_PREFIX))
    expansion.closeAllInner()
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
  const needsRelation = keys.some((key) => isDependencyTag(key))
  return actionContext(keys, (needsRelation ? relationOwnerMod(pkg) : undefined) ?? base)
}

/**
 * 执行快捷动作。
 *
 * 三个动作走的是不同链路，这是它们唯一的分歧点：
 *   - update：预下载 → 安装详情弹窗 → 用户确认（与仓库卡片安装同一套）
 *   - refresh：只重拉这一条仓库的 Release，不下载任何文件
 *   - install-dependency：跳到 MOD 安装页并定位到该仓库卡片
 *   - uninstall-conflict：跳到关系里指名的那个 MOD 卡片上（由用户自己决定删不删）
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
    locateConflict(context.conflictMod)
  } else if (context.repository) {
    void refreshRepository(context.repository)
  }
}

function runPackageAction(pkg: ModPackageNode) {
  const context = packageAction(pkg)
  if (!context) return
  if (context.action.kind === 'uninstall-conflict' && context.conflictMod) {
    // 要卸的是「对方」；先带用户到那张卡上（见 `locateConflict`）
    locateConflict(context.conflictMod)
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
  if (isDependencyTag(tagKey)) {
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
 * 多 DLL 包外壳的依赖表行：合并包内每个成员自己的关系表。
 *
 * 包的关系声明挂在仓库上、整包共用，所以整包只出一张表，放在整批子卡片**下面**
 * （先看包里有什么，再看这个包对外要求什么）；子卡片不再各自显示，同一段声明
 * 重复 N 次只是噪音。
 *
 * 不能只取一个代表成员：`release_relations` 按已装版本筛，一支 3.10.2 的 DLL
 * 命中 `==3.10.2` 规则要求「DIYLevel ≥ 0.10.0」，另一支 2.0.0 只拿到仓库级
 * 声明（不设区间）。只取首个成员会让表里写「已满足」、包标题却按另一支算出
 * 「缺依赖」—— 同一张卡上下自相矛盾。合并口径见 `mergedModDependencyRows`。
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
 * 包的折叠状态与独立卡是同一份外层状态（`expansion`）：两者互斥，收起包时
 * 包内子卡的展开状态也一起清。只记「用户展开过的包」而不是「收起过的包」：
 * 默认收起时，新扫出来的包不在集合里，本身就是默认收起，不需要额外登记。
 */

/**
 * 包外壳的展开 / 收起。
 *
 * 滚动策略与独立 MOD 卡片同一套（`expandWithScroll` → `expandScroll.ts`）：
 * 包 body 撑开后优先让整包可见，收起时把展开那次滚动倒回去。收起必须交给
 * `collapseWithScroll`：它先量折叠前的位置再改状态，自己先改就量不到了。
 */
function togglePackage(node: ModPackageNode, event: MouseEvent) {
  const head = event.currentTarget
  const shell = head instanceof Element ? head.closest('.package-shell') : null
  const card = shell instanceof HTMLElement ? shell : null
  if (expansion.isOuterOpen(node.key)) {
    // 收起连带清掉包内子卡的展开状态（`useCardExpansion` 的规则）：
    // 下次展开是一个全新的包，子卡要重新点开才展开。
    collapseWithScroll(card, '.package-body', () => expansion.closeOuter(node.key))
    return
  }
  expandWithScroll(card, node.key, '.package-head', '.package-body', () => expansion.openOuter(node.key))
}

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

/** 游戏运行中禁改盘：开关、删除、卸载都拦（与页面其它按钮同一判据）。 */
const kitchenRunning = computed(() => !!game.value?.kitchen_running)

/**
 * 折叠槽里的卡片由 `ModGroupBody` 渲染（原因见 `levelSetGroupContext.ts`）。
 * 这里把分类数据与页面操作一次性交出去；`reactive()` 负责解包 ref/computed。
 */
provide(
  MOD_GROUP_CTX,
  reactive<ModGroupContext>({
    groups,
    busy,
    expansion,
    highlightId,
    pulseTarget,
    pulseSeq,
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
    openLogs,
    editConfig,
    openLocation,
    remove,
    bindingText,
    statusSummary,
    packageCountText,
    modDependencyRows,
    packageDependencyRows,
    openDependencyJump,
    matchesActiveStatus,
    togglePackage,
    togglePackageEnabled,
    removePackage,
    packageTagTargetId,
    packageTagTitle,
    openPackageTag,
    linkToRepository,
  }),
)
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
      data-tutorial="mods-local-bulk" :class="{ 'is-mixed': !bulkSwitchValue }" :value="bulkSwitchValue"
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
      data-tutorial="mods-local-actions"
      :title="t('overview.refreshOnlyTheRepositoriesYourInstalledModsAreBoundToAndCheckForNewVersions')" @click="checkUpdates">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('overview.check') }}
    </n-button>
    <n-button size="small" secondary data-tutorial="mods-local-open-dir" @click="openModsDirectory">
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
      <!--
        折叠项里只挂组件、不写卡片：`n-collapse` 的动态 prop 只有
        `expanded-names`，同分类增删卡时它可能拿到同一个数组引用，整棵子树
        会被跳过。分类标题与分类内容各自有自己的渲染 effect，才跟得上数据。
      -->
      <n-collapse-item v-for="node in groups" :key="node.key" :name="node.key">
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
