/**
 * 「连点解锁隐蔽入口」的纯规则（不依赖 Vue / 路由 / i18n）。
 *
 * 地图发布页只对作者开放，工具页的入口收起后仍需一条不占界面的进入方式：
 * 标题栏版本号在短时间内被连点若干次即解锁。规则本身与「解锁后去哪儿」无关，
 * 单独下沉到这里，`AppTitleBar` 只负责收集点击并 emit，路由跳转留在 `App.vue`。
 *
 * 时间戳用调用方给的 `now`（毫秒），不读 `Date.now()`：这样窗口行为可测，
 * 也不会被系统时间跳变悄悄影响。
 */

/** 连点次数：与「连续点击版本号 5 次」的约定一致。 */
export const SECRET_ENTRY_TAPS = 5

/**
 * 连点窗口（毫秒）：**所有**有效点击都必须落在最近这段时间内。
 * 慢慢点满 5 次不算 —— 每次点击都会把过期（距今 >= windowMs）的旧点击丢掉，
 * 所以隔几秒点一下永远凑不满。3 秒足够正常连点，又不至于日常误触。
 */
export const SECRET_ENTRY_WINDOW_MS = 3000

/**
 * 记一次点击，返回窗口内保留的时间戳序列（纯函数，不改入参）。
 *
 * 未来时间戳（系统时间回拨或调用方传错）一并丢掉，避免它们把后续点击
 * 全判成「过期」而永远解锁不了。
 */
export function recordTap(
  history: readonly number[],
  now: number,
  windowMs: number = SECRET_ENTRY_WINDOW_MS,
): number[] {
  const kept = history.filter((at) => now >= at && now - at < windowMs)
  kept.push(now)
  return kept
}

/** 是否凑够了连点次数。达成后由调用方清空序列，避免第 6 次点击又触发一次。 */
export function tapSequenceReady(
  history: readonly number[],
  required: number = SECRET_ENTRY_TAPS,
): boolean {
  return history.length >= required
}
