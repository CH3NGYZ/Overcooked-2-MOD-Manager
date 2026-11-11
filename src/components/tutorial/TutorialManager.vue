<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, shallowRef, watch } from 'vue'
import { NAlert, NButton, NCollapse, NCollapseItem, NEmpty, NSpin, NTag } from 'naive-ui'
import AppModal from '../AppModal.vue'
import { useCardExpansion } from '../../composables/useCardExpansion'
import { collapseCardWithScroll, scrollExpandedCardIntoView } from '../../expandScroll'
import { t } from '../../i18n'
import { backendMessageOf } from '../../locale/backend'
import { api } from '../../services/tauri'
import { useNotify } from '../../useNotify'
import { replayTutorial, startTutorial } from '../../tutorial/engine'
import { normalizeStepIds } from '../../tutorial/progress'
import { tutorialSections } from '../../tutorial/steps'
import type { TutorialSection } from '../../tutorial/types'

const props = defineProps<{ show: boolean }>()
const emit = defineEmits<{ 'update:show': [value: boolean] }>()
const message = useNotify()
const loading = shallowRef(false)
const watched = shallowRef<Set<string> | null>(null)
const error = shallowRef('')
const expansion = useCardExpansion()
const { outerKeys } = expansion
type Playback = { sectionId: string; fromStepId?: string } | 'all'
let pending: Playback | null = null
let generation = 0
let disposed = false

// The same catalog drives playback. Reopening reevaluates platform conditions;
// a hidden step never contributes to this platform's count or progress.
const sections = computed(() => {
  void props.show
  return tutorialSections.map((section: TutorialSection) => {
    const steps = section.steps.filter(step => !step.when || step.when())
    return { ...section, steps, seen: steps.filter(step => watched.value?.has(step.id)).length }
  }).filter(section => section.steps.length > 0)
})
const totals = computed(() => sections.value.reduce((sum, section) => ({
  seen: sum.seen + section.seen,
  total: sum.total + section.steps.length,
}), { seen: 0, total: 0 }))

async function loadProgress() {
  const ticket = ++generation
  loading.value = true
  watched.value = null
  error.value = ''
  try {
    const config = await api.config()
    if (disposed || ticket !== generation || !props.show) return
    watched.value = new Set(normalizeStepIds(config.tutorial_watched_steps ?? []))
  } catch (reason) {
    if (disposed || ticket !== generation || !props.show) return
    error.value = backendMessageOf(reason)
    message.error(error.value)
  } finally {
    if (!disposed && ticket === generation) loading.value = false
  }
}

function close() {
  pending = null
  emit('update:show', false)
}
function play(selection: Playback) {
  if (!props.show || disposed || pending) return
  pending = selection
  emit('update:show', false)
}
function afterLeave() {
  if (disposed || props.show) return
  const selection = pending
  pending = null
  if (!selection) return
  try {
    if (selection === 'all') replayTutorial()
    else startTutorial(selection)
  } catch (reason) { message.error(backendMessageOf(reason)) }
}

function toggleSection({ name, event }: { name: string | number; event: MouseEvent }) {
  const key = String(name)
  const card = (event.currentTarget as HTMLElement | null)?.closest<HTMLElement>('[data-tutorial-manager-section]')
  if (!card) return
  const body = () => card.querySelector('.tutorial-manager-steps')
  if (expansion.isOuterOpen(key)) {
    collapseCardWithScroll({ card, body: body() }, () => expansion.closeOuter(key))
    return
  }
  expansion.openOuter(key)
  void nextTick(() => {
    if (disposed || !props.show || !expansion.isOuterOpen(key) || !card.isConnected) return
    scrollExpandedCardIntoView({ card, anchor: card, body: body(), bounds: { top: 0 } })
  })
}

watch(() => props.show, show => {
  generation += 1
  if (show) {
    pending = null
    expansion.closeAll()
    void loadProgress()
  } else loading.value = false
}, { immediate: true, flush: 'sync' })
onBeforeUnmount(() => { disposed = true; generation += 1; pending = null })
</script>

