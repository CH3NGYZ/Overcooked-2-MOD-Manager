import { t } from './i18n'

/**
 * 状态标签「点了会跳到哪」的唯一实现（MOD 卡片与包标题共用）。
 *
 * 规则本身很短，但有两套标签语义要同时照顾：
 *   · 依赖类标签（缺依赖 / 冲突）跳的是**能提供/冲突的那个仓库**，
 *     不是这张卡自己绑定的仓库；
 *   · 其余结论类标签跳的是自己绑定的仓库。
 *
 * 原来这份判定写在 MOD 管理页里，卡片标签下沉成组件后必须共用同一份 ——
 * 两处各写一遍时，点「缺依赖」在一处跳到依赖仓库、另一处跳到自己的仓库。
 */
export function tagTargetId(
  tagKey: string,
  boundRepositoryId: string | undefined,
  /** 依赖仓库 id：由调用方从 `RelationStatus` 解析（`relationJumpRepositoryId`）。 */
  dependencyRepositoryId?: string,
): string | undefined {
  if (tagKey === 'missing_dependency' || tagKey === 'dependency_conflict') {
    return dependencyRepositoryId
  }
  return boundRepositoryId
}

/**
 * 标签的悬停提示：说清点了会跳到哪、为什么可跳/不可跳。
 *
 * 没有目标时返回 `undefined`（不挂 title），只有「缺依赖」会额外解释
 * 「线上没有可安装来源」—— 否则用户会以为标签坏掉了。
 */
export function tagTargetTitle(tagKey: string, target: string | undefined): string | undefined {
  if (!target) {
    return tagKey === 'missing_dependency' ? t('modsOnline.noInstallableSource') : undefined
  }
  return tagKey === 'missing_dependency' || tagKey === 'dependency_conflict'
    ? t('modsOnline.viewDependencyInModInstall')
    : t('dialogs.viewInModInstall')
}
