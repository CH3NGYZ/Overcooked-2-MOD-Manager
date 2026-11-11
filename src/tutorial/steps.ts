import type { TutorialSection } from './types'
import { chromeSteps } from './steps.chrome'
import { sidebarNavSteps } from './steps.navigation'
import { overviewSteps } from './steps.overview'
import { modsLocalSteps, modsOnlineSteps } from './steps.mods'
import { settingsSteps } from './steps.settings'
import { sponsorSteps, toolsSteps } from './steps.tools'
import { diyChefSteps } from './steps.diychef'
import { mapsOnlineSteps, levelsSteps } from './steps.maps'
import { renameSteps } from './steps.rename'

/**
 * 内置教程的全部步骤，顺序即播放顺序。
 *
 * 每页先高亮侧栏、说明用途，下一步才切到该页内容。按侧栏顺序经过总览、
 * MOD、地图、改多彩名、暖暖，最后是工具与设置。地图卡片复用 MOD 的操作
 * 模式，每页只概述一次；工具仍先介绍支持作者，再讲维护动作。
 * 页面步骤和导航步骤分别维护，这里只编排顺序。旧 id 保持稳定以合并已看进度。
 */
export const tutorialSections: TutorialSection[] = [
  { id: 'chrome', titleKey: 'tutorial.manager.basics', steps: chromeSteps },
  { id: 'overview', titleKey: 'nav.overview', steps: [sidebarNavSteps.overview, ...overviewSteps] },
  { id: 'mods-online', titleKey: 'nav.modInstall', steps: [sidebarNavSteps.modsOnline, ...modsOnlineSteps] },
  { id: 'mods-local', titleKey: 'nav.modManager', steps: [sidebarNavSteps.modsLocal, ...modsLocalSteps] },
  { id: 'maps-online', titleKey: 'levelSet.download', steps: [sidebarNavSteps.mapsOnline, ...mapsOnlineSteps] },
  { id: 'levels', titleKey: 'levelSet.title', steps: [sidebarNavSteps.levels, ...levelsSteps] },
  { id: 'rename', titleKey: 'rename.colorfulName', steps: [sidebarNavSteps.rename, ...renameSteps] },
  { id: 'diychef', titleKey: 'diychef.title', steps: [sidebarNavSteps.diychef, ...diyChefSteps] },
  { id: 'tools', titleKey: 'nav.tools', steps: [sidebarNavSteps.tools, ...sponsorSteps, ...toolsSteps] },
  { id: 'settings', titleKey: 'nav.settings', steps: [sidebarNavSteps.settings, ...settingsSteps] },
]

export const tutorialSteps = tutorialSections.flatMap(section => section.steps)