<template>
  <AppModal :show="show" :title="t('tutorial.manager.title')" max-width="min(760px, 92vw)"
    card-class="tutorial-manager-modal" @update:show="close" @after-leave="afterLeave">
    <div class="tutorial-manager" data-tutorial-manager>
      <p class="tutorial-manager-description">{{ t('tutorial.manager.description') }}</p>
      <div class="tutorial-manager-summary" aria-live="polite">
        <span v-if="loading" class="tutorial-manager-loading"><n-spin size="small" />{{ t('tutorial.manager.loading') }}</span>
        <span v-else-if="watched">{{ t('tutorial.manager.progress', totals) }}</span>
        <span v-else>{{ t('tutorial.manager.progressUnknown') }}</span>
      </div>
      <n-alert v-if="error" type="warning" class="tutorial-manager-error">
        {{ t('tutorial.manager.readFailed') }}
        <div class="tutorial-manager-retry"><n-button size="small" :loading="loading" @click="loadProgress">{{ t('tutorial.manager.retry') }}</n-button></div>
      </n-alert>
      <n-collapse v-if="sections.length" :expanded-names="outerKeys" :trigger-areas="['main', 'arrow']"
        class="tutorial-manager-sections" @item-header-click="toggleSection">
        <n-collapse-item v-for="section in sections" :key="section.id" :name="section.id"
          :data-tutorial-manager-section="section.id">
          <template #header>
            <button type="button" class="tutorial-manager-section-heading" :aria-expanded="expansion.isOuterOpen(section.id)"
              :aria-controls="`tutorial-manager-steps-${section.id}`">
              <b>{{ t(section.titleKey) }}</b>
              <span class="tutorial-manager-count">{{ t('tutorial.manager.stepCount', { count: section.steps.length }) }}</span>
              <span v-if="watched" class="tutorial-manager-count">{{ t('tutorial.manager.progress', { seen: section.seen, total: section.steps.length }) }}</span>
            </button>
          </template>
          <template #header-extra>
            <n-button size="small" secondary :data-tutorial-manager-play-section="section.id"
              @click.stop="play({ sectionId: section.id })">{{ t('tutorial.manager.playSection') }}</n-button>
          </template>
          <ol :id="`tutorial-manager-steps-${section.id}`" class="tutorial-manager-steps">
            <li v-for="step in section.steps" :key="step.id" class="tutorial-manager-step">
              <span class="tutorial-manager-step-title">{{ t(step.titleKey) }}</span>
              <n-tag v-if="watched" size="small" :bordered="false">
                {{ t(watched.has(step.id) ? 'tutorial.manager.seen' : 'tutorial.manager.unseen') }}
              </n-tag>
              <n-button size="small" quaternary :data-tutorial-manager-play-step="step.id"
                :aria-label="t('tutorial.manager.playStepLabel', { title: t(step.titleKey) })"
                @click="play({ sectionId: section.id, fromStepId: step.id })">{{ t('tutorial.manager.playStep') }}</n-button>
            </li>
          </ol>
        </n-collapse-item>
      </n-collapse>
      <n-empty v-else :description="t('tutorial.manager.empty')" />
    </div>
    <template #footer>
      <n-button data-tutorial-manager-close @click="close">{{ t('nav.close') }}</n-button>
      <n-button type="primary" :disabled="!sections.length" data-tutorial-manager-play-all @click="play('all')">
        {{ t('tutorial.manager.playAll') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.tutorial-manager { display: flex; flex-direction: column; gap: 14px; width: min(680px, 100%); min-width: 0; }
.tutorial-manager-description { margin: 0; color: var(--oc2-text-3); font-size: 13px; line-height: 1.6; }
.tutorial-manager-summary { color: var(--oc2-text-2); font-size: 13px; }
.tutorial-manager-loading { display: inline-flex; align-items: center; gap: 10px; }
.tutorial-manager-error { margin-bottom: 2px; }
.tutorial-manager-retry { margin-top: 8px; }
.tutorial-manager-section-heading { display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px; width: 100%; padding: 0 10px 0 0; border: 0; background: none; color: inherit; font: inherit; text-align: left; cursor: pointer; }
.tutorial-manager-section-heading:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 4px; border-radius: 3px; }
.tutorial-manager-count { color: var(--oc2-text-3); font-size: 12px; font-weight: 400; }
.tutorial-manager-steps { display: flex; flex-direction: column; gap: 6px; list-style: none; padding: 0; margin: 0; }
.tutorial-manager-step { display: flex; align-items: center; gap: 10px; min-width: 0; padding: 8px 10px; border-radius: 7px; background: var(--oc2-row); }
.tutorial-manager-step-title { flex: 1; min-width: 0; overflow-wrap: anywhere; font-size: 13px; }
.tutorial-manager-step :deep(.n-button), .tutorial-manager-step :deep(.n-tag) { flex-shrink: 0; }
</style>
