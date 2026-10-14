<template>
  <Teleport to="body">
    <div v-if="shouldShowGuide" class="modal-backdrop platform-setup-overlay"
      @pointerdown="backdropPressed = $event.target === $event.currentTarget"
      @pointercancel="backdropPressed = false" @click.self="backdropPressed && closeGuide()">
      <section ref="dialog" class="platform-setup-dialog" role="dialog" aria-modal="true"
        aria-labelledby="platform-setup-title" tabindex="-1"
        @keydown.esc.stop.prevent="closeGuide" @keydown.tab.stop="trapModalFocus">
        <header class="platform-setup-header">
          <h2 id="platform-setup-title">{{ t('CrossOver 设置向导') }}</h2>
          <button class="close-button" @click="closeGuide" :title="t('关闭')" :aria-label="t('关闭')"><X :size="20" /></button>
        </header>
        <div ref="content" class="platform-setup-content" tabindex="0" :aria-label="t('教程内容')">
          <div class="setup-step" :key="stepIndex">
            <p class="setup-step-progress" aria-live="polite">{{ t('步骤 {current} / {total}').replace('{current}', String(stepIndex + 1)).replace('{total}', String(steps.length)) }}</p>
            <h3 ref="stepHeading" tabindex="-1">{{ t(currentStep.title) }}</h3>
            <p v-for="paragraph in currentStep.paragraphs" :key="paragraph">{{ t(paragraph) }}</p>
            <template v-for="image in currentStep.images" :key="image">
              <img v-if="getTutorialImagePath(image)" :src="getTutorialImagePath(image)"
                :alt="t(currentStep.title)" class="tutorial-image" />
            </template>
            <div v-if="stepIndex === steps.length - 1" class="info-box">
              <strong>{{ t('提示：') }}</strong>
              {{ t('macOS 获取的游戏目录为 CrossOver 容器内路径，例如：/Users/用户名/Library/Application Support/CrossOver/Bottles/厨房/drive_c/Program Files (x86)/Steam/steamapps/common/Overcooked! 2') }}
            </div>
          </div>
        </div>
        <footer class="platform-setup-footer">
          <button class="secondary-button" :disabled="stepIndex === 0" @click="goToStep(stepIndex - 1)"><ArrowLeft :size="16" />{{ t('上一步') }}</button>
          <button v-if="stepIndex < steps.length - 1" class="primary-button" @click="goToStep(stepIndex + 1)">{{ t('下一步') }}<ArrowRight :size="16" /></button>
          <button v-else class="primary-button" @click="markCompleted"><Check :size="16" />{{ t('我已完成所需操作') }}</button>
        </footer>
      </section>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref, computed, nextTick, watch, onUnmounted } from 'vue'
import { ArrowLeft, ArrowRight, Check, X } from 'lucide-vue-next'
import { crossoverSetupSteps as steps } from '../platformSetupSteps'
import { t } from '../i18n'
import { trapModalFocus } from '../composables/modalFocus'
import type { AppPlatform, GameRuntime } from '../types'

const props = defineProps<{
  visible: boolean
  platform?: AppPlatform
  runtime?: GameRuntime
}>()

const emit = defineEmits<{
  close: []
  completed: []
}>()

const dialog = ref<HTMLElement | null>(null)
const content = ref<HTMLElement | null>(null)
const stepHeading = ref<HTMLElement | null>(null)
const stepIndex = ref(0)
const backdropPressed = ref(false)
const currentStep = computed(() => steps[stepIndex.value]!)
let trigger: HTMLElement | null = null
const tutorialImages = import.meta.glob<string>('../assets/tutorial/**/*.{png,jpg,jpeg,webp}', {
  eager: true, query: '?url', import: 'default',
})

const shouldShowGuide = computed(() => {
  return props.visible && props.platform === 'macos' && props.runtime === 'wine'
})

function closeGuide() {
  emit('close')
}

function markCompleted() {
  emit('completed')
  closeGuide()
}

function getTutorialImagePath(imageName: string): string {
  return tutorialImages[`../assets/tutorial/${imageName}`] || ''
}

async function goToStep(index: number) {
  if (index < 0 || index >= steps.length || !shouldShowGuide.value) return
  stepIndex.value = index
  await nextTick()
  if (!shouldShowGuide.value) return
  content.value?.scrollTo({ top: 0, behavior: 'instant' })
  stepHeading.value?.focus({ preventScroll: true })
}

watch(shouldShowGuide, async (visible) => {
  backdropPressed.value = false
  if (visible) {
    trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
    stepIndex.value = 0
  }
  await nextTick()
  if (visible && shouldShowGuide.value) {
    content.value?.scrollTo({ top: 0, behavior: 'instant' })
    dialog.value?.focus()
  } else if (!visible && trigger?.isConnected) trigger.focus()
}, { immediate: true })

onUnmounted(() => { if (trigger?.isConnected) trigger.focus() })
</script>

<style scoped>
.platform-setup-overlay {
  background: rgba(0, 0, 0, 0.75);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 10000;
  padding: 0;
}
.platform-setup-dialog {
  background: var(--surface);
  border: 1px solid var(--line);
  border-radius: 10px;
  width: min(800px, 90%);
  height: 90%;
  max-width: 90%;
  max-height: 90%;
  min-height: 0;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  box-shadow: var(--shadow);
}
.platform-setup-header {
  flex: 0 0 auto;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--line);
}
.platform-setup-header h2 { margin: 0; font-size: 16px; }
.close-button {
  flex: 0 0 28px;
  width: 28px;
  height: 28px;
  padding: 0;
  border: 0;
  background: transparent;
  color: var(--muted);
  display: grid;
  place-items: center;
}
.close-button:hover { color: var(--text); }
.platform-setup-content {
  flex: 1 1 0;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  padding: 16px;
  overflow-wrap: anywhere;
}
.setup-step h3 { margin: 0 0 12px; color: var(--blue); font-size: 16px; }
.setup-step p { margin: 0 0 12px; color: var(--text); line-height: 1.6; }
.setup-step .setup-step-progress { color: var(--muted); font-size: 12px; }
.tutorial-image {
  display: block;
  width: 100%;
  height: auto;
  margin: 12px 0;
  border: 1px solid var(--line);
  border-radius: 6px;
}
.platform-setup-footer {
  flex: 0 0 auto;
  padding: 12px 16px;
  border-top: 1px solid var(--line);
  display: flex;
  justify-content: space-between;
  gap: 12px;
}
.platform-setup-footer button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  min-width: 0;
  white-space: normal;
}
.info-box {
  background: var(--surface-2);
  border: 1px solid var(--line);
  border-radius: 6px;
  padding: 12px;
  margin-top: 12px;
  color: var(--blue);
  line-height: 1.6;
}
</style>
