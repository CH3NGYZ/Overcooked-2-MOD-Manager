import { computed, ref, shallowRef } from 'vue'
import { api, watchGameState } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { demoActive } from '../tutorial/demo'
import type { Dashboard, GameInfo, PublicConfig, InstalledMod } from '../types'

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
const localMods = shallowRef<InstalledMod[]>([])
const modsLoading = ref(false)
const modsError = ref('')
const modsGeneration = shallowRef(0)
const modsStatus = shallowRef<'idle' | 'loading' | 'ready' | 'error'>('idle')
/** Selected by the backend's GUID/version resolver, including disabled copies. */
const diyLevelTarget = shallowRef<string | null>(null)
let modsLoaded = false
// Request identity is frontend-only; trusted generations come exclusively from IPC.
let modsRequestRevision = 0
let basePending: Promise<void> | undefined
let modsPending: Promise<void> | undefined
let updateRevision = 0
let configRevision = 0
let gameStateRevision = 0
let latestGame: GameInfo | undefined
let gameIdentity: GameInfo | undefined
let bindingRevision = 0
let bindingPending: Promise<void> | undefined

/** A different game root/runtime invalidates both metadata and its trusted scan token. */
function invalidateLocalMods() {
  modsRequestRevision += 1
  modsPending = undefined
  modsLoaded = false
  modsLoading.value = false
  modsStatus.value = 'idle'
  modsGeneration.value = 0
  diyLevelTarget.value = null
  modsError.value = ''
  localMods.value = []
  bindingRevision += 1
  updateRevision += 1
  const current = dashboard.value
  if (current) dashboard.value = { ...current, mods: [], updates: [] }
}

function configGameChanged(before: PublicConfig | undefined, after: PublicConfig) {
  return !!before?.game && !!after.game &&
    (before.game.path !== after.game.path || before.game.runtime !== after.game.runtime)
}

function gamePathChanged(before: GameInfo | undefined, after: GameInfo) {
  return !!before && before.path !== after.path
}

/** Both independently arriving lanes reconcile against the backend's current
 * repository configuration. Coalesce updates; reject responses overtaken by
 * a newer TOML or local scan without blocking either lane's initial display. */
function rebindLocalMods(): Promise<void> {
  if (demoActive.value) return Promise.resolve()
  bindingRevision += 1
  if (!modsLoaded || !localMods.value.length) return Promise.resolve()
  if (bindingPending) return bindingPending
  bindingPending = (async () => {
    let revision: number
    do {
      revision = bindingRevision
      if (demoActive.value || !modsLoaded || !localMods.value.length) break
      const installed = localMods.value
      try {
        const rebound = await api.rebindLocalMods(installed)
        if (revision !== bindingRevision || installed !== localMods.value) continue
        localMods.value = rebound
        updateRevision += 1
        const current = dashboard.value
        if (current) dashboard.value = { ...current, mods: rebound }
      } catch (reason) {
        if (revision === bindingRevision && installed === localMods.value) {
          modsError.value = backendMessageOf(reason)
        }
      }
    } while (revision !== bindingRevision)
  })().finally(() => { bindingPending = undefined })
  return bindingPending
}

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
  if (gamePathChanged(gameIdentity, value)) invalidateLocalMods()
  gameIdentity = value
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
  startGameFocusWatch()
}

/**
 * 主动重算一次游戏状态。
 *
 * 事件通道负责「进程出现/消失」的推送，但界面还有几个会落后的时机：
 * 用户从游戏切回管理器、刚点完停止、某个操作因为 `game.running` 被拒。
 * 那时按钮会卡在「停止游戏」、整页的破坏性操作全是灰的，而后端其实早就
 * 判定为没在运行。这里让后端当场重算并推送，前端立刻收敛。
 *
 * 浏览器预览没有 IPC、旧安装包也没有这条命令 —— 都静默跳过，事件通道兜底。
 */
async function recheckGameState() {
  if (demoActive.value) return
  try {
    applyGame(await api.refreshGameState())
  } catch {
    // 见上：没有 IPC / 命令不存在都不该冒泡到界面
  }
}

/**
 * 切回窗口或标签页可见时补一次重算。
 *
 * macOS 上「游戏退出」最常见的情形就是用户先切走、在游戏里退出、再切回来；
 * 那一瞬间观察器可能刚好错过变化，或者观察器已经因为单轮失败停过一轮。
 * 用窗口焦点做触发点不增加常驻轮询，代价只有一次 IPC。
 */
