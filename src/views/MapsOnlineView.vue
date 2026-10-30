<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton, NEmpty, NSpin, NTag } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { t, locale, pickI18n } from '../i18n'
import { api } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { tagColorFor } from '../theme'
import { useNotify } from '../useNotify'
import type { MapInfo, RequiredFramework } from '../types'

/**
 * 地图下载页。
 *
 * ## 与 MOD 安装页的关系
 *
 * 骨架同 `ModsOnlineView`：吸顶分组标题、卡片网格、页头聚合标签、单卡独立
 * 刷新与安装状态。用户在两页之间切换不会有陌生感。数据模型不同，所以没有
 * 复用同一份组件：
 *
 * - MOD 卡片要展示「已安装版本 vs Release」，需要插件绑定与更新状态；
 *   地图的本地版本要先解析 `info` 才拿得到，那是管理页的职责。
 * - 地图是「一个仓库 = 一个关卡集」，没有包层级、没有依赖标签的多分支。
 *
 * 这一页只回答两件事：这张图是什么（简介 / 作者 / 关卡集名），以及怎么装上。
 * 装完由管理页负责开关与版本对照 —— 职责分开，两页都不必同时持有两套模型。
 */

const message = useNotify()

const maps = ref<MapInfo[]>([])
const loading = ref(true)
const refreshing = ref<Record<string, boolean>>({})
const installing = ref<Record<string, boolean>>({})
const refreshingAll = ref(false)

/** 按 `group_i18n` 分组，与 MOD 安装页同一套分组思路。 */
const groups = computed(() => {
  const byGroup = new Map<string, { label: string; items: MapInfo[] }>()
  for (const map of maps.value) {
    const label = pickI18n(map.group_i18n, map.group ?? '') || t('common.other')
    const bucket = byGroup.get(label)
    if (bucket) bucket.items.push(map)
    else byGroup.set(label, { label, items: [map] })
  }
  return [...byGroup.values()].sort((left, right) =>
    left.label.localeCompare(right.label, locale.value),
  )
})

async function load() {
  loading.value = maps.value.length === 0
  try {
    maps.value = await api.maps()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void load()
})

/** 展示名：多语言表 → 单语言镜像 → 仓库坐标。 */
function mapName(map: MapInfo): string {
  return pickI18n(map.name_i18n, map.name) || `${map.username}/${map.repository}`
}

/** 简介：下载页要留足说明，用户才知道这张图是什么。 */
function mapNote(map: MapInfo): string {
  return pickI18n(map.note_i18n, map.note ?? '')
}

function isRefreshing(map: MapInfo): boolean {
  return !!refreshing.value[map.id]
}

function isInstalling(map: MapInfo): boolean {
  return !!installing.value[map.id]
}

/** 是否已经拿到 Release（决定安装按钮能不能点）。 */
function hasRelease(map: MapInfo): boolean {
  return !!map.latest_tag
}

/** 依赖提示：地图跑不起来最常见的原因是框架版本不够。 */
function dependencyHint(map: MapInfo): string {
  const frameworks = map.required_frameworks ?? []
  if (!frameworks.length) return ''
  return frameworks
    .map((framework) => describeFramework(framework))
    .join(' · ')
}

/** 一条依赖的展示文案：GUID 太抽象，补上「已装多少」。 */
function describeFramework(framework: RequiredFramework): string {
  const required = framework.min_version ? `${framework.guid} ≥ ${framework.min_version}` : framework.guid
  return framework.installed_version
    ? `${required}（${t('levelSet.installedVersion', { version: framework.installed_version })}）`
    : required
}

/** 没满足的依赖：安装按钮仍然可点（用户可能想先装上再升级框架），但要先说清楚。 */
function unmetHint(map: MapInfo): string {
  const unmet = map.unmet_frameworks ?? []
  if (!unmet.length) return ''
  return t('levelSet.frameworksNotMet', {
    list: unmet.map((framework) => describeFramework(framework)).join(' · '),
  })
}

function setFlag(flags: typeof refreshing, id: string, value: boolean) {
  const next = { ...flags.value }
  if (value) next[id] = true
  else delete next[id]
  flags.value = next
}

/** 刷新一张地图的 Release（用户主动点刷新 → 绕过 10 分钟缓存）。 */
async function refresh(map: MapInfo) {
  if (isRefreshing(map) || isInstalling(map)) return
  setFlag(refreshing, map.id, true)
  try {
    await api.refreshMapReleases(map.id, true)
    await load()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    setFlag(refreshing, map.id, false)
  }
}

/**
 * 安装。
 *
 * 后端会先回收同名的旧关卡集（进回收站）再写入新版本，所以不需要额外的
 * 「覆盖确认」：旧数据不会丢，走的是和 MOD 在线重装同一条策略。
 */
async function install(map: MapInfo) {
  if (isInstalling(map) || isRefreshing(map)) return
  setFlag(installing, map.id, true)
  try {
    await api.installMapRelease(map.id, map.latest_tag ?? undefined)
    message.success(t('levelSet.installDone', { name: mapName(map) }))
    await load()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    setFlag(installing, map.id, false)
  }
}

