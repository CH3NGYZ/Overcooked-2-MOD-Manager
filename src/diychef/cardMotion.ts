import { EASE_STANDARD, prefersReducedMotion } from '../motion.ts'
import type { ChefCardBounds } from './reorder.ts'
import { createChefDragLayout } from './dragLayout.ts'

export const CHEF_MOVE_MS = 300
export const CHEF_PREVIEW_MS = 180
export const CHEF_LIFT_MS = 120
type Bounds = Pick<ChefCardBounds, 'left' | 'top' | 'width' | 'height'>
interface CardFrame { element: HTMLElement; bounds: Bounds; clone?: HTMLElement; viewportBounds?: Bounds }
type Frames = Map<string, CardFrame>
interface Flight { element: HTMLElement; target: HTMLElement; animation: Animation; opacity: string }
interface Move { animation: Animation; bounds: Bounds }

function localBounds(box: Bounds, root: DOMRect): Bounds {
  return { left: box.left - root.left, top: box.top - root.top, width: box.width, height: box.height }
}
function transform(from: Bounds, to: Bounds) {
  return `translate(${from.left - to.left}px, ${from.top - to.top}px)`
}
function moved(from: Bounds, to: Bounds) {
  return Math.abs(from.left - to.left) + Math.abs(from.top - to.top)
    + Math.abs(from.width - to.width) + Math.abs(from.height - to.height) > .5
}

/** Freeze decoded portraits before a cross-grid unmount releases their Blob URL. */
export function cloneChefCard(source: HTMLElement): HTMLElement {
  const clone = source.cloneNode(true) as HTMLElement
  for (const element of [clone, ...clone.querySelectorAll<HTMLElement>('[id], [data-chef-id], [data-chef-focus], [data-tutorial]')]) {
    for (const name of ['id', 'data-chef-id', 'data-chef-focus', 'data-tutorial', 'data-chef-motion-state']) element.removeAttribute(name)
  }
  const images = clone.querySelectorAll('img, canvas')
  source.querySelectorAll<HTMLImageElement | HTMLCanvasElement>('img, canvas').forEach((image, index) => {
    const isImage = image.tagName === 'IMG'
    const width = isImage ? (image as HTMLImageElement).naturalWidth : image.width
    const height = isImage ? (image as HTMLImageElement).naturalHeight : image.height
    if (!width || !height || (isImage && !(image as HTMLImageElement).complete)) return
    const canvas = document.createElement('canvas')
    canvas.width = width; canvas.height = height
    const context = canvas.getContext('2d')
    if (!context) return
    try { context.drawImage(image, 0, 0) } catch { return /* a portrait may still be decoding */ }
    canvas.className = 'chef-motion-image'
    images[index]?.replaceWith(canvas)
  })
  clone.classList.remove('is-dragging', 'chef-drag-preview', 'chef-motion-bridge')
  clone.setAttribute('aria-hidden', 'true'); clone.inert = true
  Object.assign(clone.style, { opacity: '1', transform: '', scale: '', visibility: '', margin: '0', pointerEvents: 'none' })
  return clone
}

/** Page-local FLIP lane. Card positions are relative to the page, so scrolling
 * moves both endpoints together instead of moving a fixed ghost off its target. */
