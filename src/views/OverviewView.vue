<script setup lang="ts">
import { requestToggleChanges } from '../modChanges/requests'
/*
 * 总览页：把「框架 → 在线 → 本地」摆成两条纵列，回答「现在要不要管」。
 *
 *   ｜左列 MOD 链路 ｜ BepInEx → 在线 MOD → 本地 MOD
 *   ｜右列 地图链路 ｜ DIYLevel → 在线地图 → 本地地图
 *
 * 不按「框架 / MOD / 地图」横着分三个区：那样每区只有两张卡，
 * 用户得在三条横带之间来回跳，才能拼出「框架没装 → 后面的内容都不生效」
 * 这个顺序；两列纵向排列后，一列自上而下就是实际操作顺序，
 * 左右并排还能顺手对比「MOD 与地图各装了多少、各有几个要处理」。
 * 回收站卡片在这里删掉了 —— 它属于「工具」页的维护入口，放在总览会让
 * 「现在要不要管」多出一个与游玩无关的计数。
 *
 * 刻意不做的东西：趋势折线（没有历史存储）、状态饼图（十几项画饼图没信息量）、
 * 操作时间线（没有审计日志）。这页是「装完就走」的控制台，不是监控面板。
 *
 * 操作一律走「跳转到对应页面的定位位置」，不在这里直接执行：
 * 更新要看预览、依赖和被覆盖的文件，刷新失败要看仓库的错误详情，
 * 在总览里一键执行等于绕过这些信息。
 */
