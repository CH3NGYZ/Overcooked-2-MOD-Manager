/**
 * 地图源数据与刷新引擎（**模块级单例**）。
 *
 * ## 为什么不是页面内的 `ref`
 *
 * 整源刷新要依次拉「索引 → 子源 → 每张图的 Release」，真实网络下好几秒。
 * 用户在这期间切走再切回来时，页面实例已经卸载重建 —— 状态挂在页面上的话
 * 就全丢了：回来只剩一个旧列表，用户完全不知道后台那次刷新还在跑，
 * 也看不到任何加载反馈。
 *
 * MOD 安装页没有这个问题，因为它的刷新状态在 `useRepositoryRefresh` 里同样
 * 是模块级单例。这里与之对齐：数据与状态都在模块作用域，页面只是渲染它们。
 *
 * ## 与 `useRepositoryRefresh` 的差别
 *
 * MOD 侧的数据源是 Dashboard（`game`/`mods`/`config`），刷新引擎只持有
 * 「谁在刷」。地图侧的数据是独立的一份（`maps`），没有对应的 store，
 * 所以数据和引擎一起放在这里。
 */
import { ref, type Ref } from 'vue'
import { api } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { useNotify, type Notify } from '../useNotify'
import type { MapInfo } from '../types'

/** 地图源声明的全部地图（含 Release 缓存）。 */
const maps = ref<MapInfo[]>([])
/** 首次读数据中（只影响整页加载态，不影响刷新）。 */
const loading = ref(true)
/** 每张地图的 Release 刷新状态。 */
const refreshing = ref<Record<string, boolean>>({})
/** 每个地图源的刷新状态（源树重拉 + 该源下全部 Release）。 */
const refreshingSources = ref<Record<string, boolean>>({})
/** 每张地图的安装状态。安装进行中切走再回来也要保持。 */
const installing = ref<Record<string, boolean>>({})
/** 整源刷新进行中（浮动按钮的转圈与整页遮罩都用它）。 */
const refreshingAll = ref(false)

/**
 * 通知入口。
 *
 * `useNotify()` 依赖 `n-message-provider` 的注入，必须在组件 setup 里取，
 * 所以这里延后到第一次调用 `useMapSources()` 时初始化；之后复用同一个实例
 * （它的底层是全局 provider，不随发起组件销毁而失效）。
 */
let message: Notify | null = null

function setFlag(flags: Ref<Record<string, boolean>>, id: string, value: boolean) {
  const next = { ...flags.value }
  if (value) next[id] = true
  else delete next[id]
  flags.value = next
}

/**
 * 读一次地图列表（后端缓存，只读不联网）。
 *
 * `force` 区分两种调用方：
 *   · 页面挂载时读 —— 整源刷新进行中就**跳过**。否则切走再切回来时，这里会用
 *     旧缓存把刚清空的列表填回去，遮罩随之提前消失，用户看到的是「还在刷新
 *     但看起来已经好了」。
 *   · 刷新流程自己收尾时读 —— 必须强制，那时列表正等着换成新数据。
 */
async function loadMaps(options: { force?: boolean } = {}) {
  if (refreshingAll.value && !options.force) return
  loading.value = maps.value.length === 0
  try {
    maps.value = await api.maps()
  } catch (reason) {
    message?.error(backendMessageOf(reason))
  } finally {
    loading.value = false
  }
}

async function syncDependencySnapshots() {
  await api.syncDependencySnapshots().catch(() => undefined)
}

/**
 * 逐个刷新一批地图的 Release，返回失败信息。
 *
 * 单个失败不阻断同批其它条目：一次刷几十张图时逐条弹错会把用户淹没，所以
 * 错误交回调用方合并上报（与 MOD 安装页的批量刷新同一套做法）。
 *
 * 结束后把线上依赖声明写回本地清单并重读列表：卡片状态与依赖表要立刻反映
 * 新缓存，否则用户刷完看到还是旧版本。
 */
async function refreshReleases(list: MapInfo[]): Promise<string[]> {
  const errors: string[] = []
  for (const map of list) {
    if (refreshing.value[map.id] || installing.value[map.id]) continue
    setFlag(refreshing, map.id, true)
    try {
      await api.refreshMapReleases(map.id, true)
    } catch (reason) {
      errors.push(backendMessageOf(reason))
    } finally {
      setFlag(refreshing, map.id, false)
    }
  }
  await syncDependencySnapshots()
  await loadMaps({ force: true })
  return errors
}

/**
 * 刷新一个源。
 *
 * 后端没有「只刷新一个源」的命令：地图源树的加载本身就是一次整体请求，
 * 所以这里先重拉源树（可能新增/删除地图、改分组），再刷新这个源下的
 * 全部 Release。重拉后按源 ID 重新取一次条目，不吃重拉前的旧对象。
 */
async function refreshSource(node: { id: string }) {
  if (refreshingAll.value || refreshingSources.value[node.id]) return
  setFlag(refreshingSources, node.id, true)
  try {
    try {
      maps.value = await api.refreshMapSources()
    } catch (reason) {
      message?.error(backendMessageOf(reason))
    }
    const scoped = maps.value.filter((map) => (map.source_id || '__none__') === node.id)
    const errors = await refreshReleases(scoped)
    if (errors.length) message?.error(errors[0])
  } finally {
    setFlag(refreshingSources, node.id, false)
  }
}

/**
 * 刷新全部：先清空列表，再重拉源树，最后逐个刷 Release。
 *
 * **必须清空**：刷新是「重新读源文档并重建列表」，旧卡片留在屏幕上会让人
 * 以为只是部分条目在更新，而且旧对象在被替换前仍然可点（可能已经下线或改名）。
 * 这与 MOD 安装页是同一条逻辑 —— 那边由 `refreshEverything()` 先
 * `clearRepositorySourceCache` 把列表清空。
 *
 * 失败时把旧列表还原：源文档拉不到就白屏，用户会以为地图全没了；留着旧数据
 * 至少能看、能装，下一次刷新再试。
 */
async function refreshAll() {
  if (refreshingAll.value) return
  refreshingAll.value = true
  const previous = maps.value
  maps.value = []
  try {
    try {
      maps.value = await api.refreshMapSources()
    } catch (reason) {
      maps.value = previous
      message?.error(backendMessageOf(reason))
    }
    const errors = await refreshReleases(maps.value)
    errors.forEach((error) => message?.error(error))
  } finally {
    refreshingAll.value = false
  }
}

export function useMapSources() {
  if (!message) message = useNotify()
  return {
    maps,
    loading,
    refreshing,
    refreshingSources,
    installing,
    refreshingAll,
    loadMaps,
    refreshAll,
    refreshSource,
    refreshReleases,
    setFlag,
    syncDependencySnapshots,
  }
}