export function createChefCardMotion(root: () => HTMLElement | null) {
  const moves = new Map<HTMLElement, Move>()
  const flights = new Map<string, Flight>()
  const slots = createChefDragLayout(root)
  let adopted: { id: string; element: HTMLElement } | null = null
  let previewKey = ''

  function clearPreview() { slots.clear(); previewKey = '' }
  function stop() {
    clearPreview()
    for (const [element, { animation }] of moves) {
      animation.cancel(); delete element.dataset.chefMotionState
    }
    moves.clear()
    for (const flight of flights.values()) {
      flight.animation.cancel(); flight.element.remove(); flight.target.style.opacity = flight.opacity
      delete flight.target.dataset.chefMotionState
    }
    flights.clear()
    adopted = null
    if (root()) root()!.dataset.chefMotion = 'idle'
  }
  function capture(chefId: string): Frames {
    const host = root(), result: Frames = new Map()
    if (!host) { stop(); return result }
    const box = host.getBoundingClientRect()
    // Read current visual positions before cancelling interrupted animations.
    for (const element of host.querySelectorAll<HTMLElement>('[data-chef-id]')) {
      const id = element.dataset.chefId!, flight = flights.get(id)
      const fixed = adopted?.id === id, visual = fixed ? adopted!.element : flight?.element ?? element
      const visualBox = visual.getBoundingClientRect()
      result.set(id, { element, bounds: localBounds(visualBox, box), viewportBounds: fixed ? visualBox : undefined,
        clone: id === chefId || flight ? cloneChefCard(visual) : undefined })
    }
    adopted = null
    stop()
    return result
  }
  function play(before: Frames, duration = CHEF_MOVE_MS, phase = 'move') {
    const host = root()
    if (!host || prefersReducedMotion()) return
    const box = host.getBoundingClientRect()
    // Finish all geometry reads before starting animations or hiding targets.
    const after = Array.from(host.querySelectorAll<HTMLElement>('[data-chef-id]')).map(element => ({
      id: element.dataset.chefId!, element, bounds: localBounds(element.getBoundingClientRect(), box),
    }))
    for (const { id, element, bounds } of after) {
      const old = before.get(id)
      if (!old || !bounds.width || !bounds.height) continue
      // A released fixed ghost stays at the pointer while focus scrolls the page.
      // Only its start is rebased; ordinary cards continue following the page.
      const start = old.viewportBounds ? localBounds(old.viewportBounds, box) : old.bounds
      if (!moved(start, bounds)) continue
      const bridge = old.clone && (old.element !== element || old.clone.dataset.chefMotionId === id)
      const moving = bridge ? old.clone! : element
      if (bridge) {
        moving.classList.add('chef-motion-bridge')
        moving.dataset.chefMotionId = id
        Object.assign(moving.style, { position: 'absolute', left: `${bounds.left}px`, top: `${bounds.top}px`,
          width: `${bounds.width}px`, height: `${bounds.height}px`, zIndex: '20', transformOrigin: 'top left' })
        host.append(moving)
        // Establish the old appearance before changing enabled/disabled colour.
        moving.getBoundingClientRect()
        moving.classList.toggle('is-enabled', element.classList.contains('is-enabled'))
        moving.classList.toggle('is-default', element.classList.contains('is-default'))
        const label = moving.querySelector('.chef-default-label')
        if (!element.classList.contains('is-default')) label?.remove()
        else if (!label) {
          const nextLabel = element.querySelector('.chef-default-label')
          if (nextLabel) moving.querySelector('.chef-tile-main')?.append(nextLabel.cloneNode(true))
        }
      }
      if (typeof moving.animate !== 'function') { if (bridge) moving.remove(); continue }
      const opacity = element.style.opacity
      if (bridge) element.style.opacity = '0'
      moving.dataset.chefMotionState = phase
      const animation = moving.animate([
        { transform: transform(start, bounds), transformOrigin: 'top left' },
        { transform: 'translate(0px, 0px)', transformOrigin: 'top left' },
      ], { duration, easing: EASE_STANDARD })
      if (bridge) flights.set(id, { element: moving, target: element, animation, opacity })
      else moves.set(element, { animation, bounds })
      const finish = () => {
        if (bridge) {
          if (flights.get(id)?.animation !== animation) return
          flights.delete(id); moving.remove(); element.style.opacity = opacity
        } else {
          if (moves.get(element)?.animation !== animation) return
          moves.delete(element)
        }
        delete moving.dataset.chefMotionState
        if (!moves.size && !flights.size && root()) root()!.dataset.chefMotion = 'idle'
      }
      animation.onfinish = finish
    }
    host.dataset.chefMotion = moves.size || flights.size ? phase : 'idle'
  }
  function adopt(chefId: string, ghost: HTMLElement) {
    adopted = { id: chefId, element: ghost }
    ghost.dataset.chefMotionId = chefId
  }
  function pickup(chefId: string) {
    const before = capture(chefId), source = before.get(chefId), host = root()
    if (!source?.clone || !host) return null
    const origin = host.getBoundingClientRect()
    before.delete(chefId)
    return { ghost: source.clone, bounds: { width: source.bounds.width, height: source.bounds.height, left: source.bounds.left + origin.left, top: source.bounds.top + origin.top },
      // The caller samples stable hit-test geometry before resuming neighbours.
      resume: () => { slots.place(chefId, chefId); play(before, CHEF_PREVIEW_MS, 'preview') } }
  }
  function preview(sourceId: string, targetId: string | null, after: boolean, positions: readonly ChefCardBounds[], overEnabled = false) {
    const key = `${sourceId}/${targetId}/${after}/${overEnabled}`
    if (key === previewKey) return
    // Sample current positions before restoring the previous real slot. Both
    // same-grid and cross-grid insertions then use the browser's actual grid.
    const before = capture('')
    before.delete(sourceId)
    const enabled = root()?.querySelector<HTMLElement>('[data-chef-group="enabled"]')
    const empty = overEnabled && positions.every(position => position.id === sourceId)
    const enabledGrid = enabled?.classList.contains('chef-grid') ? enabled : enabled?.querySelector<HTMLElement>('.chef-grid')
    slots.place(sourceId, empty && targetId === sourceId ? null : targetId, after, empty ? enabledGrid : null)
    previewKey = key
    play(before, CHEF_PREVIEW_MS, 'preview')
  }
  function snapshot(group: HTMLElement | null) {
    const origin = root()?.getBoundingClientRect()
    return slots.snapshot(group, element => {
      const end = moves.get(element)?.bounds
      // Preview animations never feed their transient transform back into hit
      // testing. Their saved endpoints still follow the page's current scroll.
      return end && origin ? { ...end, left: end.left + origin.left, top: end.top + origin.top } : element.getBoundingClientRect()
    })
  }
  return { capture, play, stop, adopt, pickup, preview, clearPreview, snapshot, hasAdopted: () => !!adopted }
}
