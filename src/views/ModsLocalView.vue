<script setup lang="ts">
import { t, locale } from '../i18n'
import { tagColorFor } from '../theme'
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import {
  NButton, NCollapse, NCollapseItem, NCollapseTransition, NEmpty, NFloatButton, NInput, NModal,
  NSpin, NSwitch, NTabPane, NTabs, NTag,
} from 'naive-ui'
import {
  DocumentTextOutline, FolderOpenOutline, LinkOutline, SettingsOutline, SwapVerticalOutline, TrashOutline,
} from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { isTauri, safeTauri } from '../runtime'
import { compareVersions, repositoryForMod } from '../modStatus'
import { installedModTags, recomputeModUpdate } from '../statusTags'
import { useFocusHighlight, useRepositoryLink } from '../crossLink'
import { useDashboard } from '../composables/useDashboardStore'
import ModConfigEditor from '../components/ModConfigEditor.vue'
import ModDetailGrid from '../components/ModDetailGrid.vue'
import type { InstalledMod, ModPreview, ModSourceKind, Repository } from '../types'

const message = useMessage()
const dialog = useDialog()
const { load, ensureLoaded, loading, mods, game, config } = useDashboard()

/** 状态标签跳转：点标签跳到 MOD 安装页，并高亮对应仓库卡片 3 秒。 */
const { linkToRepository } = useRepositoryLink()
/** 从 MOD 安装页跳回来时，高亮绑定到该仓库的 MOD 卡片 3 秒并滚动到它。 */
const { highlightId } = useFocusHighlight()

const sortBy = ref<'name' | 'version' | 'enabled'>('name')
const sortLabels = computed(() => ({ name: t('按名称'), version: t('按版本'), enabled: t('按启用') }))
function cycleSort() {
  const order = ['name', 'version', 'enabled'] as const
  sortBy.value = order[(order.indexOf(sortBy.value) + 1) % order.length]
}
const sortLabel = computed(() => sortLabels.value[sortBy.value])
const busy = ref<Record<string, boolean>>({})
const expanded = ref<Record<string, boolean>>({})

// 切页面复用已有 Dashboard；开关/删除后仍用 load() 取最新扫描结果
onMounted(() => { void ensureLoaded() })

/**
 * 把「后端扫描结果」和「当前仓库缓存」合成一份有效状态。
 *
 * 后端 `labels` / `update_available` 是扫描那一刻算的，而仓库 Release 是
 * 异步刷新回来的；刷新期间 Dashboard 不重读（会重扫磁盘），标签就会停在
 * 「版本未知」。这里用当前仓库缓存重算，让刷新结果立刻生效，
 * 且整个过程是纯内存计算 —— 刷一个仓库只影响它绑定的那几个 MOD。
 */
const effectiveMods = computed<InstalledMod[]>(() =>
  mods.value.map((mod) => {
    const recomputed = recomputeModUpdate(mod, repositories.value)
    if (!recomputed) return mod
    const stripped = (mod.labels ?? []).filter((label) => label !== 'latest' && label !== 'update_available')
    return {
      ...mod,
      update_available: recomputed.update_available,
      labels: recomputed.update_available ? ['update_available', ...stripped] : ['latest', ...stripped],
    }
  }),
)

const sortedMods = computed(() => {
  const list = [...effectiveMods.value]
  if (sortBy.value === 'name') list.sort((a, b) => modTitle(a).localeCompare(modTitle(b)))
  if (sortBy.value === 'version') {
    list.sort((a, b) => {
      const compared = compareVersions(b.version, a.version)
      return compared ?? (b.version ?? '').localeCompare(a.version ?? '')
    })
  }
  if (sortBy.value === 'enabled') list.sort((a, b) => Number(b.enabled) - Number(a.enabled))
  return list
})

const enabledCount = computed(() => mods.value.filter((mod) => mod.enabled).length)

const updateCount = computed(() => effectiveMods.value.filter((mod) => mod.update_available).length)

/**
 * 顶部聚合标签：按状态汇总整页 MOD 数量。
 *
 * 顺序与「越需要注意越靠前」一致，和单个 MOD 右侧标签用的是同一套 tone，
 * 因此颜色永远同步（都来自 theme.ts 的 statusToneColors）。
 * 纯依赖 DLL 不参与：它不是 MOD，只是随包安装的文件。
 */
