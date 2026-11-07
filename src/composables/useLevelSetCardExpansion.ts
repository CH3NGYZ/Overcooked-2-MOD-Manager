import { nextTick, onBeforeUnmount, onMounted } from 'vue'
import { collapseCardWithScroll, naturalContentHeight, scrollExpandedCardIntoView } from '../expandScroll'
import { useCardExpansion } from './useCardExpansion'
import type { LevelSetInfo, LevelSetPlugin } from '../types'

export function useLevelSetCardExpansion() {
  const levelExpansion = useCardExpansion({ singleOuter: true })

  /**
   * 卡片行点击：展开自己，或收起已展开的那张。
   *
   * 详情区不阻止冒泡（点详情里的文字/值等于「点空白」，由 `onDocumentClick`
   * 收起），所以必须让文档监听知道这一次点击已经处理过：否则同一次点击里
   * 这里先把该项 toggle 开，文档监听紧接着把它置回 `null`，内容只出现一帧
   * ——折叠过渡动画还没播完就被打断，看起来像「点了没反应」。
   *
   * 展开 / 收起的滚动策略见 `expandScroll.ts`：这一张是用户点的，滚；
   * 因展开 B 而被动收起的 A 不滚（单展开语义下 A 只是让位）。
   */
  let cardClickHandled = false

  /** 卡片元素锚点：批量/展开状态都由 ref 驱动，找 DOM 只能靠页面自己打的 `data-level-dir`。 */
  function cardOf(dirName: string): HTMLElement | null {
    return document.querySelector<HTMLElement>(`[data-level-dir="${CSS.escape(dirName)}"]`)
  }

  function activate(set: LevelSetInfo, card?: HTMLElement) {
    cardClickHandled = true
    const previous = levelExpansion.outerKeys.value[0] ?? null
    if (levelExpansion.isOuterOpen(set.dir_name)) {
      // 收起：量位置、改状态都交给 collapseCardWithScroll —— 它必须在折叠前量，
      // 再把展开那次滚动倒回去，折叠与回滚同时发生，看起来才是「原路收回去」。
      const target = card ?? cardOf(set.dir_name)
      const collapse = () => levelExpansion.closeOuter(set.dir_name)
      if (!target) {
        collapse()
        return
      }
      collapseCardWithScroll({ card: target, body: target.querySelector('.card-detail') }, collapse)
      return
    }
    // 单展开语义：展开 B 会让 A 收起。A 在 B 上面时，收起动画会把 B 整体往上带，
    // 量 B 的 top 时必须减掉 A 会缩掉的高度，否则会多滚一截。
    const previousCard = previous ? cardOf(previous) : null
    const previousBody = previousCard?.querySelector('.card-detail') ?? null
    /*
     * A 名下展开过的随包插件会被一起清掉（`useCardExpansion` 的规则），A 的详情
     * 内容因此在改状态的同一步就变矮 —— 收掉的高度必须在改状态之前量，之后量会
     * 低估，滚动就冲过头。
     */
    const shrink = previousCard && previousBody ? naturalContentHeight(previousBody) : 0
    levelExpansion.openOuter(set.dir_name)
    if (!card) return
    const anchor = card.querySelector('.card-row')
    if (!anchor) return
    void nextTick().then(() => {
      scrollExpandedCardIntoView({
        card,
        anchor,
        body: card.querySelector('.card-detail'),
        collapsing: previousCard
          ? { card: previousCard, body: previousBody, shrink }
          : null,
      })
    })
  }

  /*
   * 随包插件的展开状态在 `levelExpansion` 的内层里（键由 `useCardExpansion` 拼）。
   * 与集卡片不同：集是单展开（同时只看一张），插件是「查看某个 DLL 的信息」，
   * 用户可能想对比两个，所以按 `集目录 + 文件名` 记多份。
   */

  /**
   * 随包插件卡片的展开 / 收起。
   *
   * 二级卡片与集卡片走同一套滚动策略（`expandScroll.ts`）：展开时先让整卡可见、
   * 放不下才把卡片外框钉到页头下沿，收起时把折叠后的卡片带到可视区正中。
   * 不滚的话，插件卡片展开后底部会被视口切掉，用户得自己往下找。
   *
   * 只滚「用户点的这一张」：别的插件卡片被被动影响时没有调用点。
   */
  function togglePlugin(set: LevelSetInfo, plugin: LevelSetPlugin, card?: HTMLElement) {
    // 这次点击已经由插件卡片处理；不设标记的话，document 上「点空白收起」
    // 会顺手把外层的集卡片一起收掉，用户就看不到刚展开的插件详情了。
    cardClickHandled = true
    const close = () => levelExpansion.closeInner(set.dir_name, plugin.file_name)
    if (levelExpansion.isInnerOpen(set.dir_name, plugin.file_name)) {
      if (!card) {
        close()
        return
      }
      collapseCardWithScroll({ card, body: card.querySelector('.card-detail') }, close)
      return
    }
    levelExpansion.openInner(set.dir_name, plugin.file_name)
    if (!card) return
    const anchor = card.querySelector('.card-row')
    if (!anchor) return
    void nextTick().then(() => {
      scrollExpandedCardIntoView({ card, anchor, body: card.querySelector('.card-detail') })
    })
  }

  /**
   * 点击落点所在的**二级卡片**（集详情里的随包插件卡）。
   *
   * 一级卡与二级卡渲染出来都是 `.mod-card`，靠「是否长在某个 `.card-detail` 里」
   * 区分：随包插件卡片只可能出现在集详情内部，一级卡自己不在任何详情区里。
   */
  function nestedPluginCard(target: Element): HTMLElement | null {
    const card = target.closest('.mod-card')
    if (!(card instanceof HTMLElement)) return null
    return card.closest('.card-detail') ? card : null
  }

  /**
   * 收起某张随包插件卡片。
   *
   * 卡片只带 `data-plugin-of` / `data-plugin-file` 两个原始字段（内层键里的 NUL
   * 写不进 HTML 属性），拼键规则仍只有 `useCardExpansion` 一处实现。
   */
  function collapsePluginCard(card: HTMLElement) {
    const dirName = card.dataset.pluginOf
    const fileName = card.dataset.pluginFile
    if (!dirName || !fileName) return
    if (!levelExpansion.isInnerOpen(dirName, fileName)) return
    collapseCardWithScroll({ card, body: card.querySelector('.card-detail') }, () => {
      levelExpansion.closeInner(dirName, fileName)
    })
  }

  /**
   * 点击卡片行之外的地方收起已展开的详情。
   *
   * 「空白」包含详情区自身：展开后点详情里的任意文字、标签或值都算点空白，
   * 应当收起（详情内容是只读的，没有需要保留点击的元素）。判定规则是「点击
   * 目标是否属于这一行的顶栏」——落在 `.card-row`（开关 / 标题 / 状态标签 /
   * 工具按钮）里说明用户在操作这一行，交给各自的 `@click.stop` 与处理函数，
   * 不在这里收起，避免一次点击既展开又收起。
   *
   * 二级卡片（集详情里的随包插件）要先认领：它的详情区同时落在集卡片的
   * `.card-detail` 里，不先判它就会被当成「点空白」，把外层集卡片一起收掉
   * （用户看到的就是「点插件详情，地图包整个收起来了」）。
   *
   * 收起后把展开那次滚动倒回去（单展开语义下它是唯一展开过的那张卡）。
   */
  function onDocumentClick(event: MouseEvent) {
    if (cardClickHandled) {
      cardClickHandled = false
      return
    }
    const target = event.target
    const nested = target instanceof Element ? nestedPluginCard(target) : null
    if (nested) {
      collapsePluginCard(nested)
      return
    }
    const dirName = levelExpansion.outerKeys.value[0] ?? null
    if (dirName === null) return
    if (target instanceof Element && target.closest('.card-row')) return
    const card = cardOf(dirName)
    const collapse = () => levelExpansion.closeOuter(dirName)
    if (!card) {
      collapse()
      return
    }
    collapseCardWithScroll({ card, body: card.querySelector('.card-detail') }, collapse)
  }

  onMounted(() => {
    document.addEventListener('click', onDocumentClick)
  })
  onBeforeUnmount(() => {
    document.removeEventListener('click', onDocumentClick)
  })

  return { levelExpansion, activate, togglePlugin }
}
