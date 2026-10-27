/**
 * 一次性庆祝动画（安装 / 更新成功）。
 *
 * 为什么单独一个模块而不是直接在各组件里 import：
 *   1. `canvas-confetti` 只在成功那一刻需要 —— 用动态 `import()` 让它
 *      留在独立的 chunk 里，不进首屏包；
 *   2. 颜色必须取自当前主题色（`--oc2-accent`），不能写死一组彩带色，
 *      否则换主题时彩带还是旧颜色，和标签、按钮脱节；
 *   3. `prefers-reduced-motion` 的判断与「避免并发叠加」只在这里做一次。
 *
 * 场合约束（依据 ui-animation：delight 与频率成反比）：
 *   只在**低频、高价值**的时刻放：MOD 安装成功、更新成功。
 *   刷新、开关、删除这类高频操作不放 —— 天天见的东西不能每次都放烟花。
 */
import { prefersReducedMotion } from './motion'

/** 正在播放的动画数量：成功提示可能连点，避免两轮彩带叠在一起。 */
let active = 0

/**
 * 从当前主题色推导一组彩带颜色。
 *
 * 读 CSS 变量而不是 import theme 对象，是为了让「应用主题色」这条链路
 * 依然只有 `theme.ts` 一个源头，这里只是消费方。
 */
function themeColors(): string[] {
  const fallback = ['#22c55e', '#3b82f6', '#f59e0b']
  if (typeof getComputedStyle !== 'function') return fallback
  const style = getComputedStyle(document.documentElement)
  const accent = style.getPropertyValue('--oc2-accent').trim()
  const contrast = style.getPropertyValue('--oc2-tone-moderate-bg').trim()
  return [accent, contrast].filter(Boolean).length
    ? [accent || fallback[0], contrast || fallback[1], fallback[2]]
    : fallback
}

/**
 * 播放一次「安装成功」庆祝。
 *
 * 形态选择：两侧向中间上方斜射的短促彩带（约 900ms），不是满屏铺开的
 * 长时间烟花 —— 桌面上还有后续操作（关闭弹窗、继续浏览），
 * 动画不该在用户已经进入下一步时还在背景里飘。
 */
export async function celebrateInstall(): Promise<void> {
  if (prefersReducedMotion()) return
  if (active > 0) return
  active += 1
  try {
    const { default: confetti } = await import('canvas-confetti')
    const colors = themeColors()
    const base = { particleCount: 28, spread: 62, startVelocity: 34, ticks: 110, colors, scalar: 0.9 }
    // 两侧同时起，形成中心向外的收束感，比单点爆发更「完成」
    void confetti({ ...base, origin: { x: 0.28, y: 0.72 }, angle: 58 })
    void confetti({ ...base, origin: { x: 0.72, y: 0.72 }, angle: 122 })
  } catch {
    // 彩带是锦上添花：加载失败绝不能让安装流程报错
  } finally {
    // 让连点有最小间隔，但不阻塞调用方
    window.setTimeout(() => { active = Math.max(0, active - 1) }, 900)
  }
}
