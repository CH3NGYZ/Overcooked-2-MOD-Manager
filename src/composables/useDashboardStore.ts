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

/**
 * 把后端推送的最新游戏状态合进快照。
 *
 * **必须整体替换 `dashboard.value`，不能就地改 `dashboard.value.game`。**
 * `dashboard` 是 `shallowRef`：它只追踪 `.value` 这个引用本身的替换，
 * 对内部属性的修改不产生依赖通知。就地赋值会让所有 computed
 * （`game`、`game.kitchen_running`、`frameworkOk` 等）永远拿到旧值 ——
 * 表现就是「游戏从 Steam 手动启动后，管理器界面一直显示未运行」，
 * 而后端日志里明明已经推送了 `游戏运行=true`。
 *
 * 同理不能用 `reactive` 包一层：`Dashboard` 里的 `mods` 是几万条的大数组，
 * 深响应会拖慢每次扫描结果的赋值。正确的做法就是浅引用 + 整对象替换。
 */
function applyGame(value: GameInfo) {
  latestGame = value
  gameStateRevision += 1
  const current = dashboard.value
  if (current) dashboard.value = { ...current, game: value }
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
   * 用一个已知的最新配置替换 Dashboard 里的 config，其余字段原样保留。
   *
   * 用途：命令本身会返回「操作后的完整配置」（例如清空源缓存），
   * 这时没必要再发一次 `get_config`，直接写回即可，列表当场更新。
   * 还不存在任何快照时退回完整读取。
   */
  function applyConfigSnapshot(config: PublicConfig) {
    const current = dashboard.value
    if (!current) return
    dashboard.value = { ...current, config }
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
    return !!info && info.installed
  })

  return {
    dashboard, loading, backgroundLoading, error,
    config, game, mods, enabledModCount, updateCount, frameworkOk,
    load, reloadConfig, applyConfigSnapshot, ensureLoaded, refresh,
  }
}
