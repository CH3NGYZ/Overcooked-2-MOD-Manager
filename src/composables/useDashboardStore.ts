import { computed, ref, shallowRef } from 'vue'
import { api, watchGameState } from '../services/tauri'
import type { Dashboard, GameInfo, PublicConfig } from '../types'

/**
 * 全局仪表盘状态：dashboard 快照 + 游戏状态事件合并。
 * MOD 扫描可能慢于游戏状态事件，事件到达时保留较新的 game 快照。
 */
const dashboard = shallowRef<Dashboard | null>(null)
/** 首次加载（还没有任何数据）：页面显示居中加载态。 */
const loading = ref(false)
/** 后台重载计数：已有数据时刷新，不再遮罩、不禁用交互。 */
const backgroundLoading = ref(0)
const error = ref('')
let gameStateRevision = 0
let latestGame: GameInfo | undefined

function applyGame(value: GameInfo) {
  latestGame = value
  gameStateRevision += 1
  if (dashboard.value) dashboard.value.game = value
}

let watching = false
export function startGameWatch() {
  if (watching) return
  watching = true
  void watchGameState(applyGame).catch(() => undefined)
}

async function readDashboard() {
  const revision = gameStateRevision
  const value = await api.dashboard()
  if (revision !== gameStateRevision && latestGame) value.game = latestGame
  return value
}

export function useDashboard() {
  /**
   * 读取 Dashboard。
   *
   * 只有「首次加载」（此时还没有任何数据）才进入 `loading` 状态显示整页遮罩；
   * 已有数据时的刷新（切页面、每仓刷新完成后的重载）走后台计数，
   * 既不遮罩也不禁用交互，避免刷新期间整页突然变灰、什么都点不了。
   */
  async function load() {
    const initial = dashboard.value === null
    if (initial) loading.value = true
    else backgroundLoading.value += 1
    error.value = ''
    try {
      dashboard.value = await readDashboard()
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : String(reason)
    } finally {
      if (initial) loading.value = false
      else backgroundLoading.value = Math.max(0, backgroundLoading.value - 1)
    }
  }

  async function refresh() {
    await load()
  }

  /**
   * 只重新读取配置（仓库 / 仓库源 / 设置），不重新探测游戏目录、不重新扫描 MOD。
   *
   * 刷新仓库源文档时，父源应用后需要立刻看到它声明的子源与仓库；
   * 但游戏状态和已安装 MOD 并没有变化，重新全量扫描纯属浪费
   * （一次刷新要经过父源 + 多个子源，全量扫描会被重复十几次）。
   */
  async function reloadConfig() {
    try {
      const config = await api.config()
      const current = dashboard.value
      if (!current) {
        // 还没有任何快照：走完整读取
        await load()
        return
      }
      dashboard.value = { ...current, config }
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : String(reason)
    }
  }

  /**
   * 页面进入时使用：已有数据就直接复用，避免每次切页面都重新扫描并重绘整页。
   * 需要最新数据时由刷新引擎或用户手动刷新触发。
   */
  async function ensureLoaded() {
    if (dashboard.value) return
    await load()
  }

  const config = computed<PublicConfig | null>(() => dashboard.value?.config ?? null)
  const game = computed<GameInfo | null>(() => dashboard.value?.game ?? null)
  const mods = computed(() => dashboard.value?.mods ?? [])
  const enabledModCount = computed(() => mods.value.filter((mod) => mod.enabled).length)
  const updateCount = computed(() => mods.value.filter((mod) => mod.update_available).length)
  const frameworkOk = computed(() => {
    const info = game.value?.bepinex
    return !!info && info.installed && info.complete
  })

  return {
    dashboard, loading, backgroundLoading, error,
    config, game, mods, enabledModCount, updateCount, frameworkOk,
    load, reloadConfig, ensureLoaded, refresh,
  }
}
