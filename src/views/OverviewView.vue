<script setup lang="ts">
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
  levelSetState, levelSetIsRefreshing, mapForLevelSet, mapVersionForLevelSet, mapVersionRefs,
} from '../levelSetStatus.ts'
import { useNotify } from '../useNotify'
import { FOCUS_QUERY } from '../crossLink'
import type { LevelSetInfo } from '../types'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { demoActive, withDemoConfig, withDemoDiyLevelTarget, withDemoGame, withDemoLevelSets, withDemoMaps, withDemoMods } from '../tutorial/demo'

const router = useRouter()
const message = useNotify()
const { ensureLoaded, ensureLocalModsLoaded, load, modsLoading: realModsLoading, error: realError, mods: realMods, diyLevelTarget: realDiyLevelTarget, game: realGame, config: realConfig } = useDashboard()
const mods = computed(() => withDemoMods(realMods.value))
const config = computed(() => withDemoConfig(realConfig.value))
const game = computed(() => withDemoGame(realGame.value))
const diyLevelTarget = computed(() => withDemoDiyLevelTarget(realDiyLevelTarget.value))
const modsLoading = computed(() => !demoActive.value && realModsLoading.value)
const error = computed(() => demoActive.value ? '' : realError.value)
const frameworkOk = computed(() => game.value?.bepinex.installed === true)

const { runtimeState } = useModRepositoryRefresh()
const { maps: realMaps, loadMaps: loadOnlineMaps, refreshingHint } = useMapSources()
const maps = computed(() => withDemoMaps(realMaps.value))
/** 本地地图扫描状态（`useLevelSetScan` 的模块级单例，切页不丢）。 */
const { scanning: levelSetScanning, scanLevelSets } = useLevelSetScan()

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
  void loadDiyLevel()
  void loadOnlineMaps()
  void loadLocalMaps()
})

/* ===== BepInEx 框架 ===== */

const frameworkVersion = computed(() => game.value?.bepinex.version ?? '')
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
  return frameworkVersion.value ? `BepInEx ${frameworkVersion.value}` : t('tools.versionUnknown')
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

/**
 * 地图集数量。
 *
 * 只数 `levels/` 下的**文件夹**（一个文件夹 = 一个地图集），散文件不算。
 * 这个数据只有专用的 `diy_level_info` 命令才有（MOD 扫描不带它），
 * 所以要单独读一次；失败按 0 处理，不能让整张卡片报错。
 */
const diyLevelPacks = ref(0)
const diyLevelLoading = ref(false)

async function loadDiyLevel() {
  if (demoActive.value) return
  diyLevelLoading.value = true
  try {
    await ensureLocalModsLoaded()
    if (demoActive.value) return
    const info = await api.diyLevelInfo()
    diyLevelPacks.value = info.level_packs ?? 0
  } catch {
    diyLevelPacks.value = 0
  } finally {
    diyLevelLoading.value = false
  }
}

const diyLevelSub = computed(() => {
  if (!diyLevelOk.value) return t('framework.customMapsLoadOnlyAfterInstallingThis')
  const version = diyLevelMod.value?.version ?? t('tools.versionUnknown')
  const packs = demoActive.value ? withDemoLevelSets([]).length : diyLevelPacks.value
  return `${t('modsOnline.versions')} ${version} · ${t('framework.mapPacks')} ${packs}`
})

/** 切换 DIYLevel 启用状态：走后端与 MOD 列表相同的 .dll 改名逻辑。 */
async function toggleDiyLevel(enabled: boolean) {
  if (working.value) return
  working.value = true
  try {
    const result = await api.setDiyLevelEnabled(enabled)
    if (result.ok) {
      message.success(translateBackendMessage(result.message))
    } else {
      message.error(translateBackendMessage(result.message))
    }
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    await load().catch(() => undefined)
    await loadDiyLevel()
    working.value = false
  }
}

