import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import type { Locale } from '../i18n'
import { t } from '../i18n'
import type { PublicConfig, SettingsPatch, TutorialStepConfig, TutorialScenario } from '../types'
import {
  cloneTutorialSteps,
  normalizeTutorialStep,
  resolveTutorialView,
  tutorialEditorStepValues,
  tutorialFormatTemplate,
  tutorialLocaleOptions,
  tutorialRepositoryLoadingStepIds,
  tutorialScenarioOptions,
  validateTutorialLanguageMaps,
  type TutorialStep,
  type View,
} from './steps'

type ViewportRect = { left: number; top: number; width: number; height: number }
type TutorialStyle = Record<string, string>

export type TutorialZoomControls = {
  min: number
  max: number
  step: number
  set: (value: number) => void | Promise<void>
}

export type TutorialApi = {
  getBuiltinTutorial: () => Promise<TutorialStepConfig[]>
  parseTutorialConfig: (text: string) => Promise<TutorialStepConfig[]>
  serializeTutorialConfig: (steps: TutorialStepConfig[]) => Promise<string>
  copyText: (text: string) => Promise<unknown>
}

export type TutorialHost = {
  api: TutorialApi
  getStoredSteps: () => TutorialStepConfig[]
  getSeenStepIds: () => string[]
  persistSettings: (patch: SettingsPatch) => Promise<PublicConfig>
  getLocale: () => Locale
  getZoom: () => number
  zoomControls?: TutorialZoomControls
  getActiveView: () => View
  setActiveView: (view: View) => void
  prepareStep: (step: TutorialStep) => void | Promise<void>
  finishPresentation: () => void
  showError: (reason: unknown) => void
  showSuccess: (message: string) => void
  run: (label: string, action: () => Promise<unknown>) => Promise<unknown>
}

const emptyRect = (): ViewportRect => ({ left: 0, top: 0, width: 0, height: 0 })
const tutorialNavigationOrder: View[] = ['overview', 'framework', 'repositories', 'installed', 'sponsor']
const metricViews: Record<string, View> = {
  'overview-bepinex': 'framework',
  'overview-local-mods': 'installed',
  'overview-cloud-mods': 'repositories',
}

