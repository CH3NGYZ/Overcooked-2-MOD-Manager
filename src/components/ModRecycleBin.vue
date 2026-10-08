<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { FileText, RefreshCw, RotateCcw, Trash2, X } from 'lucide-vue-next'
import { api } from '../services/tauri'
import { locale, t } from '../i18n'
import { trapModalFocus } from '../composables/modalFocus'
import type { ModRecycleEntry } from '../types'

const props = defineProps<{ open: boolean; gameBlocked: boolean; busy: boolean }>()
const emit = defineEmits<{ close: []; restored: [] }>()
const entries = ref<ModRecycleEntry[]>([])
const loading = ref(false)
const operation = ref<'restore' | 'delete' | null>(null)
const pendingId = ref('')
const deleting = ref<ModRecycleEntry | null>(null)
const error = ref('')
const notice = ref('')
const dialog = ref<HTMLElement | null>(null)
const confirmation = ref<HTMLElement | null>(null)
const backdropPressed = ref(false)
const confirmBackdropPressed = ref(false)
const disabled = computed(() => loading.value || !!operation.value || props.busy)
let requestRevision = 0
let trigger: HTMLElement | null = null
let deleteTrigger: HTMLElement | null = null

function errorText(reason: unknown) {
  return t(reason instanceof Error ? reason.message : String(reason))
}

function deletedTime(value: string) {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? t('未知时间') : date.toLocaleString(locale.value)
}

async function reload() {
  if (!props.open || loading.value || operation.value) return
  const revision = ++requestRevision
  loading.value = true
  error.value = ''
  try {
    const result = await api.getModRecycleBin()
    if (revision === requestRevision && props.open) entries.value = result
  } catch (reason) {
    if (revision === requestRevision && props.open) error.value = errorText(reason)
  } finally {
    if (revision === requestRevision) loading.value = false
  }
}

watch(() => props.open, async (open) => {
  if (!open) {
    requestRevision += 1
    loading.value = false
    deleting.value = null
    await nextTick()
    if (trigger?.isConnected) trigger.focus()
    return
  }
  trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null
  error.value = ''
  notice.value = ''
  entries.value = []
  await nextTick()
  dialog.value?.focus()
  await reload()
}, { immediate: true })

onUnmounted(() => { requestRevision += 1 })

function close() {
  if (!operation.value && !deleting.value) emit('close')
}

function askDelete(entry: ModRecycleEntry, event: Event) {
  if (disabled.value) return
  error.value = ''
  notice.value = ''
  deleteTrigger = event.currentTarget as HTMLElement
  deleting.value = entry
  void nextTick(() => confirmation.value?.focus())
}

async function cancelDelete() {
  if (operation.value) return
  deleting.value = null
  error.value = ''
  await nextTick()
  if (deleteTrigger?.isConnected) deleteTrigger.focus()
  else dialog.value?.focus()
}

function onEscape(event: KeyboardEvent) {
  event.preventDefault()
  event.stopPropagation()
  if (deleting.value) void cancelDelete()
  else close()
}

