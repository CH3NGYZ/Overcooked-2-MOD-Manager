import { CHEF_LIFT_MS, CHEF_MOVE_MS, type createChefCardMotion } from './cardMotion.ts'
import { EASE_STANDARD, prefersReducedMotion } from '../motion.ts'

type Pickup = NonNullable<ReturnType<ReturnType<typeof createChefCardMotion>['pickup']>>

/** The pointer owns each outer layer; gathering owns only its card's transform.
 * Keeping these separate lets the pointer move immediately during the flight. */
export function createChefDragStack(pickup: Pickup, anchorId: string, deltaX: number, deltaY: number) {
  const animations = new Set<Animation>()
  const reduced = prefersReducedMotion()
  const order = [anchorId, ...pickup.members.map(member => member.id).filter(id => id !== anchorId)]
  const levels = new Map(order.map((id, level) => [id, level]))
  function animate(element: HTMLElement, keyframes: Keyframe[], duration: number) {
    if (reduced || typeof element.animate !== 'function') return
    const animation = element.animate(keyframes, { duration, easing: EASE_STANDARD, fill: 'both' })
    animations.add(animation)
    animation.onfinish = () => { animations.delete(animation); animation.cancel() }
    animation.oncancel = () => animations.delete(animation)
  }
  const layers = pickup.members.map(({ id, ghost: element, bounds }) => {
    const level = levels.get(id)!, offset = Math.min(level, 4) * 5
    const transform = `translate3d(${offset}px, ${offset}px, 0)`
    const layer = document.createElement('div')
    layer.className = 'chef-drag-layer'; layer.setAttribute('aria-hidden', 'true'); layer.inert = true
    Object.assign(layer.style, { position: 'fixed', left: `${pickup.bounds.left}px`, top: `${pickup.bounds.top}px`,
      width: `${bounds.width}px`, height: `${bounds.height}px`, zIndex: String(3000 - level), pointerEvents: 'none',
      transform: `translate3d(${deltaX}px, ${deltaY}px, 0)` })
    element.classList.add('chef-drag-preview')
    element.dataset.chefMotionState = 'drag'; element.dataset.chefMotionId = id
    Object.assign(element.style, { position: 'absolute', left: '0px', top: '0px', width: `${bounds.width}px`,
      height: `${bounds.height}px`, pointerEvents: 'none', opacity: '1', margin: '0', transform,
      boxShadow: '0 12px 32px color-mix(in srgb, var(--oc2-text-1) 18%, transparent)' })
    layer.append(element); document.body.append(layer)
    // Compensate for the first pointer movement, so every member's first frame
    // is its own visible pickup position, including cards in another grid row.
    animate(element, [
      { transform: `translate3d(${bounds.left - pickup.bounds.left - deltaX}px, ${bounds.top - pickup.bounds.top - deltaY}px, 0)` },
      { transform },
    ], CHEF_MOVE_MS)
    animate(element, [
      { boxShadow: '0 1px 3px color-mix(in srgb, var(--oc2-text-1) 8%, transparent)' },
      { boxShadow: element.style.boxShadow },
    ], CHEF_LIFT_MS)
    return { id, element, layer }
  })
  if (layers.length > 1) {
    const badge = document.createElement('span')
    badge.className = 'chef-drag-count'; badge.textContent = String(layers.length)
    layers.find(member => member.id === anchorId)?.element.append(badge)
  }
  return {
    members: layers.map(({ id, element }) => ({ id, element })),
    move(x: number, y: number, enabled: boolean) {
      for (const { layer, element } of layers) {
        layer.style.transform = `translate3d(${x}px, ${y}px, 0)`
        element.classList.toggle('is-enabled', enabled)
      }
    },
    stop() {
      for (const animation of animations) animation.cancel()
      animations.clear()
      for (const { layer } of layers) layer.remove()
    },
  }
}
