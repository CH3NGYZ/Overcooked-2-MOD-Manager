<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton, NEmpty, NSpin, NSwitch, NTag, useDialog } from 'naive-ui'
import { FolderOpenOutline, RefreshOutline } from '@vicons/ionicons5'
import { t, locale } from '../i18n'
import { api } from '../services/tauri'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { tagColorFor } from '../theme'
import { useNotify } from '../useNotify'
import {
  LEVEL_SET_STATE_TAGS,
  LEVEL_SET_STATE_TONES,
  levelSetDisplayName,
  levelSetState,
  mapVersionForLevelSet,
  type LevelSetState,
  type MapVersionRef,
} from '../levelSetStatus.ts'
import type { LevelSetInfo, MapInfo, ZipLevelSet } from '../types'

/**
 * 地图管理页。
 *
 * ## 与 MOD 管理页的关系
 *
 * 布局与卡片外壳刻意保持一致（同一套 `--oc2-*` 变量、同样的圆角与间距），
 * 用户在两页之间切换时视觉连续。但**数据模型完全不同**：
 *
 * - MOD 是 BepInEx 插件，身份靠 BepInPlugin 的 GUID；
 * - 地图是 `levels/` 下的一个目录，身份靠 info 里的 `levelSetName + author`。
 *
 * 所以没有把「地图」塞进 `InstalledMod`：那样每个使用点都要判断「这是哪种」，
 * 而两边的字段（GUID vs 关卡数、依赖关系 vs 版本）几乎没有交集。
 */

const message = useNotify()
const dialog = useDialog()

const sets = ref<LevelSetInfo[]>([])
const loading = ref(true)
const busy = ref<Record<string, boolean>>({})
const importing = ref(false)
/** 地图源声明的地图（用于把本地关卡集绑到在线版本）。 */
const maps = ref<MapInfo[]>([])

/**
 * 从地图源算出「本地关卡集 → 在线版本」的对照表。
 *
 * 身份是 `author + levelSetName`，不是 `uid`：`uid` 是
 * `MD5(baseUID + version)`，每发一版就变，拿它当身份会把每次更新都认成
 * 新地图（见 `levelSetStatus.ts` 的 `isSameLevelSet`）。
 *
 * 只有刷新过 Release 的地图才有 tag —— 没刷新的地图不该让本地关卡集显示
 * 「最新版」，那是无根据的结论。
 */
const versionRefs = computed<MapVersionRef[]>(() =>
  maps.value
    .filter((map) => !!map.latest_tag)
    .map((map) => ({
      tag: map.latest_tag as string,
      levelSetName: map.level_set_name,
      author: map.author,
    })),
)

/** 从 ZIP 识别出的待导入关卡集。 */
const pendingImport = ref<ZipLevelSet[]>([])
/** 用户在导入确认里勾选的目录名。 */
const selectedDirs = ref<string[]>([])

const enabledCount = computed(() => sets.value.filter((set) => set.enabled).length)
const disabledCount = computed(() => sets.value.filter((set) => !set.enabled).length)

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

async function load() {
  loading.value = sets.value.length === 0
  try {
    /*
     * 两次读取互不依赖，一起发出去。地图列表失败不该让本地关卡集消失 ——
     * 那是最基础的信息，用户至少要看得到自己装了哪些图。
     */
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

onMounted(() => {
  void load()
})

/**
 * 单集的展示状态。
 *
 * `remote` 是地图源对这个关卡集的版本声明（`author + levelSetName` 匹配）；
 * 没刷过 Release 的地图不产生声明 —— 把「还没查」显示成「最新版」是错的结论。
 */
function stateOf(set: LevelSetInfo): LevelSetState {
  return levelSetState(set, mapVersionForLevelSet(set, versionRefs.value))
}

function stateTag(set: LevelSetInfo): { text: string; tone: ReturnType<typeof toneOf> } {
  const state = stateOf(set)
  return { text: LEVEL_SET_STATE_TAGS[state], tone: toneOf(state) }
}

function toneOf(state: LevelSetState) {
  return LEVEL_SET_STATE_TONES[state]
}

/** 状态说明：为什么是「未绑定」、为什么能更新。没有额外信息时不显示。 */
function stateHint(set: LevelSetInfo): string {
  const state = stateOf(set)
  if (state === 'unbound') return t('levelSet.unboundHint')
  if (state === 'update_available') {
    const remote = mapVersionForLevelSet(set, versionRefs.value)
    return remote ? t('levelSet.updateAvailableHint', { version: remote.tag }) : ''
  }
  return ''
}

/**
 * 切换开关。
 *
 * 后端在游戏运行时会返回 `levelSet.gameRunning` —— 移动 `levels/` 里的目录
 * 可能与 DIYLevel 正在做的枚举冲突，这条约束在实机验证前不放行。前端不自己
 * 判断游戏状态，直接按后端结果回滚开关，避免两处口径不一致。
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
    await load()
  } catch (reason) {
    // 开关失败时列表保持原状：下一次 load 会拿到真实磁盘状态
    message.error(backendMessageOf(reason))
  } finally {
    const next = { ...busy.value }
    delete next[set.dir_name]
    busy.value = next
  }
}

/** 选 ZIP → 识别里面的关卡集 → 弹确认。 */
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
    pendingImport.value = found
    selectedDirs.value = found.map((set) => set.dir_name)
    // 用确认框展示清单：只有「会覆盖已有版本」时才需要用户额外留意
    const replacing = found.filter((set) => set.exists)
    const detail = replacing.length
      ? t('levelSet.importConfirmReplace', {
          names: replacing.map((set) => set.name || set.dir_name).join('、'),
        })
      : t('levelSet.importConfirm', { count: found.length })
    /*
     * 用 Naive 的 dialog 而不是自绘弹窗：项目里所有确认都走它，
     * 视觉与交互（点外部关闭、Esc、焦点）已经统一。
     */
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
    const result = await api.importLevelSets(path, selectedDirs.value)
    message.success(t('levelSet.imported', { count: result.installed.length }))
    await load()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    importing.value = false
    pendingImport.value = []
    selectedDirs.value = []
  }
}

