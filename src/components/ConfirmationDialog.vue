<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { X } from 'lucide-vue-next'
import { t } from '../i18n'
import { trapModalFocus } from '../composables/modalFocus'
import type { ConfirmationRequest } from '../composables/useConfirmationDialog'

const props = defineProps<{ request: ConfirmationRequest | null }>()
const emit = defineEmits<{ answer: [confirmed: boolean] }>()
const cancelButton = ref<HTMLButtonElement | null>(null)
const confirmButton = ref<HTMLButtonElement | null>(null)
const backdropPressed = ref(false)
let trigger: HTMLElement | null = null

watch(() => props.request, async (request) => {
  backdropPressed.value = false
  if (request) {
    trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
    await nextTick()
    if (props.request === request) {
      (request.acknowledge ? confirmButton.value : cancelButton.value)?.focus()
    }
  } else {
    await nextTick()
    if (!props.request && trigger?.isConnected) trigger.focus()
  }
})
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="request" class="modal-backdrop confirmation-backdrop"
        @pointerdown="backdropPressed = $event.target === $event.currentTarget"
        @pointercancel="backdropPressed = false"
        @click.self="backdropPressed && emit('answer', false)"
        @keydown.esc.stop.prevent="emit('answer', false)">
        <section class="modal confirmation-modal" role="alertdialog" aria-modal="true"
          aria-labelledby="confirmation-title" aria-describedby="confirmation-message"
          @keydown.tab.stop="trapModalFocus">
          <header class="modal-head">
            <h3 id="confirmation-title">{{ request.title }}</h3>
            <button class="icon-button" :title="t('关闭')" :aria-label="t('关闭')" @click="emit('answer', false)"><X :size="17" /></button>
          </header>
          <p id="confirmation-message" class="confirmation-message">{{ request.message }}</p>
          <footer class="modal-actions">
            <button v-if="!request.acknowledge" ref="cancelButton" class="secondary-button" @click="emit('answer', false)">{{ t('取消') }}</button>
            <button ref="confirmButton" :class="request.danger ? 'danger-button' : 'primary-button'" @click="emit('answer', true)">{{ t('确定') }}</button>
          </footer>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>
