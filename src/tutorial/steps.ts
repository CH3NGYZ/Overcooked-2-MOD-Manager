import type { TutorialStep } from './types'
import { chromeSteps, sidebarNavSteps } from './steps.chrome'
import { overviewSteps } from './steps.overview'
import { modsLocalSteps, modsOnlineSteps } from './steps.mods'
import { settingsSteps } from './steps.settings'
import { sponsorSteps, toolsSteps } from './steps.tools'

/**
 * 内置教程的全部步骤，顺序即播放顺序。
 *
 * 排列按用户实际会走的路：先认清骨架（标题栏 / 侧栏），再一页一页往下讲
 * （总览 → MOD 安装 → MOD 管理 → 地图 → 工具 → 设置）。三段主流程换页前，
 * 先插一步把高亮挪到侧栏对应的格子（`sidebarNavSteps` 的前两个）—— 用户
 * 看到的是「点这一格会去哪」，而不是页面忽然变了。
 *
 * 地图两页只留「高亮侧栏标签 + 切过去」两步：它与 MOD 两页是同构的
 * （同一套卡片、同一套展开与开关），重复讲一遍只会让教程变长。工具页先讲
 * 收款码再讲四组维护动作（用户要求先看到「能不能支持作者」）。设置页把两个
 * 框架行拆成两步紧跟游戏目录，macOS 的「游戏类型」排在最前（只在 mac 上播）。
 *
 * 各段内容在 `steps.<域>.ts` 里，每一段都只写自己那几屏的步骤与文案 key；
 * 这里只负责顺序。新增一段时同时改这个数组与它自己的文件。
 */
// 这四个「导航过渡」步骤要插在主流程中间，按名字取出来，别用下标。
const [sidebarModsOnline, sidebarModsLocal, sidebarMapsOnline, sidebarLevels] = sidebarNavSteps

export const tutorialSteps: TutorialStep[] = [
  ...chromeSteps,
  ...overviewSteps,
  sidebarModsOnline,
  ...modsOnlineSteps,
  sidebarModsLocal,
  ...modsLocalSteps,
  sidebarMapsOnline,
  sidebarLevels,
  ...sponsorSteps,
  ...toolsSteps,
  ...settingsSteps,
]
