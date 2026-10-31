<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton, NEmpty, NFloatButton, NSpin, NTabPane, NTabs, NTag, useDialog } from 'naive-ui'
import { FolderOpenOutline, RefreshOutline, TrashOutline } from '@vicons/ionicons5'
import { t, locale, pickI18n } from '../i18n'
import { api } from '../services/tauri'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { tagColorFor, tagColorForAny } from '../theme'
import { useNotify } from '../useNotify'
import { useDashboard } from '../composables/useDashboardStore'
import { useFocusHighlight, useMapLink } from '../crossLink'
import LocalItemCard from '../components/LocalItemCard.vue'
import DependencyTable from '../components/DependencyTable.vue'
import type { DependencyRow } from '../components/DependencyTable.vue'
import {
  frameworkStates,
  levelSetCardTags,
  levelSetDisplayName,
  levelSetState,
  mapForLevelSet,
  mapVersionForLevelSet,
  mapVersionRefs,
  type FrameworkState,
  type LevelSetCardTag,
} from '../levelSetStatus.ts'
import type { LevelSetInfo, MapInfo } from '../types'

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
 * ## 性能约定
 *
 * 进入页面只读一次 `api.levelSets()` 与 `api.maps()`。开关、删除、批量开关
 * 只影响本地关卡集，之后**只重读关卡集**；地图源列表带 Release 缓存，
 * 与这些操作无关。「检测更新」是唯一重拉 Release 的入口，浮动刷新是唯一
 * 「两端都重读」的入口。
 */

const message = useNotify()
const dialog = useDialog()
const { config, game, mods, ensureLoaded } = useDashboard()
const { linkToMap } = useMapLink()
/** 跨页跳转落点（从地图下载页点状态标签跳过来时带 `focus=<地图 id>`）。 */
const { highlightId } = useFocusHighlight()

/** 工具栏：两个按钮（打开、删除），列宽与间距与 MOD 管理页同源。 */
const TOOL_BUTTON_WIDTH = 30
const TOOL_GAP = 8
const toolsMinWidth = TOOL_BUTTON_WIDTH * 2 + TOOL_GAP

const sets = ref<LevelSetInfo[]>([])
/** 地图源声明的地图（含 Release 缓存），只用于版本对照与依赖判定。 */
const maps = ref<MapInfo[]>([])
const loading = ref(true)
/** 每个集自己的开关请求状态（互不阻塞）。 */
const busy = ref<Record<string, boolean>>({})
/** 每个集自己的删除请求状态。 */
const deleting = ref<Record<string, boolean>>({})
/** 当前展开详情的目录名：一次只展开一个，点另一张卡就换过去。 */
const expandedDir = ref<string | null>(null)

const enabledCount = computed(() => sets.value.filter((set) => set.enabled).length)
const disabledCount = computed(() => sets.value.filter((set) => !set.enabled).length)
/** 游戏是否探测到 —— 没探测到时本地 MOD 列表必然为空，不能因此判「缺依赖」。 */
const gameProbed = computed(() => !!game.value?.path)
const kitchenRunning = computed(() => !!game.value?.kitchen_running)
/** 删除走回收站还是直接删（与设置页同一开关，文案与后端行为都跟它走）。 */
const recyclesOnDelete = computed(() => config.value?.recycle_deleted_mods !== false)

/** 已排序：启用的在前，同组按展示名排。 */
const sortedSets = computed(() =>
  [...sets.value].sort((left, right) => {
    if (left.enabled !== right.enabled) return left.enabled ? -1 : 1
    return levelSetDisplayName(left, locale.value).localeCompare(
      levelSetDisplayName(right, locale.value),
      locale.value,
    )
  }),
)

/**
 * 从地图源算出「本地关卡集 → 在线版本」的对照表。
 *
 * 只有刷新过 Release 的地图才有 tag —— 没刷新的地图不该让本地关卡集显示
 * 「最新版」，那是无根据的结论（见 `levelSetStatus.ts` 的 `mapVersionRefs`）。
 */
const versionRefs = computed(() => mapVersionRefs(maps.value))

/**
 * 首次进入：两端数据一起读。
 *
 * 两次读取互不依赖，一起发出去。地图列表失败不该让本地关卡集消失 ——
 * 那是最基础的信息，用户至少要看得到自己装了哪些图。
 */
