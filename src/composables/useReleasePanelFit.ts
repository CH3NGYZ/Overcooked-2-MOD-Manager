import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  toValue,
  watch,
  type MaybeRefOrGetter,
} from 'vue'
import {
  fitsMinLogHeight,
  releasePanelHeight,
  RELEASE_LOG_MIN_HEIGHT,
} from '../releasePanelFit'

type ReleasePanelTab = 'info' | 'install'

interface UseReleasePanelFitOptions {
  activeTab: MaybeRefOrGetter<ReleasePanelTab>
  hasLog: MaybeRefOrGetter<boolean>
  logHtml: MaybeRefOrGetter<string>
  /** Values that can change the fixed rows' height and therefore require a refit. */
  dependencies?: ReadonlyArray<MaybeRefOrGetter<unknown>>
}

/**
 * Keeps the release detail panel's install tab inside the drawer viewport.
 *
 * The panel is the only owner of this DOM measurement. The pure height and
 * scroll-boundary rules remain in `releasePanelFit.ts`; this composable only
 * coordinates refs, browser layout, observer notifications and cleanup.
 */
export function useReleasePanelFit(options: UseReleasePanelFitOptions) {
  const panelElement = ref<HTMLElement | null>(null)
  const logBoxElement = ref<HTMLElement | null>(null)
  /** `null` means the short-window fallback leaves the panel unconstrained. */
  const fittedPanelHeight = ref<number | null>(null)

  const activeTabValue = () => toValue(options.activeTab)
  const panelStyle = computed(() =>
    fittedPanelHeight.value ? { height: `${fittedPanelHeight.value}px` } : undefined,
  )
  const logBoxStyle = computed(() =>
    activeTabValue() === 'install' ? { minHeight: `${RELEASE_LOG_MIN_HEIGHT}px` } : undefined,
  )
  const isFitted = computed(() => fittedPanelHeight.value !== null)

  let fitObserver: ResizeObserver | null = null
  let fitFrame = 0
  let fitting = false
  let fitPending = false

  /**
   * Locks the panel to the drawer's visible height, then lets the internal
   * flex chain distribute the remaining space to the release log.
   */
  async function fitInstallPanel() {
    const panel = panelElement.value
    if (!panel) {
      fittedPanelHeight.value = null
      return
    }
    /*
     * The scrollbar container shrinks to content height on the info tab. Keep
     * the height measured on the install tab so switching tabs cannot collapse
     * the panel.
     */
    if (activeTabValue() !== 'install') return
    const viewport = panel.closest('.n-scrollbar-container') as HTMLElement | null
    const wrapper = panel.parentElement
    const drawer = panel.closest('.n-drawer') as HTMLElement | null
    if (!viewport || !wrapper) return
    fitting = true
    const wrapperStyle = getComputedStyle(wrapper)
    const inset = parseFloat(wrapperStyle.paddingTop) + parseFloat(wrapperStyle.paddingBottom)
    /*
     * Use the drawer's fixed height instead of clientHeight: the latter can
     * shrink to the current content while the panel is being measured.
     */
    const drawerRect = drawer?.getBoundingClientRect()
    const panelRect = panel.getBoundingClientRect()
    const available = drawerRect
      ? drawerRect.height - (panelRect.top - drawerRect.top) - parseFloat(wrapperStyle.paddingBottom)
      : viewport.clientHeight - inset
    fittedPanelHeight.value = releasePanelHeight(available)
    await nextTick()
    /*
     * If the minimum log area cannot fit, release the height lock. The drawer
     * body can then scroll the full panel instead of clipping the install row.
     */
    if (activeTabValue() === 'install') {
      const logBox = logBoxElement.value
      const installRow = panel.querySelector<HTMLElement>('.install-row')
      if (logBox) {
        await new Promise(requestAnimationFrame)
        const logRect = logBox.getBoundingClientRect()
        const fittedPanelRect = panel.getBoundingClientRect()
        const installRect = installRow?.getBoundingClientRect()
        if (!fitsMinLogHeight(logBox.offsetHeight)
          || logRect.bottom > fittedPanelRect.bottom + 0.5
          || (installRect !== undefined && logRect.bottom > installRect.top + 0.5)) {
          fittedPanelHeight.value = null
        }
      }
    }
    fitting = false
    if (fitPending) {
      fitPending = false
      scheduleFitInstallPanel()
    }
  }

  function scheduleFitInstallPanel() {
    if (fitting) {
      fitPending = true
      return
    }
    if (fitFrame) cancelAnimationFrame(fitFrame)
    fitFrame = requestAnimationFrame(() => {
      fitFrame = 0
      void fitInstallPanel()
    })
  }

  onMounted(() => {
    const panel = panelElement.value
    const viewport = panel?.closest('.n-scrollbar-container') as HTMLElement | null
    if (!panel || !viewport) return
    /* Do not observe the panel itself: writing its height would self-trigger. */
    fitObserver = new ResizeObserver(() => scheduleFitInstallPanel())
    fitObserver.observe(viewport)
    for (const selector of ['.detail-head', '.n-tabs-nav', '.summary-row', '.log-head', '.install-row']) {
      const element = panel.querySelector(selector)
      if (element) fitObserver.observe(element)
    }
    if (activeTabValue() === 'install') scheduleFitInstallPanel()
  })

  onBeforeUnmount(() => {
    fitObserver?.disconnect()
    fitObserver = null
    if (fitFrame) cancelAnimationFrame(fitFrame)
    fitFrame = 0
  })

  const watchSources = [
    () => activeTabValue(),
    () => toValue(options.hasLog),
    () => toValue(options.logHtml),
    ...(options.dependencies ?? []).map((source) => () => toValue(source)),
  ]
  watch(watchSources, scheduleFitInstallPanel)

  return {
    panelElement,
    logBoxElement,
    panelStyle,
    logBoxStyle,
    isFitted,
  }
}
