import { t } from './i18n'
import type { RefreshOutcome } from './refreshState'

/**
 * 刷新结果 → 可展示文案。
 *
 * 收口在这里而不是让每个页面自己拼：同一批刷新会在两个页面显示
 * （管理页的摘要标签、下载页的浮动提示），两处各拼一遍迟早一处写成
 * 「已检查 6 个」，另一处写成「共 6 项已刷新」，用户会以为是两件事。
 *
 * 这里只做「拼句子」，不碰 DOM、不碰组件 —— 页面用同一个字符串渲染
 * 标签的 `title` 与 toast。
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

/**
 * 悬停说明：完成时间 + 逐条失败原因。
 *
 * 时间用 `toLocaleTimeString` 而不是「3 分钟前」：相对时间要跟着时钟每
 * 几十秒重算一次，为一个悬停提示挂定时器不上算；绝对时间也更好核对
 * （用户常拿它和「我刚才点的」对时间）。
 */
export function describeRefreshOutcomeTitle(outcome: RefreshOutcome, locale: string): string {
  const time = new Date(outcome.finishedAt)
    .toLocaleTimeString(locale, { hour: '2-digit', minute: '2-digit' })
  const lines = [`${t('refresh.lastCheck')} · ${time}`]
  if (outcome.errors.length) {
    lines.push(`${t('refresh.failureReasons')}:`)
    lines.push(...outcome.errors.map((error) => `· ${error}`))
  }
  return lines.join('\n')
}