const statusSummaryTags = computed(() => {
  const real = effectiveMods.value.filter((mod) => !mod.is_dependency)
  const countLabel = (key: string) =>
    real.filter((mod) => (mod.labels as string[] | undefined)?.includes(key)).length
  const definitions: Array<{ key: string; tone: string; count: number; text: string }> = [
    { key: 'duplicate', tone: 'contrast-strong', count: countLabel('duplicate'), text: '重复' },
    { key: 'update_available', tone: 'contrast', count: countLabel('update_available'), text: '可更新' },
    { key: 'duplicate_lower_version', tone: 'contrast-weak', count: countLabel('duplicate_lower_version'), text: '低版本' },
    { key: 'latest', tone: 'accent', count: countLabel('latest'), text: '最新版' },
    { key: 'not_release_asset', tone: 'accent', count: countLabel('not_release_asset'), text: '已安装' },
  ]
  return definitions.filter((entry) => entry.count > 0)
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
    // 单 DLL 包直接用这张卡片的标题，让「单包」与「包内 DLL」看起来是同一种东西
    const label = list[0]?.package_name?.trim() || modTitle(list[0])
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
 * 包内 DLL 的标题：插件名优先（让同包的不同 DLL 能区分开）。
 * 包名由外层「包标题行」负责展示，这里不再重复。
 */
function modTitle(mod: InstalledMod) {
  return mod.plugin_name?.trim() || mod.title || mod.product_name || mod.name
}

/**
 * 单 DLL 包的标题：那张卡片既是包本身也是唯一成员，
 * 所以优先用包名（与多 DLL 包的标题行保持同一语义）。
 */
function singleModTitle(mod: InstalledMod) {
  return mod.package_name?.trim() || modTitle(mod)
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
 */
function onCardClick(event: MouseEvent, mod: InstalledMod) {
  const target = event.target
  if (target instanceof Element && target.closest('.card-detail')) return
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
  attachDragDrop()
})
onBeforeUnmount(() => {
  document.removeEventListener('click', onDocumentClick)
  detachDragDrop()
  // 页面被切走时如果预览还开着，把后端暂存一起清掉
  void discardPreview()
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
 * 工具栏列宽：按「两个可选项占位 + 两个固定按钮」计算固定总宽。
 * 可选项（查看日志、配置编辑）在最左侧，任一缺失时用同宽占位补上，
 * 这样「打开位置」「删除」永远落在最右同一列，图标竖排对齐。
 */
const TOOL_BUTTON_WIDTH = 30
const TOOL_GAP = 2
/** 可选项与固定按钮之间的 .tool-spacer 最小宽度，真实列宽必须把它算进去。 */
const TOOL_SPACER_MIN = 2
/*
 * 4 个按钮位（日志、配置、定位、删除）+ 4 个 gap + spacer 最小值：
 * 4*30 + 4*2 + 2 = 130px。少算一个按钮位会让整列按钮错位。
 */
const toolColumnWidth = computed(
  () => 4 * TOOL_BUTTON_WIDTH + 4 * TOOL_GAP + TOOL_SPACER_MIN,
)

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
 * 安装 MOD 弹窗：本地文件 / 在线直链共用一套「解析 → 预览 → 立即安装」。
 *
 * 两个来源的区别只在第一步拿到文件的途径：
 *   - 本地：拖放或文件选择器，得到绝对路径；
 *   - 在线：后端下载一次到暂存目录。
 * 拿到文件后都由后端 `analyze_mod_source` 解析，把包内每个 DLL 的元数据
 * 返回给前端展示；用户点「立即安装」才走 `confirm_mod_install` 真正写入。
 * 这样本地和在线看到的是同一套信息，安装清单也由同一个函数创建。
 */
const installOpen = ref(false)
const installTab = ref<'local' | 'online'>('local')
const installUrl = ref('')
/** 解析 / 安装进行中（两者共用一个忙碌态，按钮互斥）。 */
const installing = ref(false)
/** 当前预览：null 表示还没选文件。 */
const preview = ref<ModPreview | null>(null)
/** 是否正在等待拖放（用于高亮拖放区）。 */
const dragActive = ref(false)
/** 解析中（拖放区显示 loading）。 */
const awaitingSource = ref(false)

function openInstall() {
  installTab.value = 'local'
  installUrl.value = ''
  preview.value = null
  installOpen.value = true
}

/** 取消/关闭时清理后端暂存，避免留垃圾文件。 */
async function discardPreview() {
  const current = preview.value
  preview.value = null
  if (current) await api.cancelModPreview(current.preview_id).catch(() => undefined)
}

/** 关闭弹窗（按钮、遮罩、Esc 都走这里）。 */
async function closeInstall() {
  await discardPreview()
  installOpen.value = false
}

/** 切换来源时丢弃上一份预览（v-model 会直接改 installTab）。 */
watch(installTab, () => {
  if (preview.value) void discardPreview()
})

/** 统一的解析入口：本地路径或在线 URL。 */
async function analyze(kind: ModSourceKind, value: string) {
  awaitingSource.value = true
  try {
    await discardPreview()
    preview.value = await api.analyzeModSource(kind, value)
  } catch (reason) {
    fail(reason)
  } finally {
    awaitingSource.value = false
  }
}

async function pickLocalMod() {
  const path = await api.chooseModFile()
  if (!path) return
  await analyze('local', path)
}

async function downloadAndAnalyze() {
  const url = installUrl.value.trim()
  if (!/^https?:\/\//i.test(url)) { message.warning(t('请输入 zip 或 DLL 的直链网址')); return }
  await analyze('url', url)
}

/** 放弃当前预览，回到选择/输入状态。 */
async function resetPreview() {
  await discardPreview()
  installUrl.value = ''
}

/** 用户确认安装。 */
async function confirmInstall() {
  const current = preview.value
  if (!current) return
  installing.value = true
  try {
    const result = await api.confirmModInstall(current.preview_id)
    result.ok ? message.success(result.message) : message.error(result.message)
    if (result.ok) {
      // 后端已在 confirm 里删掉暂存，这里只需清掉本地引用
      preview.value = null
      installOpen.value = false
      installUrl.value = ''
      await load()
    }
  } catch (reason) { fail(reason) } finally { installing.value = false }
}

/* ===== 拖放：Tauri 的 webview 事件给绝对路径，不是浏览器 File 对象 ===== */

/** 当前注册的拖放解除函数。 */
let dragUnlisten: (() => void) | null = null

/**
 * 注册拖放监听。
 *
 * 只在真实 Tauri 窗口里有效：浏览器预览没有 drag-drop 权限，
 * 且 `getCurrentWebview()` 在非 Tauri 环境会抛错（见 runtime.ts）。
 * 事件路径是绝对路径数组，取第一个交给后端解析。
 */
function attachDragDrop() {
  if (!isTauri() || dragUnlisten) return
  void safeTauri(async () => {
    const { getCurrentWebview } = await import('@tauri-apps/api/webview')
    const unlisten = await getCurrentWebview().onDragDropEvent((event) => {
      if (!installOpen.value) return
      const payload = event.payload
      if (payload.type === 'over' || payload.type === 'enter') {
        dragActive.value = true
        return
      }
      if (payload.type === 'leave') {
        dragActive.value = false
        return
      }
      if (payload.type !== 'drop') return
      dragActive.value = false
      const path = payload.paths?.[0]
      if (!path) return
      if (!/\.(dll|zip)$/i.test(path)) {
        message.warning(t('只支持 .dll 或 .zip 文件'))
        return
      }
      installTab.value = 'local'
      void analyze('local', path)
    })
    dragUnlisten = unlisten
  })
}

function detachDragDrop() {
  dragUnlisten?.()
  dragUnlisten = null
}

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
      class="state-tag" :color="tagColorFor(entry.tone)">
      {{ entry.count }} {{ t(entry.text) }}
    </n-tag>
  </Teleport>

  <!-- 页头右侧：打开 MOD 目录 + 安装 MOD（本地文件 / 在线直链） -->
  <Teleport to="#page-header-actions" defer>
    <n-button size="small" secondary @click="openModsDirectory">
      <FolderOpenOutline width="14" height="14" />{{ t('打开目录') }}
    </n-button>
    <n-button size="small" type="primary" @click="openInstall">
      <LinkOutline width="14" height="14" />{{ t('安装 MOD') }}
    </n-button>
  </Teleport>

  <!--
    安装 MOD：本地文件 / 在线直链共用一套「解析 → 预览 → 立即安装」。
    未选文件时显示拖放区（本地页）或直链输入（在线页）；
    拿到文件后换成预览面板，展示包内每个 DLL 的元数据。
  -->
  <n-modal :show="installOpen" preset="card" :title="t('安装 MOD')" :bordered="false"
    style="width: min(620px, 92vw)" @update:show="(value: boolean) => { if (!value) void closeInstall() }">
    <n-tabs v-model:value="installTab" type="segment" animated>
      <n-tab-pane name="local" :tab="t('本地文件')">
        <!--
          拖放区：点击也能打开文件选择器。
          Tauri 接管 WebView2 的拖放后会固定显示「禁止」光标（它只发事件、
          不接受放置），CSS 无法覆盖那个系统光标。所以改用拖入时的高亮
          来表达「可以放」：进入即描边 + 底色提亮 + 加号放大，指针形态
          不再承担提示职责。
        -->
        <div v-if="!preview" class="drop-zone" :class="{ active: dragActive, busy: awaitingSource }"
          role="button" tabindex="0" @click="pickLocalMod" @keydown.enter.prevent="pickLocalMod">
          <n-spin v-if="awaitingSource" size="small" />
          <template v-else>
            <span class="drop-plus">+</span>
            <span class="drop-hint">{{ t('把 .dll 或 .zip 拖到这里，或点击选择文件') }}</span>
          </template>
        </div>
      </n-tab-pane>
      <n-tab-pane name="online" :tab="t('在线直链')">
        <div v-if="!preview" class="online-form">
          <p class="muted">{{ t('粘贴 zip 或 DLL 的下载直链，由管理器下载并解析。') }}</p>
          <n-input v-model:value="installUrl" :placeholder="t('https://example.com/mod.zip')" clearable />
          <div class="install-actions">
            <n-button type="primary" :loading="awaitingSource" :disabled="awaitingSource"
              @click="downloadAndAnalyze">
              <LinkOutline width="14" height="14" />{{ t('下载并解析') }}
            </n-button>
          </div>
        </div>
      </n-tab-pane>
    </n-tabs>

    <!-- 预览：单 mod 显示完整元数据；包显示逐 DLL 列表（mod / 依赖） -->
    <div v-if="preview" class="install-preview">
      <div class="preview-head">
        <b>{{ preview.package_name }}</b>
        <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">
          {{ preview.kind === 'single' ? t('单个 MOD') : t('MOD 包') }}
        </n-tag>
        <span class="muted">
          {{ t('{count} 个 DLL · {mods} 个 MOD · {deps} 个依赖',
            { count: preview.total_dlls, mods: preview.mod_count, deps: preview.dependency_count }) }}
        </span>
      </div>

      <!-- 单 mod：逐项展示解析出的元数据 -->
      <div v-if="preview.kind === 'single'" class="preview-grid">
        <div class="preview-item"><span class="muted">{{ t('名称') }}</span><b>{{ preview.entries[0]?.name || '—' }}</b></div>
        <div class="preview-item"><span class="muted">GUID</span><b class="mono">{{ preview.entries[0]?.guid || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('版本') }}</span><b class="mono">{{ preview.entries[0]?.version || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('标题') }}</span><b>{{ preview.entries[0]?.title || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('产品名') }}</span><b>{{ preview.entries[0]?.product_name || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('作者') }}</span><b>{{ preview.entries[0]?.company || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('原始文件名') }}</span><b class="mono">{{ preview.entries[0]?.original_filename || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('文件') }}</span><b class="mono">{{ preview.entries[0]?.path || '—' }}</b></div>
      </div>

      <!-- 包：只列出每个 DLL 的名称 + 版本，依赖单独标注 -->
      <div v-else class="preview-list">
        <div v-for="entry in preview.entries" :key="entry.path" class="preview-row">
          <n-tag size="tiny" :bordered="false" class="state-tag"
            :color="tagColorFor(entry.role === 'dependency' ? 'muted' : 'accent')">
            {{ entry.role === 'dependency' ? t('依赖') : t('MOD') }}
          </n-tag>
          <b class="preview-name">{{ entry.name || entry.path }}</b>
          <span class="muted mono">{{ entry.version || '—' }}</span>
        </div>
        <div v-if="!preview.entries.length" class="muted">{{ t('这个压缩包里没有 DLL 文件') }}</div>
      </div>

      <div class="install-actions">
        <n-button secondary :disabled="installing" @click="resetPreview">
          {{ t('重新选择') }}
        </n-button>
        <n-button type="primary" :loading="installing" @click="confirmInstall">
          {{ t('立即安装') }}
        </n-button>
      </div>
    </div>
  </n-modal>

  <n-spin :show="loading">
    <div class="toolbar">
      <span class="muted">
        {{ t('共 {count} 个 · 启用 {enabled} · 可更新 {updates} · 排序：{sort}',
          { count: mods.length, enabled: enabledCount, updates: updateCount, sort: sortLabel }) }}
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
              :color="tagColorFor('contrast')">
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
              :class="{
                disabled: !mod.enabled,
                expanded: isExpanded(mod.dll_file),
                'focus-flash': highlightId === bindingRepositoryId(mod),
              }" @click="onCardClick($event, mod)">

              <!-- 单行：左「开关 + 标题」… 右「标签 | 工具栏」 -->
              <div class="card-row">
                <div class="card-left">
                  <n-switch class="card-switch" size="small" :value="mod.enabled" :loading="!!busy[mod.dll_file]"
                    @click.stop @update:value="(value: boolean) => toggle(mod, value)" />
                  <b class="card-title" :title="singleModTitle(mod)">{{ singleModTitle(mod) }}</b>
                </div>

                <div class="card-right">
                  <!-- 状态标签组：固定宽度按内容自适应，整体贴住分割线左侧 -->
                  <span class="tag-group">
                    <!-- 配色走 Naive n-tag 原生 :color，由 theme.ts 统一计算 -->
                    <n-tag v-for="tag in modTags(mod)" :key="tag.key" size="tiny" :bordered="false"
                      class="state-tag state-tag--link" :color="tagColorFor(tag.tone)"
                      :title="bindingRepositoryId(mod) ? t('在 MOD 安装中查看') : undefined"
                      @click.stop="openInRepository(mod)">{{ tag.text }}</n-tag>
                  </span>

                  <!-- 分割线：把标签区和操作按钮区分开 -->
                  <span class="tool-divider" />

                  <!-- 工具栏：可选按钮在最左（用占位保持列宽），固定按钮贴右 -->
                  <span class="tool-group" :style="{ minWidth: `${toolColumnWidth}px` }">
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
                    <span class="tool-spacer" />
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
              <n-collapse-transition :show="isExpanded(mod.dll_file)">
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
                    <n-tag v-if="pkg.updateCount" size="tiny" :bordered="false"
                      class="state-tag" :color="tagColorFor('contrast')">
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
                  <span class="tool-group" :style="{ minWidth: `${toolColumnWidth}px` }" @click.stop>
                    <n-button size="tiny" quaternary type="error" :title="t('整包删除')"
                      :disabled="game?.kitchen_running" @click="removePackage(pkg)">
                      <TrashOutline width="14" height="14" />
                    </n-button>
                  </span>
                </div>
              </header>

              <!-- 包展开/收起：同样用 Naive 的折叠过渡 -->
              <n-collapse-transition :show="isPackageExpanded(pkg.key)">
              <div class="package-body">
                <article v-for="mod in pkg.mods" :key="mod.dll_file" class="mod-card"
                  :data-focus-id="bindingRepositoryId(mod)"
                  :class="{
                    disabled: !mod.enabled,
                    expanded: isExpanded(mod.dll_file),
                    'focus-flash': highlightId === bindingRepositoryId(mod),
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
                      <span class="tool-group" :style="{ minWidth: `${toolColumnWidth}px` }">
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
                        <span class="tool-spacer" />
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

                  <n-collapse-transition :show="isExpanded(mod.dll_file)">
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
  <n-float-button position="fixed" bottom="28" right="32" :tooltip="`${t('排序')}：${sortLabel}`"
    @click="cycleSort">
    <SwapVerticalOutline width="17" height="17" />
  </n-float-button>

  <!-- 配置编辑器（Monaco）：编辑该 MOD 在 BepInEx/config 下的 .cfg -->
  <ModConfigEditor v-if="configTarget" v-model:show="configOpen" :dll-file="configTarget.dll_file"
    :title="modTitle(configTarget)" />
</template>

<style scoped>
.toolbar {
  display: flex; align-items: center; gap: 12px; margin-bottom: 14px;
}
.toolbar > .muted { margin-right: auto; }
/*
 * 拖放区：虚线矩形 + 居中加号，点击等效于「选择文件」。
 * 拖入时高亮边框，解析中换成 loading 并禁止再拖。
 */
.drop-zone {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 10px;
  min-height: 140px;
  padding: 20px;
  border: 2px dashed var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-sunken);
  cursor: pointer;
  transition: border-color .16s ease, background-color .16s ease,
    box-shadow .16s ease, transform .16s ease;
  user-select: none;
}
.drop-zone:hover,
.drop-zone:focus-visible { border-color: var(--oc2-accent); outline: none; }
/*
 * 拖入高亮：这是「可以放」的唯一提示（Tauri 的系统光标无法改成允许态）。
 * 描边变实线主题色 + 底色提亮 + 外发光 + 加号放大，四个信号叠加，
 * 让用户在系统「禁止」光标下依然能确定这里是放置目标。
 */
.drop-zone.active {
  border-style: solid;
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 14%, var(--oc2-sunken));
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--oc2-accent) 28%, transparent);
}
.drop-zone.active .drop-plus { transform: scale(1.25); }
.drop-zone.active .drop-hint { color: var(--oc2-text-1); }
.drop-zone.busy { cursor: progress; }
.drop-plus {
  font-size: 34px;
  line-height: 1;
  font-weight: 300;
  color: var(--oc2-accent);
  transition: transform .16s ease;
}
.drop-hint {
  font-size: 12.5px;
  color: var(--oc2-text-3);
  transition: color .16s ease;
}
/* 预览区：与详情区同款的紧凑信息密度 */
.install-preview {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 14px;
  padding-top: 14px;
  border-top: 1px solid var(--oc2-row-line);
}
.preview-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.preview-head b { font-size: 13.5px; color: var(--oc2-text-1); }
.preview-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 16px;
}
.preview-item { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.preview-item span { font-size: 11px; }
.preview-item b {
  font-size: 12px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow-wrap: anywhere;
}
.preview-list { display: flex; flex-direction: column; gap: 6px; max-height: 260px; overflow-y: auto; }
.preview-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border-radius: 8px;
  background: var(--oc2-sunken);
}
.preview-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 安装/预览底部的操作行：靠右 */
.install-actions { margin-top: 4px; display: flex; justify-content: flex-end; gap: 8px; }
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
/* 跨页跳转高亮：3 秒后自动褪去（时长由 crossLink.ts 控制） */
.mod-card.focus-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 16%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
}
/* 可点击的状态标签：手型 + hover 反馈，表示能跳到 MOD 安装页 */
.state-tag--link { cursor: pointer; }
.state-tag--link:hover { filter: brightness(1.25); }
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
 *   [tag-group 自适应] [divider 固定] [tool-group 固定 98px]
 * tag-group 用固定最小宽度，避免「标签多少」把分割线推来推去；
 * 标签在该列内右对齐，所以所有行的分割线、按钮列都落在同一条竖线上。
 * 98px 的来历：3 个 30px 按钮 + 3 个 2px gap（含 .tool-spacer 占位）。
 */
.tag-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  /* 至少容纳两个标签（约 2×60px），保证标签区起点一致 */
  min-width: 128px;
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
/* 工具栏：可选项在最左、固定按钮贴右，中间用 spacer 推开 */
.tool-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
  flex: 0 0 auto;
}
.tool-group :deep(.n-button) {
  width: 30px;
  min-width: 30px;
  padding: 0;
}
/* 没有配置按钮时保留同宽占位，保证右侧两个按钮列对齐 */
.tool-placeholder {
  width: 30px;
  height: 1px;
  flex: 0 0 30px;
}
/* 把可选项与固定按钮推成两端：可选项贴左，固定按钮贴右 */
.tool-spacer { flex: 1 1 auto; min-width: 2px; }
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
