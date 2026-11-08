<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import AppModal from './AppModal.vue'
import { t } from '../i18n'
import { renderReadme, renderMermaidBlocks } from '../markdown'
import {
  modInstallNote, noteSecondsRemaining, continueModInstallNote, finishModInstallNotes,
} from '../modInstallNotes'

const body = ref<HTMLElement | null>(null)
// 业务取消立即 resolve，展示快照留到 AppModal 的退场结束；不让空正文参与缩小。
const displayedNote = shallowRef(modInstallNote.value)
const html = computed(() => renderReadme(displayedNote.value?.text ?? ''))
watch(modInstallNote, async (note) => {
  if (!note) return
  displayedNote.value = note
  await nextTick()
  body.value?.closest('.app-modal-scroll')?.scrollTo(0, 0)
  void renderMermaidBlocks(body.value)
}, { flush: 'post', immediate: true })
function afterLeave() {
  // 动画期间开启新一轮时，旧的 after-leave 不能清掉新内容。
  if (!modInstallNote.value) displayedNote.value = null
}
onUnmounted(() => finishModInstallNotes())
</script>

<template>
  <AppModal :show="!!modInstallNote" :title="t('dialogs.readModNoteFirst')"
    max-width="min(720px, 94%)" :mask-closable="false" card-class="mod-install-note"
    @update:show="(value: boolean) => { if (!value) finishModInstallNotes() }"
    @after-leave="afterLeave">
    <h3>{{ displayedNote?.name }}</h3>
    <div ref="body" class="install-note-markdown" v-html="html"></div>
    <template #footer>
      <NButton @click="finishModInstallNotes()">{{ t('common.action.cancel') }}</NButton>
      <NButton type="primary" :disabled="noteSecondsRemaining > 0" @click="continueModInstallNote">
        {{ t('dialogs.continueAfterReading') }}{{ noteSecondsRemaining ? ` (${noteSecondsRemaining})` : '' }}
      </NButton>
    </template>
  </AppModal>
</template>

<style scoped>
h3 { margin: 0 0 12px; }
.install-note-markdown { line-height: 1.7; overflow-wrap: anywhere; }
.install-note-markdown :deep(img), .install-note-markdown :deep(svg) { max-width: 100%; }
.install-note-markdown :deep(pre) { overflow-x: auto; padding: 12px; background: var(--oc2-sunken); }
.install-note-markdown :deep(table) { display: block; overflow-x: auto; border-collapse: collapse; }
.install-note-markdown :deep(td), .install-note-markdown :deep(th) { border: 1px solid var(--oc2-row-line); padding: 6px; }
.install-note-markdown :deep(a) { color: var(--oc2-accent); }
.install-note-markdown :deep(blockquote) { border-left: 3px solid var(--oc2-row-line); margin-left: 0; padding-left: 12px; }
</style>
