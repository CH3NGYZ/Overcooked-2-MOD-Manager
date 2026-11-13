import type { ChefCardBounds } from './reorder.ts'

/** Every dragged member keeps a real grid slot. Source nodes stay connected for
 * pointer capture; this lane changes only temporary layout, never the draft. */
export function createChefDragLayout(root: () => HTMLElement | null) {
  let placeholders: HTMLElement[] = []
  const saved = new Map<HTMLElement, string | null>()
  function remember(element: HTMLElement) {
    if (!saved.has(element)) saved.set(element, element.getAttribute('style'))
  }
  function clear() {
    for (const placeholder of placeholders) placeholder.remove()
    placeholders = []
    for (const [element, style] of saved) {
      if (style === null) element.removeAttribute('style')
      else element.setAttribute('style', style)
      delete element.dataset.chefDragSource
    }
    saved.clear()
  }
  function place(sourceId: string, targetId: string | null, after = false, destination?: HTMLElement | null, memberIds: readonly string[] = [sourceId], disabled = false) {
    clear()
    const host = root(), cards = Array.from(host?.querySelectorAll<HTMLElement>('[data-chef-id]') ?? [])
    const source = cards.find(element => element.dataset.chefId === sourceId)
    const target = cards.find(element => element.dataset.chefId === targetId)
    const home = source?.closest<HTMLElement>('.chef-grid'), grid = target?.closest<HTMLElement>('.chef-grid') ?? destination ?? home
    if (!source || !home || !grid) return
    const members = memberIds.flatMap(id => cards.find(card => card.dataset.chefId === id) ?? [])
    const memberSet = new Set(members)
    const original = new Map<HTMLElement, HTMLElement[]>()
    for (const card of cards) {
      const parent = card.closest<HTMLElement>('.chef-grid')
      if (parent) original.set(parent, [...(original.get(parent) ?? []), card])
    }
    const byCard = new Map<HTMLElement, HTMLElement>()
    // Read every source size before hiding any member; all lanes use the same
    // border-box size, including fractional widths from WebView zoom.
    const measured = members.map(card => ({ card, bounds: card.getBoundingClientRect(), style: getComputedStyle(card) }))
    for (const { card, bounds, style } of measured) {
      const width = parseFloat(style.width) || bounds.width, height = parseFloat(style.height) || bounds.height
      const placeholder = card.cloneNode(false) as HTMLElement
      for (const attribute of ['id', 'data-chef-id', 'data-chef-motion-state', 'data-chef-focus', 'data-tutorial']) placeholder.removeAttribute(attribute)
      placeholder.classList.remove('is-default', 'is-dragging', 'is-selected')
      placeholder.classList.add('chef-placeholder')
      placeholder.dataset.chefPlaceholder = card.dataset.chefId!
      placeholder.setAttribute('aria-hidden', 'true'); placeholder.inert = true
      Object.assign(placeholder.style, { position: 'relative', left: '', top: '', width: '', height: `${height}px`,
        minHeight: `${height}px`, opacity: '1', visibility: 'visible', transform: '', scale: '', margin: '0', pointerEvents: 'none' })
      remember(card)
      card.dataset.chefDragSource = 'true'
      Object.assign(card.style, { position: 'absolute', left: '0', top: '0', width: `${width}px`, height: `${height}px`,
        opacity: '0', visibility: 'hidden', pointerEvents: 'none' })
      placeholders.push(placeholder); byCard.set(card, placeholder)
    }
    const arrangements = new Map<HTMLElement, HTMLElement[]>()
    for (const [parent, siblings] of original) arrangements.set(parent, siblings.filter(card => !memberSet.has(card)))
    if (disabled) {
      const destinations = new Set<HTMLElement>()
      for (const card of members) {
        const section = host?.querySelector<HTMLElement>(`[data-chef-group="${card.dataset.chefSource}"]`)
        const parent = section?.classList.contains('chef-grid') ? section : section?.querySelector<HTMLElement>('.chef-grid')
        const lane = parent ?? card.closest<HTMLElement>('.chef-grid')!
        destinations.add(lane)
        const order = arrangements.get(lane) ?? []
        order.push(byCard.get(card)!); arrangements.set(lane, order)
      }
      for (const lane of destinations) arrangements.get(lane)?.sort((a, b) => Number(a.dataset.chefOrder ?? 0) - Number(b.dataset.chefOrder ?? 0))
    } else if (targetId === null && !destination) {
      // Outside both regions, retain each member's home slot until cancel/drop.
      for (const [parent, siblings] of original) arrangements.set(parent, siblings.map(card => byCard.get(card) ?? card))
    } else {
      const siblings = original.get(grid) ?? [], order = arrangements.get(grid) ?? []
      const originalIndex = siblings.slice(0, Math.max(0, siblings.indexOf(source))).filter(card => !memberSet.has(card)).length
      const insertion = target && !memberSet.has(target) ? order.indexOf(target) + Number(after) : originalIndex
      order.splice(Math.max(0, insertion), 0, ...members.map(card => byCard.get(card)!)); arrangements.set(grid, order)
    }
    for (const [parent, order] of arrangements) for (const [index, element] of order.entries()) {
      if (!placeholders.includes(element)) remember(element)
      element.style.order = String(index)
      if (placeholders.includes(element)) parent.append(element)
    }
  }
  function snapshot(group: HTMLElement | null, measure: (element: HTMLElement) => Pick<ChefCardBounds, 'left' | 'top' | 'width' | 'height'>): ChefCardBounds[] {
    const elements = Array.from(group?.querySelectorAll<HTMLElement>('[data-chef-id], [data-chef-placeholder]') ?? [])
      .filter(element => element.dataset.chefDragSource !== 'true')
      .sort((a, b) => (parseFloat(getComputedStyle(a).order) || 0) - (parseFloat(getComputedStyle(b).order) || 0))
    return elements.map(element => {
      const { left, top, width, height } = measure(element)
      return { left, top, width, height, id: element.dataset.chefId ?? element.dataset.chefPlaceholder!,
        isDefault: element.classList.contains('is-default') }
    })
  }
  return { place, clear, snapshot }
}
