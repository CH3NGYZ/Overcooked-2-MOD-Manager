import { t } from './i18n'
import type { RefreshOutcome } from './refreshState'

/**
 * 刷新结果 → 可展示文案。
 *
 * MOD 和地图管理页共用检查完成提示的格式，只换条目量词。
 */

/** 摘要按哪种条目措辞：仓库（个）还是地图（张）。 */
export type RefreshDomain = 'repositories' | 'maps'

/**
 * 一行摘要：`已检查 6 个仓库 · 2 个有更新 · 1 个失败`。
 *
 * 「有更新」与「失败」只在非零时出现：0 个有更新时写出来只会让人
 * 以为需要处理什么，而它恰恰是「都最新」这个好消息。
 */
export function describeRefreshOutcome(outcome: RefreshOutcome, domain: RefreshDomain): string {
  const parts = [
    t(domain === 'maps' ? 'refresh.checkedMaps' : 'refresh.checkedRepositories',
      { count: outcome.refreshed }),
  ]
  if (outcome.updated > 0) parts.push(t('refresh.updated', { count: outcome.updated }))
  if (outcome.errors.length) parts.push(t('refresh.failed', { count: outcome.errors.length }))
  return parts.join(' · ')
}