let focusWatching = false
function startGameFocusWatch() {
  if (focusWatching || typeof window === 'undefined') return
  focusWatching = true
  let last = 0
  const recheck = () => {
    if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return
    const now = Date.now()
    // focus 与 visibilitychange 常常一起触发，1 秒内只认第一次
    if (now - last < 1000) return
    last = now
    void recheckGameState()
  }
  window.addEventListener('focus', recheck)
  document.addEventListener('visibilitychange', recheck)
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
  function loadBase(): Promise<void> {
    if (demoActive.value) return Promise.resolve()
    if (basePending) return basePending
    const configBefore = dashboard.value?.config
    const initial = dashboard.value === null
    if (initial) loading.value = true
    else backgroundLoading.value += 1
    error.value = ''
    basePending = (async () => {
      try {
        const value = await readDashboard()
        // A slow summary response must not roll back newer source configuration.
        const current = dashboard.value
        const newerConfig = current && current.config !== configBefore
        const config = newerConfig ? current.config : value.config
        const game = newerConfig && configGameChanged(value.config, current.config) ? current.game : value.game
        const identityChanged = configGameChanged(current?.config, config) || gamePathChanged(gameIdentity, game)
        if (configGameChanged(current?.config, config)) {
          invalidateLocalMods()
          gameIdentity = undefined
        } else if (gamePathChanged(gameIdentity, game)) invalidateLocalMods()
        gameIdentity = game
        latestGame = game
        dashboard.value = {
          ...value,
          config,
          game,
          mods: localMods.value,
          updates: dashboard.value?.updates ?? [],
        }
        await rebindLocalMods()
        // A root discovered by this summary may have overtaken the parallel
        // scan. Resume once against the newly established identity.
        if (identityChanged) await loadLocalMods()
      } catch (reason) {
        error.value = backendMessageOf(reason)
      } finally {
        if (initial) loading.value = false
        else backgroundLoading.value = Math.max(0, backgroundLoading.value - 1)
        basePending = undefined
      }
    })()
    return basePending
  }

  function loadLocalMods(): Promise<void> {
    if (demoActive.value) return Promise.resolve()
    if (modsPending) return modsPending
    modsLoading.value = true
    modsStatus.value = 'loading'
    modsError.value = ''
    const revision = ++modsRequestRevision
    modsPending = (async () => {
      try {
        const snapshot = await api.localMods()
        if (revision !== modsRequestRevision) return
        localMods.value = snapshot.mods
        modsGeneration.value = snapshot.generation
        diyLevelTarget.value = snapshot.diy_level_target ?? null
        modsStatus.value = 'ready'
        modsLoaded = true
        updateRevision += 1
        const current = dashboard.value
        if (current) dashboard.value = { ...current, mods: localMods.value }
        await rebindLocalMods()
      } catch (reason) {
        if (revision !== modsRequestRevision) return
        modsStatus.value = 'error'
        modsError.value = backendMessageOf(reason)
      } finally {
        if (revision === modsRequestRevision) {
          modsLoading.value = false
          modsPending = undefined
        }
      }
    })()
    return modsPending
  }

  async function checkModUpdates() {
    if (demoActive.value) return
    try {
      await ensureLocalModsLoaded()
    } catch {
      // A failed DLL scan cannot establish which installed versions to compare.
      return
    }
    if (demoActive.value) return
    const revision = ++updateRevision
    const installed = localMods.value
    const updates = await api.modUpdates(installed).catch(() => undefined)
    const current = dashboard.value
    if (updates && current && revision === updateRevision && installed === localMods.value) {
      dashboard.value = { ...current, updates }
    }
  }

  function invalidateModUpdates() { updateRevision += 1 }

  async function load() {
    // Each lane publishes immediately; only callers needing a complete reload await both.
    await Promise.all([loadBase(), loadLocalMods()])
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
  async function reloadConfig(options: { throwOnError?: boolean } = {}) {
    if (demoActive.value) return
    const revision = ++configRevision
    try {
      const config = await api.config()
      if (revision !== configRevision) return
      const current = dashboard.value
      if (!current) {
        // 还没有任何快照：走完整读取
        await loadBase()
        return
      }
      if (configGameChanged(current.config, config)) {
        invalidateLocalMods()
        gameIdentity = undefined
      }
      dashboard.value = { ...dashboard.value!, config }
      await rebindLocalMods()
    } catch (reason) {
      error.value = backendMessageOf(reason)
      if (options.throwOnError) throw reason
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
    configRevision += 1
    const current = dashboard.value
    if (!current) return
    if (configGameChanged(current.config, config)) {
      invalidateLocalMods()
      gameIdentity = undefined
    }
    dashboard.value = { ...dashboard.value!, config }
    void rebindLocalMods()
  }

  /**
   * 页面进入时使用：已有数据就直接复用，避免每次切页面都重新扫描并重绘整页。
   * 需要最新数据时由刷新引擎或用户手动刷新触发。
   */
  async function ensureLoaded() {
    if (demoActive.value) return
    if (!modsLoaded) void loadLocalMods()
    if (!dashboard.value) await loadBase()
  }

  /** Editors need a successfully completed DLL scan, including an empty result. */
  async function ensureLocalModsLoaded() {
    for (;;) {
      if (demoActive.value) return modsGeneration.value
      const pending = modsPending ?? (modsStatus.value === 'ready' ? undefined : loadLocalMods())
      const revision = modsRequestRevision
      if (pending) await pending
      // A root/runtime change may replace the scan while this caller waits.
      if (revision !== modsRequestRevision) continue
      if (modsStatus.value !== 'ready') throw new Error(modsError.value)
      return modsGeneration.value
    }
  }

  const config = computed<PublicConfig | null>(() => dashboard.value?.config ?? null)
  const game = computed<GameInfo | null>(() => dashboard.value?.game ?? null)
  const mods = computed(() => localMods.value)
  const enabledModCount = computed(() => mods.value.filter((mod) => mod.enabled).length)
  const updateCount = computed(() => mods.value.filter((mod) => mod.update_available).length)
  const frameworkOk = computed(() => {
    const info = game.value?.bepinex
    return !!info && info.installed
  })

  return {
    dashboard, loading, backgroundLoading, error, modsLoading, modsError, modsGeneration, modsStatus,
    config, game, mods, diyLevelTarget, enabledModCount, updateCount, frameworkOk,
    load, loadBase, loadLocalMods, checkModUpdates, invalidateModUpdates, reloadConfig, applyConfigSnapshot, ensureLoaded, ensureLocalModsLoaded, refresh, recheckGameState,
  }
}
