import type { InstalledMod, Repository } from './types'
import { repositoryForMod } from './modStatus.ts'
import { primaryRelation, resolveRelations, type RelationStatus } from './modRelations.ts'
import { modUpdateVerdict, replaceVersionConclusion } from './statusTags.ts'

/**
 * 「这张卡现在该显示什么状态」的唯一实现（MOD 侧）。
 *
 * ## 为什么要单独一个模块
 *
 * 这段判定原来写在 MOD 管理页的 `effectiveMods` 里，只有页面自己能用。
 * 但卡片上的标签**不在**页面的重渲染路径上：`n-collapse-item` 的内容区由
 * naive-ui 的 `CollapseItemContent` 拿着稳定插槽渲染，父级重渲染时
 * `shouldUpdateComponent` 直接跳过，插槽闭包不再求值 —— 于是页头的聚合
 * 已经变成「4 刷新中」，每张卡的标签还停在「最新版」。
 *
 * 所以判定必须能被**卡片自己**调用（组件的渲染效应会订阅 `refreshing` 等
 * 响应式来源，与父级是否重渲染无关）。放在这里还带来两个好处：
 *   · 页头聚合与卡片标签用的是同一份实现，不会一个说刷新中、一个说最新版；
 *   · 纯函数，Node 测试可以直接 import。
 */

/** 刷新状态的只读快照（字段与引擎同名）。 */
export interface RuntimeRefreshState {
  refreshing: Record<string, boolean>
  /** 排队中（已在这一批里、还没轮到）。 */
  queued: Record<string, boolean>
  sourceRefreshing: Record<string, boolean>
  refreshingAll: boolean
}

/**
 * 某个仓库现在是否在刷新。
 *
 * 三处来源都要算上，漏一个就会出现「安装页在刷、管理页标签不动」：
 *   · 该仓库自己正在抓 Release（`refreshing`）；
 *   · 它已经在这一批里排队（`queued`）—— 并发只有 4，20 个仓库里 16 个在排队，
 *     排队的不显示「刷新中」会让用户以为「只有那四个在查」；
 *   · 整批「刷新全部」正在进行（`refreshingAll`）；
 *   · 它所属的仓库源正在重拉文档（`sourceRefreshing`）—— 源文档阶段还没有
 *     按仓库发的 Release 请求，只有源级状态能反映进度。
 *
 * 仓库文件（RAW）模式恒为 false：它没有版本可比较，刷新也不该把
 * 「已安装」改成「刷新中」。
 */
export function repositoryRefreshing(
  repo: Repository | undefined,
  state: RuntimeRefreshState,
): boolean {
  if (!repo || repo.download_mode === 'repository_file') return false
  if (state.refreshing[repo.id] || state.queued[repo.id] || state.refreshingAll) return true
  return !!(repo.source_id && state.sourceRefreshing[repo.source_id])
}

export interface RuntimeModStatus {
  /** 合成后的 MOD：`labels` / `update_available` 已按当前缓存与刷新状态重算。 */
  mod: InstalledMod
  /** 与其它插件的关系问题（缺依赖 / 冲突），与版本结论相互独立。 */
  issue?: RelationStatus
  /** 绑定到的仓库是否正在刷新（卡片可以直接用它决定要不要显示「刷新中」）。 */
  refreshing: boolean
}

/**
 * 把「后端扫描结果」与「当前仓库缓存 + 刷新状态」合成一份有效状态。
 *
 * 三层来源，优先级从高到低：
 *   1. 绑定的仓库（或它所在的源）**正在刷新** → 「刷新中」。
 *      必须排在结论前面：仓库通常已经有上一轮的 Release 缓存，先按缓存出
 *      结论的话，整批刷新期间每张卡都还挂着「最新版」—— 用户点完「检查更新」
 *      看不到任何反应，只会以为按钮没生效。
 *   2. 仓库有 Release 且版本可比 → 用缓存重算（刷新完立刻生效）
 *   3. 其余情况 → 保留后端扫描时的标签
 *
 * 关系判定（缺依赖 / 冲突）是另一件事，永远往标签最前面插，不参与上面的取舍。
 */
export function runtimeModStatus(
  mod: InstalledMod,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[],
  state: RuntimeRefreshState,
): RuntimeModStatus {
  const repo = repositoryForMod([...repositories], mod)
  const issue = mod.is_dependency
    ? undefined
    : (() => {
        if (!repo) return undefined
        // 用**已安装版本**筛关系表：装的是 1.0 就该按 1.0 的要求算
        return primaryRelation(resolveRelations(repo, [...mods], [...repositories], mod.version))
      })()
  const refreshing = repositoryRefreshing(repo, state)

  /** 关系标签永远置顶，且不和版本结论互相覆盖。 */
  const withRelation = (labels: InstalledMod['labels']): InstalledMod['labels'] => {
    const base = (labels as string[] | undefined)
      ?.filter((key) => key !== 'missing_dependency' && key !== 'dependency_conflict') ?? []
    if (!issue) return base as InstalledMod['labels']
    // 冲突比缺依赖更该被看见：缺依赖只是不工作，冲突可能已经让游戏崩了
    const label = issue.state === 'conflict' ? 'dependency_conflict' : 'missing_dependency'
    return [label, ...base] as InstalledMod['labels']
  }

  if (refreshing) {
    return {
      issue,
      refreshing,
      mod: {
        ...mod,
        update_available: false,
        labels: withRelation(replaceVersionConclusion(
          mod.labels as string[] | undefined,
          'refreshing',
        ) as InstalledMod['labels']),
      } as InstalledMod,
    }
  }

  const verdict = modUpdateVerdict(mod, [...repositories])
  if (verdict.known) {
    /*
     * 用 `replaceVersionConclusion` 而不是自己拼数组：旧实现只过滤
     * latest/update_available，会把 not_release_asset、repository_unavailable
     * 等旧结论留在原地，出现「最新版 + 仓库不可用」这种自相矛盾的组合。
     */
    return {
      issue,
      refreshing,
      mod: {
        ...mod,
        update_available: verdict.updateAvailable,
        labels: withRelation(replaceVersionConclusion(
          mod.labels as string[] | undefined,
          verdict.updateAvailable ? 'update_available' : 'latest',
        ) as InstalledMod['labels']),
      } as InstalledMod,
    }
  }

  return { issue, refreshing, mod: { ...mod, labels: withRelation(mod.labels) } as InstalledMod }
}
