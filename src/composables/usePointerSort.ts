import { onBeforeUnmount, ref, type Ref } from 'vue'

// Sort a single list from its handle. DOM rectangles are in physical viewport
// coordinates; scrollTop is logical, so edge scrolling accounts for UI zoom.
export function usePointerSort<T extends { id: string }>(
  viewport: Ref<HTMLElement | null>,
  listFor: (kind: string) => T[],
) {
  const dragging = ref<{ kind: string; id: string } | null>(null)
  let pointerId: number | null = null
  let startY = 0
  let pointerY = 0
  let original: T[] = []
  let pending: { kind: string; id: string } | null = null
  let frame = 0
  let lastTime = 0
  let rows: HTMLElement[] = []
  let lastY = NaN
  let lastScroll = NaN

  function reorderAtPointer() {
    const active = dragging.value
    const root = viewport.value
    if (!active || !root) return
    const list = listFor(active.kind)
    const indices = new Map(list.map((item, index) => [item.id, index]))
    const from = indices.get(active.id) ?? -1
    if (from < 0) return stop(true)
    let destination = from
    for (const row of rows) {
      const index = indices.get(row.dataset.sortId ?? '') ?? -1
      const rect = row.getBoundingClientRect()
      const middle = rect.top + rect.height / 2
      if (index >= 0 && index < from && pointerY < middle) {
        destination = Math.min(destination, index)
      }
      if (index > from && pointerY > middle) destination = Math.max(destination, index)
    }
    if (destination !== from) {
      const item = list.splice(from, 1)[0]!
      list.splice(destination, 0, item)
    }
  }

  function tick(time: number) {
    const root = viewport.value
    if (!dragging.value || !root) return stop(true)
    const rect = root.getBoundingClientRect()
    const edge = Math.min(64, rect.height / 4)
    const direction = pointerY < rect.top + edge
      ? -Math.min(1, (rect.top + edge - pointerY) / edge)
      : pointerY > rect.bottom - edge
        ? Math.min(1, (pointerY - rect.bottom + edge) / edge)
        : 0
    const elapsed = lastTime ? Math.min(time - lastTime, 32) : 16
    lastTime = time
    const scale = root.offsetHeight ? rect.height / root.offsetHeight : 1
    if (direction) root.scrollTop += direction * elapsed * .65 / (scale || 1)
    if (pointerY !== lastY || root.scrollTop !== lastScroll) {
      lastY = pointerY
      lastScroll = root.scrollTop
      reorderAtPointer()
    }
    if (dragging.value) frame = requestAnimationFrame(tick)
  }

  function move(event: PointerEvent) {
    if (event.pointerId !== pointerId || !pending) return
    pointerY = event.clientY
    if (!dragging.value && Math.abs(pointerY - startY) >= 4) {
      dragging.value = pending
      frame = requestAnimationFrame(tick)
    }
    if (dragging.value) event.preventDefault()
  }

  function finish(event: PointerEvent) {
    if (event.pointerId === pointerId) stop(event.type === 'pointercancel')
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      stop(true)
    }
  }

  function cancel() { stop(true) }

  function stop(revert = false) {
    if (revert && dragging.value) {
      const list = listFor(dragging.value.kind)
      list.splice(0, list.length, ...original)
    }
    cancelAnimationFrame(frame)
    frame = 0
    lastTime = 0
    dragging.value = null
    pointerId = null
    pending = null
    original = []
    rows = []
    lastY = lastScroll = NaN
    window.removeEventListener('pointermove', move)
    window.removeEventListener('pointerup', finish)
    window.removeEventListener('pointercancel', finish)
    window.removeEventListener('blur', cancel)
    window.removeEventListener('keydown', keydown, true)
  }

  function begin(event: PointerEvent, kind: string, id: string) {
    if (event.button !== 0 || !event.isPrimary || !viewport.value) return
    stop()
    pointerId = event.pointerId
    pointerY = startY = event.clientY
    original = [...listFor(kind)]
    rows = [...viewport.value.querySelectorAll<HTMLElement>('[data-sort-id]')]
      .filter((row) => row.dataset.sortKind === kind)
    pending = { kind, id }
    event.preventDefault()
    window.addEventListener('pointermove', move, { passive: false })
    window.addEventListener('pointerup', finish)
    window.addEventListener('pointercancel', finish)
    window.addEventListener('blur', cancel)
    window.addEventListener('keydown', keydown, true)
  }

  function moveWithKeyboard(event: KeyboardEvent, kind: string, id: string) {
    const offset = event.key === 'ArrowUp' ? -1 : event.key === 'ArrowDown' ? 1 : 0
    if (!offset) return
    event.preventDefault()
    const list = listFor(kind)
    const from = list.findIndex((item) => item.id === id)
    const destination = from + offset
    if (from < 0 || destination < 0 || destination >= list.length) return
    const item = list.splice(from, 1)[0]!
    list.splice(destination, 0, item)
  }

  onBeforeUnmount(() => stop())
  return { dragging, begin, stop, moveWithKeyboard }
}
