import { computed, ref, shallowRef } from 'vue'
import { api, watchGameState } from '../services/tauri'
import type { Dashboard, GameInfo, PublicConfig } from '../types'

/**
 * 全局仪表盘状态：dashboard 快照 + 游戏状态事件合并。
 * MOD 扫描可能慢于游戏状态事件，事件到达时保留较新的 game 快照。
 */
const dashboard = shallowRef<Dashboard | null>(null)
const loading = ref(false)
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
  async function load() {
    loading.value = true
    error.value = ''
    try {
      dashboard.value = await readDashboard()
    } catch (reason) {
      error.value = reason instanceof Error ? reason.message : String(reason)
    } finally {
      loading.value = false
    }
  }

  async function refresh() {
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
    dashboard, loading, error,
    config, game, mods, enabledModCount, updateCount, frameworkOk,
    load, refresh,
  }
}
