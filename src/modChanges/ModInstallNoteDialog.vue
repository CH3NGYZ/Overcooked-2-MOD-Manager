<script setup lang="ts">
import { computed, shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import { t } from '../i18n'
import { renderReadme } from '../markdown'
import { installNoteKey, type InstallNote } from './planNotes'

const props = defineProps<{ notes: readonly InstallNote[] }>()
const emit = defineEmits<{ continue: [notes: readonly InstallNote[]]; cancel: [] }>()
const articles = computed(() => props.notes.map(note => ({ ...note, html: renderReadme(note.text) })))
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
    <article v-for="article in articles" :key="article.id" class="install-note">
      <h3 class="install-note-name">{{ article.name }}</h3>
      <div class="install-note-markdown markdown-body" v-html="article.html"></div>
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
.install-note-name { margin: 0 0 8px; overflow-wrap: anywhere; }
.install-note-markdown { line-height: 1.65; overflow-wrap: anywhere; }
.install-note-markdown :deep(> :last-child) { margin-bottom: 0; }
.install-note-markdown :deep(img), .install-note-markdown :deep(svg) { max-width: 100%; }
.install-note-markdown :deep(pre) { overflow-x: auto; }
.install-note-markdown :deep(a) { color: var(--oc2-accent); }
</style>
