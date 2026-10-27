<script setup lang="ts">
import { t, locale } from '../i18n'
import { buttonColorFor, tagColorFor, tagColorForAny } from '../theme'
import { PULSE_DURATION } from '../motion'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import {
  NButton, NCollapse, NCollapseItem, NCollapseTransition, NEmpty, NFloatButton,
  NSpin, NSwitch, NTag,
} from 'naive-ui'
import {
  ArrowUpCircleOutline, DocumentTextOutline, FolderOpenOutline, RefreshOutline, SettingsOutline,
  TrashOutline,
} from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { repositoryForMod } from '../modStatus'
import { STATUS_META, installedModTags, recomputeModUpdate, replaceVersionConclusion } from '../statusTags'
import { useFocusHighlight, useRepositoryLink, useStatusHighlight } from '../crossLink'
import { useDashboard } from '../composables/useDashboardStore'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import ModConfigEditor from '../components/ModConfigEditor.vue'
import ModDetailGrid from '../components/ModDetailGrid.vue'
import ModInstallDialog from '../components/ModInstallDialog.vue'
import ModInstallSourceDialog from '../components/ModInstallSourceDialog.vue'
import { useModInstallFlow } from '../composables/useModInstallFlow'
import { useRepositoryPreview } from '../composables/useRepositoryPreview'
import type { InstalledMod, Repository } from '../types'

const message = useMessage()
const dialog = useDialog()
const { load, ensureLoaded, loading, mods, game, config } = useDashboard()
/** 刷新状态：用于把「正在抓取 Release」的 MOD 显示成检查中而不是旧结论。 */
const { refreshing, refreshingAll } = useRepositoryRefresh()

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
    const recomputed = recomputeModUpdate(mod, repositories.value)
    if (recomputed) {
      /*
       * 用统一函数替换版本结论，而不是自己拼数组。
       * 之前的实现只过滤 latest/update_available，会把 not_release_asset、
       * repository_unavailable 等旧结论留在原地，出现「最新版 + 仓库不可用」
       * 这种自相矛盾的组合。
       */
      return {
        ...mod,
        update_available: recomputed.update_available,
        labels: replaceVersionConclusion(
          mod.labels as string[] | undefined,
          recomputed.update_available ? 'update_available' : 'latest',
        ) as InstalledMod['labels'],
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
        labels: replaceVersionConclusion(mod.labels as string[] | undefined, 'checking') as InstalledMod['labels'],
      } as InstalledMod
    }
    return mod
  }),
)

/** 某个仓库当前是否正在抓取 Release（单仓刷新或整批刷新都算）。 */
function isRepositoryRefreshing(repositoryId: string) {
  return !!refreshing.value[repositoryId] || refreshingAll.value
}

/** 列表固定按标题排序（排序切换入口已移除，见上方说明）。 */
const sortedMods = computed(() =>
  [...effectiveMods.value].sort((a, b) => modTitle(a).localeCompare(modTitle(b))),
)

const enabledCount = computed(() => mods.value.filter((mod) => mod.enabled).length)

const updateCount = computed(() => effectiveMods.value.filter((mod) => mod.update_available).length)

/**
 * 顶部聚合标签：按状态汇总整页 MOD 数量。
 *
 * 顺序与「越需要注意越靠前」一致，和单个 MOD 右侧标签用的是同一套 tone，
 * 因此颜色永远同步（都来自 theme.ts 的 statusToneColors）。
 * 纯依赖 DLL 不参与：它不是 MOD，只是随包安装的文件。
 */
/**
 * 顶部聚合标签：按状态汇总整页 MOD 数量。
 *
 * 覆盖 MOD 管理页可能出现的**全部**标签，不只挑几个。数据源是
 * STATUS_META（文案与 tone）而不是本地再抄一份文案 —— 这样以后在
 * statusTags.ts 里加标签，这里自动出现，不会漏。
 *
 * 排序按「轻重缓急」= 语义 tone 的严重度，同档内再按固定业务顺序。
 * 越需要用户处理越靠前，正常/中性收在最后。
 */