/**
 * 刷新全部。
 *
 * 两步：先重拉源树（索引 → 子源，拿到最新的地图清单），再逐个仓库刷 Release。
 * 顺序不能反 —— 源树可能新增/删除了地图，先刷 Release 会给已经不存在的仓库
 * 发请求，新加的地图又拿不到版本。
 *
 * 单个仓库失败不中断：一个作者仓库挂了，其他地图仍应刷新成功。
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
    await load()
  } finally {
    refreshingAll.value = false
  }
}
</script>

<template>
  <!-- 页头聚合：地图总数，与 MOD 安装页同一套标签外观 -->
  <Teleport to="#page-header-title-extra" defer>
    <n-tag size="small" :bordered="false" class="state-tag" :color="tagColorFor('accent')">
      {{ maps.length }} {{ t('levelSet.mapCount') }}
    </n-tag>
  </Teleport>

  <Teleport to="#page-header-actions" defer>
    <n-button size="small" secondary :loading="refreshingAll" :disabled="refreshingAll"
      :title="t('levelSet.refreshAllHint')" @click="refreshAll">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('modsOnline.refresh') }}
    </n-button>
  </Teleport>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !maps.length" class="empty" :description="t('levelSet.noMapSources')" />

    <template v-else>
      <section v-for="group in groups" :key="group.label" class="map-group">
        <h2 class="map-group-title">{{ group.label }}</h2>
        <div class="map-grid">
          <article v-for="map in group.items" :key="map.id" class="map-card">
            <div class="map-head">
              <span class="map-title">{{ mapName(map) }}</span>
              <n-tag v-if="map.builtin" size="tiny" :bordered="false" :color="tagColorFor('muted')">
                {{ t('levelSet.builtinSource') }}
              </n-tag>
              <n-tag v-if="map.latest_tag" size="tiny" :bordered="false"
                :color="tagColorFor('accent')">
                {{ map.latest_tag }}
              </n-tag>
              <n-tag v-else-if="map.last_error" size="tiny" :bordered="false"
                :color="tagColorFor('critical')">
                {{ t('modsOnline.repositoryRefreshFailed') }}
              </n-tag>
            </div>

            <!-- 简介：作者写的地图说明，按当前语言取 -->
            <p v-if="mapNote(map)" class="map-note">{{ mapNote(map) }}</p>

            <div class="map-meta">
              <span>{{ map.username }}/{{ map.repository }}</span>
              <span v-if="map.level_set_name">{{ map.level_set_name }}</span>
              <span v-if="map.author">{{ map.author }}</span>
            </div>

            <!-- 依赖要求：框架版本不够时地图跑不起来，必须提前说 -->
            <div v-if="dependencyHint(map)" class="map-deps">
              {{ t('levelSet.requiresFrameworks', { list: dependencyHint(map) }) }}
            </div>
            <div v-if="unmetHint(map)" class="map-deps step is-unmet">
              {{ unmetHint(map) }}
            </div>

            <div class="map-actions">
              <n-button size="small" secondary :loading="isRefreshing(map)"
                :disabled="isRefreshing(map) || isInstalling(map)" @click="refresh(map)">
                {{ t('modsOnline.refresh') }}
              </n-button>
              <n-button size="small" type="primary" :loading="isInstalling(map)"
                :disabled="!hasRelease(map) || isRefreshing(map) || isInstalling(map)"
                :title="hasRelease(map) ? undefined : t('levelSet.refreshFirstHint')"
                @click="install(map)">
                {{ t('modsOnline.install') }}
              </n-button>
            </div>
          </article>
        </div>
      </section>
    </template>
  </n-spin>
</template>

<style scoped>
/*
 * 与 MOD 安装页同一套视觉尺度：分组标题 + 卡片网格 + 同样的圆角与主题变量。
 * 结构更简单 —— 地图是一仓一集，没有包层级、没有关系标签分支。
 */
.map-group {
  margin-bottom: 20px;
}

.map-group-title {
  position: sticky;
  top: 0;
  z-index: 2;
  margin: 0 0 10px;
  padding: 6px 2px;
  font-size: 13px;
  font-weight: 600;
  color: var(--oc2-text-3);
  background: var(--oc2-body);
}

.map-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
  gap: 12px;
}

.map-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
}

.map-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
}

.map-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.map-title {
  font-weight: 600;
}

.map-note {
  margin: 0;
  font-size: 12px;
  line-height: 1.5;
  color: var(--oc2-text-3);
}

.map-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 12px;
  color: var(--oc2-text-3);
}

.map-deps {
  font-size: 12px;
  color: var(--oc2-text-3);
}

/* 未满足的依赖用警示色：它是「装上也跑不起来」的实际阻碍 */
.map-deps.is-unmet {
  color: var(--oc2-tone-moderate-text);
}

.map-actions {
  display: flex;
  gap: 8px;
  margin-top: 2px;
}

.empty {
  padding: 48px 0;
}
</style>
