import { isMacWindow } from '../platform'
import type { TutorialStep } from './types'

// 先让管理器找到游戏与框架，再讲通用、来源、下载和本地处理性能。
// 步骤只指向相应行或分组，不触发切源、保存或框架操作。
export const settingsSteps: TutorialStep[] = [
  {
    id: 'settings-game-runtime',
    titleKey: 'tutorial.settings.gameRuntime.title',
    bodyKey: 'tutorial.settings.gameRuntime.body',
    target: '[data-tutorial="settings-game-runtime"]',
    route: 'settings',
    // 只有 macOS 有「原生 / CrossOver」两种运行时；Windows 上这一行不存在。
    when: isMacWindow,
  },
  {
    id: 'settings-game-path',
    titleKey: 'tutorial.settings.gamePath.title',
    bodyKey: 'tutorial.settings.gamePath.body',
    target: '[data-tutorial="settings-game-path"]',
    route: 'settings',
  },
  {
    id: 'settings-bepinex',
    titleKey: 'tutorial.settings.bepinex.title',
    bodyKey: 'tutorial.settings.bepinex.body',
    target: '[data-tutorial="settings-bepinex"]',
    route: 'settings',
  },
  {
    id: 'settings-diylevel',
    titleKey: 'tutorial.settings.diylevel.title',
    bodyKey: 'tutorial.settings.diylevel.body',
    target: '[data-tutorial="settings-diylevel"]',
    route: 'settings',
  },
  {
    id: 'settings-switches',
    titleKey: 'tutorial.settings.switches.title',
    bodyKey: 'tutorial.settings.switches.body',
    target: '[data-tutorial="settings-switches"]',
    route: 'settings',
  },
  {
    id: 'settings-data-sources',
    titleKey: 'tutorial.settings.dataSources.title',
    bodyKey: 'tutorial.settings.dataSources.body',
    target: '[data-tutorial="settings-data-sources"]',
    route: 'settings',
  },
  {
    id: 'settings-download',
    titleKey: 'tutorial.settings.download.title',
    bodyKey: 'tutorial.settings.download.body',
    target: '[data-tutorial="settings-download"]',
    route: 'settings',
  },
  {
    id: 'settings-performance',
    titleKey: 'tutorial.settings.performance.title',
    bodyKey: 'tutorial.settings.performance.body',
    target: '[data-tutorial="settings-performance"]',
    route: 'settings',
  },
]
