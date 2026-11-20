<script setup lang="ts">
import { shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import MarkdownBody from '../components/MarkdownBody.vue'
import { t } from '../i18n'
import { installNoteKey, type InstallNote } from './planNotes'

const props = defineProps<{ notes: readonly InstallNote[] }>()
const emit = defineEmits<{ continue: [notes: readonly InstallNote[]]; cancel: [] }>()
const READ_SECONDS = 5
const remaining = shallowRef(READ_SECONDS)

watch(() => JSON.stringify(props.notes.map(installNoteKey)), (_, __, onCleanup) => {
  remaining.value = READ_SECONDS
  const deadline = performance.now() + READ_SECONDS * 1000
  const timer = setInterval(() => {
    remaining.value = Math.max(0, Math.ceil((deadline - performance.now()) / 1000))
    if (!remaining.value) clearInterval(timer)
  }, 100)
  onCleanup(() => clearInterval(timer))
}, { immediate: true })

function continueReading() {
  if (remaining.value === 0) emit('continue', props.notes)
}
</script>

<template>
  <AppModal :show="true" :title="t('dialogs.readModNoteFirst')" max-width="min(640px, 94%)"
    :mask-closable="false" card-class="mod-install-note"
    @update:show="(show: boolean) => { if (!show) emit('cancel') }">
    <article v-for="article in notes" :key="article.id" class="install-note">
      <h3 class="install-note-name">{{ article.name }}</h3>
      <MarkdownBody :source="article.text" />
    </article>
    <template #footer>
      <NButton @click="emit('cancel')">{{ t('common.action.cancel') }}</NButton>
      <NButton type="primary" :disabled="remaining > 0" @click="continueReading">
        {{ t('dialogs.continueAfterReading') }}{{ remaining ? ` (${remaining})` : '' }}
      </NButton>
    </template>
  </AppModal>
</template>

<style scoped>
.install-note + .install-note { margin-top: 16px; }
.install-note-name { margin: 0 0 12px; overflow-wrap: anywhere; }
</style>