async function openLevelsDirectory() {
  try {
    const result = await api.openModDirectory()
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) {
    message.error(backendMessageOf(reason))
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

  <!-- 页头右侧：导入 ZIP → 打开地图目录 -->
  <Teleport to="#page-header-actions" defer>
    <n-button size="small" type="primary" :loading="importing" :disabled="importing" @click="pickZip">
      {{ t('levelSet.importZip') }}
    </n-button>
    <n-button size="small" secondary @click="openLevelsDirectory">
      <template #icon><FolderOpenOutline width="14" height="14" /></template>
      {{ t('modsLocal.openFolder') }}
    </n-button>
    <n-button size="small" secondary :loading="loading" @click="load">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('modsOnline.refresh') }}
    </n-button>
  </Teleport>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !sortedSets.length" class="empty"
      :description="t('levelSet.noLevelSets')" />

    <div v-else class="level-grid">
      <article v-for="set in sortedSets" :key="set.dir_name" class="level-card"
        :class="{ 'is-disabled': !set.enabled }">
        <!-- 缩略图位：解析器暂未导出 Texture2D，先显示名称首字，结构留好 -->
        <div class="level-thumb" aria-hidden="true">
          <span>{{ levelSetDisplayName(set, locale).slice(0, 1) }}</span>
        </div>

        <div class="level-body">
          <div class="level-head">
            <span class="level-title">{{ levelSetDisplayName(set, locale) }}</span>
            <n-tag size="tiny" :bordered="false" :color="tagColorFor(stateTag(set).tone)">
              {{ t(stateTag(set).text) }}
            </n-tag>
          </div>

          <div class="level-meta">
            <span v-if="set.author">{{ set.author }}</span>
            <span v-if="set.version">v{{ set.version }}</span>
            <span>{{ t('levelSet.levelCount', { count: set.level_count }) }}</span>
          </div>

          <!-- 解析失败时把原因说清楚：目录名还在，开关仍然可用 -->
          <div v-if="set.error" class="level-error">{{ translateBackendMessage(set.error) }}</div>
          <!-- 状态说明：未绑定 / 可更新时补一句原因，避免标签语义靠猜 -->
          <div v-else-if="stateHint(set)" class="level-hint">{{ stateHint(set) }}</div>
        </div>

        <div class="level-tools">
          <n-switch size="small" :value="set.enabled" :loading="!!busy[set.dir_name]"
            :title="t('levelSet.toggleHint')"
            @update:value="(value: boolean) => toggle(set, value)" />
        </div>
      </article>
    </div>
  </n-spin>
</template>

<style scoped>
/*
 * 卡片外壳与 MOD 管理页共用同一组 `--oc2-*` 变量与圆角/间距尺度，
 * 所以两页切换时视觉连续。结构不同：这里没有 GUID、关系标签、包层级，
 * 只有缩略图位 + 名称/作者/版本/关卡数 + 开关。
 */
.level-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
  gap: 12px;
}

.level-card {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  transition: border-color .16s ease, background-color .16s ease;
}

.level-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
}

/* 停用的卡片整体压暗，但保持可读 —— 用户还要能看清是什么再决定启用 */
.level-card.is-disabled {
  opacity: .62;
}

/*
 * 缩略图占位。
 *
 * 解析器目前只取文本元数据，`screenshot` 是 Unity 的 Sprite（DXT/ETC 纹理），
 * 需要额外的纹理解码，属于第二阶段。这里先占住位置与尺寸，到时把 <span>
 * 换成 <img> 即可，布局不用再调。
 */
.level-thumb {
  flex: 0 0 auto;
  width: 56px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 8px;
  background: color-mix(in srgb, var(--oc2-accent) 16%, transparent);
  color: var(--oc2-accent);
  font-size: 18px;
  font-weight: 600;
}

.level-body {
  flex: 1 1 auto;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.level-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.level-title {
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.level-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 12px;
  color: var(--oc2-text-3);
}

.level-error {
  font-size: 12px;
  color: var(--oc2-tone-critical-text);
}

.level-hint {
  font-size: 12px;
  color: var(--oc2-text-3);
  line-height: 1.4;
}

.level-tools {
  flex: 0 0 auto;
}

.empty {
  padding: 48px 0;
}
</style>
