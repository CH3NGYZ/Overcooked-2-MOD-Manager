export const CHEF_POINTER_THRESHOLD = 6
export interface SelectionRect { left: number; top: number; right: number; bottom: number }
export function chefSelectionRect(startX: number, startY: number, x: number, y: number): SelectionRect {
  return { left: Math.min(startX, x), top: Math.min(startY, y), right: Math.max(startX, x), bottom: Math.max(startY, y) }
}
export function intersectsChef(rect: SelectionRect, card: SelectionRect): boolean {
  return rect.left <= card.right && rect.right >= card.left && rect.top <= card.bottom && rect.bottom >= card.top
}
/** Read order, rather than Set insertion order, owns the frozen drag group. */
export function chefDragMembers(anchorId: string, selected: ReadonlySet<string>, readingOrder: readonly string[]): string[] {
  return selected.has(anchorId) ? readingOrder.filter(id => selected.has(id)) : [anchorId]
}
