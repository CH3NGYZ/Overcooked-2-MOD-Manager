<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { NButton, NEmpty, NSpin, NTag } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { t, locale, pickI18n } from '../i18n'
import { api } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { tagColorFor } from '../theme'
import { useNotify } from '../useNotify'
import { useDashboard } from '../composables/useDashboardStore'
import { useInstallPlanGate } from '../composables/useInstallPlanGate'
import { versionDisplay } from '../modStatus'
import { markdownSummary } from '../markdown'
import type { Repository } from '../types'
import OnlineCard, { type CardAction, type CardDependencyTag } from '../components/OnlineCard.vue'
import InstallPlanDialog from '../components/InstallPlanDialog.vue'
import type { MapInfo } from '../types'

/**
 * 地图下载页。
 *
 * ## 与 MOD 安装页的关系
 *
 * 卡片**就是同一个组件**（`OnlineCard.vue`）：圆角、状态标签、两段式版本号、
 * 下载量、主按钮的配色与图标规则全部共用，两页之间不会有视觉漂移。
 *
 * 这一页只负责「把地图数据翻译成卡片的输入」：
 *
 * | 卡片字段 | MOD 来源 | 地图来源 |
 * |---|---|---|
 * | 状态 | 已装插件的 GUID + 版本 | 地图是否有 Release（本地关卡集版本在管理页） |
 * | 依赖提示 | 插件依赖关系 | 框架依赖（DIYLevel / Web Runtime） |
 * | 主按钮 | 安装 / 重装 / 更新 / 刷新 | 安装 / 刷新 |
 *
 * 安装前检查与下载进度也共用：地图的前置是框架级组件，缺了地图跑不起来，
 * 所以必须先弹检查；下载走 MOD 那条候选竞速，进度条由全局 `DownloadStrip` 显示。
 */

const message = useNotify()
const { mods, load } = useDashboard()

const maps = ref<MapInfo[]>([])
const loading = ref(true)
const refreshing = ref<Record<string, boolean>>({})
const installing = ref<Record<string, boolean>>({})
const refreshingAll = ref(false)

/**
 * 安装前检查。
 *
 * 地图的 `relations` 与 MOD 仓库的 `relations` 是同一套模型，所以判定逻辑也
 * 共用 `useInstallPlanGate`：它需要一个 `Repository` 作为关系来源。地图不是
 * `Repository`，这里做一次**投影**（`referenceRepository`）把声明搬过去 ——
 * 只填关系判定真正会读的字段。
 *
 * 为什么要投影而不是把 `MapInfo` 直接塞进去：`buildInstallPlan` 还会找「能
 * 满足依赖的来源仓库」，那需要在 `repositories` 列表里按 `plugin_guid` 匹配。
 * 地图的依赖是框架（DIYLevel），它的安装入口在设置页而不是地图页，
 * 所以候选来源列表传空 —— 于是缺框架只会显示为「提醒」，不会给一个点了
 * 也没用的「安装依赖」按钮。
 */
const planGate = useInstallPlanGate({
  mods: () => mods.value,
  repositories: () => [],
  gameBlocked,
  onApplied: () => load(),
})

function gameBlocked() {
  // 地图安装允许在游戏运行时进行（与本地 ZIP 导入同一条策略），所以这里不拦
  return false
}

/** 把地图的框架依赖投影成一个只用于关系判定的 `Repository`。 */
function referenceRepository(map: MapInfo): Repository {
  return {
    id: map.id,
    name: map.name,
    name_i18n: map.name_i18n,
    provider: map.provider,
    username: map.username,
    repository: map.repository,
    // 下面这些字段关系判定不读，给安全默认值
    download_mode: 'release',
    branch: '',
    file_path: '',
    channel: 'all',
    asset_rules: [],
    install_target: '',
    note: '',
    plugin_guid: '',
    local_product_name: '',
    local_original_filename: '',
    version_regex: '',
    max_release_count: 10,
    download_threads: 0,
    enabled: map.enabled,
    builtin: map.builtin,
    cached_releases: map.latest_release ? [map.latest_release] : [],
    relations: (map.required_frameworks ?? []).map((framework) => ({
      kind: 'requires' as const,
      guid: framework.guid,
      min_version: framework.min_version ?? '',
      max_version: '',
    })),
  }
}

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

