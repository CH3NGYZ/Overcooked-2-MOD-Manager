/**
 * 本地关卡集扫描（`get_level_sets`）的共享状态（**模块级单例**）。
 *
 * ## 为什么状态不能挂在页面上
 *
 * 想显示「正在读取本地地图」的不止地图管理页：总览页的「本地地图」卡也要读同一份
 * 列表，而它读的时候用户可能已经切到别的页面。状态跟着页面实例走，切走就丢 ——
 * 回来只剩一个上次的 0，或者干脆重起一次扫描。与 `useMapSources` /
 * `useModRepositoryRefresh` 一致：状态放模块作用域，页面只负责渲染。
 *
 * 最近一次本地结果也由此保存，供两页展示与已安装优先排序复用；在飞扫描
 * 合并为一次，但不等待任何在线地图请求。在线与本地分别发布、分别处理失败。
 */
import { computed, readonly, ref, shallowRef, watch } from 'vue'
import { api, watchLevelSetScan } from '../services/tauri'
import { useDashboard } from './useDashboardStore'
import { demoActive } from '../tutorial/demo'
import type { LevelSetInfo, LevelSetScanProgress } from '../types'
import type { UnlistenFn } from '@tauri-apps/api/event'

/**
 * 在飞扫描数；并发调用复用 pendingScan，同一轮只扫描一次。
 */
const inFlight = ref(0)
const dashboard = useDashboard()
const result = shallowRef<{ sets: LevelSetInfo[], generation: number, revision: number } | null>(null)
const error = shallowRef<unknown>(null)
const empty: LevelSetInfo[] = []
const levelSets = computed(() => result.value?.sets ?? empty)
const hasResult = computed(() => result.value !== null)
const scanRevision = shallowRef(0)
const hasCurrentResult = computed(() => dashboard.modsStatus.value === 'ready'
  && result.value?.generation === dashboard.modsGeneration.value && result.value.revision === scanRevision.value)
const contextRevision = shallowRef(0)
// A DLL re-scan on the same game/target may keep displaying the last result.
// A different root or target must clear it even if the identity later reverts.
// A shallow ref passed directly to this multi-source watcher would also fire
// on identical game fields whenever a new online config snapshot arrives.
watch([() => dashboard.game.value?.path, () => dashboard.config.value?.game?.path,
  () => dashboard.config.value?.game?.runtime, () => dashboard.diyLevelTarget.value], () => {
  result.value = null
  error.value = null
  contextRevision.value += 1
}, { flush: 'sync' })
let pendingScan: Promise<LevelSetInfo[]> | undefined
let waitingForLocalMods = false

/** A map edit can overtake a read without changing the DLL generation. */
function invalidate() { scanRevision.value += 1 }

/** A DLL notice revokes old replies before the replacement DLL scan can start. */
function setWaitingForLocalMods(waiting: boolean) {
  if (waiting && !waitingForLocalMods) invalidate()
  waitingForLocalMods = waiting
}

/** 是否有扫描在飞（总览卡片的按钮转圈读它）。 */
const scanning = computed(() => !demoActive.value && inFlight.value > 0)

/** 最近一次扫描进度；扫描结束即清空，避免下次开始前挂着上一轮的 x/y。 */
const progress = ref<LevelSetScanProgress | null>(null)

let listening: Promise<UnlistenFn | null> | null = null

/**
 * 进度事件监听常驻一份。
 *
 * 事件只在 `get_level_sets` 期间发，而这份数据谁都可能读，所以订阅不能跟着某个
 * 页面挂载/卸载 —— 第一次扫描时订阅，此后一直有效，页面不关心退订。
 */
function ensureProgressListener(): void {
  if (listening) return
  listening = watchLevelSetScan((next) => {
    // 扫描结束才迟到的进度不要写回来：下一次读开始前界面会显示上一轮的「x/y」。
    if (inFlight.value > 0) progress.value = next
  }).catch(() => null)
}

/**
 * 读一次全部关卡集，并在此期间维护「正在读取」与进度。
 *
 * **所有入口都必须走这里**，不要直接调 `api.levelSets()`：绕过它就等于让总览
 * 卡片看不到那次读取，这个模块存在的意义也就没了。
 */
export function scanLevelSets({ reuseCurrentGeneration = false } = {}): Promise<LevelSetInfo[]> {
  // Keep this cache real. Pages project the tutorial list without publishing it here.
  if (demoActive.value) return Promise.resolve(levelSets.value)
  if (pendingScan) return pendingScan
  if (waitingForLocalMods || (reuseCurrentGeneration && hasCurrentResult.value)) return Promise.resolve(levelSets.value)
  ensureProgressListener()
  error.value = null
  progress.value = null
  inFlight.value += 1
  pendingScan = (async () => {
    try {
      await listening
      for (;;) {
        // Yield to the lifecycle so it can rescan DLLs before retrying maps.
        if (demoActive.value || waitingForLocalMods) return levelSets.value
        // DIYLevel's root comes from the completed DLL scan; online map reads
        // remain independent of this local dependency.
        const generation = await dashboard.ensureLocalModsLoaded()
        if (demoActive.value || waitingForLocalMods) return levelSets.value
        const context = contextRevision.value
        const revision = scanRevision.value
        const isCurrent = () => revision === scanRevision.value && context === contextRevision.value && dashboard.modsStatus.value === 'ready'
          && generation === dashboard.modsGeneration.value
        if (!isCurrent()) continue
        try {
          const sets = await api.levelSets()
          if (!isCurrent()) continue
          if (demoActive.value) return levelSets.value
          result.value = { sets, generation, revision }
          return sets
        } catch (reason) {
          if (!isCurrent()) continue
          throw reason
        }
      }
    } catch (reason) {
      if (waitingForLocalMods) return levelSets.value
      if (!demoActive.value) error.value = reason
      throw reason
    } finally {
      inFlight.value -= 1
      progress.value = null
      pendingScan = undefined
    }
  })()
  return pendingScan
}

export function useLevelSetScan() {
  return { scanning, progress, scanLevelSets, levelSets, hasResult, hasCurrentResult, error,
    invalidate, setWaitingForLocalMods, contextRevision: readonly(contextRevision) }
}
