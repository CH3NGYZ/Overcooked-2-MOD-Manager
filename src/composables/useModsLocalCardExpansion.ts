import { nextTick, onBeforeUnmount, onMounted } from 'vue'
import {
  cardByExpandKey,
  collapseCardWithScroll,
  naturalContentHeight,
  scrollExpandedCardIntoView,
} from '../expandScroll'
import { useCardExpansion } from './useCardExpansion'
import type { InstalledMod } from '../types'
import type { ModPackageNode } from '../components/levelSetGroupContext'

export function useModsLocalCardExpansion() {
  const expansion = useCardExpansion({ singleOuter: true })
  /** 独立 MOD 卡的外层键前缀：点空白只收独立卡、不收盘外壳，靠它区分。 */
  const MOD_CARD_PREFIX = 'mod:'
  function modCardKey(dllFile: string) {
    return `${MOD_CARD_PREFIX}${dllFile}`
  }

  /**
   * 内层卡片展开后的滚动（策略见 `expandScroll.ts`）。
   *
   * 内层展开不会被动收起别的卡片（外层互斥只发生在外层），所以不需要收缩补偿，
   * 展开后单独量一次就够。
   */
  function scrollAfterExpand(card: HTMLElement | null | undefined, anchorSelector: string, bodySelector: string) {
    if (!card) return
    const anchor = card.querySelector(anchorSelector)
    if (!anchor) return
    void nextTick().then(() => {
      scrollExpandedCardIntoView({ card, anchor, body: card.querySelector(bodySelector) })
    })
  }

  /** 一张外层卡片的内容层：包外壳是 `.package-body`，独立卡是 `.card-detail`。 */
  function bodyOfOuterCard(card: HTMLElement | null) {
    if (!card) return null
    return card.classList.contains('package-shell')
      ? card.querySelector('.package-body')
      : card.querySelector('.card-detail')
  }

  /**
   * 展开一张外层卡片：单展开下上一张会被动收起，先把它的收缩量量好再改状态。
   *
   * `shrink` 必须在改状态之前量：外层收起会连带清掉它的内层展开态，改完再量，
   * 内层已经率先塌掉，收缩量被算少 —— 下方卡片的标题会被上方收起动画一起
   * 拽向视口顶（与 `expandScroll.ts` 里同一个 247px 量级的坑）。
   *
   * 滚动只在这一处发生：调用点不许再补一次 `scrollExpandedCardIntoView`，
   * 那样会把同一次展开滚两遍。
   */
  function expandWithScroll(
    card: HTMLElement | null | undefined,
    key: string,
    anchorSelector: string,
    bodySelector: string,
    open: () => void,
  ) {
    const previousKey = expansion.outerKeys.value.find((openKey) => openKey !== key)
    const previousCard = previousKey ? cardByExpandKey(previousKey) : null
    const previousBody = bodyOfOuterCard(previousCard)
    const shrink = previousBody ? naturalContentHeight(previousBody) : 0
    open()
    void nextTick().then(() => {
      // 跨页 focus 展开时卡片可能刚随数据渲染出来，调用点拿不到元素，这里按键补找
      const target = card ?? cardByExpandKey(key)
      const anchor = target?.querySelector(anchorSelector)
      if (!target || !anchor) return
      scrollExpandedCardIntoView({
        card: target,
        anchor,
        body: target.querySelector(bodySelector),
        collapsing: previousCard ? { card: previousCard, body: previousBody, shrink } : null,
      })
    })
  }

  /**
   * 收起一张卡片：把状态改动交给 `collapseCardWithScroll` 里的回调。
   *
   * 顺序由它保证：先同步量折叠前的位置，再改状态让详情开始折，最后把展开那次滚动
   * 倒回去（两者同时发生）。调用方自己先改状态的话，量到的就是折叠后的位置。
   */
  function collapseWithScroll(card: HTMLElement | null | undefined, bodySelector: string, collapse: () => void) {
    if (!card) {
      collapse()
      return
    }
    collapseCardWithScroll({ card, body: card.querySelector(bodySelector) }, collapse)
  }

  /**
   * 点击卡片顶栏展开/收起详情。
   *
   * 详情区不再阻止冒泡（点它就是「点空白」，由 onDocumentClick 收起），
   * 所以这里必须排除来自 `.card-detail` 的事件，否则点详情会先被
   * onCardClick 切换一次、再被文档监听收起，表现为「闪一下就没了」。
   *
   * 同时要告诉 onDocumentClick「这次点击已处理」：否则同一次点击里
   * 本函数先把该项 toggle 开，文档监听紧接着把所有展开项置回 false，
   * 内容只出现一帧就消失（折叠过渡动画还没播完就被打断）。
   */
  let cardClickHandled = false

  function onCardClick(event: MouseEvent, mod: InstalledMod, packageKey?: string) {
    const target = event.target
    if (target instanceof Element && target.closest('.card-detail')) return
    cardClickHandled = true
    const card = event.currentTarget instanceof HTMLElement ? event.currentTarget : null
    /*
     * 独立卡是外层（与包外壳同一级、互斥），包内子卡是内层（父键是包 key）；
     * 传了包 key 说明这是包内子卡，否则是独立卡。两侧交互一致：展开时滚到
     * 可见，收起时把展开那次滚动倒回去。
     */
    if (packageKey === undefined) {
      const key = modCardKey(mod.dll_file)
      if (expansion.isOuterOpen(key)) {
        collapseWithScroll(card, '.card-detail', () => expansion.closeOuter(key))
        return
      }
      expandWithScroll(card, key, '.card-row', '.card-detail', () => expansion.openOuter(key))
      return
    }
    if (expansion.isInnerOpen(packageKey, mod.dll_file)) {
      collapseWithScroll(card, '.card-detail', () => expansion.closeInner(packageKey, mod.dll_file))
      return
    }
    expansion.openInner(packageKey, mod.dll_file)
    scrollAfterExpand(card, '.card-row', '.card-detail')
  }

  /**
   * 点击详情之外的空白收起已展开的 MOD。
   *
   * 「空白」的范围包含详情区自身：用户展开卡片后，点详情里的文字/标签/值
   * 也算点空白，应当收起（详情内容是只读的，没有需要保留点击的元素）。
   * 判定规则是「点击目标是否属于本行顶栏」：
   *   - `.card-row`（单 MOD 行的开关/标题/标签/工具按钮）
   *   - `.package-head`（包标题行）
   *   - `.tool-group` / `.card-switch`（按钮与开关自身）
   * 落在这些里面说明用户是在操作这一行，交给各自的 @click.stop / 处理函数，
   * 不在这里收起，避免一次点击既展开又收起。
   *
   * 收起后只有「同时只展开了这一张」时把展开那次滚动倒回去：一次收掉多张时
   * 没有哪一张是用户正在看的那张，滚动只会把视图甩到别处。
   */
  function onDocumentClick(event: MouseEvent) {
    /*
     * 卡片自身的点击已由 onCardClick 处理（它会 toggle），
     * 这里必须让路，否则同一次点击会「先展开、再全部收起」。
     * 标记在 document 监听执行后清除，保证只影响当前这一次点击。
     */
    if (cardClickHandled) {
      cardClickHandled = false
      return
    }
    /*
     * 「点空白收起」的范围只到独立卡与全部内层：包外壳的展开 / 收起由标题行
     * 自己的点击控制（`togglePackage`），点包 body 里的空白把整包装收掉会让
     * 用户以为自己点错了位置 —— 这条与改动前一致。外层键用前缀区分，逐个走
     * `closeOuterWhere`（它连带清内层），页面不自己删键。
     */
    const openCards = [
      ...expansion.outerKeys.value
        .filter((key) => key.startsWith(MOD_CARD_PREFIX))
        .map((key) => key.slice(MOD_CARD_PREFIX.length)),
      ...expansion.innerKeys.value.map((key) => expansion.childOf(key)),
    ]
    if (!openCards.length) return
    const target = event.target
    if (target instanceof Element && target.closest('.card-row, .package-head')) return
    const collapseAll = () => {
      expansion.closeOuterWhere((key) => key.startsWith(MOD_CARD_PREFIX))
      expansion.closeAllInner()
    }
    if (openCards.length !== 1) {
      collapseAll()
      return
    }
    const card = document.querySelector<HTMLElement>(`[data-dll-file="${CSS.escape(openCards[0])}"]`)
    collapseWithScroll(card, '.card-detail', collapseAll)
  }
  /**
   * 包外壳的展开 / 收起。
   *
   * 滚动策略与独立 MOD 卡片同一套（`expandWithScroll` → `expandScroll.ts`）：
   * 包 body 撑开后优先让整包可见，收起时把展开那次滚动倒回去。收起必须交给
   * `collapseWithScroll`：它先量折叠前的位置再改状态，自己先改就量不到了。
   */
  function togglePackage(node: ModPackageNode, event: MouseEvent) {
    const head = event.currentTarget
    const shell = head instanceof Element ? head.closest('.package-shell') : null
    const card = shell instanceof HTMLElement ? shell : null
    if (expansion.isOuterOpen(node.key)) {
      // 收起连带清掉包内子卡的展开状态（`useCardExpansion` 的规则）：
      // 下次展开是一个全新的包，子卡要重新点开才展开。
      collapseWithScroll(card, '.package-body', () => expansion.closeOuter(node.key))
      return
    }
    expandWithScroll(card, node.key, '.package-head', '.package-body', () => expansion.openOuter(node.key))
  }

  onMounted(() => {
    document.addEventListener('click', onDocumentClick)
  })
  onBeforeUnmount(() => {
    document.removeEventListener('click', onDocumentClick)
  })

  return { expansion, modCardKey, onCardClick, togglePackage }
}
