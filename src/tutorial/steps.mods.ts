import type { TutorialContext, TutorialStep } from './types'

/**
 * MOD 两页（`/mods-online` 安装、`/mods-local` 管理）的步骤。
 *
 * 用户对这两屏的要求是「把下载与管理各讲一遍」：安装页要讲清
 * 搜索 → 卡片 → 右侧抽屉 → 分段 bar → 手动选历史版本 → 安装按钮；
 * 管理页要讲清页头的状态标签、「全部启用 / 全部禁用」、检测更新、
 * 打开目录，最后指出一张本地卡片长什么样。
 *
 * ## 唯一需要 `prepare` 的段落
 *
 * 安装页的后四步目标全在右侧抽屉里，不先点开一张卡片就没有东西可指。
 * `prepare` 只做两件只读的事：点卡片（`ctx.click`）+ 等面板渲染出来
 * （`ctx.waitFor`）。**绝不点安装按钮**、不调写接口、不触发下载与刷新 ——
 * 教程跑一遍不能改变用户的安装状态。
 *
 * ## 选择器约定
 *
 * 页面里补 `data-tutorial="…"`，教程只认这些 id：
 * - 安装页：`mods-online-search`（页头搜索框）、`mods-online-card`
 *   （整棵树里的第一张仓库卡，页面只给第一张加属性）。
 * - 共享详情面板：`detail-panel`（`ReleaseDetailPanel.vue` 根元素，Lead 已加，
 *   地图下载页共用同一个）。面板**内部**那三个锚点不往组件里加属性，
 *   改为在面板根下查它自己的稳定类名 —— 面板是共享组件，教程不该为了
 *   指路去改动它的模板。
 * - 管理页：`mods-local-tags`（页头状态标签组）、`mods-local-bulk`
 *   （「全部启用 / 全部禁用」分段器）、`mods-local-actions`（检测更新）、
 *   `mods-local-open-dir`（打开 MOD 目录）。
 *
 * 管理页的卡片不在页面模板里：它由共享组件 `ModGroupBody` 渲染，
 * 所以 `mods-local-card` 用**函数 target** 查组件本来就会带在根元素上的
 * `data-expand-key`（展开状态的身份键），取文档顺序第一张。
 *
 * ## 文案
 *
 * 写在 `src/locale/tutorialMods.ts`（导出名 `tutorialMods`，域 `tutorial`），
 * 五语并排，key 形如 `onlineSearch.title` → 完整 tag `tutorial.mods.onlineSearch.title`。
 */

/** 共享详情面板的根元素（`ReleaseDetailPanel.vue` 上的 `data-tutorial="detail-panel"`）。 */
const DETAIL_PANEL = '[data-tutorial="detail-panel"]'
/** 安装页第一张仓库卡；只有整棵树的第一张带这个属性。 */
const ONLINE_CARD = '[data-tutorial="mods-online-card"]'
/**
 * 面板里的「下载」分段页签。
 *
 * 不能用 `:nth-child(2)`：`type="segment"` 的导航条里，胶囊内先有一个空的
 * `.n-tabs-wrapper` 占位，真实页签排在它后面，而且每个页签还套了一层
 * `.n-tabs-tab-wrapper`。Naive 把 pane 的 `name` 落在 `.n-tabs-tab` 的
 * `data-name` 上，按它取才是稳定的。
 */
const INSTALL_TAB = `${DETAIL_PANEL} .n-tabs-tab[data-name="install"]`

/**
 * 详情抽屉的固定高度容器。
 *
 * 面板根元素的高度由内容决定：停在「说明」页签时 README 一加载就长高，
 * 拿它当目标会让高亮框在抽屉里越撑越大。抽屉内容区是 Naive `Scrollbar`
 * 的 `.n-scrollbar-container` —— 它始终等于抽屉高度，滚动发生在它内部。
 *
 * 取不到（Naive 换了 DOM 结构、或面板没挂在抽屉里）就退回根元素：
 * 宁可框跟着内容长，也不要这一步没有目标。
 */
function detailPanelBox(): Element[] {
  const panel = document.querySelector(DETAIL_PANEL)
  if (!panel) return []
  const viewport = panel.closest('.n-drawer')?.querySelector('.n-scrollbar-container')
  return [viewport ?? panel]
}

/**
 * 点开第一张仓库卡，把右侧详情抽屉等出来。
 *
 * 抽屉已经开着就直接返回：来回翻步时上一步的 cleanup 未必跑过，
 * 重复点卡片只会让抽屉像个弹窗一样闪一下。
 */
async function openDetail(ctx: TutorialContext): Promise<void> {
  if (document.querySelector(DETAIL_PANEL)) return
  // 没有卡片（源还在联网读、或筛完为空）就别干等：等 1.5 秒只为让
  // 这一步的居中卡片晚一点出现，体验反而更差。
  if (!ctx.click(ONLINE_CARD)) return
  await ctx.waitFor(DETAIL_PANEL, 1500)
}

/**
 * 打开详情并切到「下载」页签。
 *
 * 面板默认停在「说明」，而 `n-tab-pane` 的 `display-directive` 是默认的
 * `if` —— 没切过去时 `.metric-version` 与 `.install-row` 根本不在 DOM 里，
 * 只等目标出现会永远等不到。
 */
