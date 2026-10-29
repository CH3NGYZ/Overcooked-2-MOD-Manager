<script setup lang="ts">
import { t, locale } from '../i18n'
import { buttonColorFor, tagColorFor, tagColorForAny } from '../theme'
import { PULSE_DURATION } from '../motion'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useDialog } from 'naive-ui'
import { useNotify } from '../useNotify'
import {
  NButton, NCollapse, NCollapseItem, NCollapseTransition, NEmpty, NFloatButton,
  NSpin, NSwitch, NTag,
} from 'naive-ui'
import {
  ArrowUpCircleOutline, DocumentTextOutline, DownloadOutline, FolderOpenOutline, RefreshOutline,
  SettingsOutline, TrashOutline,
} from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { repositoryForMod } from '../modStatus'
import { primaryRelation, resolveRelations, type RelationStatus } from '../modRelations'
import { repositoryIsAvailable } from '../repositoryAvailability'
import {
  installedModTags, modUpdateVerdict, replaceVersionConclusion,
  summarizeUnits, packageStatuses, type PackageStatus, type StatusSummary,
} from '../statusTags'
import { statusActionFor, type StatusActionSpec } from '../statusActions'
import { useFocusHighlight, useRepositoryLink, useStatusHighlight } from '../crossLink'
import { useDashboard } from '../composables/useDashboardStore'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import ModConfigEditor from '../components/ModConfigEditor.vue'
import ModDetailGrid from '../components/ModDetailGrid.vue'
import ModInstallDialog from '../components/ModInstallDialog.vue'
import ModInstallSourceDialog from '../components/ModInstallSourceDialog.vue'
import { useModInstallFlow } from '../composables/useModInstallFlow'
import { useRepositoryPreview } from '../composables/useRepositoryPreview'
import { useModRemoval } from '../composables/useModRemoval'
import type { InstalledMod, Repository } from '../types'

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
      body: (name: string) => t('「{name}」将被移入回收站，可在回收站中恢复。继续？', { name }),
      action: t('移入回收站'),
      done: '已移入回收站',
    }
  : {
      body: (name: string) => t('「{name}」将被直接删除，无法恢复。继续？', { name }),
      action: t('直接删除'),
      done: '已删除',
    })

