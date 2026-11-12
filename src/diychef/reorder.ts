export interface ChefCardBounds {
  id: string
  left: number
  top: number
  width: number
  height: number
  isDefault: boolean
}

export const CHEF_SWAP_OVERLAP = .5
export interface ChefDropCandidate { id: string; after: boolean }

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
  currentTarget?: ChefDropCandidate | null
}

/** Positions follow the current preview slot order, with its placeholder as source. */
export function chefDropTarget(input: DropInput): ChefDropCandidate | null {
  const { source, positions, scrollShift } = input
  const left = source.left + input.deltaX, top = source.top + input.deltaY
  const centerX = left + source.width / 2, centerY = top + source.height / 2
  const sourceIndex = positions.findIndex(position => position.id === input.sourceId)
  const retained = () => {
    const current = positions.find(position => position.id === input.currentTarget?.id)
    return current ? { id: current.id, after: !!input.currentTarget?.after || (current.isDefault && input.preserveDefault) } : null
  }
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
  // Positive overlap below the threshold cannot fall through to pointer-nearest:
  // an edge grab otherwise bypasses the same area requirement.
  if (target && overlap < CHEF_SWAP_OVERLAP) {
    return retained()
  }
  if (!target && positions.some(position => position.id !== input.sourceId
    && input.pointerX >= position.left && input.pointerX <= position.left + position.width
    && input.pointerY >= position.top - scrollShift && input.pointerY <= position.top - scrollShift + position.height)) {
    return retained()
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
  // The source now represents the live placeholder, not its original DOM slot.
  // Resting inside that slot must preserve the insertion already previewed.
  if (target.id === input.sourceId) return retained() ?? { id: input.sourceId, after: false }
  const targetIndex = positions.indexOf(target)
  // Crossing into the next card means moving beyond it, even while the pointer
  // is still on its near side, once the shared overlap threshold is reached.
  const after = overlapping && sourceIndex >= 0
    ? sourceIndex < targetIndex
    : centerX > target.left + target.width / 2
  return { id: target.id, after: after || (target.isDefault && input.preserveDefault) }
}
