<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { FileText, RefreshCw, X } from 'lucide-vue-next'
import { locale, t } from '../i18n'
import { markdownLinkUrl, renderMarkdown, repositoryMarkdownBaseUrl } from '../markdown'
import { api } from '../services/tauri'
import { trapModalFocus } from '../composables/modalFocus'
import type { Repository, RepositoryReadme } from '../types'

const props = defineProps<{ target: { repository: Repository; kind: 'note' | 'readme' } | null }>()
const emit = defineEmits<{ close: [] }>()
const document = ref<RepositoryReadme | null>(null)
const loading = ref(false)
const error = ref('')
const dialog = ref<HTMLElement | null>(null)
const backdropPressed = ref(false)
const content = computed(() => props.target?.kind === 'note'
  ? props.target.repository.note_i18n?.[locale.value] || props.target.repository.note
  : document.value?.content ?? '')
const contentHtml = computed(() => renderMarkdown(content.value, props.target?.kind === 'readme'
  ? document.value?.url : props.target ? repositoryMarkdownBaseUrl(props.target.repository) : undefined))
let revision = 0
let requestId: string | undefined
let trigger: HTMLElement | null = null

function cancel() {
  revision += 1
  if (requestId) void api.cancelRepositoryReadme(requestId).catch(() => undefined)
  requestId = undefined
  loading.value = false
}

async function load() {
  cancel()
  const target = props.target
  if (!target || target.kind !== 'readme') return
  const current = revision
  const id = crypto.randomUUID()
  requestId = id
  loading.value = true
  document.value = null
  error.value = ''
  try {
    const result = await api.repositoryReadme(target.repository.id, id)
    if (current === revision) document.value = result
  } catch (reason) {
    if (current === revision) error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    if (current === revision) { loading.value = false; requestId = undefined }
  }
}

watch(() => props.target, async (target, previous) => {
  cancel()
  document.value = null
  error.value = ''
  if (!target) {
    await nextTick()
    if (trigger?.isConnected) trigger.focus()
    return
  }
  if (!previous) trigger = window.document.activeElement instanceof HTMLElement ? window.document.activeElement : null
  await nextTick()
  dialog.value?.focus()
  if (target === props.target) await load()
}, { immediate: true })

async function openLink(event: MouseEvent) {
  const url = markdownLinkUrl(event)
  if (!url) return
  try {
    await api.openReleasePage(url)
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  }
}

function close() { cancel(); emit('close') }
onUnmounted(cancel)
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="target" class="modal-backdrop repository-document-backdrop"
        @pointerdown="backdropPressed = $event.target === $event.currentTarget"
        @click.self="backdropPressed && close()" @keydown.esc.stop.prevent="close">
        <section ref="dialog" class="modal repository-note-modal" role="dialog" aria-modal="true"
          aria-labelledby="repository-document-title" tabindex="-1" @keydown.tab="trapModalFocus">
          <header class="modal-head">
            <div><h3 id="repository-document-title"><FileText :size="19" />{{ target.repository.name_i18n?.[locale] || target.repository.name }}</h3>
              <p class="muted-text">{{ t(target.kind === 'note' ? 'MOD 简介' : '仓库说明（README.md）') }}</p>
            </div>
            <div class="button-row">
              <button v-if="target.kind === 'readme'" class="icon-button" :disabled="loading" :title="t('重新读取仓库说明')" :aria-label="t('重新读取仓库说明')" @click="load"><RefreshCw :size="16" /></button>
              <button class="icon-button" :title="t('关闭')" :aria-label="t('关闭')" @click="close"><X :size="18" /></button>
            </div>
          </header>
          <div v-if="loading" class="repository-document-state" role="status"><RefreshCw class="spin-icon" :size="30" />{{ t('正在读取仓库说明') }}</div>
          <div v-else-if="error" class="repository-document-state danger" role="alert">{{ t(error) }}</div>
          <div v-else-if="target.kind === 'readme' && document && !document.exists" class="repository-document-state" role="status">{{ t('文件不存在') }}</div>
          <article v-else class="repository-note-content markdown-body" @click="openLink" v-html="contentHtml"></article>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>
