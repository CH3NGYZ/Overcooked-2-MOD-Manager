import { onMounted, onUnmounted, ref, watch, type Ref } from 'vue'

export function useListNavigationFocus(options: {
  list: Ref<HTMLElement | null>
  targetIds: () => string[]
  targetAttribute: string
  stickyHeadingSelector: string
  onFinished: () => void
}) {
  const focusedIds = ref<string[]>([])
  let startTimer: ReturnType<typeof setTimeout> | undefined
  let endTimer: ReturnType<typeof setTimeout> | undefined
  let scrollFrame: number | undefined

  function cancelFocus() {
    clearTimeout(startTimer)
    clearTimeout(endTimer)
    if (scrollFrame !== undefined) cancelAnimationFrame(scrollFrame)
    scrollFrame = undefined
    focusedIds.value = []
  }

  function finishFocus() {
    cancelFocus()
    options.onFinished()
  }

  function focusTargets() {
    const ids = new Set(options.targetIds())
    const rows = Array.from(options.list.value?.querySelectorAll<HTMLElement>(`[${options.targetAttribute}]`) ?? [])
      .filter((element) => ids.has(element.getAttribute(options.targetAttribute) ?? ''))
    const row = rows[0]
    const container = options.list.value?.closest<HTMLElement>('.main-scroll')
    if (!row || !container) { finishFocus(); return }

    const scale = container.getBoundingClientRect().height / container.clientHeight || 1
    let headingHeight = 16
    for (let parent = row.parentElement; parent && parent !== container; parent = parent.parentElement) {
      for (const heading of parent.querySelectorAll<HTMLElement>(options.stickyHeadingSelector)) {
        headingHeight += heading.offsetHeight
      }
    }
    const start = container.scrollTop
    const destination = Math.max(0, Math.min(container.scrollHeight - container.clientHeight,
      start + (row.getBoundingClientRect().top - container.getBoundingClientRect().top) / scale - headingHeight))
    const began = performance.now()
    function animate(now: number) {
      if (!row.isConnected) { finishFocus(); return }
      const progress = Math.min(1, (now - began) / 650)
      container!.scrollTop = start + (destination - start) * (1 - Math.pow(1 - progress, 3))
      if (progress < 1) scrollFrame = requestAnimationFrame(animate)
      else {
        scrollFrame = undefined
        const viewport = container!.getBoundingClientRect()
        const allFit = rows.every((element) => {
          const bounds = element.getBoundingClientRect()
          return bounds.top >= viewport.top + headingHeight * scale - 1
            && bounds.bottom <= viewport.bottom - 8 * scale
        })
        focusedIds.value = (allFit ? rows : [row]).map((element) => element.getAttribute(options.targetAttribute)!)
        row.focus({ preventScroll: true })
        endTimer = setTimeout(finishFocus, 4500)
      }
    }
    scrollFrame = requestAnimationFrame(animate)
  }

  function scheduleFocus() {
    cancelFocus()
    if (options.targetIds().length) startTimer = setTimeout(focusTargets, 450)
  }

  onMounted(scheduleFocus)
  watch(options.targetIds, scheduleFocus, { flush: 'post' })
  onUnmounted(() => {
    cancelFocus()
    if (options.targetIds().length) options.onFinished()
  })
  return focusedIds
}