export function useTutorialController(host: TutorialHost) {
  const visible = ref(false)
  const introductionVisible = ref(false)
  const previewing = ref(false)
  const manualPreview = ref(false)
  const scenario = ref<TutorialScenario>('live')
  const pendingSteps = ref<TutorialStep[]>([])
  const stepIndex = ref(0)
  const saving = ref(false)
  const presenting = ref(false)
  const spotlightRect = ref<ViewportRect>(emptyRect())
  const cardElement = ref<HTMLElement | null>(null)
  const cardSize = ref({ width: 0, height: 0 })
  const editorVisible = ref(false)
  const exitConfirmVisible = ref(false)
  const replayMenuVisible = ref(false)
  const editorSteps = ref<TutorialStep[]>([])
  const builtinSteps = ref<TutorialStep[]>([])
  const pickerActive = ref(false)
  const pickerIndex = ref<number | null>(null)
  const pickerHoverRect = ref<ViewportRect>(emptyRect())
  const pickerHoverSelector = ref('')
  const importVisible = ref(false)
  const importText = ref('')
  const importPreview = ref<TutorialStep[] | null>(null)
  const importError = ref('')
  const importing = ref(false)
  const exportVisible = ref(false)
  const exportCode = ref('')
  const fakeProgressPercent = ref(0)

  let targetMutationObserver: MutationObserver | undefined
  let targetResizeObserver: ResizeObserver | undefined
  let cardResizeObserver: ResizeObserver | undefined
  let layoutFrame: number | undefined
  let layoutTimer: number | undefined
  let fakeProgressTimer: number | undefined
  const pendingFrames = new Set<number>()
  const pendingFrameResolvers = new Map<number, () => void>()
  const pendingTimers = new Set<number>()
  let disposed = false
  let presentationVersion = 0
  let inputListenersActive = false
  let wheelLastTime = -Infinity
  let wheelDistance = 0
  let wheelGestureConsumed = false

  const currentStep = computed(() => introductionVisible.value ? null : pendingSteps.value[stepIndex.value] ?? null)
  const navigationBusy = computed(() => saving.value || presenting.value)
  const inputNavigationActive = computed(() => visible.value
    && !exitConfirmVisible.value && !replayMenuVisible.value && !editorVisible.value
    && !importVisible.value && !exportVisible.value && !pickerActive.value)
  const zoomStepActive = computed(() => currentStep.value?.id === 'interface-zoom')
  const zoomPercent = computed(() => Math.round(host.getZoom() * 100))
  const canZoomOut = computed(() => !!host.zoomControls && host.getZoom() > host.zoomControls.min)
  const canZoomIn = computed(() => !!host.zoomControls && host.getZoom() < host.zoomControls.max)
  const progress = computed(() => `${stepIndex.value + 1} / ${pendingSteps.value.length}`)
  const configuredSteps = computed(() => {
    const stored = host.getStoredSteps()
    return stored.length ? cloneTutorialSteps(stored) : builtinSteps.value
  })
  const repositoryLoadingActive = computed(() => previewing.value
    && tutorialRepositoryLoadingStepIds.has(currentStep.value?.id || ''))
  const blocksRepositoryRefresh = computed(() => visible.value || previewing.value)
  const spotlightStyle = computed<TutorialStyle>(() => {
    const scale = Math.max(host.getZoom(), 0.01)
    return {
      left: `${spotlightRect.value.left / scale}px`,
      top: `${spotlightRect.value.top / scale}px`,
      width: `${spotlightRect.value.width / scale}px`,
      height: `${spotlightRect.value.height / scale}px`,
    }
  })
  const cardStyle = computed<TutorialStyle>((): TutorialStyle => {
    if (introductionVisible.value) {
      return {
        left: '50%',
        top: '50%',
        width: 'min(760px, calc(100% - 32px))',
        maxHeight: 'calc(100% - 32px)',
        transform: 'translate(-50%, -50%)',
        transition: 'none',
      }
    }
    const scale = Math.max(host.getZoom(), 0.01)
    const rect = {
      left: spotlightRect.value.left / scale,
      top: spotlightRect.value.top / scale,
      width: spotlightRect.value.width / scale,
      height: spotlightRect.value.height / scale,
    }
    const viewportWidth = window.innerWidth / scale
    const viewportHeight = window.innerHeight / scale
    const viewportPadding = 16 / scale
    const cardWidth = Math.min(360 / scale, Math.max(1, viewportWidth - viewportPadding * 2))
    const fallbackHeight = Math.max(160 / scale, Math.min(236 / scale, viewportHeight - viewportPadding * 2))
    const cardHeight = Math.min(
      cardSize.value.height || fallbackHeight,
      Math.max(1, viewportHeight - viewportPadding * 2),
    )
    const maxHeight = `${Math.max(1, viewportHeight - viewportPadding * 2)}px`
    const coversViewport = rect.left <= viewportPadding && rect.top <= viewportPadding
      && rect.left + rect.width >= viewportWidth - viewportPadding
      && rect.top + rect.height >= viewportHeight - viewportPadding
    // A viewport-sized target has no outside space to avoid. Anchor its card
    // independently of measured height so zoom/reflow cannot choose a new side.
    if (zoomStepActive.value || coversViewport) {
      return {
        left: '50%',
        top: '50%',
        width: `${cardWidth}px`,
        maxHeight,
        transform: 'translate(-50%, -50%)',
        transition: 'none',
      }
    }
    const gap = 28 / scale
    const avoid = 16 / scale
    const safeRect = {
      left: rect.left - avoid,
      top: rect.top - avoid,
      width: rect.width + avoid * 2,
      height: rect.height + avoid * 2,
    }
    const maxLeft = Math.max(viewportPadding, viewportWidth - cardWidth - viewportPadding)
    const maxTop = Math.max(viewportPadding, viewportHeight - cardHeight - viewportPadding)
    const candidates = [
      { left: rect.left, top: rect.top + rect.height + gap },
      { left: rect.left, top: rect.top - cardHeight - gap },
      { left: rect.left + rect.width + gap, top: rect.top },
      { left: rect.left - cardWidth - gap, top: rect.top },
    ].map((candidate) => {
      const left = Math.max(viewportPadding, Math.min(candidate.left, maxLeft))
      const top = Math.max(viewportPadding, Math.min(candidate.top, maxTop))
      const overlapWidth = Math.max(0, Math.min(left + cardWidth, safeRect.left + safeRect.width) - Math.max(left, safeRect.left))
      const overlapHeight = Math.max(0, Math.min(top + cardHeight, safeRect.top + safeRect.height) - Math.max(top, safeRect.top))
      const horizontalGap = left >= safeRect.left + safeRect.width
        ? left - (safeRect.left + safeRect.width)
        : safeRect.left - (left + cardWidth)
      const verticalGap = top >= safeRect.top + safeRect.height
        ? top - (safeRect.top + safeRect.height)
        : safeRect.top - (top + cardHeight)
      return { left, top, overlap: overlapWidth * overlapHeight, gap: Math.max(0, horizontalGap) + Math.max(0, verticalGap) }
    })
    const placement = candidates
      .filter((candidate) => candidate.overlap === 0)
      .sort((left, right) => right.gap - left.gap)[0]
      || candidates.reduce((best, candidate) => candidate.overlap < best.overlap ? candidate : best)
    return {
      left: `${placement.left}px`,
      top: `${placement.top}px`,
      width: `${cardWidth}px`,
      maxHeight,
    }
  })
  const pickerHighlightStyle = computed<TutorialStyle>(() => {
    const scale = Math.max(host.getZoom(), 0.01)
    return {
      left: `${pickerHoverRect.value.left / scale}px`,
      top: `${pickerHoverRect.value.top / scale}px`,
      width: `${pickerHoverRect.value.width / scale}px`,
      height: `${pickerHoverRect.value.height / scale}px`,
    }
  })

  function stepText(step: TutorialStep, field: 'title' | 'description') {
    const map = field === 'title' ? step.title_i18n : step.description_i18n
    return map[host.getLocale()]?.trim() || map['zh-CN']?.trim() || t(step[field])
  }

  async function adjustZoom(direction: -1 | 1) {
    const controls = host.zoomControls
    if (!controls) return
    const next = Math.min(controls.max, Math.max(controls.min, host.getZoom() + direction * controls.step))
    try {
      await controls.set(next)
      await nextTick()
      updateRect()
      updateCardSize()
    } catch (reason) {
      host.showError(reason)
    }
  }

  function trackFrame(callback: FrameRequestCallback) {
    const frame = window.requestAnimationFrame((time) => {
      pendingFrames.delete(frame)
      if (!disposed) callback(time)
    })
    pendingFrames.add(frame)
    return frame
  }

  function waitForFrame() {
    return new Promise<void>((resolve) => {
      let frame = 0
      frame = window.requestAnimationFrame(() => {
        pendingFrames.delete(frame)
        pendingFrameResolvers.delete(frame)
        resolve()
      })
      pendingFrames.add(frame)
      pendingFrameResolvers.set(frame, resolve)
    })
  }

  function trackTimeout(callback: () => void, delay: number) {
    const timer = window.setTimeout(() => {
      pendingTimers.delete(timer)
      if (!disposed) callback()
    }, delay)
    pendingTimers.add(timer)
    return timer
  }

  function stopFakeProgress() {
    if (fakeProgressTimer !== undefined) window.clearInterval(fakeProgressTimer)
    fakeProgressTimer = undefined
  }

  function startFakeProgress() {
    stopFakeProgress()
    fakeProgressPercent.value = 0
    fakeProgressTimer = window.setInterval(() => {
      fakeProgressPercent.value = Math.min(100, fakeProgressPercent.value + 1)
      if (fakeProgressPercent.value >= 100) stopFakeProgress()
    }, 100)
  }

  function targetElement(step: TutorialStep | null) {
    if (!step) return null
    try {
      const selected = document.querySelector(step.selector)
      if (selected instanceof Element) return selected
    } catch {
      // Invalid selectors are reported by the editor; keep the overlay usable.
    }
    return document.querySelector('.topbar')
  }

  function targetElements(step: TutorialStep | null) {
    if (!step) return []
    try {
      return Array.from(document.querySelectorAll(step.selector)).filter((element): element is Element => element instanceof Element)
    } catch {
      return []
    }
  }

  function targetExists(step: TutorialStep | null) {
    if (!step) return false
    try {
      return !!document.querySelector(step.selector)
    } catch {
      return false
    }
  }

  function elementRect(element: Element): ViewportRect {
    const rect = element.getBoundingClientRect()
    return { left: rect.left, top: rect.top, width: rect.width, height: rect.height }
  }

  function targetRect(step: TutorialStep | null): ViewportRect | null {
    const elements = targetElements(step)
    if (!elements.length) return null
    const rects = elements.map((element) => element.getBoundingClientRect())
    const left = Math.min(...rects.map((rect) => rect.left))
    const top = Math.min(...rects.map((rect) => rect.top))
    const right = Math.max(...rects.map((rect) => rect.right))
    const bottom = Math.max(...rects.map((rect) => rect.bottom))
    return { left, top, width: Math.max(0, right - left), height: Math.max(0, bottom - top) }
  }

  function updateRect() {
    if (!visible.value) return
    const next = targetRect(currentStep.value)
    if (!next || next.width <= 0 || next.height <= 0) {
      const target = targetElement(currentStep.value)
      const fallback = target instanceof HTMLImageElement ? target.parentElement : null
      if (fallback) {
        const fallbackRect = elementRect(fallback)
        if (fallbackRect.width > 0 && fallbackRect.height > 0) spotlightRect.value = fallbackRect
      }
      return
    }
    spotlightRect.value = next
  }

  function updateCardSize() {
    const card = cardElement.value
    if (!card) return
    // Layout dimensions keep measurements independent of the portal scale.
    // Measured height informs avoidance; measured width never drives CSS width.
    const width = card.offsetWidth
    const height = card.offsetHeight
    if (width <= 0 || height <= 0) return
    if (cardSize.value.width === width && cardSize.value.height === height) return
    cardSize.value = { width, height }
  }

  function scheduleRectUpdate() {
    if (layoutFrame !== undefined || disposed) return
    layoutFrame = trackFrame(() => {
      layoutFrame = undefined
      updateRect()
    })
  }

  function observeCardLayout() {
    cardResizeObserver?.disconnect()
    cardResizeObserver = undefined
    const card = cardElement.value
    if (!card || typeof ResizeObserver === 'undefined') return
    cardResizeObserver = new ResizeObserver(updateCardSize)
    cardResizeObserver.observe(card)
    updateCardSize()
  }

  function observeTargetLayout() {
    targetMutationObserver?.disconnect()
    targetResizeObserver?.disconnect()
    if (layoutTimer !== undefined) {
      window.clearTimeout(layoutTimer)
      pendingTimers.delete(layoutTimer)
      layoutTimer = undefined
    }
    const target = targetElement(currentStep.value)
    if (!target) return
    if (typeof ResizeObserver !== 'undefined') {
      targetResizeObserver = new ResizeObserver(scheduleRectUpdate)
      targetResizeObserver.observe(target)
    }
    if (typeof MutationObserver !== 'undefined') {
      targetMutationObserver = new MutationObserver(scheduleRectUpdate)
      targetMutationObserver.observe(target, { attributes: true, childList: true, subtree: true, characterData: true })
    }
    if (target instanceof HTMLImageElement && !target.complete) {
      target.addEventListener('load', scheduleRectUpdate, { once: true })
    }
    layoutTimer = trackTimeout(() => {
      layoutTimer = undefined
      scheduleRectUpdate()
    }, 450)
  }

  async function waitForTarget(step: TutorialStep) {
    for (let attempt = 0; attempt < 20 && !disposed; attempt += 1) {
      const rect = targetRect(step)
      if (targetExists(step) && rect && rect.width > 0 && rect.height > 0) break
      await waitForFrame()
    }
    const resolvedElement = targetElement(step)
    if (resolvedElement instanceof HTMLImageElement && (!resolvedElement.complete || resolvedElement.naturalWidth === 0)) {
      await new Promise<void>((resolve) => {
        let complete = false
        const finish = () => {
          if (complete) return
          complete = true
          window.clearTimeout(timer)
          pendingTimers.delete(timer)
          resolvedElement.removeEventListener('load', finish)
          resolvedElement.removeEventListener('error', finish)
          resolve()
        }
        resolvedElement.addEventListener('load', finish, { once: true })
        resolvedElement.addEventListener('error', finish, { once: true })
        const timer = trackTimeout(finish, 1200)
      })
    }
    await waitForFrame()
  }

  function disconnectLayoutObservers() {
    targetMutationObserver?.disconnect()
    targetResizeObserver?.disconnect()
    cardResizeObserver?.disconnect()
    targetMutationObserver = undefined
    targetResizeObserver = undefined
    cardResizeObserver = undefined
    if (layoutFrame !== undefined) {
      window.cancelAnimationFrame(layoutFrame)
      pendingFrames.delete(layoutFrame)
      layoutFrame = undefined
    }
    if (layoutTimer !== undefined) {
      window.clearTimeout(layoutTimer)
      pendingTimers.delete(layoutTimer)
      layoutTimer = undefined
    }
  }

  async function showIntroduction() {
    if (disposed || presenting.value) return
    const version = ++presentationVersion
    presenting.value = true
    introductionVisible.value = true
    scenario.value = 'live'
    stopFakeProgress()
    disconnectLayoutObservers()
    spotlightRect.value = emptyRect()
    cardSize.value = { width: 0, height: 0 }
    try {
      await nextTick()
      if (!disposed && visible.value && version === presentationVersion) observeCardLayout()
    } finally {
      if (version === presentationVersion) presenting.value = false
    }
  }

  async function showStep() {
    const step = currentStep.value
    if (!step || disposed || presenting.value) return
    const version = ++presentationVersion
    const isCurrent = () => !disposed && visible.value && version === presentationVersion
    presenting.value = true
    try {
      scenario.value = step.scenario
      stopFakeProgress()
      if (step.scenario === 'framework-installing' || step.scenario === 'download-progress') startFakeProgress()
      const view = step.view || resolveTutorialView(pendingSteps.value, stepIndex.value)
      if (view && host.getActiveView() !== view) host.setActiveView(view)
      await host.prepareStep(step)
      if (!isCurrent()) return
      await nextTick()
      const target = targetElement(step)
      target?.scrollIntoView({ block: 'nearest', inline: 'nearest' })
      await nextTick()
      await waitForTarget(step)
      if (!isCurrent()) return
      observeTargetLayout()
      observeCardLayout()
      updateRect()
      updateCardSize()
    } finally {
      if (version === presentationVersion) presenting.value = false
    }
  }

  function finish() {
    presentationVersion += 1
    presenting.value = false
    stopFakeProgress()
    disconnectLayoutObservers()
    stopPicker(false)
    exitConfirmVisible.value = false
    visible.value = false
    introductionVisible.value = false
    cardSize.value = { width: 0, height: 0 }
    previewing.value = false
    manualPreview.value = false
    scenario.value = 'live'
    host.finishPresentation()
  }

  async function persistSeenStepIds(ids: string[]) {
    const seen = Array.from(new Set([...host.getSeenStepIds(), ...ids]))
    try {
      await host.persistSettings({ onboarding_seen_steps: seen })
      return true
    } catch (reason) {
      host.showError(reason)
      return false
    }
  }

  async function loadBuiltInSteps() {
    try {
      builtinSteps.value = (await host.api.getBuiltinTutorial()).map(normalizeTutorialStep)
      return builtinSteps.value
    } catch (reason) {
      console.warn('[tutorial] builtin tutorial load failed', reason)
      return builtinSteps.value
    }
  }

  async function startOnboarding(forceReplay = false): Promise<boolean> {
    if (!builtinSteps.value.length && !host.getStoredSteps().length) await loadBuiltInSteps()
    const seen = new Set(host.getSeenStepIds())
    const configured = configuredSteps.value
    const next = forceReplay ? configured : configured.filter((step) => !seen.has(step.id))
    if (!next.length) return false
    pendingSteps.value = next
    stepIndex.value = 0
    manualPreview.value = false
    previewing.value = true
    visible.value = true
    await showIntroduction()
    return true
  }

  async function nextStep() {
    if (!visible.value || navigationBusy.value) return
    if (introductionVisible.value) {
      introductionVisible.value = false
      cardSize.value = { width: 0, height: 0 }
      await showStep()
      return
    }
    const step = currentStep.value
    if (!step) return
    if (manualPreview.value) {
      if (stepIndex.value >= pendingSteps.value.length - 1) {
        finish()
        return
      }
      stepIndex.value += 1
      await showStep()
      return
    }
    saving.value = true
    try {
      if (!await persistSeenStepIds([step.id])) return
      if (stepIndex.value >= pendingSteps.value.length - 1) {
        finish()
        return
      }
      stepIndex.value += 1
      await showStep()
    } finally {
      saving.value = false
    }
  }

  async function previousStep() {
    if (!visible.value || navigationBusy.value || introductionVisible.value) return
    if (stepIndex.value === 0) {
      if (!manualPreview.value) await showIntroduction()
      return
    }
    stepIndex.value -= 1
    await showStep()
  }

  async function skip() {
    if (saving.value) return
    if (manualPreview.value) {
      finish()
      return
    }
    const ids = pendingSteps.value.map((step) => step.id)
    saving.value = true
    try {
      if (await persistSeenStepIds(ids)) finish()
    } finally {
      saving.value = false
    }
  }

  function requestFinish() {
    exitConfirmVisible.value = true
  }

  async function confirmFinish() {
    exitConfirmVisible.value = false
    await skip()
  }

  function openEditor() {
    replayMenuVisible.value = false
    editorSteps.value = cloneTutorialSteps(configuredSteps.value)
    editorVisible.value = true
  }

  function openReplayMenu() {
    replayMenuVisible.value = true
  }

  async function replay() {
    replayMenuVisible.value = false
    await startOnboarding(true)
  }

  async function preview() {
    if (saving.value) return
    const steps = tutorialEditorStepValues(editorSteps.value)
    if (!steps.length) {
      host.showError(t('教程至少需要一个步骤'))
      return
    }
    pendingSteps.value = steps.map(normalizeTutorialStep)
    stepIndex.value = 0
    manualPreview.value = true
    introductionVisible.value = false
    previewing.value = true
    editorVisible.value = false
    visible.value = true
    await showStep()
  }

  function addStep() {
    editorSteps.value.push({
      id: `custom-${Date.now()}-${editorSteps.value.length + 1}`,
      selector: '.app-shell',
      title: '新的教程步骤',
      description: '填写这个界面区域的说明。',
      scenario: 'live',
      title_i18n: {
        'zh-CN': '新的教程步骤',
        'zh-TW': '',
        en: '',
        ko: '',
        ja: '',
      },
      description_i18n: {
        'zh-CN': '填写这个界面区域的说明。',
        'zh-TW': '',
        en: '',
        ko: '',
        ja: '',
      },
    })
  }

  function removeStep(index: number) {
    editorSteps.value.splice(index, 1)
  }

  function escapeCssIdentifier(value: string) {
    return value.replace(/[^a-zA-Z0-9_-]/g, (character) => `\\${character}`)
  }

  function selectorForElement(element: Element) {
    const parts: string[] = []
    let current: Element | null = element
    while (current && current !== document.body && parts.length < 8) {
      const tag = current.tagName.toLowerCase()
      const classes = Array.from(current.classList)
        .filter((name) => name.trim())
        .slice(0, 3)
        .map((name) => `.${escapeCssIdentifier(name)}`)
        .join('')
      let part = `${tag}${classes}`
      if (current.id.trim()) {
        parts.unshift(`#${escapeCssIdentifier(current.id.trim())}`)
        break
      }
      const parentElement: Element | null = current.parentElement
      if (parentElement) {
        const sameTag = Array.from(parentElement.children).filter((child: Element) => child.tagName.toLowerCase() === tag)
        if (sameTag.length > 1) part += `:nth-of-type(${sameTag.indexOf(current) + 1})`
      }
      parts.unshift(part)
      current = parentElement
    }
    return parts.join(' > ') || 'body'
  }

  function pickerElement(target: Element | null) {
    if (!target) return null
    return target.closest('button, a, input, select, textarea, [role="button"]') || target
  }

  function updatePickerHover(event: PointerEvent) {
    if (!pickerActive.value) return
    const hovered = document.elementFromPoint(event.clientX, event.clientY)
    const target = hovered instanceof Element && !hovered.closest('.tutorial-picker-banner')
      ? pickerElement(hovered)
      : null
    if (!target) {
      pickerHoverRect.value = emptyRect()
      pickerHoverSelector.value = ''
      return
    }
    pickerHoverRect.value = elementRect(target)
    pickerHoverSelector.value = selectorForElement(target)
  }

  function stopPicker(showEditor = true) {
    pickerActive.value = false
    pickerIndex.value = null
    pickerHoverRect.value = emptyRect()
    pickerHoverSelector.value = ''
    document.removeEventListener('click', handlePickerClick, true)
    document.removeEventListener('pointermove', updatePickerHover, true)
    document.removeEventListener('keydown', handlePickerKeydown, true)
    if (showEditor) editorVisible.value = true
  }

  async function startPicker(index: number) {
    const step = editorSteps.value[index]
    const view = step?.view || resolveTutorialView(editorSteps.value, index)
    if (!view) {
      host.showError(t('请先选择进入页面'))
      return
    }
    stopPicker(false)
    pickerIndex.value = index
    pickerActive.value = true
    editorVisible.value = false
    host.setActiveView(view)
    await nextTick()
    document.addEventListener('click', handlePickerClick, true)
    document.addEventListener('pointermove', updatePickerHover, true)
    document.addEventListener('keydown', handlePickerKeydown, true)
  }

  function handlePickerKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && pickerActive.value) {
      event.preventDefault()
      stopPicker()
    }
  }

  function handlePickerClick(event: MouseEvent) {
    if (!pickerActive.value) return
    const target = event.target instanceof Element ? pickerElement(event.target) : null
    if (!target || target.closest('.tutorial-picker-banner')) return
    event.preventDefault()
    event.stopPropagation()
    event.stopImmediatePropagation()
    const index = pickerIndex.value
    if (index !== null && editorSteps.value[index]) editorSteps.value[index].selector = selectorForElement(target)
    stopPicker()
  }

  function openImport() {
    importText.value = ''
    importPreview.value = null
    importError.value = ''
    importVisible.value = true
  }

  async function previewImport() {
    if (importing.value || !importText.value.trim()) return
    importing.value = true
    importError.value = ''
    importPreview.value = null
    try {
      const steps = (await host.api.parseTutorialConfig(importText.value)).map(normalizeTutorialStep)
      if (!steps.length) throw new Error(t('教程至少需要一个步骤'))
      if (!validateTutorialLanguageMaps(steps)) throw new Error(t('教程格式必须包含四种语言'))
      importPreview.value = steps
    } catch (reason) {
      importError.value = reason instanceof Error ? reason.message : String(reason)
    } finally {
      importing.value = false
    }
  }

  function applyImport() {
    if (!importPreview.value) return
    editorSteps.value = cloneTutorialSteps(importPreview.value)
    importVisible.value = false
    host.showSuccess(t('教程格式已导入'))
  }

  async function copyFormat() {
    await host.run('正在复制教程格式', () => host.api.copyText(tutorialFormatTemplate))
  }

  async function exportConfig() {
    try {
      exportCode.value = await host.api.serializeTutorialConfig(tutorialEditorStepValues(editorSteps.value))
      exportVisible.value = true
    } catch (reason) {
      host.showError(reason)
    }
  }

  async function copyExport() {
    await host.run('正在复制教程格式', () => host.api.copyText(exportCode.value))
  }

  function selectExport(event: FocusEvent) {
    if (event.target instanceof HTMLTextAreaElement) event.target.select()
  }

  function validateSelector(selector: string) {
    try {
      document.querySelector(selector)
      return true
    } catch {
      return false
    }
  }

  async function saveEditor() {
    if (saving.value) return
    const steps = tutorialEditorStepValues(editorSteps.value)
    if (!steps.length) {
      host.showError(t('教程至少需要一个步骤'))
      return
    }
    const seenIds = new Set<string>()
    for (const step of steps) {
      if (!step.selector || !step.title || !step.description) {
        host.showError(t('教程步骤不能为空'))
        return
      }
      if (!validateSelector(step.selector)) {
        host.showError(`${t('教程选择器无效')}: ${step.selector}`)
        return
      }
      if (seenIds.has(step.id)) {
        host.showError(t('教程步骤 ID 不能重复'))
        return
      }
      seenIds.add(step.id)
    }
    saving.value = true
    try {
      await host.persistSettings({ tutorial_steps: steps, onboarding_seen_steps: [] })
      editorVisible.value = false
      host.showSuccess(t('教程配置已保存'))
      await startOnboarding()
    } catch (reason) {
      host.showError(reason)
    } finally {
      saving.value = false
    }
  }

  async function restoreBuiltIn() {
    if (saving.value) return
    saving.value = true
    try {
      await host.persistSettings({ tutorial_steps: [], onboarding_seen_steps: [] })
      editorVisible.value = false
      host.showSuccess(t('已恢复内置教程'))
      await loadBuiltInSteps()
      await startOnboarding()
    } catch (reason) {
      host.showError(reason)
    } finally {
      saving.value = false
    }
  }

  function jumpToView(view: View) {
    if (!visible.value || navigationBusy.value) return
    const index = pendingSteps.value.findIndex((step, index) => index > stepIndex.value
      && (step.view === view || resolveTutorialView(pendingSteps.value, index) === view))
    if (index < 0) return
    stepIndex.value = index
    void showStep()
  }

  async function overlayClick(event: MouseEvent) {
    if (!visible.value || navigationBusy.value || introductionVisible.value) return
    const clicked = event.target instanceof Element ? event.target : null
    if (clicked?.closest('.tutorial-card')) return
    if (zoomStepActive.value) return
    const rect = spotlightRect.value
    if (event.clientX < rect.left || event.clientX > rect.left + rect.width
      || event.clientY < rect.top || event.clientY > rect.top + rect.height) return
    const overlay = event.currentTarget instanceof HTMLElement ? event.currentTarget : null
    if (!overlay) return
    const previousPointerEvents = overlay.style.pointerEvents
    overlay.style.pointerEvents = 'none'
    const underlying = document.elementFromPoint(event.clientX, event.clientY)
    overlay.style.pointerEvents = previousPointerEvents
    const nav = underlying?.closest('.nav-item')
    if (nav) {
      const index = Array.from(document.querySelectorAll('.nav-item')).indexOf(nav)
      const view = tutorialNavigationOrder[index]
      if (view) host.setActiveView(view)
    } else {
      const metric = underlying?.closest('.metric')
      if (metric) {
        const view = metricViews[metric.getAttribute('data-tutorial') || '']
        if (view) host.setActiveView(view)
      }
    }
    event.preventDefault()
    event.stopPropagation()
    await nextStep()
  }

  function editableTarget(target: EventTarget | null) {
    return target instanceof Element && !!target.closest(
      'input, textarea, select, [contenteditable]:not([contenteditable="false"]), [role="textbox"], [role="combobox"], [role="slider"], [role="spinbutton"], [role="listbox"]',
    )
  }

  function navigateFromInput(direction: -1 | 1) {
    if (!inputNavigationActive.value || navigationBusy.value) return
    // Finishing is explicit: residual wheel events must never close the tutorial.
    if (direction > 0 && !introductionVisible.value && stepIndex.value >= pendingSteps.value.length - 1) return
    const action = direction < 0 ? previousStep() : nextStep()
    void action.catch((reason) => host.showError(reason))
  }

  function handleNavigationKeydown(event: KeyboardEvent) {
    if (!inputNavigationActive.value || event.defaultPrevented || event.isComposing
      || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey || editableTarget(event.target)) return
    const direction = event.key === 'ArrowUp' || event.key === 'ArrowLeft' ? -1
      : event.key === 'ArrowDown' || event.key === 'ArrowRight' ? 1 : 0
    if (!direction) return
    event.preventDefault()
    if (!event.repeat) navigateFromInput(direction)
  }

  function resetWheelGesture() {
    wheelLastTime = -Infinity
    wheelDistance = 0
    wheelGestureConsumed = false
  }

  function scrollableTutorialRegion(target: Element | null, direction: number) {
    const boundary = target?.closest('.tutorial-card, .modal')
    if (!boundary) return null
    let element: Element | null = target
    while (element) {
      if (element instanceof HTMLElement && element.scrollHeight > element.clientHeight + 1
        && /^(auto|scroll)$/.test(window.getComputedStyle(element).overflowY)
        && (direction < 0 ? element.scrollTop > 1 : element.scrollTop + element.clientHeight < element.scrollHeight - 1)) return element
      if (element === boundary) break
      element = element.parentElement
    }
    return null
  }

  function handleNavigationWheel(event: WheelEvent) {
    if (!inputNavigationActive.value || event.defaultPrevented || event.ctrlKey || event.metaKey
      || event.altKey || event.shiftKey || Math.abs(event.deltaX) > Math.abs(event.deltaY)
      || event.deltaY === 0 || editableTarget(event.target)) return
    const now = performance.now()
    if (now - wheelLastTime > 220) resetWheelGesture()
    wheelLastTime = now
    const distance = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? window.innerHeight : 1)
    const target = event.target instanceof Element ? event.target : null
    let scrollable = scrollableTutorialRegion(target, distance)
    if (scrollable) {
      wheelGestureConsumed = true
      return
    }
    const rect = spotlightRect.value
    if (target?.closest('.tutorial-overlay') && !target.closest('.tutorial-card')
      && event.clientX >= rect.left && event.clientX <= rect.left + rect.width
      && event.clientY >= rect.top && event.clientY <= rect.top + rect.height) {
      const underlying = document.elementsFromPoint(event.clientX, event.clientY)
        .find((element) => !element.closest('.tutorial-overlay')) || null
      scrollable = scrollableTutorialRegion(underlying, distance)
      if (scrollable) {
        event.preventDefault()
        scrollable.scrollTop += distance / Math.max(host.getZoom(), 0.01)
        wheelGestureConsumed = true
        return
      }
    }
    event.preventDefault()
    if (navigationBusy.value) wheelGestureConsumed = true
    if (wheelGestureConsumed) return
    if (Math.sign(wheelDistance) !== Math.sign(distance)) wheelDistance = 0
    wheelDistance += distance
    if (Math.abs(wheelDistance) < 45) return
    wheelGestureConsumed = true
    navigateFromInput(wheelDistance < 0 ? -1 : 1)
  }

  function setInputListeners(active: boolean) {
    resetWheelGesture()
    if (active === inputListenersActive) return
    inputListenersActive = active
    if (active) {
      document.addEventListener('keydown', handleNavigationKeydown, true)
      document.addEventListener('wheel', handleNavigationWheel, { capture: true, passive: false })
    } else {
      document.removeEventListener('keydown', handleNavigationKeydown, true)
      document.removeEventListener('wheel', handleNavigationWheel, true)
    }
  }

  function dispose() {
    if (disposed) return
    disposed = true
    presentationVersion += 1
    setInputListeners(false)
    stopPicker(false)
    stopFakeProgress()
    disconnectLayoutObservers()
    for (const frame of pendingFrames) {
      window.cancelAnimationFrame(frame)
      pendingFrameResolvers.get(frame)?.()
    }
    pendingFrames.clear()
    pendingFrameResolvers.clear()
    for (const timer of pendingTimers) window.clearTimeout(timer)
    pendingTimers.clear()
    window.removeEventListener('resize', scheduleRectUpdate)
    document.removeEventListener('scroll', scheduleRectUpdate, true)
    document.body.classList.remove('tutorial-mode-active')
  }

  watch(visible, (isVisible) => {
    document.body.classList.toggle('tutorial-mode-active', isVisible)
    if (isVisible) {
      void nextTick(() => {
        updateRect()
        updateCardSize()
      })
    }
  }, { immediate: true })
  watch(inputNavigationActive, (active) => setInputListeners(active && !disposed), { immediate: true, flush: 'sync' })
  watch(() => host.getZoom(), () => {
    scheduleRectUpdate()
    void nextTick(updateCardSize)
  })
  watch(() => host.getLocale(), () => {
    void nextTick(() => {
      scheduleRectUpdate()
      updateCardSize()
    })
  })
  onMounted(() => {
    window.addEventListener('resize', scheduleRectUpdate)
    document.addEventListener('scroll', scheduleRectUpdate, true)
  })
  onUnmounted(dispose)

  return {
    visible,
    introductionVisible,
    previewing,
    manualPreview,
    scenario,
    pendingSteps,
    stepIndex,
    saving,
    navigationBusy,
    currentStep,
    zoomStepActive,
    zoomPercent,
    canZoomOut,
    canZoomIn,
    adjustZoom,
    progress,
    configuredSteps,
    repositoryLoadingActive,
    blocksRepositoryRefresh,
    spotlightStyle,
    cardStyle,
    cardElement,
    pickerActive,
    pickerHoverRect,
    pickerHoverSelector,
    pickerHighlightStyle,
    editorVisible,
    editorSteps,
    exitConfirmVisible,
    replayMenuVisible,
    importVisible,
    importText,
    importPreview,
    importError,
    importing,
    exportVisible,
    exportCode,
    fakeProgressPercent,
    tutorialLocaleOptions,
    tutorialScenarioOptions,
    tutorialFormatTemplate,
    formatTemplate: tutorialFormatTemplate,
    stepText,
    updateRect,
    showStep,
    finish,
    startOnboarding,
    nextStep,
    previousStep,
    requestFinish,
    confirmFinish,
    openEditor,
    openReplayMenu,
    replay,
    preview,
    addStep,
    removeStep,
    startPicker,
    stopPicker,
    openImport,
    previewImport,
    applyImport,
    copyFormat,
    exportConfig,
    copyExport,
    selectExport,
    saveEditor,
    restoreBuiltIn,
    jumpToView,
    overlayClick,
    loadBuiltInSteps,
    dispose,
  }
}
