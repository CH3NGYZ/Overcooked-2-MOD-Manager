/**
 * 头像预取的纯逻辑：键归一化 + 挑出「这次要请求哪些 owner」。
 *
 * 单独成文件的唯一原因是**可测试**：`avatars.ts` 依赖 Vue 的 `ref` 与 Tauri IPC，
 * Node 里跑不起来，只能对源码做结构性断言；而「同一个 owner 只请求一次」这条
 * 规则恰恰是最容易写错、也最值得用真实代码跑一遍的部分。
 *
 * 这一层不碰缓存、不发请求：由 `avatars.ts` 把「已缓存 / 已请求」告诉它。
 */
import type { Provider } from './types'

/**
 * 参与头像预取的最小结构。
 *
 * 取 `provider` + `username` 而不是某个具体模型：MOD 仓库（`ModRepository`）与
 * 地图（`MapInfo`）都长这个样子，两个安装页因此能把各自的列表直接传进同一个
 * `prefetchAvatars`，不需要为地图再写一份预取。
 */
export interface AvatarSource {
  provider: Provider
  username?: string | null
}

/** 一个待预取目标：键归一化过，owner 保留原文（后端按原文查缓存与拼 URL）。 */
export interface AvatarTarget {
  key: string
  provider: Provider
  owner: string
}

/**
 * `provider:owner` 的归一化键。
 *
 * 平台不同即不同人（同名 owner 在 GitHub / Gitee 是两个人），所以 provider
 * 进键；owner 大小写与首尾空白不影响命中（GitHub 用户名大小写不敏感，
 * 用户在源文档里写的与后端返回的可能不一致）。
 */
export function avatarKey(provider: Provider, owner: string): string {
  return `${provider}:${owner.trim().toLowerCase()}`
}

/**
 * 从一批条目里挑出需要请求的 owner。
 *
 * 1. 没有 `username` 的跳过（社区搜索结果里常见）；
 * 2. `isKnown` 为真的跳过（已缓存或本次会话已经请求过）；
 * 3. 批内按归一化键去重，保留第一次出现的原文与顺序。
 *
 * `isKnown` 由调用方给，是因为「缓存」和「已请求」是两层状态，纯逻辑不该知道
 * 它们分别存在哪里。
 */
export function pendingAvatarTargets(
  sources: readonly AvatarSource[],
  isKnown: (key: string) => boolean,
): AvatarTarget[] {
  const targets: AvatarTarget[] = []
  const seen = new Set<string>()
  for (const source of sources) {
    const owner = source.username?.trim()
    if (!owner) continue
    const key = avatarKey(source.provider, owner)
    if (seen.has(key) || isKnown(key)) continue
    seen.add(key)
    targets.push({ key, provider: source.provider, owner })
  }
  return targets
}
