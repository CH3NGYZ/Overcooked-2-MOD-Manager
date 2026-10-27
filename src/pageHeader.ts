import { ref } from 'vue'

/**
 * 页头（页面标题行）的状态与挂载点。
 *
 * 页头固定不动（不参与页面切换动画），有两个可被页面填充的区域，
 * 页面用 Vue 的 `<Teleport :to="'#' + PAGE_HEADER_XXX_ID">` 把内容送进去：
 *
 *   PAGE_HEADER_TITLE_EXTRA_ID  h1 右侧的补充信息区（窄，适放标签）
 *   PAGE_HEADER_ACTIONS_ID      整行最右侧的操作区（适放按钮/搜索框）
 *
 * 两个容器的布局规则在 `styles/global.css`：
 *   - 操作区强制不换行，窗口真的不够宽时整块换到第二行；
 *     行内只允许声明了 `min-width: 0` 的项（搜索框）收缩，
 *     固定文案的项写 `white-space: nowrap`，页面无需自己算换行。
 *
 * 另外提供「标题覆盖」：同一路由下的多阶段页面（如改多彩名 →
 * 开始自定义）用 setPageTitleOverride 换标题文字，App 会做淡入淡出。
 * 路由切换时 App 调用 resetPageHeader() 复位，避免状态泄漏。
 */
export const PAGE_HEADER_TITLE_EXTRA_ID = 'page-header-title-extra'
export const PAGE_HEADER_ACTIONS_ID = 'page-header-actions'

export const pageTitleOverride = ref<string | null>(null)

export function setPageTitleOverride(title: string | null) {
  pageTitleOverride.value = title
}

/** 路由切换时复位全部页头状态。 */
export function resetPageHeader() {
  pageTitleOverride.value = null
}