onMounted(() => {
  void loadMaps()
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

/** 是否已经拿到 Release（决定状态标签与按钮）。 */
function hasRelease(map: MapInfo): boolean {
  return !!map.latest_tag
}

/**
 * 卡片状态：只按「有没有刷新到 Release」判断。
 *
 * 本地关卡集的版本在**管理页**才读得到（要解析 `info`），所以这里不能声称
 * 「最新版 / 可更新」—— 把没检查的说成已是最新，会让用户错过真实更新。
 */
function statusOf(map: MapInfo): { text: string; tone: string } {
  if (map.last_error) return { text: t('modsOnline.repositoryRefreshFailed'), tone: 'severe' }
  if (!hasRelease(map)) return { text: t('modsOnline.notRefreshed'), tone: 'muted' }
  return { text: t('levelSet.releaseReady'), tone: 'accent' }
}

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

/**
 * 依赖提示标签。
 *
 * 后端已经给出了 `unmet_frameworks`（对照本地已装版本判定），所以这里只
 * 决定「显示还是不显示」。地图的前置是框架而不是可安装的仓库，因此标签
 * **不可点击** —— 点了也无处可去，假装可点只会制造一次无反应的点击。
 */
function dependencyOf(map: MapInfo): CardDependencyTag | null {
  const unmet = map.unmet_frameworks ?? []
  if (!unmet.length) return null
  return {
    text: 'levelSet.missingFramework',
    tone: 'critical',
    title: t('levelSet.frameworksNotMet', { list: unmet.map(describeFramework).join(' · ') }),
    clickable: false,
  }
}

/** 一条依赖的展示文案：GUID 太抽象，补上「已装多少」。 */
function describeFramework(framework: { guid: string; min_version?: string; installed_version?: string | null }): string {
  const required = framework.min_version ? `${framework.guid} ≥ ${framework.min_version}` : framework.guid
  return framework.installed_version
    ? `${required}（${t('levelSet.installedVersion', { version: framework.installed_version })}）`
    : required
}

/** 主按钮：没有 Release 时是「刷新」，否则「安装」。 */
function actionOf(map: MapInfo): CardAction {
  const refresh = !hasRelease(map)
  return {
    kind: refresh ? 'refresh' : 'install',
    label: refresh ? t('modsOnline.refresh') : t('modsOnline.install'),
    loading: refresh ? isRefreshing(map) : isInstalling(map),
    disabled: isRefreshing(map) || isInstalling(map),
  }
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
 *   1. 安装前检查（框架缺失 / 版本不够 → 只提醒，不阻止）
 *   2. 下载 + 校验（后端走候选竞速，进度由全局 DownloadStrip 显示）
 *   3. 落盘（先回收同名旧关卡集）
 */
async function install(map: MapInfo) {
  if (isInstalling(map) || isRefreshing(map)) return
  // 前置检查：地图缺框架时装上也跑不起来，必须先让用户看到
  const ready = await planGate.run(referenceRepository(map), map.latest_tag ?? undefined)
  if (!ready) return

  setFlag(installing, map.id, true)
  try {
    await api.installMapRelease(map.id, map.latest_tag ?? undefined)
    message.success(t('levelSet.installDone', { name: mapName(map) }))
    await loadMaps()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    setFlag(installing, map.id, false)
  }
}

/** 主按钮点击：没 Release 就刷新，有就安装。 */
function runAction(map: MapInfo) {
  if (!hasRelease(map)) void refresh(map)
  else void install(map)
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
    await loadMaps()
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
          <!-- 卡片就是 MOD 安装页那张：只把数据换成地图的 -->
          <OnlineCard v-for="map in group.items" :key="map.id"
            :provider="map.provider" :owner="map.username" :title="mapName(map)"
            :note="mapNote(map)"
            :status-text="statusOf(map).text" :status-tone="statusOf(map).tone"
            :dependency="dependencyOf(map)"
            :version="versionOf(map)" :refreshing="isRefreshing(map)"
            :downloads="downloadCount(map)"
            :action="actionOf(map)"
            :error="map.last_error"
            @action="runAction(map)" />
        </div>
      </section>
    </template>
  </n-spin>

  <!-- 安装前检查：框架依赖不满足时只提醒，不阻止安装 -->
  <InstallPlanDialog v-model:show="planGate.open.value" :plan="planGate.plan.value"
    :target-label="planGate.target.value"
    @finished="(ready: boolean) => { void planGate.finish(ready) }" />
</template>

<style scoped>
/*
 * 只保留分组与网格：卡片内部样式全在 `OnlineCard.vue`，
 * 与 MOD 安装页共用一份，两页之间不会各自漂移。
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
  grid-template-columns: repeat(auto-fill, minmax(min(310px, 100%), 1fr));
  gap: 12px;
  margin: 2px 28px 6px 28px;
}

.empty {
  padding: 48px 0;
}
</style>