async function load() {
  loading.value = sets.value.length === 0
  try {
    const [levelSets, mapList] = await Promise.all([
      api.levelSets(),
      api.maps().catch(() => [] as MapInfo[]),
    ])
    sets.value = levelSets
    maps.value = mapList
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
 * 地图源列表（`api.maps()`）不会因此变化，重拉纯属浪费。
 */
async function reloadSets() {
  try {
    sets.value = await api.levelSets()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

onMounted(() => {
  /*
   * 依赖缺失判定与「删除是否进回收站」都要读 Dashboard（mods / config），
   * 而 Dashboard 由页面自己按需加载。直接进本页时它可能还是空的：那时
   * 「依赖缺失」标签会消失、删除确认会退回默认文案 —— 都是无声的错误结论。
   * 已有数据时 `ensureLoaded` 直接返回，不会重复扫描。
   */
  void ensureLoaded()
  void load()
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

/** 卡片状态标签：先依赖提醒，再版本结论（由 `levelSetStatus` 统一排序）。 */
function tagsOf(set: LevelSetInfo): LevelSetCardTag[] {
  return levelSetCardTags(set, maps.value, mods.value, gameProbed.value)
}

/** 绑定地图的框架依赖在本地是否满足。 */
function boundFrameworks(set: LevelSetInfo): FrameworkState[] {
  return frameworkStates(boundMapOf(set), mods.value)
}

function missingFrameworks(set: LevelSetInfo): FrameworkState[] {
  return boundFrameworks(set).filter((state) => !state.satisfied)
}

/**
 * 标签悬停提示：标签本身很短，把「为什么」放在 title 里。
 *
 * - 依赖缺失：列出具体缺哪个前置、要求什么版本；
 * - 有更新（可点）：说明点下去会去哪儿，避免用户以为是就地更新。
 */
function tagTitle(set: LevelSetInfo, tag: LevelSetCardTag): string | undefined {
  if (tag.kind === 'dependency') {
    const missing = missingFrameworks(set)
    const list = missing
      .map((state) => `${state.guid}${state.minVersion ? ` ≥ ${state.minVersion}` : ''}`)
      .join(' · ')
    return t('levelSet.frameworksNotMet', { list: list || t('levelSet.dependencyMissing') })
  }
  if (tag.mapId) return t('levelSet.viewInMapDownload')
  return undefined
}

function onTagClick(tag: LevelSetCardTag) {
  if (tag.mapId) linkToMap(tag.mapId)
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

/** 框架依赖的本地状态文案（优先说「已装什么版本」，再给结论）。 */
function frameworkText(state: FrameworkState): string {
  if (state.installedVersion) {
    return state.satisfied
      ? t('levelSet.installedVersion', { version: state.installedVersion })
      : t('levelSet.versionMismatch', { version: state.installedVersion })
  }
  return t('levelSet.notInstalled')
}

/**
 * 依赖表行。
 *
 * 与地图下载页的详情、MOD 安装页的详情共用同一个表格组件
 * （`DependencyTable.vue`）：同一张地图在两个页面里看到的依赖必须长得一样，
 * 否则用户会怀疑哪边漏报了一条。
 */
function dependencyRows(set: LevelSetInfo): DependencyRow[] {
  return boundFrameworks(set).map((state) => ({
    key: state.guid,
    name: state.guid,
    requirement: state.minVersion ? `≥ ${state.minVersion}` : '',
    status: frameworkText(state),
    tone: state.satisfied ? 'mild' : 'critical',
  }))
}

/** 卡片行点击：展开自己，或收起已展开的那张。 */
function activate(set: LevelSetInfo) {
  expandedDir.value = expandedDir.value === set.dir_name ? null : set.dir_name
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

/** 检测更新：只刷新「本地已装地图绑定的地图」的 Release，与 MOD 管理页同一规则。 */
const checkingUpdates = ref(false)
async function checkUpdates() {
  if (checkingUpdates.value) return
  const targets = new Map<string, MapInfo>()
  for (const set of sets.value) {
    const map = boundMapOf(set)
    if (map) targets.set(map.id, map)
  }
  if (!targets.size) {
    message.warning(t('levelSet.noBoundMaps'))
    return
  }
  checkingUpdates.value = true
  try {
    // 串行：地图数量少，而且用户要的是「逐张图正确」而不是最快的并发
    for (const id of targets.keys()) await api.refreshMapReleases(id, true)
    maps.value = await api.maps()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    checkingUpdates.value = false
  }
}
</script>

<template>
  <!-- 页头标题右侧：状态聚合，与 MOD 管理页同一套标签外观 -->
  <Teleport to="#page-header-title-extra" defer>
    <n-tag v-if="enabledCount" size="small" :bordered="false" class="state-tag"
      :color="tagColorFor('accent')">
      {{ enabledCount }} {{ t('modsLocal.enabled2') }}
    </n-tag>
    <n-tag v-if="disabledCount" size="small" :bordered="false" class="state-tag"
      :color="tagColorFor('muted')">
      {{ disabledCount }} {{ t('modsLocal.disabled') }}
    </n-tag>
  </Teleport>

  <!--
    页头右侧，从左到右：批量开关 → 检测更新 → 打开目录（与 MOD 管理页同序）。
    安装入口不在这页：它属于安装页的职责，见地图下载页的「安装地图」。
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
    <n-button size="small" secondary :loading="checkingUpdates" :disabled="checkingUpdates || loading"
      :title="t('levelSet.checkUpdatesHint')" @click="checkUpdates">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('overview.check') }}
    </n-button>
    <n-button size="small" secondary @click="openLevelsDirectory">
      <template #icon><FolderOpenOutline width="14" height="14" /></template>
      {{ t('modsLocal.openFolder') }}
    </n-button>
  </Teleport>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !sortedSets.length" class="empty"
      :description="t('levelSet.noLevelSets')" />

    <div v-else class="level-list">
      <LocalItemCard v-for="set in sortedSets" :key="set.dir_name"
        :data-focus-id="focusIdOf(set)"
        :enabled="set.enabled"
        :toggle-loading="!!busy[set.dir_name]"
        :title="levelSetDisplayName(set, locale)"
        :title-tooltip="set.error ? translateBackendMessage(set.error) : levelSetDisplayName(set, locale)"
        :disabled="!set.enabled"
        :expanded="expandedDir === set.dir_name"
        :focus-flash="!!highlightId && highlightId === focusIdOf(set)"
        :tools-min-width="toolsMinWidth"
        :tool-gap="TOOL_GAP"
        :tool-button-width="TOOL_BUTTON_WIDTH"
        @toggle="(value: boolean) => toggle(set, value)"
        @activate="activate(set)">
        <!-- 状态标签：依赖缺失在前，版本结论在后；「有更新」可点跳下载页 -->
        <template #status>
          <span class="tag-group">
            <n-tag v-for="tag in tagsOf(set)" :key="tag.kind + tag.text" size="tiny"
              :bordered="false" class="state-tag"
              :class="{ 'state-tag--link': !!tag.mapId }"
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
            <div v-if="set.name_zh" class="detail-row">
              <span class="detail-label">{{ t('levelSet.nameZh') }}</span>
              <span class="detail-value">{{ set.name_zh }}</span>
            </div>
            <div class="detail-row">
              <span class="detail-label">{{ t('modsLocal.author') }}</span>
              <span class="detail-value">{{ set.author || '—' }}</span>
            </div>
            <div class="detail-row">
              <span class="detail-label">{{ t('levelSet.version') }}</span>
              <span class="detail-value">{{ set.version || '—' }}</span>
            </div>
            <div class="detail-row">
              <span class="detail-label">{{ t('levelSet.levelCountLabel') }}</span>
              <span class="detail-value">{{ set.level_count }}</span>
            </div>
            <div class="detail-row">
              <span class="detail-label">{{ t('levelSet.directory') }}</span>
              <span class="detail-value mono">{{ set.dir_name }}</span>
            </div>
            <div class="detail-row">
              <span class="detail-label">{{ t('levelSet.location') }}</span>
              <span class="detail-value mono">{{ set.enabled ? 'levels/' : 'disabled/' }}</span>
            </div>
            <!-- 解析失败：原因放在详情里，卡片行只留标签 -->
            <div v-if="set.error" class="detail-row">
              <span class="detail-label">{{ t('levelSet.parseError') }}</span>
              <span class="detail-value is-error">{{ translateBackendMessage(set.error) }}</span>
            </div>
            <div v-if="hintOf(set)" class="detail-row">
              <span class="detail-label">{{ t('levelSet.statusHint') }}</span>
              <span class="detail-value">{{ hintOf(set) }}</span>
            </div>
            <div class="detail-row">
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
          </div>

          <!-- 依赖框架：本地缺失的标红，用户重新启用前就能看到；与下载页详情、MOD 安装页详情共用同一张依赖表 -->
          <div v-if="boundFrameworks(set).length" class="detail-section">
            <DependencyTable :rows="dependencyRows(set)" mono-name />
          </div>
        </template>
      </LocalItemCard>
    </div>
  </n-spin>

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
 * 列表：与 MOD 管理页的卡片列表同一竖向节奏（10px）。
 * 卡片外壳、悬停、展开动画都由 LocalItemCard 负责，这里只管插槽内容。
 */
.level-list {
  display: flex;
  flex-direction: column;
  gap: 10px;
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

/* 详情：两列信息网格，值允许换行（GUID / 目录名可能很长） */
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

.empty {
  padding: 48px 0;
}
</style>