async function openInstallTab(ctx: TutorialContext): Promise<void> {
  await openDetail(ctx)
  ctx.click(INSTALL_TAB)
  await ctx.waitFor(`${DETAIL_PANEL} .install-row`, 1200)
}

export const modsOnlineSteps: TutorialStep[] = [
  {
    id: 'mods-online-search',
    titleKey: 'tutorial.mods.onlineSearch.title',
    bodyKey: 'tutorial.mods.onlineSearch.body',
    route: 'mods-online',
    target: '[data-tutorial="mods-online-search"]',
    padding: 6,
  },
  {
    id: 'mods-online-card',
    titleKey: 'tutorial.mods.onlineCard.title',
    bodyKey: 'tutorial.mods.onlineCard.body',
    route: 'mods-online',
    // 页面只给整棵树的第一张卡加这个属性（`tutorialCardId`）：列表为空或
    // 源还在联网读时找不到它 —— 引擎退化成居中卡片继续讲，不 `optional`：
    // 2500 ms 的等待窗口正好留给首次拉仓库源。
    target: ONLINE_CARD,
    padding: 8,
  },
  {
    id: 'mods-online-detail',
    titleKey: 'tutorial.mods.onlineDetail.title',
    bodyKey: 'tutorial.mods.onlineDetail.body',
    route: 'mods-online',
    // 框住整个抽屉的滚动视口，而不是随 README 长高的面板根（见 detailPanelBox）。
    target: detailPanelBox,
    padding: 0,
    prepare: openDetail,
    // prepare 已经把面板等出来了，不必再干等默认的 2500 ms。`optional` 在
    // 这个引擎里只缩短等待窗口（400 ms），目标仍找不到就退化成居中卡片。
    optional: true,
  },
  {
    id: 'mods-detail-tabs',
    titleKey: 'tutorial.mods.detailTabs.title',
    bodyKey: 'tutorial.mods.detailTabs.body',
    route: 'mods-online',
    target: `${DETAIL_PANEL} .n-tabs-nav`,
    prepare: openDetail,
    optional: true,
  },
  {
    id: 'mods-detail-version',
    titleKey: 'tutorial.mods.detailVersion.title',
    bodyKey: 'tutorial.mods.detailVersion.body',
    route: 'mods-online',
    target: `${DETAIL_PANEL} .metric-version`,
    prepare: openInstallTab,
    optional: true,
  },
  {
    id: 'mods-detail-install',
    titleKey: 'tutorial.mods.detailInstall.title',
    bodyKey: 'tutorial.mods.detailInstall.body',
    route: 'mods-online',
    target: `${DETAIL_PANEL} .install-row`,
    prepare: openInstallTab,
    optional: true,
    /*
     * 离开安装页把抽屉关掉：下一步要换到管理页，抽屉留着会盖住新页面，
     * 用户往回翻也会叠成一个「一直开着」的抽屉。`.detail-close` 是面板头部
     * 的关闭按钮（`emit('close')` → 宿主把 `detailId` 置空）。
     * 放在最后一步而不是 `mods-online-detail`：中间三步都还要指着面板。
     */
    cleanup: (ctx) => {
      ctx.click(`${DETAIL_PANEL} .detail-close`)
    },
  },
]

/** 管理页（`/mods-local`）的步骤，接在安装页后面。 */
export const modsLocalSteps: TutorialStep[] = [
  {
    id: 'mods-local-tags',
    titleKey: 'tutorial.mods.localTags.title',
    bodyKey: 'tutorial.mods.localTags.body',
    route: 'mods-local',
    target: '[data-tutorial="mods-local-tags"]',
    padding: 4,
  },
  {
    id: 'mods-local-bulk',
    titleKey: 'tutorial.mods.localBulk.title',
    bodyKey: 'tutorial.mods.localBulk.body',
    route: 'mods-local',
    // 分段器只在有本地 MOD 时渲染（`v-if="effectiveMods.length"`）；
    // 空列表时退化成居中卡片，文案本身能独立看懂。
    target: '[data-tutorial="mods-local-bulk"]',
    padding: 4,
  },
  {
    id: 'mods-local-actions',
    titleKey: 'tutorial.mods.localActions.title',
    bodyKey: 'tutorial.mods.localActions.body',
    route: 'mods-local',
    target: '[data-tutorial="mods-local-actions"]',
  },
  {
    id: 'mods-local-open-dir',
    titleKey: 'tutorial.mods.localOpenDir.title',
    bodyKey: 'tutorial.mods.localOpenDir.body',
    route: 'mods-local',
    target: '[data-tutorial="mods-local-open-dir"]',
  },
  {
    id: 'mods-local-card',
    titleKey: 'tutorial.mods.localCard.title',
    bodyKey: 'tutorial.mods.localCard.body',
    route: 'mods-local',
    /*
     * 卡片由共享组件 `ModGroupBody` 渲染，页面模板里没有它的 v-for，
     * 所以按它本来就会带在根元素上的 `data-expand-key` 取文档顺序第一个
     * 展示单元：单个 MOD 是卡片本身，多 DLL 的包是一个 `.package-shell`。
     * 不去改共享组件，也不能给每张卡加同名 `data-tutorial` —— 重复 key
     * 会让 `querySelector` 落到哪一张取决于 DOM 顺序，等于没说清楚。
     */
    target: () => document.querySelector('[data-expand-key]'),
    padding: 8,
  },
]