async function operate(entry: ModRecycleEntry, kind: 'restore' | 'delete') {
  if (!props.open || disabled.value || (kind === 'restore' && props.gameBlocked)) return
  operation.value = kind
  pendingId.value = entry.id
  error.value = ''
  notice.value = ''
  try {
    const result = await (kind === 'restore' ? api.restoreRecycledMod(entry.id) : api.deleteRecycledMod(entry.id))
    if (!result.ok) throw new Error(result.message)
    entries.value = entries.value.filter((item) => item.id !== entry.id)
    deleting.value = null
    notice.value = t(result.message)
    if (kind === 'restore') emit('restored')
    await nextTick()
    dialog.value?.focus()
  } catch (reason) {
    error.value = errorText(reason)
  } finally {
    operation.value = null
    pendingId.value = ''
  }
}
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="open" class="modal-backdrop mod-recycle-backdrop" tabindex="-1"
        @pointerdown="backdropPressed = $event.target === $event.currentTarget"
        @click.self="backdropPressed && close()" @keydown.esc="onEscape">
        <section ref="dialog" class="modal mod-recycle-modal" role="dialog" aria-modal="true"
          aria-labelledby="mod-recycle-title" tabindex="-1" :inert="!!deleting" :aria-hidden="!!deleting || undefined"
          @keydown.tab="trapModalFocus">
          <header class="modal-head mod-recycle-head">
            <div>
              <h3 id="mod-recycle-title"><Trash2 :size="19" />{{ t('回收站') }}</h3>
              <p class="muted-text">{{ t('管理器删除的 MOD 暂存在程序目录下，可恢复到原位置；彻底删除后无法恢复。') }}</p>
            </div>
            <div class="button-row">
              <button class="icon-button" :disabled="disabled" :title="t('立即刷新')" :aria-label="t('立即刷新')" @click="reload">
                <RefreshCw :size="17" :class="{ 'spin-icon': loading }" />
              </button>
              <button class="icon-button" :disabled="!!operation" :title="t('关闭')" :aria-label="t('关闭')" @click="close"><X :size="17" /></button>
            </div>
          </header>
          <p v-if="gameBlocked" class="mod-recycle-warning">{{ t('游戏运行中，暂时不能恢复 MOD。') }}</p>
          <p v-if="error" class="mod-recycle-error" role="alert">{{ error }}</p>
          <p v-if="notice" class="mod-recycle-notice" role="status">{{ notice }}</p>
          <div v-if="loading" class="mod-recycle-empty" role="status"><RefreshCw class="spin-icon" :size="26" />{{ t('正在读取回收站') }}</div>
          <div v-else-if="!entries.length && !error" class="mod-recycle-empty"><Trash2 :size="28" />{{ t('回收站为空') }}</div>
          <div v-else class="mod-recycle-list" :aria-busy="!!operation">
            <article v-for="entry in entries" :key="entry.id" class="mod-recycle-entry">
              <div class="mod-recycle-entry-head">
                <div class="mod-recycle-entry-title"><b>{{ entry.name }}</b><span v-if="entry.release_tag" class="badge muted">{{ entry.release_tag }}</span></div>
                <div class="button-row mod-recycle-actions">
                  <button class="secondary-button small-button" :disabled="disabled || gameBlocked" @click="operate(entry, 'restore')">
                    <RefreshCw v-if="pendingId === entry.id && operation === 'restore'" class="spin-icon" :size="14" /><RotateCcw v-else :size="14" />{{ t('恢复') }}
                  </button>
                  <button class="icon-button small danger-icon" :disabled="disabled" :title="t('彻底删除')" :aria-label="t('彻底删除')" @click="askDelete(entry, $event)"><Trash2 :size="15" /></button>
                </div>
              </div>
              <div class="mod-recycle-meta"><span>{{ t('删除时间') }}: {{ deletedTime(entry.deleted_at) }}</span><span>{{ t('文件数量') }}: {{ entry.file_count }}</span></div>
              <p class="mod-recycle-game-path"><span>{{ t('原游戏目录') }}</span><span>{{ entry.game_path }}</span></p>
              <details class="mod-recycle-files">
                <summary tabindex="0"><FileText :size="14" />{{ t('查看文件清单') }}</summary>
                <ul><li v-for="file in entry.files" :key="file">{{ file }}</li></ul>
              </details>
            </article>
          </div>
        </section>
        <Transition name="modal-fade">
          <div v-if="deleting" class="modal-backdrop mod-recycle-confirm-backdrop"
            @pointerdown="confirmBackdropPressed = $event.target === $event.currentTarget"
            @click.self="confirmBackdropPressed && cancelDelete()">
            <section ref="confirmation" class="modal mod-recycle-confirm" role="alertdialog" aria-modal="true"
              aria-labelledby="mod-recycle-confirm-title" aria-describedby="mod-recycle-confirm-description"
              tabindex="-1" @keydown.tab="trapModalFocus">
              <header class="modal-head"><h3 id="mod-recycle-confirm-title">{{ t('彻底删除 MOD') }}</h3><button class="icon-button" :disabled="!!operation" :title="t('关闭')" :aria-label="t('关闭')" @click="cancelDelete"><X :size="17" /></button></header>
              <b>{{ deleting.name }}</b>
              <p id="mod-recycle-confirm-description" class="mod-recycle-warning">{{ t('将永久删除回收站中的 MOD 文件和保存的安装清单，此操作无法撤销。') }}</p>
              <p v-if="error" class="mod-recycle-error" role="alert">{{ error }}</p>
              <footer class="modal-actions"><button class="secondary-button" :disabled="!!operation" @click="cancelDelete">{{ t('取消') }}</button><button class="danger-button" :disabled="disabled" @click="operate(deleting, 'delete')"><RefreshCw v-if="operation === 'delete'" class="spin-icon" :size="15" /><Trash2 v-else :size="15" />{{ t('彻底删除') }}</button></footer>
            </section>
          </div>
        </Transition>
      </div>
    </Transition>
  </Teleport>
</template>
