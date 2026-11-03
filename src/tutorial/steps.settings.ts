import { isMacWindow } from '../platform'
import type { TutorialStep } from './types'

/**
 * 设置页（`/settings`）的步骤。
 *
 * 这一页全是「改一次、长期生效」的路径与开关，所以顺序不是页面从上到下的
 * 顺序，而是**按用户理解的因果**排的：先给方向（管理器凭什么找到游戏，
 * macOS 上还要先决定原生还是 CrossOver），再讲会长期改变行为的开关，
 * 然后是只影响速度不影响结果的并发。启动时的联网检查排在开关前：它是
 * 「更新从哪来」的前提，界面上也就在开关上面。
 *
 * 游戏目录之后紧接着是两个框架行：这两行是「装完游戏就要处理」的东西，
 * 而且就在游戏目录下面，分别讲一步比合成一个框更清楚。macOS 的「游戏类型」
 * 只在 macOS 上出现（`when` 在开始时求值一次），其他平台直接跳过。
 *
 * 教程在这一页**只讲解、不操作**：提权重启会弹 UAC、卸载框架会连带删掉整套 MOD，
 * 都属于 `.tmp/tutorial-contract.md` 第 7 节明令禁止的副作用。正文里提到的按钮
 * 只用来指路，点不点是用户的事。所以这些步骤都没有 `prepare` / `cleanup`。
 *
 * 目标全部是页面上的常驻分组与常驻行（没有被 `v-if` 挡掉的分支），因此都不用
 * `optional`：万一真的量不到，引擎退化成居中卡片，教程照样讲得下去。
 */
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
    id: 'settings-update',
    titleKey: 'tutorial.settings.update.title',
    bodyKey: 'tutorial.settings.update.body',
    target: '[data-tutorial="settings-update"]',
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
    id: 'settings-performance',
    titleKey: 'tutorial.settings.performance.title',
    bodyKey: 'tutorial.settings.performance.body',
    target: '[data-tutorial="settings-performance"]',
    route: 'settings',
  },
]