/** 整包删除的确认文案与动作名：同样随开关切换。 */
const deletePackageConfirm = computed(() => recyclesOnDelete.value
  ? {
      body: (name: string, count: number) =>
        t('「{name}」的 {count} 个文件将被整包移入回收站，可在回收站中恢复。继续？', { name, count }),
      action: t('整包移入回收站'),
      done: '已移入回收站',
    }
  : {
      body: (name: string, count: number) =>
        t('「{name}」的 {count} 个文件将被直接删除，无法恢复。继续？', { name, count }),
      action: t('整包删除'),
      done: '已删除',
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
/** 刷新状态：用于把「正在抓取 Release」的 MOD 显示成检查中而不是旧结论。 */
const { refreshing, refreshingAll, refreshOne, refreshMany } = useRepositoryRefresh()

/**
 * 检测更新：刷新**全部**仓库源与 Release，等价于 MOD 安装页的「刷新全部」。
 *
 * 复用同一引擎入口（`refreshEverything`）而不是另写一条调用链：
 * 它会先清本次源的旧缓存、等排队请求结束，再重拉源文档并逐个刷新 Release，
 * 这些顺序是「刷新后不显示旧列表」的前提。这里自己拼调用顺序必然分叉。
 */
const checkingUpdates = ref(false)

/**
 * 检测更新：**只刷新与已安装 MOD 绑定的仓库**。
 *
 * 与 MOD 安装页的「刷新全部」不是一回事：
 *   · 那边是「浏览可用 MOD」，要重新拉源文档、刷新所有仓库；
 *   · 这里是「我这几个 MOD 有没有新版」，只关心它们各自绑定的仓库。
 *
 * 复用 `refreshMany`（它本来就是「刷新给定的仓库列表」），不需要新后端命令：
 * 后端按仓库 ID 刷新 Release 的能力已经存在，只是这里传入的 ID 集合更小。
 *
 * 顺带的好处：装 4 个 MOD、来自 3 个仓库时只发 3 个 Release 请求，
 * 而不是二十多个 —— 更快，也不浪费匿名 API 额度。
 */
async function checkUpdates() {
  if (checkingUpdates.value || refreshingAll.value) return
  /*
   * 收集「当前地区可见、且确实有本地 MOD 绑定」的仓库。
   *
   * 三点约束：
   *   · 用 `repositoryIsAvailable` 而不是只看 enabled —— 地区不适用等于
   *     该仓库不存在，不能给它发请求（否则会报「不属于当前下载渠道」）；
   *   · 跳过 RAW 模式：它没有 Release 可查（`refreshMany` 内部也会跳过，
   *     这里先滤掉避免无意义的排队）；
   *   · 同一仓库只刷一次：多个 MOD 常来自同一个仓库（同一个包里的多个 DLL）。
   */
  const region = config.value?.region
  const sources = config.value?.repository_sources
  const targets = new Map<string, Repository>()
  for (const mod of effectiveMods.value) {
    if (mod.is_dependency) continue
    const repo = repositoryForMod(repositories.value, mod)
    if (!repo) continue
    if (!repositoryIsAvailable(repo, sources, region)) continue
    if (repo.download_mode === 'repository_file') continue
    targets.set(repo.id, repo)
  }

  if (!targets.size) {
    message.warning(t('没有已安装 MOD 绑定到可用的在线仓库'))
    return
  }
  void api.writeFrontendLog('info',
    `检测更新：匹配到 ${targets.size} 个仓库（已安装 MOD 共 ${effectiveMods.value.length} 个）`)

  checkingUpdates.value = true
  try {
    const errors = await refreshMany([...targets.values()])
    errors.forEach((error) => message.error(error))
  } catch (reason) {
    fail(reason)
  } finally {
    checkingUpdates.value = false
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
const { activeStatus, focusStatus } = useStatusHighlight()

/*
 * 固定按名称排序。
 *
 * 原来的「排序」切换入口是右下角浮动按钮，现在该位置改成了「重新扫描
 * 本地 MOD」，排序已无切换入口，所以把配置一起去掉 —— 留一个永远不变的
 * sortBy 只会误导后续维护者。
 */
const busy = ref<Record<string, boolean>>({})
const expanded = ref<Record<string, boolean>>({})

// 切页面复用已有 Dashboard；开关/删除后仍用 load() 取最新扫描结果
onMounted(() => { void ensureLoaded() })

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
    message.success(t('已重新扫描本地 MOD'))
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    refreshingLocal.value = false
  }
}

/**
 * 把「后端扫描结果」和「当前仓库缓存」合成一份有效状态。
 *
 * 后端 `labels` / `update_available` 是扫描那一刻算的，而仓库 Release 是
 * 异步刷新回来的；刷新期间 Dashboard 不重读（会重扫磁盘），标签就会停在
 * 上一次的结果。这里用当前仓库缓存重算，让刷新结果立刻生效，
 * 且整个过程是纯内存计算 —— 刷一个仓库只影响它绑定的那几个 MOD。
 *
 * 三层来源，优先级从高到低：
 *   1. 仓库有 Release 且版本可比 → 用缓存重算（刷新完立刻生效）
 *   2. 该仓库正在刷新且没有可比 Release → 显示「检查中」，
 *      而不是拿上一次的旧结论冒充当前状态
 *   3. 其余情况 → 保留后端扫描时的标签
 */
const effectiveMods = computed<InstalledMod[]>(() =>
  mods.value.map((mod) => {
    /*
     * 关系判定先算：它和版本结论是两件独立的事（可以同时「有更新」和
     * 「缺依赖」/「冲突」），所以要各自往 labels 里加，不能互相替换。
     */
    const issue = relationIssue(mod)
    const withRelation = (labels: InstalledMod['labels']): InstalledMod['labels'] => {
      const base = (labels as string[] | undefined)
        ?.filter((key) => key !== 'missing_dependency' && key !== 'dependency_conflict') ?? []
      if (!issue) return base as InstalledMod['labels']
      // 冲突比缺依赖更该被看见：缺依赖只是不工作，冲突可能已经让游戏崩了
      const label = issue.state === 'conflict' ? 'dependency_conflict' : 'missing_dependency'
      return [label, ...base] as InstalledMod['labels']
    }

    const verdict = modUpdateVerdict(mod, repositories.value)
    if (verdict.known) {
      /*
       * 用统一函数替换版本结论，而不是自己拼数组。
       * 之前的实现只过滤 latest/update_available，会把 not_release_asset、
       * repository_unavailable 等旧结论留在原地，出现「最新版 + 仓库不可用」
       * 这种自相矛盾的组合。
       */
      return {
        ...mod,
        update_available: verdict.updateAvailable,
        labels: withRelation(replaceVersionConclusion(
          mod.labels as string[] | undefined,
          verdict.updateAvailable ? 'update_available' : 'latest',
        ) as InstalledMod['labels']),
      } as InstalledMod
    }
    // 仓库还在抓取：明确显示「检查中」，不把旧结论当现状。
    // 但仓库文件（RAW）模式本来就没有版本可比较，刷新也不该让它变成
    // 「版本未知」—— 它的语义就是「已安装」，保持原样即可。
    const repo = repositoryForMod(repositories.value, mod)
    if (repo && repo.download_mode !== 'repository_file' && isRepositoryRefreshing(repo.id)) {
      return {
        ...mod,
        update_available: false,
        labels: withRelation(replaceVersionConclusion(mod.labels as string[] | undefined, 'checking') as InstalledMod['labels']),
      } as InstalledMod
    }
    return { ...mod, labels: withRelation(mod.labels) } as InstalledMod
  }),
)

/**
 * 这个本地 MOD 与其它插件的第一个未满足关系（只看它自己绑定的仓库声明）。
 *
 * 用**已安装版本**筛 `release_relations`：装的是 1.0 就该按 1.0 的要求算，
 * 而不是最新 Release 的要求。
 *
 * 只判断「有没有问题」，不决定「怎么处理」；冲突要卸载哪个 MOD 由
 * `status.installedMod` 给出。
 */
function relationIssue(mod: InstalledMod) {
  if (mod.is_dependency) return undefined
  const repo = repositoryForMod(repositories.value, mod)
  if (!repo) return undefined
  const statuses = resolveRelations(repo, mods.value, repositories.value, mod.version)
  return primaryRelation(statuses)
}

/**
 * 每个本地 MOD 的关系问题，按 dll_file 索引。
 *
 * 模板与动作处理都要读它（判断标签、决定卸载哪个 MOD、找可安装仓库），
 * 每次重算一遍全部关系太浪费，派生一次即可。
 */
const relationIssueByDll = computed<Record<string, RelationStatus>>(() => {
  const map: Record<string, RelationStatus> = {}
  for (const mod of mods.value) {
    const issue = relationIssue(mod)
    if (issue) map[mod.dll_file] = issue
  }
  return map
})

/** 某个仓库当前是否正在抓取 Release（单仓刷新或整批刷新都算）。 */
function isRepositoryRefreshing(repositoryId: string) {
  return !!refreshing.value[repositoryId] || refreshingAll.value
}

/** 列表固定按标题排序（排序切换入口已移除，见上方说明）。 */
const sortedMods = computed(() =>
  [...effectiveMods.value].sort((a, b) => modTitle(a).localeCompare(modTitle(b))),
)

/* ===== 全部启用 / 全部禁用 =====
 *
 * 这两个动作服务「一次性切换所有 MOD」的场景，入口只能有一个：
 * MOD 管理页的页头。放在总览页是错的 —— 它管理的是 MOD，
 * 离 MOD 列表越近越容易理解后果（页面上每张卡的开关会一起变）。
 */
const bulkWorking = ref(false)
const enabledModCount = computed(() => effectiveMods.value.filter((mod) => mod.enabled).length)
const allModsEnabled = computed(() =>
  effectiveMods.value.length > 0 && enabledModCount.value === effectiveMods.value.length)

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
      message.success(result.message)
      await load()
    } else {
      message.error(result.message)
    }
  } catch (reason) { fail(reason) } finally { bulkWorking.value = false }
}

