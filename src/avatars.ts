/**
 * 仓库 / 地图 owner 头像的前端缓存层。
 *
 * 三个职责，都只在这一个地方做：
 *   1. **按 `{provider}:{owner}` 去重**：一次刷新会刷出几十个条目，很多属于
 *      同一个 owner（所有 BepInEx 插件都是 BepInEx）。同一个 key 只请求一次；
 *      键归一化与批内去重的规则在 `avatarTargets.ts`，那是纯函数、Node 测试
 *      直接覆盖，这里不重写一遍。
 *   2. **内存缓存**：同一会话内切换页面、重渲染都不再请求；磁盘缓存由后端
 *      负责（12 小时 TTL，跨启动生效），前端永远优先读缓存。
 *   3. **失败即不记**：取不到的 owner 不进缓存，下次刷新会重试；但**本次**
 *      不会重复请求（`requested` 集合记录已尝试过的 key）。
 *
 * 组件只读 `avatarFor(provider, owner)`，返回 `null` 时自己回落到平台图标。
 *
 * 预取入口也只有 `prefetchAvatars` 一个：MOD 安装页与地图安装页各自把列表传
 * 进来即可（两个模型都是 `{ provider, username }` 形状），不允许页面里再写
 * 一份去重或另开请求。
 */
import { ref } from 'vue'
import { api } from './services/tauri'
import { avatarKey, pendingAvatarTargets, type AvatarSource } from './avatarTargets'
import { isDemoOwner } from './tutorial/demo'
import type { Provider } from './types'

/** `provider:owner` → data URL。 */
const cache = ref<Record<string, string>>({})
/** 已经请求过的 key（无论成败），避免同一会话内反复打后端。 */
const requested = new Set<string>()
/** 正在进行的批量请求；并发调用合并到同一个 Promise。 */
let inFlight: Promise<void> | null = null

/** 查一个 owner 的头像；没有返回 `null`。 */
export function avatarFor(provider: Provider, owner: string | undefined): string | null {
  const name = owner?.trim()
  if (!name) return null
  return cache.value[avatarKey(provider, name)] ?? null
}

/**
 * 清空前端头像缓存，并允许重新请求。
 *
 * 必须同时清 `requested`：那个集合记录「本次会话已经问过后端」的 key，
 * 只清 `cache` 的话，界面会立刻回落到平台图标而且**再也不会重新获取** ——
 * 因为预取会认为这些 owner 已经请求过了。
 */
export function clearAvatarCache() {
  cache.value = {}
  requested.clear()
}

/**
 * 为一批条目（仓库 / 地图，只要带 `provider` + `username`）预取头像。
 *
 * 只请求「有 owner 且没请求过」的，其余直接跳过。多个调用方并发触发时
 * 共用同一个在途 Promise —— 否则页面挂载 + 列表刷新各来一次，
 * 会产生两倍请求。
 */
export function prefetchAvatars(sources: readonly AvatarSource[]): Promise<void> {
  const pending = pendingAvatarTargets(
    /*
     * 教程示例条目的 owner 并不存在（`tutorial/demo.ts` 的 `isDemoOwner`），
     * 请求必然 404：跳过它们，否则每次教程都会在日志里留下一次失败，
     * 离线时还要白等一轮超时。示例卡片回落到平台图标，本来就是这个样子。
     */
    sources.filter((source) => !isDemoOwner(source.username)),
    (key) => requested.has(key) || !!cache.value[key],
  )
  if (!pending.length) return inFlight ?? Promise.resolve()
  if (inFlight) return inFlight

  for (const target of pending) requested.add(target.key)
  inFlight = (async () => {
    try {
      const result = await api.repositoryAvatars(
        pending.map(({ provider, owner }) => ({ provider, owner })),
      )
      if (Object.keys(result).length) {
        cache.value = { ...cache.value, ...result }
      }
    } catch {
      /*
       * 头像失败一律静默：它不是关键数据，弹错误只会打扰用户。
       * 已请求过的 key 留在 `requested` 里，所以本次会话不再重试 ——
       * 重试成本高于收益，下次启动会重新尝试（后端缓存仍然有效）。
       */
    } finally {
      inFlight = null
    }
  })()
  return inFlight
}
