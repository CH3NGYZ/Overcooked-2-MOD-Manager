/**
 * 本地关卡集扫描（`get_level_sets`）的共享状态（**模块级单例**）。
 *
 * ## 为什么状态不能挂在页面上
 *
 * 想显示「正在读取本地地图」的不止地图管理页：总览页的「本地地图」卡也要读同一份
 * 列表，而它读的时候用户可能已经切到别的页面。状态跟着页面实例走，切走就丢 ——
 * 回来只剩一个上次的 0，或者干脆重起一次扫描。与 `useMapSources` /
 * `useRepositoryRefresh` 一致：状态放模块作用域，页面只负责渲染。
 *
 * ## 只持有状态，不持有数据
 *
 * 列表本身仍由调用方保存（地图管理页要按作者分组排序、下载页要标记「已安装」、
 * 总览页只数个数），这里不存第二份，也就没有「谁的数据更新」的问题。`progress`
 * 只服务地图管理页整页加载态里的「已读取 x/y」。
 */
import { computed, ref } from 'vue'
import { api, watchLevelSetScan } from '../services/tauri'
import type { LevelSetInfo, LevelSetScanProgress } from '../types'
import type { UnlistenFn } from '@tauri-apps/api/event'

/**
 * 在飞扫描数。
 *
 * 用计数而不是布尔：进页面、导入后重读、另一页同时读都可能叠在一起，最后一个
 * 收尾才归零。写成布尔时先结束的那次会把还在跑的扫描判成「已结束」。
 */
const inFlight = ref(0)

/** 是否有扫描在飞（总览卡片的按钮转圈读它）。 */
const scanning = computed(() => inFlight.value > 0)

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
export async function scanLevelSets(): Promise<LevelSetInfo[]> {
  ensureProgressListener()
  // 新一轮从「没有进度」开始：上一轮的 x/y 会让人以为新扫描已经跑到那一步。
  if (inFlight.value === 0) progress.value = null
  inFlight.value += 1
  try {
    return await api.levelSets()
  } finally {
    inFlight.value -= 1
    // 只有最后一个收尾才清进度：并发扫描里先结束的那次不该清掉还在跑的那次。
    if (inFlight.value === 0) progress.value = null
  }
}

export function useLevelSetScan() {
  return { scanning, progress, scanLevelSets }
}