function confirmDisableAll() {
  dialog.warning({
    title: t('禁用全部 MOD'),
    content: t('将禁用全部 {count} 个 MOD，游戏将不加载任何 MOD。可以随时重新启用。',
      { count: effectiveMods.value.length }),
    positiveText: t('全部禁用'),
    negativeText: t('取消'),
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
  count: number
  updateCount: number
  enabled: boolean
  partial: boolean
}

const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])

/**
 * 状态变化脉冲：某个 MOD 的标签从「检查中」变成「最新版 / 可更新」时，
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
      const label = pickI18n(snapshot, '') || t('未分组')
      return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
    }
    // key 用语言无关的原始分组名，切换语言时折叠状态不丢
    const canonical = (repo.group || repo.group_i18n?.['zh-CN'] || pickI18n(repo.group_i18n, '')).trim()
    const label = pickI18n(repo.group_i18n, repo.group || '') || t('未分组')
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  if (mod.manual_install) return { key: '__local_install__', label: t('本地安装') }
  if (hasSnapshot) {
    const canonical = (snapshot!['zh-CN'] || pickI18n(snapshot, '')).trim()
    const label = pickI18n(snapshot, '') || t('未分组')
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  return { key: '__unbound__', label: t('未绑定') }
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
    const enabledCount = list.filter((mod) => mod.enabled).length
    /*
     * 包标题：能认出绑定仓库就用仓库名，让「装的是哪个包」一眼可读；
     * 否则退回安装包名与插件名。整包成员共享一个仓库，所以同一包内
     * 各 DLL 仍由行内插件名区分，不会因为标题统一而重名。
     */
    const label = list[0] ? displayTitle(list[0]) : ''
    /*
     * 包级状态：以整个包为主体，版本结论只留一条且不带数量
     * （包里只有一个 Release tag，说「1 最新版」会暗示还有别的东西）。
     * 实现见 statusTags.ts 的 packageStatuses。
     */
    const statuses = packageStatuses(list)
    /*
     * 包的仓库身份与快捷动作：整包成员共享同一个仓库，
     * 所以直接用「包内能绑定仓库的那一个」——与包标题、分组用的是同一代表。
     * 动作由包对外公开的状态集合解析（与单 MOD 共用 statusActions 的规则）。
     */
    const repo = repositoryForMod(repositories.value, representative(list))
    const representativeMod = representative(list)
    const action = actionContext(statuses.map((entry) => entry.key), representativeMod)?.action
    return {
      key,
      label,
      mods: list,
      grouped: list.length >= 2,
      enabled: enabledCount === list.length,
      partial: enabledCount > 0 && enabledCount < list.length,
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
    .map(([key, { label, packages }]) => {
      const nodes = [...packages.entries()].map(([pkgKey, list]) => toPackage(pkgKey, list))
      const all = nodes.flatMap((node) => node.mods)
      const enabledCount = all.filter((mod) => mod.enabled).length
      return {
        key,
        label,
        packages: nodes,
        /*
         * 分类计数按「展示单元」而不是 DLL：一个多 DLL 包在页面上只占
         * 一个外壳，说「3 个」而实际只看到两张卡片会让人以为是漏渲染。
         */
        count: nodes.length,
        updateCount: nodes.filter((node) =>
          node.statuses.some((entry) => entry.key === 'update_available'),
        ).length,
        enabled: enabledCount === all.length,
        partial: enabledCount > 0 && enabledCount < all.length,
      }
    })
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
  summarizeUnits(
    groups.value.flatMap((node) =>
      node.packages.map((pkg) => ({
        // 包对外的状态就是包标题上那几个；标记为禁用的不算版本结论
        keys: pkg.statuses.map((entry) => entry.key),
      })),
    ),
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
  message.error(reason instanceof Error ? reason.message : String(reason))
}

async function toggle(mod: InstalledMod, enabled: boolean) {
  busy.value[mod.dll_file] = true
  try {
    const result = await api.toggleLocalMod(mod.dll_file, enabled)
    result.ok ? message.success(result.message || (enabled ? '已启用' : t('已禁用'))) : message.error(result.message)
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
    title: t('卸载冲突的 MOD'),
    body: (name) => t('「{name}」与「{owner}」不兼容，需要卸载。继续？',
      { name, owner: modTitle(mod) }),
  })
}

