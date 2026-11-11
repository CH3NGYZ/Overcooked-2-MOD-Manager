export interface ChefCardBounds {
  id: string
  left: number
  top: number
  width: number
  height: number
  isDefault: boolean
}

interface DropInput {
  sourceId: string
  preserveDefault: boolean
  source: ChefCardBounds
  positions: readonly ChefCardBounds[]
  deltaX: number
  deltaY: number
  scrollShift: number
  pointerX: number
  pointerY: number
}

/** Cached card rectangles follow the scroller; the floating card follows the pointer. */
export function chefDropTarget(input: DropInput): { id: string; after: boolean } | null {
  const { source, positions, scrollShift } = input
  const left = source.left + input.deltaX, top = source.top + input.deltaY
  const centerX = left + source.width / 2, centerY = top + source.height / 2
  const sourceIndex = positions.findIndex(position => position.id === input.sourceId)
  let target: ChefCardBounds | null = null, overlap = 0, distance = Infinity
  for (const position of positions) {
    if (position.id === input.sourceId) continue
    const targetTop = position.top - scrollShift
    const width = Math.min(left + source.width, position.left + position.width) - Math.max(left, position.left)
    const height = Math.min(top + source.height, targetTop + position.height) - Math.max(top, targetTop)
    if (width <= 0 || height <= 0) continue
    const area = width * height / Math.min(source.width * source.height, position.width * position.height)
    const nextDistance = (centerX - position.left - position.width / 2) ** 2 + (centerY - targetTop - position.height / 2) ** 2
    if (area > overlap || (area === overlap && nextDistance < distance)) {
      overlap = area; distance = nextDistance; target = position
    }
  }
  const overlapping = !!target
  // Empty space still permits precise insertion. Including the source here makes
  // a short drag inside its own slot a no-op until another card is touched.
  if (!target) for (const position of positions) {
    const next = (input.pointerX - position.left - position.width / 2) ** 2
      + (input.pointerY - position.top + scrollShift - position.height / 2) ** 2
    if (next < distance) { distance = next; target = position }
  }
  if (!target) return null
  const targetIndex = positions.indexOf(target)
  // Crossing into the next card means moving beyond it, even while the pointer
  // is still on its near side. This lets adjacent cards exchange at first overlap.
  const after = overlapping && sourceIndex >= 0
    ? sourceIndex < targetIndex
    : centerX > target.left + target.width / 2
  return { id: target.id, after: after || (target.isDefault && input.preserveDefault) }
}
