import type { TutorialStep } from './types'

/**
 * 总览页（`/`，`OverviewView.vue`）的步骤。
 *
 * 要讲的是「首页这两列卡片分别代表什么」：左列是 MOD 链路，右列是地图链路，
 * 每列三张卡各管一段状态（框架 / 在线仓库 / 本地磁盘）。用户的原话是
 * 「几个卡片都代表什么」，所以一卡一步，别把三张卡塞进一句话里。
 *
 * ## 顺序
 *
 * 先讲一整列有哪些卡（一列一步），再从上到下逐张讲，讲完左列换右列。
 * 逐张讲的顺序就是卡片在页面上的顺序，也是实际操作顺序：框架没装，
 * 下面两张卡的事都无从谈起。
 *
 * ## 选择器约定（页面里补 `data-tutorial="…"`，教程只认这些 id）
 *
 * 左列：`overview-column-mods`、`overview-card-bepinex`、`overview-card-online-mods`、
 * `overview-card-local-mods`。
 * 右列：`overview-column-maps`、`overview-card-diy-level`、`overview-card-online-maps`、
 * `overview-card-local-maps`。
 *
 * 卡片是 `StatCard`（共享组件），**不要**在 `StatCard` 里写死 id —— 在页面
 * 调用处传 `data-tutorial`（属性会透传到根元素），否则 MOD 两页的卡会撞 id。
 * `StatCard` 的单根是 `n-card`，它没有关掉属性继承，`data-tutorial` 会落到
 * 卡片自己的 `.n-card` 上，高亮框因此正好框住一张卡。
 *
 * ## 为什么没有 prepare / cleanup
 *
 * 首页只展示状态、不提供安装入口：数据在 `onMounted` 就已经读好，卡片
 * 不需展开也不用切页。没有准备动作，也就不可能误触安装、删除或开关
 * （教程不允许产生副作用）。
 *
 * ## 目标会不会消失
 *
 * 不会。两张列卡片无论数据有没有回来都渲染（数值显示 0 / 未安装 / 「—」），
 * 所以这 8 步不需要 `optional`。唯一例外是 dashboard 整体加载报错时整块
 * `.overview-columns` 不渲染，那时按引擎默认规则退化成居中卡片继续讲 ——
 * 让用户知道首页本来该有这两列，比静默跳过这一段更有用。
 *
 * ## 文案
 *
 * 写在 `src/locale/tutorialOverview.ts`（导出名 `tutorialOverview`，
 * 域 `tutorial`，key 前缀 `tutorial.overview.`），五语并排。
 */
export const overviewSteps: TutorialStep[] = [
  {
    id: 'overview-column-mods',
    titleKey: 'tutorial.overview.columnMods.title',
    bodyKey: 'tutorial.overview.columnMods.body',
    target: '[data-tutorial="overview-column-mods"]',
    route: 'overview',
  },
  {
    id: 'overview-card-bepinex',
    titleKey: 'tutorial.overview.bepinex.title',
    bodyKey: 'tutorial.overview.bepinex.body',
    target: '[data-tutorial="overview-card-bepinex"]',
    route: 'overview',
  },
  {
    id: 'overview-card-online-mods',
    titleKey: 'tutorial.overview.onlineMods.title',
    bodyKey: 'tutorial.overview.onlineMods.body',
    target: '[data-tutorial="overview-card-online-mods"]',
    route: 'overview',
  },
  {
    id: 'overview-card-local-mods',
    titleKey: 'tutorial.overview.localMods.title',
    bodyKey: 'tutorial.overview.localMods.body',
    target: '[data-tutorial="overview-card-local-mods"]',
    route: 'overview',
  },
  {
    id: 'overview-column-maps',
    titleKey: 'tutorial.overview.columnMaps.title',
    bodyKey: 'tutorial.overview.columnMaps.body',
    target: '[data-tutorial="overview-column-maps"]',
    route: 'overview',
  },
  {
    id: 'overview-card-diy-level',
    titleKey: 'tutorial.overview.diyLevel.title',
    bodyKey: 'tutorial.overview.diyLevel.body',
    target: '[data-tutorial="overview-card-diy-level"]',
    route: 'overview',
  },
  {
    id: 'overview-card-online-maps',
    titleKey: 'tutorial.overview.onlineMaps.title',
    bodyKey: 'tutorial.overview.onlineMaps.body',
    target: '[data-tutorial="overview-card-online-maps"]',
    route: 'overview',
  },
  {
    id: 'overview-card-local-maps',
    titleKey: 'tutorial.overview.localMaps.title',
    bodyKey: 'tutorial.overview.localMaps.body',
    target: '[data-tutorial="overview-card-local-maps"]',
    route: 'overview',
  },
]
