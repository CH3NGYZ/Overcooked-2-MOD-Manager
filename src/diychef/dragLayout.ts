import type { ChefCardBounds } from './reorder.ts'

/** One real grid slot serves both the drag source and its candidate insertion.
 * The source node stays connected for pointer capture; only drop edits a draft. */
export function createChefDragLayout(root: () => HTMLElement | null) {
  let placeholder: HTMLElement | null = null
  const saved = new Map<HTMLElement, string | null>()
  function remember(element: HTMLElement) {
    if (!saved.has(element)) saved.set(element, element.getAttribute('style'))
  }
  function clear() {
    placeholder?.remove(); placeholder = null
    for (const [element, style] of saved) {
      if (style === null) element.removeAttribute('style')
      else element.setAttribute('style', style)
      delete element.dataset.chefDragSource
    }
    saved.clear()
  }
  function place(sourceId: string, targetId: string | null, after = false, destination?: HTMLElement | null) {
    clear()
    const host = root(), cards = Array.from(host?.querySelectorAll<HTMLElement>('[data-chef-id]') ?? [])
    const source = cards.find(element => element.dataset.chefId === sourceId)
    const target = cards.find(element => element.dataset.chefId === targetId)
    const home = source?.closest<HTMLElement>('.chef-grid'), grid = target?.closest<HTMLElement>('.chef-grid') ?? destination ?? home
    if (!source || !home || !grid) return
    const bounds = source.getBoundingClientRect(), style = getComputedStyle(source)
    // ChefTile uses border-box. offsetWidth rounds fractional grid widths and
    // must not be interpreted as a scale factor for the height.
    const width = parseFloat(style.width) || bounds.width, height = parseFloat(style.height) || bounds.height
    const siblings = Array.from(grid.querySelectorAll<HTMLElement>('[data-chef-id]'))
    const originalIndex = siblings.indexOf(source)
    const order = siblings.filter(element => element !== source)
    const insertion = target && target !== source ? order.indexOf(target) + Number(after) : Math.max(0, originalIndex)
    placeholder = source.cloneNode(false) as HTMLElement
    for (const attribute of ['id', 'data-chef-id', 'data-chef-source', 'data-chef-motion-state', 'data-chef-focus', 'data-tutorial']) placeholder.removeAttribute(attribute)
    placeholder.classList.remove('is-default', 'is-dragging')
    placeholder.classList.add('chef-placeholder')
    placeholder.dataset.chefPlaceholder = sourceId
    placeholder.setAttribute('aria-hidden', 'true'); placeholder.inert = true
    Object.assign(placeholder.style, { position: 'relative', left: '', top: '', width: '', height: `${height}px`,
      minHeight: `${height}px`, opacity: '1', visibility: 'visible', transform: '', scale: '', margin: '0', pointerEvents: 'none' })
    remember(source)
    source.dataset.chefDragSource = 'true'
    Object.assign(source.style, { position: 'absolute', left: '0', top: '0', width: `${width}px`, height: `${height}px`,
      opacity: '0', visibility: 'hidden', pointerEvents: 'none' })
    order.splice(insertion, 0, placeholder)
    for (const [index, element] of order.entries()) {
      if (element !== placeholder) remember(element)
      element.style.order = String(index)
    }
    grid.append(placeholder)
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