const TONE_PRIORITY: Record<string, number> = {
  critical: 0,  // 严重：加载警告、仓库缺失
  severe: 1,    // 重度：重复
  moderate: 2,  // 中度：可更新、仓库刷新失败、预发布
  mild: 3,      // 轻微：低版本
  muted: 4,     // 中性：检测中、版本未知、未绑定、已禁用
  accent: 5,    // 正常：最新版、已安装
}

/** 同档内的稳定排序（越靠前越需要动作）。 */
const WITHIN_TONE_ORDER: string[] = [
  'load_warning',
  'bound_repository_missing',
  'repository_unavailable',
  'duplicate',
  'update_available',
  'prerelease',
  'refresh_failed',
  'duplicate_lower_version',
  'checking',
  'unknown',
  'unbound',
  'disabled',
  'dependency',
  'not_release_asset',
  'installed',
  'latest',
]

interface SummaryTag { key: string; text: string; tone: string; count: number; index: number }

const statusSummaryTags = computed<SummaryTag[]>(() => {
  // 纯依赖 DLL 不是 MOD，只随包安装，不参与整页统计
  const real = effectiveMods.value.filter((mod) => !mod.is_dependency)
  const countOf = (key: string) =>
    real.filter((mod) => (mod.labels as string[] | undefined)?.includes(key)).length

  const entries: SummaryTag[] = []
  WITHIN_TONE_ORDER.forEach((key, index) => {
    const meta = STATUS_META[key]
    if (!meta) return
    const count = countOf(key)
    if (count > 0) entries.push({ key, text: meta.text, tone: meta.tone, count, index })
  })

  return entries.sort((a, b) => {
    const byTone = (TONE_PRIORITY[a.tone] ?? 9) - (TONE_PRIORITY[b.tone] ?? 9)
    return byTone !== 0 ? byTone : a.index - b.index
  })
})

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
  updateCount: number
  /** 包内全部成员都确认是最新版（用于包标题上的「最新版」标签）。 */
  latest: boolean
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
   */
  const snapshot = mod.group_i18n
  const snapshotGroup = (mod.group || '').trim()
  const hasSnapshot = !!snapshot && Object.keys(snapshot).length > 0
  if (repo) {
    if (hasSnapshot) {
      const canonical = (snapshotGroup || snapshot['zh-CN'] || pickI18n(snapshot, '')).trim()
      const label = pickI18n(snapshot, snapshotGroup) || t('未分组')
      return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
    }
    if (snapshotGroup) return { key: `group:${snapshotGroup.toLowerCase()}`, label: snapshotGroup }
    // key 用语言无关的原始分组名，切换语言时折叠状态不丢
    const canonical = (repo.group || repo.group_i18n?.['zh-CN'] || pickI18n(repo.group_i18n, '')).trim()
    const label = pickI18n(repo.group_i18n, repo.group || '') || t('未分组')
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  if (mod.manual_install) return { key: '__local_install__', label: t('本地安装') }
  if (hasSnapshot) {
    const canonical = (snapshotGroup || snapshot['zh-CN'] || pickI18n(snapshot, '')).trim()
    const label = pickI18n(snapshot, snapshotGroup) || t('未分组')
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  if (snapshotGroup) return { key: `group:${snapshotGroup.toLowerCase()}`, label: snapshotGroup }
  return { key: '__unbound__', label: t('未绑定') }
}

const groups = computed<ModGroupNode[]>(() => {
  // 先按分类分桶，桶内再按 package_id 聚成包
  const buckets = new Map<string, { label: string; packages: Map<string, InstalledMod[]> }>()
  for (const mod of sortedMods.value) {
    const category = modCategory(mod)
    let bucket = buckets.get(category.key)
    if (!bucket) {
      bucket = { label: category.label, packages: new Map() }
      buckets.set(category.key, bucket)
    }
    const packageKey = mod.package_id ? `pkg:${mod.package_id}` : `dll:${mod.dll_file}`
    const list = bucket.packages.get(packageKey)
    if (list) list.push(mod)
    else bucket.packages.set(packageKey, [mod])
  }

  const toPackage = (key: string, list: InstalledMod[]): ModPackageNode => {
    const enabledCount = list.filter((mod) => mod.enabled).length
    /*
     * 包标题：能认出绑定仓库就用仓库名，让「装的是哪个包」一眼可读；
     * 否则退回安装包名与插件名。整包成员共享一个仓库，所以同一包内
     * 各 DLL 仍由行内插件名区分，不会因为标题统一而重名。
     */
    const label = list[0] ? displayTitle(list[0]) : ''
    const updateCount = list.filter((mod) => mod.update_available).length
    /*
     * 纯依赖 DLL 不参与版本结论：它没有 BepInPlugin 元数据，
     * 永远不会有 latest/update_available 标签。如果把它算进 every()，
     * 任何含依赖的包（如 DIYLevel）都永远判不出「最新版」。
     * 所以只用真正的 MOD 成员判断整包状态。
     */
    const modMembers = list.filter((mod) => !mod.is_dependency)
    const latest = modMembers.length > 0
      && updateCount === 0
      && modMembers.every((mod) => mod.labels?.includes('latest'))
    return {
      key,
      label,
      mods: list,
      grouped: list.length >= 2,
      enabled: enabledCount === list.length,
      partial: enabledCount > 0 && enabledCount < list.length,
      updateCount,
      latest,
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
        count: all.length,
        updateCount: all.filter((mod) => mod.update_available).length,
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
 * 再退回安装包名与实际插件名。
 */
function displayTitle(mod: InstalledMod) {
  return boundRepositoryName(mod) || mod.package_name?.trim() || modTitle(mod)
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

function remove(mod: InstalledMod) {
  dialog.warning({
    title: t('删除 MOD'),
    content: t('「{name}」将被移入回收站，可在回收站中恢复。继续？', { name: modTitle(mod) }),
    positiveText: t('移入回收站'),
    negativeText: t('取消'),
    async onPositiveClick() {
      busy.value[mod.dll_file] = true
      try {
        const result = await api.uninstallLocalMod(mod.dll_file)
        /*
         * 后端有两种「需要二次确认」：
         *   requires_package_confirmation —— 该 DLL 属于多文件 MOD 包，
         *                                   询问是否整包一起删除（后端此时 ok=false）
         *   requires_confirmation        —— 共享依赖 / 同 GUID 等风险确认
         * 两者都要先弹「整包删除」二级确认，只有都不需要才真正删除。
         * 注意：之前只判断 requires_confirmation，导致包内 DLL 永远删不掉。
         */
        if (result.requires_package_confirmation || result.requires_confirmation) {
          dialog.warning({
            title: t('该 MOD 属于 MOD 包'),
            content: t('它与 {count} 个子 MOD 同属一个包，一并删除？', { count: result.package_mod_names?.length ?? 0 }),
            positiveText: t('整包删除'),
            negativeText: t('取消'),
            async onPositiveClick() {
              const packageResult = await api.uninstallLocalMod(mod.dll_file, true)
              packageResult.ok ? message.success(packageResult.message || '已移入回收站') : message.error(packageResult.message)
              await load()
            },
          })
        } else {
          result.ok ? message.success(result.message || '已移入回收站') : message.error(result.message)
        }
        await load()
      } catch (reason) { fail(reason) } finally { busy.value[mod.dll_file] = false }
    },
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
 * 可更新的 MOD：返回它绑定的仓库，否则 undefined。
 *
 * 只有「真的能装到更新的版本」才给按钮 ——
 * 未绑定、仓库缺失、仓库文件（RAW，无版本可比）都不算，
 * 否则按钮点下去只会下载一个和本地一样的包。
 */
function updatableRepository(mod: InstalledMod): Repository | undefined {
  if (mod.is_dependency || !mod.update_available) return undefined
  const repo = repositoryForMod(repositories.value, mod)
  if (!repo || repo.download_mode === 'repository_file') return undefined
  return repo
}

/** 触发某个 MOD 的更新：与仓库卡片「更新」走同一条预览链路。 */
function updateMod(mod: InstalledMod) {
  const repo = updatableRepository(mod)
  if (repo) void previewRepository(repo)
}

/* ===== 页内定位：点标题后的聚合标签 ===== */

/** 卡片锚点：聚合标签按状态定位时用它找 DOM，与跨页高亮的锚点同一套命名。 */
function modAnchor(mod: InstalledMod) {
  return mod.dll_file
}

/**
 * 某个状态标签命中的 MOD（按页面展示顺序）。
 *
 * 依赖 DLL 不参与：它没有版本语义，也不在聚合标签里计数，
 * 若算进来会让「2 可更新」实际高亮出 3 张卡片。
 */
function modsWithStatus(statusKey: string): InstalledMod[] {
  const matches: InstalledMod[] = []
  for (const node of groups.value) {
    for (const pkg of node.packages) {
      for (const mod of pkg.mods) {
        if (mod.is_dependency) continue
        if ((mod.labels as string[] | undefined)?.includes(statusKey)) matches.push(mod)
      }
    }
  }
  return matches
}

/** 当前页内定位的状态是否命中该卡片（命中项一起高亮，一眼看出这一组）。 */
function matchesActiveStatus(mod: InstalledMod) {
  const status = activeStatus.value
  if (!status || mod.is_dependency) return false
  return (mod.labels as string[] | undefined)?.includes(status) ?? false
}

/** 当前轮到的命中项序号：重复点击同一标签时逐张轮换，而不是每次回到第一张。 */
const statusCursor = ref<{ key: string; index: number }>({ key: '', index: 0 })

/**
 * 点聚合标签：高亮该状态的全部卡片，并把下一张命中项滚到视口中间。
 *
 * 命中项可能分布在收起的分类或 MOD 包里：先把它们全部展开，
 * 展开带 0.3s 高度动画，所以滚动要等动画结束（由 focusStatus 负责）。
 */
function locateStatus(statusKey: string) {
  const matches = modsWithStatus(statusKey)
  if (!matches.length) return

  const sameTag = statusCursor.value.key === statusKey
  const start = sameTag ? (statusCursor.value.index + 1) % matches.length : 0
  statusCursor.value = { key: statusKey, index: start }
  const anchor = modAnchor(matches[start])

  // 两级折叠都要展开：分别调用，不能写成一个 || 表达式，
  // 否则分类展开为 true 时包展开会被短路跳过。
  const groupsOpened = expandGroupsFor(matches)
  const packagesOpened = expandPackagesFor(matches)
  focusStatus(
    statusKey,
    () => document.querySelector<HTMLElement>(`[data-mod-anchor="${CSS.escape(anchor)}"]`),
    groupsOpened || packagesOpened,
  )
}

/** 展开命中卡片所在的分类；返回是否真的展开了收起的分类。 */
function expandGroupsFor(matches: InstalledMod[]) {
  const keys = new Set(matches.map((mod) => modCategory(mod).key))
  const before = collapsedGroups.value.length
  collapsedGroups.value = collapsedGroups.value.filter((key) => !keys.has(key))
  return collapsedGroups.value.length !== before
}

/** 展开命中卡片所在的 MOD 包；返回是否真的展开了收起的包。 */
function expandPackagesFor(matches: InstalledMod[]) {
  const keys = new Set(matches.map((mod) => (mod.package_id ? `pkg:${mod.package_id}` : `dll:${mod.dll_file}`)))
  const before = collapsedPackages.value.length
  collapsedPackages.value = collapsedPackages.value.filter((key) => !keys.has(key))
  return collapsedPackages.value.length !== before
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

/** 包级删除：整包移入回收站（后端 allowUnmanaged=true 即整包语义）。 */
function removePackage(node: ModPackageNode) {
  const first = node.mods[0]
  if (!first) return
  dialog.warning({
    title: t('删除 MOD 包'),
    content: t('「{name}」的 {count} 个文件将被整包移入回收站，可在回收站中恢复。继续？', {
      name: node.label,
      count: node.mods.length,
    }),
    positiveText: t('整包删除'),
    negativeText: t('取消'),
    async onPositiveClick() {
      for (const mod of node.mods) busy.value[mod.dll_file] = true
      try {
        const result = await api.uninstallLocalMod(first.dll_file, true)
        result.ok
          ? message.success(result.message || '已移入回收站')
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

  <!-- 页头右侧：打开 MOD 目录（「安装 MOD」已移到 MOD 安装页） -->
  <Teleport to="#page-header-actions" defer>
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

  <n-spin :show="loading">
    <div class="toolbar">
      <span class="muted">
        {{ t('共 {count} 个 · 启用 {enabled} · 可更新 {updates}',
          { count: mods.length, enabled: enabledCount, updates: updateCount }) }}
      </span>
    </div>

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
                'status-flash': matchesActiveStatus(mod),
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
                    可更新时的快捷「更新」按钮：与标签之间用竖线隔开，
                    视觉上属于标签区的一部分（不是右侧工具栏的常规动作）。
                    点击后走和仓库卡片「安装」完全相同的链路：
                    预览下载 → 安装详情弹窗 → 用户确认写入。
                  -->
                  <template v-if="updatableRepository(mod)">
                    <!-- 与「可更新」标签同档 moderate 的浅底样式，配色来自 theme.ts -->
                    <n-button size="tiny" :style="buttonColorFor('moderate', 'soft')" :title="t('更新')"
                      :loading="!!previewing[bindingRepositoryId(mod) ?? '']"
                      :disabled="!!game?.kitchen_running" @click.stop="updateMod(mod)">
                      <ArrowUpCircleOutline width="14" height="14" />{{ t('更新') }}
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

            <!-- 多 DLL 包：折叠外壳 + 包级操作，内部仍是同样的卡片 -->
            <section v-else class="package-shell">
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
                    包状态标签与单 MOD 共用同一套 tone 配色（theme.ts 统一计算），
                    不再手写固定色类，否则换主题色时包标签会留在旧颜色上。
                  -->
                  <span class="tag-group">
                    <!-- 用 moderate 而非 contrast：后者不在配色表里，会回退成灰色 -->
                    <n-tag v-if="pkg.updateCount" size="tiny" :bordered="false"
                      class="state-tag" :color="tagColorFor('moderate')">
                      {{ t('{count} 个可更新', { count: pkg.updateCount }) }}
                    </n-tag>
                    <n-tag v-else-if="pkg.latest" size="tiny" :bordered="false"
                      class="state-tag" :color="tagColorFor('accent')">{{ t('最新版') }}</n-tag>
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
                <article v-for="mod in pkg.mods" :key="mod.dll_file" class="mod-card"
                  :data-focus-id="bindingRepositoryId(mod)"
                  :data-mod-anchor="modAnchor(mod)"
                  :class="{
                    disabled: !mod.enabled,
                    expanded: isExpanded(mod.dll_file),
                    'focus-flash': highlightId === bindingRepositoryId(mod),
                    'status-flash': matchesActiveStatus(mod),
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

  <!-- 配置编辑器（Monaco）：编辑该 MOD 在 BepInEx/config 下的 .cfg -->
  <ModConfigEditor v-if="configTarget" v-model:show="configOpen" :dll-file="configTarget.dll_file"
    :title="modTitle(configTarget)" />
</template>

<style scoped>
.toolbar {
  display: flex; align-items: center; gap: 12px; margin-bottom: 14px;
}
.seg { display: flex; gap: 6px; }
.empty { margin: 60px 0; }
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
 * 包内 DLL 列表：左右各留 14px（与卡片自身内边距、页面通用边距一致），
 * 让内层卡片的圆角边框完整落在外壳内部，不再与外壳边框重叠成双线。
 */
.package-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 14px;
  border-top: 1px solid var(--oc2-row-line);
}
/*
 * 包内卡片：右侧内边距交给 .package-body 的 14px 承担。
 * 嵌套结构里卡片自身再留 14px 会让按钮列比包标题行左移 15px
 * （多一层 1px 边框 + 14px 内边距），而外层 14px 已经提供了
 * 按钮到外壳边框的视觉间距，因此这里右侧归零。
 * 再补 -1px 抵消内层卡片自己的右边框，按钮列才能与包标题行、
 * 单 MOD 卡片精确落在同一竖线。
 */
.package-body .mod-card {
  background: var(--oc2-sunken);
  padding-right: 0;
  margin-right: -1px;
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
 */
.mod-card.focus-flash,
.mod-card.status-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 16%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
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