/** 未安装/管理时跳设置页的 DIYLevel 抽屉。 */
function openDiyLevelSettings() {
  void router.push({ name: 'settings', query: { [FOCUS_QUERY]: 'diylevel' } })
}

/* ===== 已安装 MOD（含可更新数量） ===== */

const installedMods = computed(() => mods.value)

/**
 * 可更新的 MOD。
 *
 * 与管理页和卡片共用 runtimeModStatus：源文档准备后，按各仓库的排队/
 * 在飞状态逐个给出结论，不等待其它仓库，也不依赖扫描时的旧更新标志。
 */
const updatable = computed(() => modStatuses.value
  .filter((entry) => !entry.mod.is_dependency && entry.mod.update_available).map((entry) => entry.mod))

/**
 * 卡片副标题：有更新时报「N 个可更新」，没有时只报总数。
 * 主数值永远是已安装总数 —— 这张卡回答的是「我装了多少」，
 * 可更新只是附加信息，不该反过来占据主位（否则没更新时显示 0 很空洞）。
 */
const installedSub = computed(() => {
  const total = installedMods.value.length
  if (modsLoading.value) return t('tools.checking')
  if (!total) return t('modsLocal.noModsInstalled')
  if (localReposChecking.value) return t('tools.checking')
  const updates = updatable.value.length
  return updates
    ? t('overview.installedCanUpdate', { count: total, updates })
    : t('overview.installedAllUpToDate', { count: total })
})

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
  if (!total) return t('overview.noRepositoriesLoadedYet')
  if (!failedRepositories.value.length) {
    return t('overview.repositoriesAllHealthy', { count: total })
  }
  return t('overview.repositoriesFailedToRefresh', {
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

/** 本地关卡集（`scanLevelSets()`），与地图管理页读同一份。 */
const realLevelSets = ref<LevelSetInfo[]>([])
const levelSets = computed(() => withDemoLevelSets(realLevelSets.value))

/**
 * 地图区两份数据：地图源的条目列表（在线）与本地扫描结果（本地）。
 *
 * 两者互不依赖，所以并行读；**各自失败只影响自己的卡片**，不互相清空
 * （`api.maps()` 失败时地图下载页也会退化成空列表，口径一致）。
 * 读取失败按 0 处理，不能让总览整页报错 —— 与本地 MOD / 框架卡片的做法
 * 相同，数据没到就用空状态文案，布局不塌。
 */
async function loadLocalMaps() {
  if (demoActive.value) return
  try {
    realLevelSets.value = await scanLevelSets()
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

/**
 * 「本地地图 → 在线版本」的对照表。
 *
 * 只有刷新过 Release 的地图才有 tag —— 没刷新的地图不该让本地关卡集显示
 * 「最新版」，那是无根据的结论（见 `levelSetStatus.ts` 的 `mapVersionRefs`）。
 * 这套判定与地图管理页的卡片状态同源，总览不另写一份比较逻辑。
 */
const mapVersionList = computed(() => mapVersionRefs(maps.value))

/** 单张本地地图的状态：口径与地图管理页的 `stateOf` 完全一致。 */
function mapStateOf(set: LevelSetInfo) {
  if (levelSetIsRefreshing(set, maps.value, refreshingHint.value)) return 'unknown'
  return levelSetState(set, mapVersionForLevelSet(set, mapVersionList.value))
}

/** 其中 Release 刷新失败的地图源条目（`last_error` 非空）。 */
const failedMaps = computed(() => maps.value.filter((map) => !!map.last_error))

/** 可更新的本地地图：只有绑定了地图源、且源版本更高时才算。 */
const updatableMaps = computed(() =>
  levelSets.value.filter((set) => mapStateOf(set) === 'update_available'))

const onlineMapsSub = computed(() => {
  const total = maps.value.length
  if (!total) return t('overview.noMapsLoadedYet')
  if (!failedMaps.value.length) {
    return t('overview.mapsAllHealthy', { count: total })
  }
  return t('overview.mapsFailedToRefresh', {
    count: total,
    failed: failedMaps.value.length,
  })
})

/**
 * 主数值：读本地地图期间不显示 0。
 *
 * 列表还没回来时 0 不是结论，只是「还不知道」—— 与「读取中」的按钮并排放着
 * 会让人以为一张地图都没装。未知一律显示破折号。
 */
const localMapsChecking = computed(() => levelSetScanning.value
  || !!refreshingHint.value.entryStateUnavailable
  || levelSets.value.some((set) => levelSetIsRefreshing(set, maps.value, refreshingHint.value)))
const localMapsValue = computed(() =>
  levelSetScanning.value && !levelSets.value.length ? '—' : String(levelSets.value.length))

const localMapsSub = computed(() => {
  const total = levelSets.value.length
  // 读取期间「还没有安装任何地图」同样是没有根据的结论。
  if (!total) {
    return levelSetScanning.value ? t('levelSet.scanningPreparing') : t('overview.noMapsInstalled')
  }
  if (localMapsChecking.value) return t('tools.checking')
  const updates = updatableMaps.value.length
  return updates
    ? t('overview.installedMapsCanUpdate', { count: total, updates })
    : t('overview.installedMaps', { count: total })
})

/**
 * 本地地图卡的按钮文案：读取中 → 「读取中」+ 转圈；读完 → 「去更新」/「去管理」。
 *
 * 与本地 MOD 卡（`buttonLabel`）同一套交互：正在读的时候不让用户跳走，避免
 * 跳到一半列表又变。扫描状态来自 `useLevelSetScan` 的模块级单例，所以任何
 * 页面发起读取时这里都会转。
 */
const localMapsButtonLabel = computed(() => {
  if (levelSetScanning.value) return t('levelSet.scanning')
  if (localMapsChecking.value) return t('tools.checking')
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
  const mapId = first ? mapForLevelSet(first, maps.value)?.id : undefined
  if (!mapId) {
    void router.push({ name: 'levels' })
    return
  }
  void router.push({ name: 'levels', query: { [FOCUS_QUERY]: mapId } })
}

watch(demoActive, active => {
  if (active) return
  void ensureLoaded()
  void loadDiyLevel()
  void loadOnlineMaps()
  void loadLocalMaps()
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
            主数值是安装总数，旁边并列「N 可更新」。
            按钮随状态分支：有更新 →「去更新」并定位第一条；无更新 →「去管理」。
          -->
          <StatCard data-tutorial="overview-card-local-mods"
            :label="t('overview.localMods')" :value="modsLoading && !installedMods.length ? '—' : String(installedMods.length)"
            :extra="{ count: updatable.length, label: t('tools.update') }"
            :sub="installedSub"
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
                :disabled="!installedMods.length" @click.stop="openLocalMods">
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
            数值是启用状态，副标题给出版本与地图集数量。
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
            在线地图：主数值是地图源声明的条目数，并列「N 刷新失败」。
            有失败时「去刷新」并高亮第一张失败地图；全部正常时「去安装」。
          -->
          <StatCard data-tutorial="overview-card-online-maps"
            :label="t('overview.onlineMaps')" :value="String(maps.length)"
            :extra="{ count: failedMaps.length, label: t('tools.refreshFailed') }"
            :sub="onlineMapsSub"
            :tone="failedMaps.length ? 'warning' : 'success'"
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
            本地地图：主数值是本地关卡集数量，并列「N 可更新」。
            有更新 →「去更新」并定位第一张；无更新 →「去管理」。
          -->
          <StatCard data-tutorial="overview-card-local-maps"
            :label="t('overview.localMaps')" :value="localMapsValue"
            :extra="{ count: updatableMaps.length, label: t('tools.update') }"
            :sub="localMapsSub"
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
