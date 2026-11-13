import { computed, onBeforeUnmount, shallowRef, watch, type Ref } from 'vue'
import { CHEF_POINTER_THRESHOLD, chefDragMembers, chefSelectionRect, intersectsChef, type SelectionRect } from '../diychef/selection.ts'

interface SelectionOptions {
  root: Readonly<Ref<HTMLElement | null>>
  enabled: Readonly<Ref<boolean>>
  filterKey: Readonly<Ref<string>>
  sessionId: Readonly<Ref<string | undefined>>
  readingOrder: Readonly<Ref<readonly string[]>>
  selectableIds: Readonly<Ref<ReadonlySet<string>>>
}

/** Selection is page-only state: it never participates in a draft or undo snapshot. */
export function useChefSelection(options: SelectionOptions) {
  const selectedIds = shallowRef<ReadonlySet<string>>(new Set())
  const rectangle = shallowRef<SelectionRect | null>(null)
  let startX = 0, startY = 0, pointerId = -1, additive = false, active = false
  let baseline: ReadonlySet<string> = new Set()
  let frame = 0, latestX = 0, latestY = 0, session = 0
  const rectangleStyle = computed(() => rectangle.value ? {
    left: `${rectangle.value.left}px`, top: `${rectangle.value.top}px`,
    width: `${rectangle.value.right - rectangle.value.left}px`, height: `${rectangle.value.bottom - rectangle.value.top}px`,
  } : {})
  function cancel() {
    if (frame) cancelAnimationFrame(frame)
    frame = 0; session += 1; pointerId = -1; active = false; rectangle.value = null
    window.removeEventListener('pointermove', move)
    window.removeEventListener('pointerup', end)
    window.removeEventListener('pointercancel', abort)
    window.removeEventListener('keydown', key)
    window.removeEventListener('blur', abort)
    window.removeEventListener('resize', abort)
  }
  function clear() { cancel(); selectedIds.value = new Set() }
  function toggle(id: string) {
    if (!options.enabled.value || !options.selectableIds.value.has(id)) return
    const next = new Set(selectedIds.value)
    if (next.has(id)) next.delete(id)
    else next.add(id)
    selectedIds.value = next
  }
  function members(anchorId: string) {
    const ids = chefDragMembers(anchorId, selectedIds.value, options.readingOrder.value)
    if (!selectedIds.value.has(anchorId)) selectedIds.value = new Set()
    return ids
  }
  function update() {
    if (pointerId < 0 || !active) return
    const rect = chefSelectionRect(startX, startY, latestX, latestY)
    rectangle.value = rect
    const next = new Set(additive ? baseline : [])
    for (const card of options.root.value?.querySelectorAll<HTMLElement>('[data-chef-id]') ?? []) {
      const id = card.dataset.chefId!
      if (options.selectableIds.value.has(id) && intersectsChef(rect, card.getBoundingClientRect())) next.add(id)
    }
    selectedIds.value = next
  }
  function move(event: PointerEvent) {
    if (event.pointerId !== pointerId) return
    latestX = event.clientX; latestY = event.clientY
    if (!active && Math.hypot(latestX - startX, latestY - startY) >= CHEF_POINTER_THRESHOLD) active = true
    if (!active) return
    event.preventDefault()
    if (!frame) {
      const ticket = session
      const id = requestAnimationFrame(() => {
        if (ticket !== session || frame !== id) return
        frame = 0
        update()
      })
      frame = id
    }
  }
  function end(event: PointerEvent) {
    if (event.pointerId !== pointerId) return
    if (frame) cancelAnimationFrame(frame)
    frame = 0
    if (active) { latestX = event.clientX; latestY = event.clientY; update() }
    else selectedIds.value = new Set()
    cancel()
  }
  function abort() { selectedIds.value = baseline; cancel() }
  function key(event: KeyboardEvent) { if (event.key === 'Escape') { event.preventDefault(); abort() } }
  function start(event: PointerEvent) {
    if (!options.enabled.value || event.button !== 0 || !event.isPrimary) return
    if ((event.target as Element).closest('[data-chef-id], button, input, label, a, .n-checkbox, .chef-section-heading')) return
    cancel()
    startX = latestX = event.clientX; startY = latestY = event.clientY; pointerId = event.pointerId
    additive = event.ctrlKey || event.metaKey; baseline = selectedIds.value
    window.addEventListener('pointermove', move, { passive: false })
    window.addEventListener('pointerup', end)
    window.addEventListener('pointercancel', abort)
    window.addEventListener('keydown', key)
    window.addEventListener('blur', abort)
    window.addEventListener('resize', abort)
  }
  watch([options.filterKey, options.sessionId], clear, { flush: 'sync' })
  watch(options.enabled, value => { if (!value) clear() })
  watch(options.selectableIds, available => { selectedIds.value = new Set([...selectedIds.value].filter(id => available.has(id))) })
  onBeforeUnmount(cancel)
  return { selectedIds, rectangle, rectangleStyle, toggle, clear, members, start }
}
