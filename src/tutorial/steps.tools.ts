import type { TutorialStep } from './types'

// 只介绍交流与本地维护。引导不打开群链接、发布地图或触发数据操作。
export const toolsSteps: TutorialStep[] = [
  {
    id: 'tools-feedback',
    titleKey: 'tutorial.tools.feedback.title',
    bodyKey: 'tutorial.tools.feedback.body',
    target: '[data-tutorial="tools-feedback"]',
    route: 'tools',
    padding: 4,
  },
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
    id: 'tools-data-migration',
    titleKey: 'tutorial.tools.dataMigration.title',
    bodyKey: 'tutorial.tools.dataMigration.body',
    target: '[data-tutorial="tools-data-migration"]',
    route: 'tools',
    padding: 4,
  },
  {
    id: 'tools-data-reset',
    titleKey: 'tutorial.tools.dataReset.title',
    bodyKey: 'tutorial.tools.dataReset.body',
    target: '[data-tutorial="tools-data-reset"]',
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

// 工具章先介绍支持开发，再介绍交流和维护入口。
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