function toggleExpand(dll: string) {
  expanded.value[dll] = !expanded.value[dll]
}
function isExpanded(dll: string) {
  return !!expanded.value[dll]
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

function onCardClick(event: MouseEvent, mod: InstalledMod) {
  const target = event.target
  if (target instanceof Element && target.closest('.card-detail')) return
  cardClickHandled = true
  toggleExpand(mod.dll_file)
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
  const openKeys = Object.keys(expanded.value).filter((key) => expanded.value[key])
  if (!openKeys.length) return
  const target = event.target
  if (target instanceof Element && target.closest('.card-row, .package-head')) return
  // 只把已展开的项置为 false，不动其它键
  const next = { ...expanded.value }
  for (const key of openKeys) next[key] = false
  expanded.value = next
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

/** 该 MOD 绑定仓库的 id（优先安装清单，其次身份绑定），用于标签跳转。 */
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
    case 'install-dependency':
      /*
       * 「去安装」只对本地 MOD 有意义，且必须有仓库能提供依赖。
       *
       * 这里不做「有没有可安装来源」的判定 —— 那是安装页的职责
       * （`unmetDependency`）。本地卡片只需要区分「缺」和「不缺」，
       * 点击后跳去安装页，由那边决定给按钮还是只给标签。
       */
      if (!repository) return undefined
      return { action, repository }
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

/** 包标题的快捷动作：用包对外公开的状态集合判断。 */
function packageAction(pkg: ModPackageNode) {
  const first = pkg.mods.find((mod) => !mod.is_dependency) ?? pkg.mods[0]
  if (!first) return undefined
  return actionContext(pkg.statuses.map((entry) => entry.key), first)
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
    if (first) uninstallConflict(first, context.conflictMod)
    return
  }
  if (!context.repository) return
  if (context.action.kind === 'update') void previewRepository(context.repository)
  else if (context.action.kind === 'install-dependency') linkToRepository(context.repository.id)
  else void refreshRepository(context.repository)
}

/** 单仓刷新：复用刷新引擎，只更新这一条仓库的缓存。 */
async function refreshRepository(repo: Repository) {
  const error = await refreshOne(repo)
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

const statusUnitNodes = computed<StatusUnitNode[]>(() => {
  const nodes: StatusUnitNode[] = []
  for (const group of groups.value) {
    for (const pkg of group.packages) {
      if (pkg.grouped) {
        nodes.push({
          anchor: `unit:${pkg.key}`,
          keys: pkg.statuses.map((entry) => entry.key),
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
          keys: (mod.labels as string[] | undefined) ?? [],
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
  )
}

/** 展开命中单元所在的分类；返回是否真的展开了收起的分类。 */
function expandGroupsFor(matches: StatusUnitNode[]) {
  const keys = new Set(matches.map((unit) => unit.groupKey))
  const before = collapsedGroups.value.length
  collapsedGroups.value = collapsedGroups.value.filter((key) => !keys.has(key))
  return collapsedGroups.value.length !== before
}

/** 点击状态标签：跳到 MOD 安装页并高亮对应仓库卡片。 */
function openInRepository(mod: InstalledMod) {
  const id = bindingRepositoryId(mod)
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
  return t('{count} 个', { count: mod.package_mod_count ?? 0 })
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
    if (!result.ok) message.error(result.message)
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
    if (!result.ok) message.error(result.message)
  } catch (reason) { fail(reason) }
}

/* ===== 页头操作：打开 MOD 目录 / 安装 MOD ===== */

/** 打开当前厨房的 BepInEx/plugins 目录（只读操作，运行中也可用）。 */
async function openModsDirectory() {
  try {
    const result = await api.openModDirectory()
    if (!result.ok) message.error(result.message)
  } catch (reason) { fail(reason) }
}

/**
 * 安装流程：来源选择 → 预览 → 确认安装。
 *
 * 三段都由共用组件承担（ModInstallSourceDialog / ModInstallDialog），
 * 本页只持有流程状态；拖放监听在来源组件里，暂存清理在流程 composable 里。
 */
const {
  sourceOpen: installSourceOpen,
  previewOpen: installPreviewOpen,
  preview: installPreview,
  openPreview: openInstallPreview,
  discardPreview: discardInstallPreview,
  handleInstalled: handleInstallInstalled,
} = useModInstallFlow(() => load())

/**
 * MOD 卡片的「更新」按钮：和仓库卡片的安装共用同一条预览链路
 * （preview_only 下载 → 安装详情弹窗 → 用户确认后才写入）。
 */
const previewing = ref<Record<string, boolean>>({})
const { previewRepository } = useRepositoryPreview(openInstallPreview, {
  gameBlocked: () => {
    if (!game.value?.kitchen_running) return false
    message.warning(t('游戏运行中，已禁止安装。请先关闭游戏。'))
    return true
  },
  onBusyChange: (repositoryId, busy) => {
    if (busy) previewing.value[repositoryId] = true
    else delete previewing.value[repositoryId]
  },
})

/* ===== 包级操作 ===== */

/** 包折叠状态：默认展开，只记住用户主动收起的包。 */
const collapsedPackages = ref<string[]>([])
function isPackageExpanded(key: string) {
  return !collapsedPackages.value.includes(key)
}
function togglePackage(key: string) {
  collapsedPackages.value = collapsedPackages.value.includes(key)
    ? collapsedPackages.value.filter((item) => item !== key)
    : [...collapsedPackages.value, key]
}

/** 包级开关：走后端已有的 toggle_local_mod_package，一次切换整包。 */
async function togglePackageEnabled(node: ModPackageNode, enabled: boolean) {
  const packageId = node.mods[0]?.package_id
  if (!packageId) return
  for (const mod of node.mods) busy.value[mod.dll_file] = true
  try {
    const result = await api.toggleLocalModPackage(packageId, enabled)
    result.ok
      ? message.success(result.message || (enabled ? t('已启用') : t('已禁用')))
      : message.error(result.message)
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
  dialog.warning({
    title: t('删除 MOD 包'),
    content: confirm.body(node.label, node.mods.length),
    positiveText: confirm.action,
    negativeText: t('取消'),
    async onPositiveClick() {
      for (const mod of node.mods) busy.value[mod.dll_file] = true
      try {
        const result = await api.uninstallLocalMod(first.dll_file, true)
        result.ok
          ? message.success(result.message || confirm.done)
          : message.error(result.message)
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
    页头标题右侧：整页状态聚合。
    与每个 MOD 右侧的标签共用同一套 tone 与配色表，
    所以颜色永远一致；数量为 0 的状态不显示，避免一排空标签。
  -->
  <Teleport to="#page-header-title-extra" defer>
    <n-tag v-for="entry in statusSummaryTags" :key="entry.key" size="small" :bordered="false"
      class="state-tag state-tag--link" :color="tagColorForAny(entry.tone)" role="button" tabindex="0"
      :title="t('在页面内定位')" @click="locateStatus(entry.key)"
      @keydown.enter.prevent="locateStatus(entry.key)"
      @keydown.space.prevent="locateStatus(entry.key)">
      {{ entry.count }} {{ t(entry.text) }}
    </n-tag>
  </Teleport>

  <!--
    页头右侧，从左到右：批量开关 → 检测更新 → 打开目录。
    「检测更新」复用 MOD 安装页的刷新全部（同一个 refreshEverything 入口），
    放在 MOD 管理页是因为用户在这里看到「可更新」后，最自然的下一步就是
    「再查一次」。
  -->
  <Teleport to="#page-header-actions" defer>
    <n-button v-if="effectiveMods.length" size="small" secondary
      :disabled="bulkWorking || allModsEnabled" @click="setAllEnabled(true)">
      {{ t('全部启用') }}
    </n-button>
    <n-button v-if="effectiveMods.length" size="small" secondary type="warning"
      :disabled="bulkWorking || !enabledModCount" @click="confirmDisableAll">
      {{ t('全部禁用') }}
    </n-button>
    <n-button size="small" secondary :loading="checkingUpdates" :disabled="checkingUpdates || refreshingAll"
      :title="t('只刷新已安装 MOD 绑定的仓库，检查是否有新版本')" @click="checkUpdates">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('检测更新') }}
    </n-button>
    <n-button size="small" secondary @click="openModsDirectory">
      <FolderOpenOutline width="14" height="14" />{{ t('打开目录') }}
    </n-button>
  </Teleport>

  <!--
    安装流程三段：来源选择 → 详情预览 → 确认安装。
    与 MOD 安装页、仓库卡片共用同一套组件与流程，信息完全一致。
  -->
  <ModInstallSourceDialog v-model:show="installSourceOpen"
    @preview="openInstallPreview" />
  <ModInstallDialog v-model:show="installPreviewOpen" :preview="installPreview"
    @installed="handleInstallInstalled()"
    @discarded="discardInstallPreview" />

  <!--
    页头标题后的聚合标签已经承载了状态统计（数量、启用、可更新都在标签里），
    原来的 .toolbar 只重复这些数字，已无存在必要，移除。
  -->
  <n-spin :show="loading">
    <n-empty v-if="!loading && !sortedMods.length" :description="t('尚未安装任何 MOD')" class="empty" />

    <!-- 按安装时绑定仓库的分类折叠：同一分类的 MOD 归在一起 -->
    <n-collapse v-else :expanded-names="groupsExpanded" class="group-collapse"
      @update:expanded-names="onGroupsExpanded">
      <n-collapse-item v-for="node in groups" :key="node.key" :name="node.key">
        <template #header>
          <div class="group-head">
            <b>{{ node.label }}</b>
            <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">{{ node.count }}</n-tag>
            <n-tag v-if="node.updateCount" size="tiny" :bordered="false" class="state-tag"
              :color="tagColorFor('moderate')">
              {{ t('{count} 个可更新', { count: node.updateCount }) }}
            </n-tag>
            <n-tag v-if="!node.enabled && !node.partial" size="tiny" :bordered="false"
              class="state-tag" :color="tagColorFor('muted')">{{ t('已禁用') }}</n-tag>
            <n-tag v-else-if="node.partial" size="tiny" :bordered="false"
              class="state-tag" :color="tagColorFor('accent')">{{ t('部分启用') }}</n-tag>
          </div>
        </template>

        <div class="mod-grid">
          <template v-for="pkg in node.packages" :key="pkg.key">
            <!-- 单 DLL 包：不套外壳，直接就是那张卡片 -->
            <article v-if="!pkg.grouped" v-for="mod in pkg.mods" :key="mod.dll_file" class="mod-card"
              :data-focus-id="bindingRepositoryId(mod)"
              :data-mod-anchor="modAnchor(mod)"
              :class="{
                disabled: !mod.enabled,
                expanded: isExpanded(mod.dll_file),
                'focus-flash': highlightId === bindingRepositoryId(mod),
                'status-flash': matchesActiveStatus(modAnchor(mod)),
              }" @click="onCardClick($event, mod)">

              <!-- 单行：左「开关 + 标题」… 右「标签 | 工具栏」 -->
              <div class="card-row">
                <div class="card-left">
                  <n-switch class="card-switch" size="small" :value="mod.enabled" :loading="!!busy[mod.dll_file]"
                    @click.stop @update:value="(value: boolean) => toggle(mod, value)" />
                  <b class="card-title" :title="singleModTitle(mod)">{{ singleModTitle(mod) }}</b>
                </div>

                <div class="card-right">
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
                    状态标签组：固定宽度按内容自适应，整体贴住分割线左侧。
                    status-pulse：刷新完成后标签变化时播一次短暂高亮（见 isStatusPulsing）。
                  -->
                  <span class="tag-group" :class="{ 'status-pulse': isStatusPulsing(mod) }">
                    <!-- 配色走 Naive n-tag 原生 :color，由 theme.ts 统一计算 -->
                    <n-tag v-for="tag in modTags(mod)" :key="tag.key" size="tiny" :bordered="false"
                      class="state-tag state-tag--link" :color="tagColorForAny(tag.tone)"
                      :title="bindingRepositoryId(mod) ? t('在 MOD 安装中查看') : undefined"
                      @click.stop="openInRepository(mod)">{{ tag.text }}</n-tag>
                  </span>

                  <!-- 分割线：把标签区和操作按钮区分开 -->
                  <span class="tool-divider" />

                  <!-- 工具栏：可选按钮在最左（用占位保持列宽），固定按钮贴右 -->
                  <span class="tool-group" :style="toolGroupStyle">
                    <!-- 查看日志：只有 BepInEx/logs/<guid> 存在时才显示；缺失时保留同宽占位 -->
                    <n-button v-if="mod.has_logs" size="tiny" quaternary :title="t('查看日志')"
                      class="tool-optional" @click.stop="openLogs(mod)">
                      <DocumentTextOutline width="14" height="14" />
                    </n-button>
                    <span v-else class="tool-placeholder" />
                    <!-- 配置编辑：只有存在配置文件的 MOD 才有；缺失时保留同宽占位，
                         这样「打开位置」「删除」始终落在最右侧同一列 -->
                    <n-button v-if="mod.has_config" size="tiny" quaternary :title="t('配置编辑')"
                      class="tool-optional" @click.stop="editConfig(mod)">
                      <SettingsOutline width="14" height="14" />
                    </n-button>
                    <span v-else class="tool-placeholder" />
                    <n-button size="tiny" quaternary :title="t('打开所在文件夹并定位')"
                      :disabled="game?.kitchen_running" @click.stop="openLocation(mod)">
                      <FolderOpenOutline width="14" height="14" />
                    </n-button>
                    <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running" :title="t('删除')"
                      @click.stop="remove(mod)">
                      <TrashOutline width="14" height="14" />
                    </n-button>
                  </span>
                </div>
              </div>

              <!-- 展开：用 Naive 自带的折叠过渡，不自己写动画 -->
              <n-collapse-transition :show="isExpanded(mod.dll_file)" display-directive="show">
                <!-- 不阻止冒泡：点详情里的任意位置都算「点空白」，由 onDocumentClick 收起 -->
                <div class="card-detail">
                  <ModDetailGrid :mod="mod" :binding-text="bindingText(mod)"
                    :status-summary="statusSummary(mod)" :package-count-text="packageCountText(mod)" />
                </div>
              </n-collapse-transition>
            </article>

            <!--
              多 DLL 包：折叠外壳 + 包级操作，内部仍是同样的卡片。
              跨页跳转（从 MOD 安装页点标签过来）要落在**包外壳**上：
              包是页面上的一个展示单元，跳转进来高亮整包才符合语义；
              包内子 DLL 不是单元，所以它们不带 data-focus-id。
            -->
            <section v-else class="package-shell"
              :data-focus-id="pkg.repositoryId"
              :data-mod-anchor="`unit:${pkg.key}`"
              :class="{
                'status-flash': matchesActiveStatus(`unit:${pkg.key}`),
                'focus-flash': !!pkg.repositoryId && highlightId === pkg.repositoryId,
              }">
              <header class="package-head" @click="togglePackage(pkg.key)">
                <!-- 左：开关 + 包名 + 子 MOD 数量（数量紧跟包名；「N 个」已表明这是包） -->
                <div class="package-left">
                  <!--
                    包级开关三态：全开 / 全关 / 部分。
                    Naive 的 indeterminate 会画一个加载圈，不像「开关中间态」，
                    所以这里仍用布尔值，靠 is-partial 类把滑块和轨道做半透明表达。
                  -->
                  <n-switch class="card-switch package-switch" size="small" :value="pkg.enabled"
                    :loading="pkg.mods.some((m) => busy[m.dll_file])"
                    :class="{ 'is-partial': pkg.partial }"
                    @click.stop @update:value="(value: boolean) => togglePackageEnabled(pkg, value)" />
                  <b class="package-title" :title="pkg.label">{{ pkg.label }}</b>
                  <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">
                    {{ t('{count} 个', { count: pkg.mods.length }) }}
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
                    <span class="tool-divider" />
                  </template>

                  <!--
                    包状态标签：以包为主体。版本结论（最新版 / 可更新 / 检测中…）
                    整包只有一条、不带数量；成员级告警（重复 / 低版本 / 加载警告）
                    才带数量，因为确实可能只命中包内某几个 DLL。
                    配色与单 MOD 行同源，点击跳到 MOD 安装页对应仓库。
                  -->
                  <span class="tag-group">
                    <n-tag v-for="entry in pkg.statuses" :key="entry.key" size="tiny" :bordered="false"
                      class="state-tag state-tag--link" :color="tagColorForAny(entry.tone)"
                      :title="pkg.repositoryId ? t('在 MOD 安装中查看') : undefined"
                      @click.stop="pkg.repositoryId && linkToRepository(pkg.repositoryId)">
                      {{ entry.count ? `${entry.count} ${t(entry.text)}` : t(entry.text) }}
                    </n-tag>
                    <n-tag v-if="!pkg.enabled && !pkg.partial" size="tiny" :bordered="false"
                      class="state-tag" :color="tagColorFor('muted')">{{ t('已禁用') }}</n-tag>
                    <n-tag v-else-if="pkg.partial" size="tiny" :bordered="false"
                      class="state-tag" :color="tagColorFor('accent')">{{ t('部分启用') }}</n-tag>
                  </span>

                  <span class="tool-divider" />

                  <!--
                    包标题与单 MOD 卡片是同一层级，工具栏必须用同一列宽，
                    这样删除按钮落在同一竖线；只有一个按钮时其余位置留空。
                  -->
                  <span class="tool-group" :style="toolGroupStyle" @click.stop>
                    <n-button size="tiny" quaternary type="error" :title="t('整包删除')"
                      :disabled="game?.kitchen_running" @click="removePackage(pkg)">
                      <TrashOutline width="14" height="14" />
                    </n-button>
                  </span>
                </div>
              </header>

              <!-- 包展开/收起：同样用 Naive 的折叠过渡 -->
              <n-collapse-transition :show="isPackageExpanded(pkg.key)" display-directive="show">
              <div class="package-body">
                <!--
                  包内子 DLL 行不是独立单元：不加定位锚点，也不参与状态高亮。
                  状态结论属于整个包，高亮只画在外壳上（见 package-shell）。
                -->
                <article v-for="mod in pkg.mods" :key="mod.dll_file" class="mod-card"
                  :class="{
                    disabled: !mod.enabled,
                    expanded: isExpanded(mod.dll_file),
                  }" @click="onCardClick($event, mod)">
                  <div class="card-row">
                    <div class="card-left">
                      <n-switch class="card-switch" size="small" :value="mod.enabled" :loading="!!busy[mod.dll_file]"
                        @click.stop @update:value="(value: boolean) => toggle(mod, value)" />
                      <b class="card-title" :title="modTitle(mod)">{{ modTitle(mod) }}</b>
                    </div>

                    <!--
                      包内子 MOD 只保留三个操作（标签统一在包标题行）。
                      这一行没有状态标签，所以不放分割线 —— 没有左侧内容时
                      竖线只是一条悬空装饰。但分割线本身占了「1px 宽 + 1 个 gap」，
                      去掉后必须用等宽的占位补回，否则这一行的按钮列会比
                      包标题行、单 MOD 卡片右移 11px（gap 10 + 1px 线）。
                    -->
                    <div class="card-right">
                      <span class="tool-divider-ghost" />
                      <!--
                        包内子 MOD 也用同一列宽：这一行是包内容但按钮仍需与
                        包标题、单 MOD 卡片落在同一竖线，视觉上才是一整套工具栏。
                      -->
                      <span class="tool-group" :style="toolGroupStyle">
                        <n-button v-if="mod.has_logs" size="tiny" quaternary :title="t('查看日志')"
                          class="tool-optional" @click.stop="openLogs(mod)">
                          <DocumentTextOutline width="14" height="14" />
                        </n-button>
                        <span v-else class="tool-placeholder" />
                        <n-button v-if="mod.has_config" size="tiny" quaternary :title="t('配置编辑')"
                          class="tool-optional" @click.stop="editConfig(mod)">
                          <SettingsOutline width="14" height="14" />
                        </n-button>
                        <span v-else class="tool-placeholder" />
                        <n-button size="tiny" quaternary :title="t('打开所在文件夹并定位')"
                          :disabled="game?.kitchen_running" @click.stop="openLocation(mod)">
                          <FolderOpenOutline width="14" height="14" />
                        </n-button>
                        <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running"
                          :title="t('删除')" @click.stop="remove(mod)">
                          <TrashOutline width="14" height="14" />
                        </n-button>
                      </span>
                    </div>
                  </div>

                  <n-collapse-transition :show="isExpanded(mod.dll_file)" display-directive="show">
                    <!-- 同上：详情区不拦冒泡，点任意位置都能收起 -->
                    <div class="card-detail">
                      <ModDetailGrid :mod="mod" :binding-text="bindingText(mod)"
                        :status-summary="statusSummary(mod)" :package-count-text="packageCountText(mod)" />
                    </div>
                  </n-collapse-transition>
                </article>
              </div>
              </n-collapse-transition>
            </section>
          </template>
        </div>
      </n-collapse-item>
    </n-collapse>
  </n-spin>

  <!-- 只保留排序切换（卡片已是唯一布局） -->
  <!--
    浮动按钮：重新扫描本地 MOD（只读磁盘，不碰仓库 Release）。
    「重新读取 Dashboard」会重扫 BepInEx/plugins 并重算标签，等同用户手动
    改过文件后的一次本地刷新；仓库刷新在 MOD 安装页，两者职责分开。
  -->
  <n-float-button position="fixed" bottom="28" right="32" :tooltip="t('重新扫描本地 MOD')"
    :disabled="refreshingLocal" @click="refreshLocal">
    <RefreshOutline width="17" height="17" :class="{ 'spin-icon': refreshingLocal }" />
  </n-float-button>

  <!-- 配置编辑器：编辑该 MOD 在 BepInEx/config 下的 .cfg（Naive 文本域） -->
  <ModConfigEditor v-if="configTarget" v-model:show="configOpen"
    :dll-file="configTarget.dll_file" :title="modTitle(configTarget)" />
</template>

<style scoped>
.seg { display: flex; gap: 6px; }
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
 */
.package-body .mod-card {
  background: var(--oc2-sunken);
  padding-right: calc(14px - 1px);
}

/* 卡片：单行三段 —— 左「开关 + 标题」，右「标签 + 删除」 */
.mod-card {
  display: flex;
  flex-direction: column;
  padding: 10px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  cursor: pointer;
  transition: border-color .16s ease, background-color .16s ease, box-shadow .16s ease;
}
.mod-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
  box-shadow: 0 4px 16px rgba(0, 0, 0, .16);
}
.mod-card.expanded {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
.mod-card.disabled { opacity: .62; }
/*
 * 定位高亮：3 秒后自动褪去（时长由 crossLink.ts 的 FOCUS_DURATION 控制）。
 * 两个类共用同一套视觉 —— focus-flash 是跨页跳转（单张），
 * status-flash 是页内聚合标签定位（可能同时命中多张）。
 *
 * 入场再播一次 pulse：跳转把用户「送」到新位置后，静态换色容易被
 * 正在进行的滚动掩盖；一次轻微的轮廓脉冲能确认落点。
 * 只做 opacity/box-shadow —— 不缩放卡片，否则会抖动相邻行布局。
 */
.mod-card.focus-flash,
.mod-card.status-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 16%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
  animation: focus-pulse 1200ms cubic-bezier(0.4, 0, 0.2, 1);
}
@keyframes focus-pulse {
  0%   { box-shadow: 0 0 0 0 color-mix(in srgb, var(--oc2-accent) 70%, transparent); }
  40%  { box-shadow: 0 0 0 5px color-mix(in srgb, var(--oc2-accent) 30%, transparent); }
  100% { box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent); }
}
@media (prefers-reduced-motion: reduce) {
  .mod-card.focus-flash,
  .mod-card.status-flash { animation: none; }
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
 * 状态变化脉冲：刷新完成、标签从「检查中」变成确定状态时，
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
.card-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  min-width: 0;
}
.card-left {
  display: flex;
  align-items: center;
  gap: 11px;
  flex: 1;
  min-width: 0;
}
/*
 * 右侧一行三段：[状态标签组] | [分割线] | [工具栏]
 * 三段都垂直居中、不换行，工具栏宽度按全页最多按钮数固定，
 * 所以每个 MOD 的按钮列都对齐在同一条竖线上。
 */
.card-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: nowrap;
  gap: 10px;
  flex: 0 0 auto;
  min-width: 0;
}
/*
 * 右侧三段的定位策略：
 *   [tag-group 自适应] [divider 固定] [tool-group 固定宽度]
 * tag-group 宽度跟内容走，靠 flex-end 右对齐；分割线与工具栏列宽固定，
 * 所以所有行的分割线、按钮列都落在同一条竖线上。
 * 工具栏列宽由 toolColumnWidth 统一计算（4 个 30px 按钮 + 3 个 gap），
 * 模板里用 :style 下发，不再写死像素值。
 */
/*
 * 状态标签组：宽度跟内容走，不再预留固定最小宽度。
 * 之前用 min-width: 128px 让所有行的标签区起点一致，但单个标签时
 * 右侧会空出一大片（截图里高亮框比「最新版」宽一倍）。
 * 现在靠 flex-end + 固定的分割线/工具栏列宽保证右对齐，
 * 标签少时容器就窄，视觉上没有空白。
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
/* 竖分割线：只做视觉分隔，不占额外空间 */
.tool-divider {
  width: 1px;
  height: 16px;
  flex: 0 0 1px;
  background: var(--oc2-row-line);
}
/*
 * 分割线的隐形占位：包内子 MOD 行没有标签区、也不想画竖线，
 * 但它必须占掉与 .tool-divider 相同的宽度（1px），
 * 否则父级 .card-right 的 10px gap 会少一段，按钮列整体右移。
 */
.tool-divider-ghost {
  width: 1px;
  height: 1px;
  flex: 0 0 1px;
}
/*
 * 工具栏：四个按钮位等距排布。
 * gap 与按钮宽度来自 toolGroupStyle 下发的 CSS 变量（与列宽计算同源），
 * 所以样式和 toolColumnWidth 永远不会对不上。
 */
.tool-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--tool-gap, 8px);
  flex: 0 0 auto;
}
.tool-group :deep(.n-button) {
  width: var(--tool-button-width, 30px);
  min-width: var(--tool-button-width, 30px);
  padding: 0;
}
/* 没有日志/配置按钮时保留同宽占位，保证四个按钮列在每行都对齐 */
.tool-placeholder {
  width: var(--tool-button-width, 30px);
  height: 1px;
  flex: 0 0 var(--tool-button-width, 30px);
}
.card-switch { flex: 0 0 auto; }
.card-title {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--oc2-text-1);
  line-height: 1.35;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 展开：内容由 ModDetailGrid 渲染，动画交给 n-collapse-transition */
.card-detail {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--oc2-row-line);
}
</style>