import { computed, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { NButton, NSwitch } from 'naive-ui'
import {
  AddCircleOutline, CloudDownloadOutline, RefreshOutline, SettingsOutline,
} from '@vicons/ionicons5'
import StatCard from '../components/StatCard.vue'
import { useDashboard } from '../composables/useDashboardStore'
import { useModRepositoryRefresh } from '../composables/useModRepositoryRefresh'
import { useMapSources } from '../composables/useMapSources'
import { useLevelSetScan } from '../composables/useLevelSetScan'
import { api } from '../services/tauri'
import { modRepositoryIsAvailable } from '../repositoryAvailability'
import { t } from '../i18n'
import { runtimeModStatus } from '../modRuntimeStatus'
import {
  levelSetAggregateState, aggregateLevelSetVersion, levelSetIsRefreshing,
} from '../levelSetStatus.ts'
import { useNotify } from '../useNotify'
import { FOCUS_QUERY } from '../crossLink'
import type { LevelSetInfo } from '../types'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { demoActive, withDemoConfig, withDemoDiyLevelTarget, withDemoGame, withDemoLevelSets, withDemoMaps, withDemoMods } from '../tutorial/demo'

const router = useRouter()
const message = useNotify()
const { ensureLoaded, load, modsLoading: realModsLoading, modsGeneration,
  error: realError, mods: realMods, diyLevelTarget: realDiyLevelTarget, game: realGame, config: realConfig } = useDashboard()
const mods = computed(() => withDemoMods(realMods.value))
const config = computed(() => withDemoConfig(realConfig.value))
const game = computed(() => withDemoGame(realGame.value))
const diyLevelTarget = computed(() => withDemoDiyLevelTarget(realDiyLevelTarget.value))
const modsLoading = computed(() => !demoActive.value && realModsLoading.value)
const error = computed(() => demoActive.value ? '' : realError.value)
const frameworkOk = computed(() => game.value?.bepinex.installed === true)

const { runtimeState } = useModRepositoryRefresh()
const { maps: realMaps, catalogCount, loadMaps: loadOnlineMaps, refreshingHint } = useMapSources()
const maps = computed(() => withDemoMaps(realMaps.value))
const { scanning: levelSetScanning, levelSets: realLevelSets, hasResult: hasLocalMaps } = useLevelSetScan()
const levelSets = computed(() => withDemoLevelSets(realLevelSets.value))

function localInstalledStats(items: readonly { enabled: boolean }[] | null, reading: boolean) {
  if (items === null) {
    return { value: '—', sub: t(reading ? 'levelSet.scanning' : 'overview.localCountUnavailable') }
  }
  return {
    value: String(items.filter((item) => item.enabled).length),
    sub: t('overview.installedCount', { count: items.length }),
  }
}

/** Counts publish after scanning; each version conclusion follows its own online entry. */
const modStatuses = computed(() => mods.value.map((mod) =>
  runtimeModStatus(mod, mods.value, config.value?.repositories ?? [], runtimeState.value)))
const localReposChecking = computed(() => modsLoading.value || runtimeState.value.entryStateUnavailable
  || modStatuses.value.some((entry) => entry.refreshing))

/**
 * 本地 MOD 卡的按钮文案与状态。
 *
 * 检测中显示「检测中」+ Naive 原生 loading（转圈且不可点）—— 这是
 * 用户明确要的交互：正在查的时候不让他点，避免跳到一半数据又变。
 *
 * 检测范围只算「与本地 MOD 绑定的仓库」，不等整批刷新结束。
 */
const buttonLabel = computed(() => {
  if (localReposChecking.value) return t('tools.checking')
  return updatable.value.length ? t('overview.review') : t('overview.manage')
})

onMounted(() => {
  if (demoActive.value) return
  void ensureLoaded()
  void loadOnlineMaps()
})

/* ===== BepInEx 框架 ===== */

const frameworkEnabled = computed(() => game.value?.winhttp_state === 'modenabled')
/** 框架的完整状态文案：未装 / 已装未启用 / 正常。 */
const frameworkValue = computed(() => {
  if (!frameworkOk.value) return t('overview.notInstalled')
  return frameworkEnabled.value ? t('modsLocal.enabled2') : t('modsLocal.disabled')
})
const frameworkSub = computed(() => {
  if (!frameworkOk.value) {
    return game.value?.bepinex.has_residue
      ? t('overlays.existingModFolderFoundKeptAfterInstall')
      : t('overview.installItToLoadMods')
  }
  return game.value?.bepinex.version || t('tools.versionUnknown')
})

const working = ref(false)

/** 切换框架启用状态（只改 winhttp.dll 的名字，不碰 MOD 文件）。 */
async function toggleFramework(enabled: boolean) {
  if (working.value) return
  working.value = true
  try {
    const result = await api.switchWinhttp(enabled)
    if (result.ok) {
      message.success(result.message ? translateBackendMessage(result.message) : (enabled ? t('overlays.frameworkEnabled') : t('overlays.frameworkDisabled')))
      await load()
    } else {
      message.error(translateBackendMessage(result.message))
    }
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    working.value = false
  }
}

/** 未安装时跳转到设置页的框架管理抽屉。 */
function openFrameworkSettings() {
  void router.push({ name: 'settings', query: { [FOCUS_QUERY]: 'bepinex' } })
}

/* ===== DIYLevel 框架 ===== */

/** 与后端后续地图操作共用同一份最高版本选择，不在页面重判版本。 */
const diyLevelMod = computed(() =>
  mods.value.find((mod) => mod.dll_file === diyLevelTarget.value))

const diyLevelOk = computed(() => !!diyLevelMod.value)
const diyLevelEnabled = computed(() => diyLevelMod.value?.enabled === true)
const diyLevelValue = computed(() => {
  if (!diyLevelOk.value) return t('overview.notInstalled')
  return diyLevelEnabled.value ? t('modsLocal.enabled2') : t('modsLocal.disabled')
})

const diyLevelSub = computed(() => diyLevelMod.value
  ? diyLevelMod.value.version || t('tools.versionUnknown')
  : t('framework.customMapsLoadOnlyAfterInstallingThis'))

/** 切换 DIYLevel 启用状态：走后端与 MOD 列表相同的 .dll 改名逻辑。 */
async function toggleDiyLevel(enabled: boolean) {
  if (working.value) return
  working.value = true
  try {
    const target = realDiyLevelTarget.value
    if (!target) throw new Error('modChanges.targetChanged')
    const result = await requestToggleChanges({ dllFiles: [target], enabled })
    if (!result) return
    if (result.ok) {
      message.success(translateBackendMessage(result.message))
    } else {
      message.error(translateBackendMessage(result.message))
    }
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    working.value = false
  }
}

/** 未安装/管理时跳设置页的 DIYLevel 抽屉。 */
function openDiyLevelSettings() {
  void router.push({ name: 'settings', query: { [FOCUS_QUERY]: 'diylevel' } })
}

/* ===== 已安装 MOD（含可更新数量） ===== */

const localModsStats = computed(() => localInstalledStats(
  demoActive.value || modsGeneration.value > 0 ? mods.value : null,
  modsLoading.value,
))

/**
 * 可更新的 MOD。
 *
 * 与管理页和卡片共用 runtimeModStatus：源文档准备后，按各仓库的排队/
 * 在飞状态逐个给出结论，不等待其它仓库，也不依赖扫描时的旧更新标志。
 */
const updatable = computed(() => modStatuses.value
  .filter((entry) => !entry.mod.is_dependency && entry.mod.update_available).map((entry) => entry.mod))

/**
 * 跳到 MOD 管理页。
 *
 *   · 有更新 → 定位到第一条可更新的 MOD（复用包外壳的 `data-focus-id`）
 *   · 无更新 → 直接进管理页，不带定位
 */
function openLocalMods() {
  const first = updatable.value[0]
  if (!first) {
    void router.push({ name: 'mods-local' })
    return
  }
  const anchor = first.package_id || first.repository_id || first.dll_file
  void router.push({ name: 'mods-local', query: { [FOCUS_QUERY]: anchor } })
}

/* ===== 在线 MOD（含刷新失败提示） ===== */

/**
 * 当前地区可见的仓库 —— 口径与 MOD 安装页的「共 N 个仓库」一致
 * （都用 `modRepositoryIsAvailable`），只是这里不带搜索过滤。
 */
const onlineRepositories = computed(() =>
  (config.value?.repositories ?? []).filter(
    (repo) => modRepositoryIsAvailable(repo, config.value?.repository_sources, config.value?.region, config.value?.platform),
  ))

/** 其中刷新报错的那些。隐藏的仓库不算，它们本来就不参与刷新。 */
const failedRepositories = computed(() =>
  onlineRepositories.value.filter((repo) => repo.last_error))

/**
 * 卡片副标题：主数值是在线仓库总数；有失败时只追加失败数量。
 *
 * 不列仓库名：卡片副标题是单行、宽度有限，两个名字就能把它撑成两行
 * 并顶掉「全部正常」那种清爽观感；而具体是哪几个仓库、失败原因是什么，
 * 点「去刷新」跳到安装页后每个仓库卡片上都写着 —— 总览只需要告诉用户
 * 「有没有失败、有几个」。
 *
 * 把失败信息塞进这张卡而不是单开一张：失败仓库本身就是在线仓库的一部分，
 * 拆成两张会让「26 个仓库」和「1 个失败」看起来是两件事。
 */
const onlineSub = computed(() => {
  const total = onlineRepositories.value.length
  if (!total) return t('overview.noModsLoadedYet')
  if (!failedRepositories.value.length) {
    return t('overview.modsAllHealthy', { count: total })
  }
  return t('overview.modsFailedToRefresh', {
    count: total,
    failed: failedRepositories.value.length,
  })
})

/**
 * 卡片按钮随状态分支：
 *   · 有仓库刷新失败 → 「去刷新」，跳到安装页并高亮第一个失败仓库
 *   · 全部正常 → 「去安装」，只跳转到安装页
 */
function openOnlineRepositories() {
  const first = failedRepositories.value[0]
  if (!first) {
    void router.push({ name: 'mods-online' })
    return
  }
  void router.push({ name: 'mods-online', query: { [FOCUS_QUERY]: first.id } })
}

/* ===== 在线地图 / 本地地图 ===== */

/**
 * 「本地地图 → 在线版本」的对照表。
 *
 * 只有刷新过 Release 的地图才有 tag —— 没刷新的地图不该让本地关卡集显示
 * 「最新版」，那是无根据的结论（见 `levelSetStatus.ts` 的 `mapVersionRefs`）。
 * 这套判定与地图管理页的卡片状态同源，总览不另写一份比较逻辑。
 */
const mapVersionsOf = (set: LevelSetInfo) => aggregateLevelSetVersion(set, maps.value, refreshingHint.value)

/** 单张本地地图的状态：口径与地图管理页的 `stateOf` 完全一致。 */
function mapStateOf(set: LevelSetInfo) {
  return levelSetAggregateState(set, mapVersionsOf(set), refreshingHint.value.catalog)
}

/** 其中 Release 刷新失败的地图源条目（`last_error` 非空）。 */
const failedMaps = computed(() => maps.value.filter((map) => !!map.last_error))

/** 可更新的本地地图：只有绑定了地图源、且源版本更高时才算。 */
const updatableMaps = computed(() =>
  levelSets.value.filter((set) => mapStateOf(set) === 'update_available'))

const onlineMapsCount = computed(() => demoActive.value
  ? { count: maps.value.length, failed: failedMaps.value.length, complete: true }
  : catalogCount.value)
const onlineMapsSub = computed(() => {
  const { count, failed, complete } = onlineMapsCount.value
  if (count === null) return t('overview.noMapsLoadedYet')
  if (!complete) return t('overview.mapsCatalogUnconfirmed', { count })
  if (!failed) {
    return t('overview.mapsAllHealthy', { count })
  }
  return t('overview.mapsFailedToRefresh', { count, failed })
})

const localMapsChecking = computed(() => levelSetScanning.value
  || !!refreshingHint.value.entryStateUnavailable
  || levelSets.value.some((set) => levelSetIsRefreshing(set, maps.value, refreshingHint.value)))
const localMapsStats = computed(() => localInstalledStats(
  demoActive.value || hasLocalMaps.value ? levelSets.value : null,
  levelSetScanning.value,
))

/**
 * 本地地图卡的按钮文案：读取中 → 「读取中」+ 转圈；读完 → 「去更新」/「去管理」。
 *
 * 与本地 MOD 卡（`buttonLabel`）同一套交互：正在读的时候不让用户跳走，避免
 * 跳到一半列表又变。扫描状态来自 `useLevelSetScan` 的模块级单例，所以任何
 * 页面发起读取时这里都会转。
 */
const localMapsButtonLabel = computed(() => {
  if (levelSetScanning.value) return t('levelSet.scanning')
  if (!updatableMaps.value.length && localMapsChecking.value) return t('tools.checking')
  return updatableMaps.value.length ? t('overview.review') : t('overview.manage')
})

/**
 * 在线地图卡：有刷新失败 → 跳到地图下载页并高亮第一张失败地图；
 * 全部正常 → 只跳转到下载页。
 */
function openOnlineMaps() {
  const first = failedMaps.value[0]
  if (!first) {
    void router.push({ name: 'maps-online' })
    return
  }
  void router.push({ name: 'maps-online', query: { [FOCUS_QUERY]: first.id } })
}

/**
 * 本地地图卡：有更新 → 定位第一张有更新的地图。
 *
 * 管理页卡片的 `data-focus-id` 是它**绑定的地图 id**（见 `LevelsView`
 * 的 `focusIdOf`，身份判定同为 `mapForLevelSet`），不是目录名，
 * 所以这里也传地图 id，跳过去才能命中高亮。
 */
function openLocalMaps() {
  const first = updatableMaps.value[0]
  const mapId = first ? mapVersionsOf(first).primaryTarget?.mapId : undefined
  if (!mapId) {
    void router.push({ name: 'levels' })
    return
  }
  void router.push({ name: 'levels', query: { [FOCUS_QUERY]: mapId } })
}

watch(demoActive, active => {
  if (active) return
  void ensureLoaded()
  void loadOnlineMaps()
})
</script>

<template>
  <div class="overview">
    <div v-if="error" class="muted">{{ error }}</div>
    <div class="overview-columns">
      <!--
        左列 MOD 链路：BepInEx → 在线 MOD → 本地 MOD。
        顺序即操作顺序 —— 框架没装，下面两张卡的按钮都不可用。
      -->
      <section class="overview-column" data-tutorial="overview-column-mods">
        <h2 class="column-title">{{ t('overview.modsChain') }}</h2>
        <div class="column-cards">
          <!--
            BepInEx：整张卡可点，跳设置页并自动展开框架抽屉。
            开关单独放在右侧，点开关不会触发跳转（@click.stop）。
          -->
          <StatCard data-tutorial="overview-card-bepinex"
            :label="t('framework.bepinexFramework')" :value="frameworkValue"
            :sub="frameworkSub" :tone="frameworkOk ? 'success' : 'warning'"
            clickable @click="openFrameworkSettings">
            <template #action>
              <n-switch v-if="frameworkOk" size="small" :value="frameworkEnabled"
                :disabled="working" :title="t('overlays.frameworkEnabledGameLoadsMods')"
                @click.stop @update:value="toggleFramework" />
              <n-button v-else size="small" type="primary" @click.stop="openFrameworkSettings">
                <template #icon><AddCircleOutline width="14" height="14" /></template>
                {{ t('overlays.installFramework') }}
              </n-button>
            </template>
          </StatCard>

          <!--
            在线 MOD：整张卡可点，跳 MOD 安装页。
            主数值是可见仓库总数，旁边并列「N 刷新失败」。
            有失败时按钮「去刷新」并高亮第一个失败仓库；全部正常时「去安装」。
          -->
          <StatCard data-tutorial="overview-card-online-mods"
            :label="t('overview.onlineMods')" :value="String(onlineRepositories.length)"
            :extra="{ count: failedRepositories.length, label: t('tools.refreshFailed') }"
            :sub="onlineSub"
            :tone="failedRepositories.length ? 'warning' : 'success'"
            clickable @click="openOnlineRepositories">
            <template #action>
              <n-button size="small" :secondary="!failedRepositories.length"
                :type="failedRepositories.length ? 'warning' : 'default'"
                :disabled="!onlineRepositories.length" @click.stop="openOnlineRepositories">
                <template #icon>
                  <RefreshOutline v-if="failedRepositories.length" width="14" height="14" />
                  <CloudDownloadOutline v-else width="14" height="14" />
                </template>
                {{ failedRepositories.length ? t('overview.refresh') : t('overview.browse') }}
              </n-button>
            </template>
          </StatCard>

          <!--
            本地 MOD：整张卡可点，跳 MOD 管理页。
            主数值是启用数，旁边并列「N 可更新」；副标题保留安装总数。
            按钮随状态分支：有更新 →「去更新」并定位第一条；无更新 →「去管理」。
          -->
          <StatCard data-tutorial="overview-card-local-mods"
            :label="t('overview.localMods')" :value="localModsStats.value"
            :extra="{ count: updatable.length, label: t('tools.update') }"
            :sub="localModsStats.sub"
            :tone="updatable.length ? 'warning' : 'success'"
            clickable @click="openLocalMods">
            <template #action>
              <!--
                用 Naive 原生 loading：转圈 + 不可点。
                检测期间按钮停用是刻意的 —— 数据还在变，跳过去定位到的可能是
                即将被覆盖的旧结论。loading 只看「本地 MOD 相关仓库」，
                不等整批刷新结束（见 localReposChecking）。
              -->
              <n-button size="small" :secondary="!updatable.length"
                :type="updatable.length ? 'warning' : 'default'"
                :loading="localReposChecking"
                :disabled="!mods.length" @click.stop="openLocalMods">
                <template #icon>
                  <CloudDownloadOutline v-if="updatable.length" width="14" height="14" />
                  <SettingsOutline v-else width="14" height="14" />
                </template>
                {{ buttonLabel }}
              </n-button>
            </template>
          </StatCard>
        </div>
      </section>

      <!--
        两列之间的竖向分割线，把两条链路的边界画清楚；
        纯装饰、无文字，读屏跳过。
      -->
      <div class="column-divider" aria-hidden="true"></div>

      <!--
        右列 地图链路：DIYLevel → 在线地图 → 本地地图。
        框架卡同样放在列首 —— 没有 DIYLevel，自定义地图进游戏也不生效。
      -->
      <section class="overview-column" data-tutorial="overview-column-maps">
        <h2 class="column-title">{{ t('overview.mapsChain') }}</h2>
        <div class="column-cards">
          <!--
            DIYLevel：与 BepInEx 同形态（摘要 + 开关 + 跳转）。
            数值是启用状态，安装后副标题只显示版本号。
          -->
          <StatCard data-tutorial="overview-card-diy-level"
            :label="t('framework.diylevelFramework')" :value="diyLevelValue"
            :sub="diyLevelSub" :tone="diyLevelOk ? 'success' : 'warning'"
            clickable @click="openDiyLevelSettings">
            <template #action>
              <n-switch v-if="diyLevelOk" size="small" :value="diyLevelEnabled"
                :disabled="working" :title="t('recycle.frameworkEnabledTheGameLoadsThisFramework')"
                @click.stop @update:value="toggleDiyLevel" />
              <n-button v-else size="small" type="primary" @click.stop="openDiyLevelSettings">
                <template #icon><AddCircleOutline width="14" height="14" /></template>
                {{ t('overlays.installFramework') }}
              </n-button>
            </template>
          </StatCard>

          <!--
            在线地图：主数值按地图身份去重，并列「N 刷新失败」。
            有失败时「去刷新」并高亮第一张失败地图；全部正常时「去安装」。
          -->
          <StatCard data-tutorial="overview-card-online-maps"
            :label="t('overview.onlineMaps')" :value="onlineMapsCount.count === null ? '—' : String(onlineMapsCount.count)"
            :extra="{ count: onlineMapsCount.failed, label: t('tools.refreshFailed') }"
            :sub="onlineMapsSub"
            :tone="onlineMapsCount.failed ? 'warning' : onlineMapsCount.complete ? 'success' : 'default'"
            clickable @click="openOnlineMaps">
            <template #action>
              <n-button size="small" :secondary="!failedMaps.length"
                :type="failedMaps.length ? 'warning' : 'default'"
                :disabled="!maps.length" @click.stop="openOnlineMaps">
                <template #icon>
                  <RefreshOutline v-if="failedMaps.length" width="14" height="14" />
                  <CloudDownloadOutline v-else width="14" height="14" />
                </template>
                {{ failedMaps.length ? t('overview.refresh') : t('overview.browse') }}
              </n-button>
            </template>
          </StatCard>

          <!--
            本地地图：主数值是启用数，并列「N 可更新」；副标题保留安装总数。
            有更新 →「去更新」并定位第一张；无更新 →「去管理」。
          -->
          <StatCard data-tutorial="overview-card-local-maps"
            :label="t('overview.localMaps')" :value="localMapsStats.value"
            :extra="{ count: updatableMaps.length, label: t('tools.update') }"
            :sub="localMapsStats.sub"
            :tone="updatableMaps.length ? 'warning' : 'success'"
            clickable @click="openLocalMaps">
            <template #action>
              <n-button size="small" :secondary="!updatableMaps.length"
                :type="updatableMaps.length ? 'warning' : 'default'"
                :loading="localMapsChecking"
                :disabled="!levelSets.length" @click.stop="openLocalMaps">
                <template v-if="!levelSetScanning" #icon>
                  <CloudDownloadOutline v-if="updatableMaps.length" width="14" height="14" />
                  <SettingsOutline v-else width="14" height="14" />
                </template>
                {{ localMapsButtonLabel }}
              </n-button>
            </template>
          </StatCard>
        </div>
      </section>
    </div>
  </div>
</template>

<style scoped>
.overview {
  display: flex;
  flex-direction: column;
  /* 数据未到时不清空布局，避免卡片塌缩后再撑开 */
  min-height: 0;
}

/*
 * 两条纵向链路：等分两列、顶对齐，中间一条竖线。
 *
 * 用 `minmax(0, 1fr)` 而不是写死宽度，卡片跟着内容区收放：1280×720 窗口
 * 在默认 150% 内容缩放下等效 CSS 宽度约 853px，列宽仍有 330px 左右，
 * 卡片不会把文字挤成多行或撑出边框。两列也不设「窄了就叠成一列」的断点 ——
 * 叠起来就变成「左边链路装完才发现右边」，并排对比的意义随之消失。
 *
 * 分割线单独占一个 1px 轨道，而不是塞进 gap 里的伪元素：两列宽都是 1fr，
 * 加不加线都等宽；轨道两侧各留一个 16px 列间距，线不会贴到卡片边上，
 * 也不用为了画线去压缩任何一列。
 *
 * `align-items: start`：两列各自只占内容高度，列底边不会被另一列拉长，
 * 但首卡仍在同一条基线上（网格行起点）。
 */
.overview-columns {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 1px minmax(0, 1fr);
  gap: 16px;
  align-items: start;
}
/*
 * `align-self: stretch` 让线跟随网格行高 —— 行高就是较高那一列的完整高度
 * （列小标题 + 三张卡），所以不用写死高度，卡片变高变矮都自然贴合。
 */
.column-divider {
  align-self: stretch;
  width: 1px;
  background: var(--oc2-row-line);
}
.overview-column {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
/*
 * 列小标题：标明这一列是哪条链路。
 *
 * 层级在页头 h1（23px / 650）之下、卡片之上：只有 12px、弱化文字色与字距，
 * 扫视时先认出页面标题，再顺着小标题找到一列，不会被误读成第二个标题。
 */
.column-title {
  margin: 0 0 10px;
  color: var(--oc2-text-3);
  font-size: 12px;
  font-weight: 600;
  letter-spacing: .08em;
  text-transform: uppercase;
}
.column-cards {
  display: flex;
  flex-direction: column;
  gap: 16px;
  min-width: 0;
}
</style>
