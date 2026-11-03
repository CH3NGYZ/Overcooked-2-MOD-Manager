import type { TutorialStep } from './types'

/**
 * 工具页（`/tools`）与收款二维码的步骤。
 *
 * 二维码**排在工具页的最前面**：用户要求先看到「能不能支持作者」，再讲
 * 日志、回收站、缓存这些维护动作。它不点任何按钮，正文只是道谢，别写成广告。
 *
 * 工具页四步**没有任何 `prepare`** —— 组本身一直渲染，不需要展开或切 tab；
 * 教程只把位置指出来，不点「清空日志」「彻底删除」这类不可逆按钮，也不调
 * 任何写操作（`AGENTS.md` 第 9 节、`.tmp/tutorial-contract.md` 第 7 节）。
 *
 * 「高级」组里另有一个入口：**重新播放教程**的按钮（`replayTutorial()`）。
 * 教程只自动播一次，之后用户得能找到它 —— `tools-advanced` 这一步的高亮框
 * 覆盖整组，正好把那一行也框进去，正文里点一句「点右边按钮」。
 *
 * ## 选择器约定（页面里补 `data-tutorial="…"`）
 *
 * `tools-logs`（日志三行那一组）、`tools-recycle`（回收站那一组）、
 * `tools-cache`（缓存清理那一组）、`tools-advanced`（高级那一组）、
 * `tools-sponsor`（收款二维码卡片）。
 *
 * ## 文案
 *
 * 写在 `src/locale/tutorialTools.ts`（导出名 `tutorialTools`，域 `tutorial`，
 * key 前缀 `tutorial.tools.` / `tutorial.sponsor.`），五语并排。
 */
export const toolsSteps: TutorialStep[] = [
  {
    id: 'tools-logs',
    titleKey: 'tutorial.tools.logs.title',
    bodyKey: 'tutorial.tools.logs.body',
    target: '[data-tutorial="tools-logs"]',
    route: 'tools',
    // 组标题与第一行之间没有留白，外扩 4px 才把标题一起框进来。
    padding: 4,
  },
  {
    id: 'tools-recycle',
    titleKey: 'tutorial.tools.recycle.title',
    bodyKey: 'tutorial.tools.recycle.body',
    target: '[data-tutorial="tools-recycle"]',
    route: 'tools',
    padding: 4,
  },
  {
    id: 'tools-cache',
    titleKey: 'tutorial.tools.cache.title',
    bodyKey: 'tutorial.tools.cache.body',
    target: '[data-tutorial="tools-cache"]',
    route: 'tools',
    padding: 4,
  },
  {
    id: 'tools-advanced',
    titleKey: 'tutorial.tools.advanced.title',
    bodyKey: 'tutorial.tools.advanced.body',
    target: '[data-tutorial="tools-advanced"]',
    route: 'tools',
    padding: 4,
  },
]

/**
 * 工具页的第一步：收款二维码。
 *
 * 在日志/回收站/缓存那四步之前播 —— 先让用户知道这个管理器是有人维护的，
 * 再讲维护动作。目标不存在时（图片没加载/分组没渲染）退化成居中卡片，
 * 文案本身已经说清了它讲的是什么。
 */
export const sponsorSteps: TutorialStep[] = [
  {
    id: 'tools-sponsor',
    titleKey: 'tutorial.sponsor.title',
    bodyKey: 'tutorial.sponsor.body',
    target: '[data-tutorial="tools-sponsor"]',
    route: 'tools',
    // 卡片自己有 1px 边框，外扩 4px 让描边落在卡片外面而不是压在边框上。
    padding: 4,
  },
]
