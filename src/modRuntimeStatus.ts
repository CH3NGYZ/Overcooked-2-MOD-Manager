import type { InstalledMod, InstalledModStatusLabel, Repository } from './types'
import { repositoryForMod } from './modStatus.ts'
import { primaryRelation, relationTagView, resolveRelations, type RelationStatus } from './modRelations.ts'
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
  /**
   * 条目集合正在清空/重建（整批刷新的源文档与社区发现阶段）。
   *
   * 只有这段空窗期该由批次兜底：仓库列表被清掉后按仓库 id 的状态全都匹配
   * 不上，卡片否则只能显示一个马上会过期的旧结论。**不能拿「整批刷新进行中」
   * 代替它**——整批标志亮到 Release 阶段结束，用它判等于把每张卡都按住，
   * 逐个返回的结果会被压成整批一起返回。
   */
  entryStateUnavailable: boolean
}

/**
 * 某个仓库现在是否在刷新。
 *
 * 三处来源都要算上，漏一个就会出现「安装页在刷、管理页标签不动」：
 *   · 该仓库自己正在抓 Release（`refreshing`）；
 *   · 它已经在这一批里排队（`queued`）—— 并发只有 4，20 个仓库里 16 个在排队，
 *     排队的不显示「刷新中」会让用户以为「只有那四个在查」；
 *   · 仓库列表正在被清空重建（`entryStateUnavailable`）—— 整批刷新的源文档
 *     阶段列表是空的，唯一能表达「正在刷新」的就是它；
 *   · 它所属的仓库源正在重拉文档（`sourceRefreshing`）。
 *
 * 仓库文件（RAW）模式恒为 false：它没有版本可比较，刷新也不该把
 * 「已安装」改成「刷新中」。
 */
export function repositoryRefreshing(
  repo: Repository | undefined,
  state: RuntimeRefreshState,
): boolean {
  if (!repo || repo.download_mode === 'repository_file') return false
  /*
   * 三个记录都用 `?? {}` 兜底：热更新只换了其中一半模块时（旧页面状态对象
   * 没有新字段），直接索引 `undefined[id]` 会让整页 computed 抛错、卡片全空。
   * 这不是「防御性代码掩盖 bug」—— 字段缺失的正确答案就是「没有排队」。
   */
  const refreshing = state.refreshing ?? {}
  const queued = state.queued ?? {}
  if (refreshing[repo.id] || queued[repo.id] || state.entryStateUnavailable) return true
  return !!(repo.source_id && (state.sourceRefreshing ?? {})[repo.source_id])
}

export interface RuntimeModStatus {
  /** 合成后的 MOD：`labels` / `update_available` 已按当前缓存与刷新状态重算。 */
  mod: InstalledMod
  /** 与其它插件的关系问题（缺依赖 / 冲突），与版本结论相互独立。 */
  issue?: RelationStatus
  /** 绑定到的仓库是否正在刷新（卡片可以直接用它决定要不要显示「刷新中」）。 */
  refreshing: boolean
}

/** 关系类标签：一律由 `relationStatusLabel` 按当前关系重算，不沿用旧结论。 */
const RELATION_LABELS = new Set<string>([
  'missing_dependency',
  'dependency_conflict',
  'dependency_installable',
])

/**
 * 关系问题该挂哪一枚标签。
 *
 * 与卡片标签（`modRelations.relationTagView` / `dependencyTagFor`）同一份
 * 口径：`requires` 缺了就是「缺依赖」（线上能不能补上不改变这件事）；
 * `compatible` 有满足区间的版本可换才是「可安装」；`conflicts` 是
 * 「有冲突」，唯一出路是卸载。
 */
export function relationStatusLabel(
  issue: RelationStatus | undefined,
): InstalledModStatusLabel | undefined {
  const view = relationTagView(issue)
  if (!view) return undefined
  if (view.kind === 'conflict') return 'dependency_conflict'
  if (view.kind === 'installable') return 'dependency_installable'
  return 'missing_dependency'
}

/**
 * 把「后端扫描结果」与「当前仓库缓存 + 刷新状态」合成一份有效状态。
 *
 * 三层来源，优先级从高到低：
 *   1. 绑定的仓库（或它所在的源）**正在刷新** → 「刷新中」。
 *      必须排在结论前面：仓库通常已经有上一轮的 Release 缓存，先按缓存出
 *      结论的话，整批刷新期间每张卡都还挂着「最新版」—— 用户点完「检查更新」
 *      看不到任何反应，只会以为按钮没生效。
 *   2. 这个仓库这次刷新**失败**了（`last_error`）→ 「仓库刷新失败」。
 *      失败是这一条自己的结果，刷新引擎在它失败的那一刻就写回了 `last_error`
 *      （与后端清缓存、置错误的规则一致），所以不必等整批结束的后端重扫。
 *   3. 仓库有 Release 且版本可比 → 用缓存重算（刷新完立刻生效）
 *   4. 其余情况 → 保留后端扫描时的标签
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
      ?.filter((key) => !RELATION_LABELS.has(key)) ?? []
    const label = relationStatusLabel(issue)
    if (!label) return base as InstalledMod['labels']
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

  /*
   * 刷新失败当场显示：`last_error` 由刷新引擎在该仓库失败的那一刻写回
   * （与后端同一条规则：清缓存 + 置错）。判定顺序必须在「按缓存重算」之前 ——
   * 失败时旧缓存已经被清空或不可信，再拿它出结论就会显示成一个假的最新版。
   */
  if (repo?.last_error) {
    return {
      issue,
      refreshing,
      mod: {
        ...mod,
        update_available: false,
        labels: withRelation(replaceVersionConclusion(
          mod.labels as string[] | undefined,
          'repository_unavailable',
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
