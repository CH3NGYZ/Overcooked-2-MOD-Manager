import type { TutorialStep } from './types'

/**
 * 骨架与标题栏的步骤（教程的前半段）。
 *
 * 这一段的顺序是有讲究的：先解释「你正看着的是什么应用」，再指向侧栏
 * （后面每一步都要靠侧栏换页），最后讲标题栏那五颗全局设置 —— 用户后面
 * 无论学到哪一页，调缩放、换语言、换主题色的入口都已经知道了。
 *
 * 标题栏的五颗按钮属于「一读就懂、但没人告诉你它叫什么」的区域，
 * 所以正文里都写清「点下去会发生什么」，而不是复述按钮标签。
 */
export const chromeSteps: TutorialStep[] = [
  {
    id: 'intro',
    titleKey: 'tutorial.intro.title',
    // 首屏没有具体目标：居中卡片先讲清「这是教程、可以随时退出」。
    // 卡片必须能一眼看到「跳过」和「下一步在哪」，所以不用挖洞。
    bodyKey: 'tutorial.intro.body',
    cardWidth: 430,
    placement: 'center',
  },
  {
    id: 'sidebar',
    titleKey: 'tutorial.chrome.sidebar.title',
    bodyKey: 'tutorial.chrome.sidebar.body',
    target: '[data-tutorial="app-sidebar"]',
    // 侧栏是一整列窄条，卡片放右边会盖住内容区，强制放左侧会出屏。
    // 交给 auto：右侧空间最大，会落在内容区左边缘，正好不挡高亮。
    placement: 'right',
    padding: 0,
  },
  {
    id: 'titlebar-game',
    titleKey: 'tutorial.chrome.game.title',
    bodyKey: 'tutorial.chrome.game.body',
    target: '[data-tutorial="titlebar-game-toggle"]',
    padding: 8,
  },
  {
    id: 'titlebar-zoom',
    titleKey: 'tutorial.chrome.zoom.title',
    bodyKey: 'tutorial.chrome.zoom.body',
    target: '[data-tutorial="titlebar-zoom"]',
    padding: 8,
  },
  {
    id: 'titlebar-language',
    titleKey: 'tutorial.chrome.language.title',
    bodyKey: 'tutorial.chrome.language.body',
    target: '[data-tutorial="titlebar-language"]',
    padding: 8,
  },
  {
    id: 'titlebar-theme-mode',
    titleKey: 'tutorial.chrome.themeMode.title',
    bodyKey: 'tutorial.chrome.themeMode.body',
    target: '[data-tutorial="titlebar-theme-mode"]',
    padding: 8,
  },
  {
    id: 'titlebar-accent',
    titleKey: 'tutorial.chrome.accent.title',
    bodyKey: 'tutorial.chrome.accent.body',
    target: '[data-tutorial="titlebar-accent"]',
    padding: 8,
  },
]

/**
 * 「下一页在侧栏哪一格」的过渡步骤。
 *
 * 三段主流程（MOD 安装 → MOD 管理 → 地图）换页时，先停一秒把高亮挪到侧栏
 * 对应的格子，用户看到的就不只是「页面忽然变了」，而是「我点这一格会去哪」。
 * 这是教程里唯一一组**不指内容、只指导航**的步骤，所以和标题栏那组分开维护。
 *
 * `route` 是**当前所在页**，不是目标页：高亮出现在用户还没离开的那一页上，
 * 下一步的 `route` 才负责跳转。地图两格是例外 —— 那两页没有内容步骤了，
 * 高亮和跳转必须在同一步完成，所以 `route` 直接写目标页。
 */
export const sidebarNavSteps: TutorialStep[] = [
  {
    id: 'sidebar-mods-online',
    titleKey: 'tutorial.chrome.modsOnline.title',
    bodyKey: 'tutorial.chrome.modsOnline.body',
    target: '[data-tutorial="nav-mods-online"]',
    route: 'overview',
    placement: 'right',
    padding: 0,
  },
  {
    id: 'sidebar-mods-local',
    titleKey: 'tutorial.chrome.modsLocal.title',
    bodyKey: 'tutorial.chrome.modsLocal.body',
    target: '[data-tutorial="nav-mods-local"]',
    route: 'mods-online',
    placement: 'right',
    padding: 0,
  },
  {
    id: 'sidebar-maps-online',
    titleKey: 'tutorial.chrome.mapsOnline.title',
    bodyKey: 'tutorial.chrome.mapsOnline.body',
    target: '[data-tutorial="nav-maps-online"]',
    route: 'maps-online',
    placement: 'right',
    padding: 0,
  },
  {
    id: 'sidebar-levels',
    titleKey: 'tutorial.chrome.levels.title',
    bodyKey: 'tutorial.chrome.levels.body',
    target: '[data-tutorial="nav-levels"]',
    route: 'levels',
    placement: 'right',
    padding: 0,
  },
]
