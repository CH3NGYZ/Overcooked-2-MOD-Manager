<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import { FileCog, RefreshCw, Save, X } from 'lucide-vue-next'
import type { editor } from 'monaco-editor'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { trapModalFocus } from '../composables/modalFocus'
import type { InstalledMod, ModConfigDocument } from '../types'

const props = defineProps<{
  mod: InstalledMod; gameBlocked: boolean; busy: boolean
  confirmDiscard: () => Promise<boolean>
}>()
const emit = defineEmits<{ close: [] }>()
const container = ref<HTMLElement | null>(null)
const dialog = ref<HTMLElement | null>(null)
const loading = ref(true)
const saving = ref(false)
const confirming = ref(false)
const dirty = ref(false)
const error = ref('')
const notice = ref('')
const document = ref<ModConfigDocument | null>(null)
const canSave = computed(() => dirty.value && !loading.value && !saving.value && !confirming.value && !props.busy && !props.gameBlocked)
let instance: editor.IStandaloneCodeEditor | undefined
let model: editor.ITextModel | undefined
let subscription: { dispose: () => void } | undefined
let alive = true
let originalContent = ''
const trigger = window.document.activeElement instanceof HTMLElement ? window.document.activeElement : null

async function allowDiscard() {
  if (!dirty.value) return true
  if (confirming.value) return false
  confirming.value = true
  try { return await props.confirmDiscard() } finally { confirming.value = false }
}

async function close() {
  if (saving.value || confirming.value) return
  if (await allowDiscard() && alive) emit('close')
}

async function reload() {
  if (saving.value || confirming.value || !await allowDiscard() || !alive) return
  loading.value = true
  error.value = ''
  notice.value = ''
  try {
    const [loaded, { monaco }] = await Promise.all([
      api.readModConfig(props.mod.dll_file), import('../services/configEditor'),
    ])
    if (!alive || !container.value) return
    document.value = loaded
    subscription?.dispose()
    instance?.dispose()
    model?.dispose()
    model = monaco.editor.createModel(loaded.content, 'ini')
    model.setEOL(loaded.content.includes('\r\n') ? monaco.editor.EndOfLineSequence.CRLF : monaco.editor.EndOfLineSequence.LF)
    originalContent = model.getValue()
    instance = monaco.editor.create(container.value, {
      model, theme: 'vs-dark', automaticLayout: true, minimap: { enabled: false },
      fontSize: 14, tabSize: 4, wordWrap: 'on', scrollBeyondLastLine: false,
      readOnly: props.gameBlocked || props.busy, ariaLabel: t('MOD 配置编辑器'),
      fixedOverflowWidgets: false,
    })
    subscription = model.onDidChangeContent(() => {
      dirty.value = model?.getValue() !== originalContent
      notice.value = ''
    })
    instance.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => { void save() })
    dirty.value = false
    instance.focus()
  } catch (reason) {
    if (alive) error.value = t(reason instanceof Error ? reason.message : String(reason))
  } finally { if (alive) loading.value = false }
}

async function save() {
  if (!canSave.value || !document.value || !model) return
  const content = model.getValue()
  saving.value = true
  instance?.updateOptions({ readOnly: true })
  error.value = ''
  notice.value = ''
  try {
    const saved = await api.saveModConfig(props.mod.dll_file, document.value.revision, content)
    if (!alive) return
    document.value = saved
    originalContent = content
    dirty.value = model.getValue() !== content
    notice.value = t('配置已保存')
  } catch (reason) {
    if (alive) error.value = t(reason instanceof Error ? reason.message : String(reason))
  } finally {
    saving.value = false
    instance?.updateOptions({ readOnly: props.gameBlocked || props.busy })
  }
}

watch(() => props.gameBlocked || props.busy || confirming.value,
  (blocked) => instance?.updateOptions({ readOnly: blocked || saving.value }))
onMounted(() => { dialog.value?.focus(); void reload() })
onUnmounted(() => {
  alive = false
  subscription?.dispose()
  instance?.dispose()
  model?.dispose()
  if (trigger?.isConnected) trigger.focus()
})
const backdropPressed = ref(false)
</script>

<template>
  <Teleport to="body">
    <div class="modal-backdrop mod-config-backdrop"
      @pointerdown="backdropPressed = $event.target === $event.currentTarget"
      @pointercancel="backdropPressed = false"
      @click.self="backdropPressed && close()">
      <section ref="dialog" class="modal mod-config-modal" role="dialog" aria-modal="true" aria-labelledby="mod-config-title" tabindex="-1"
        @keydown.esc.stop.prevent="close" @keydown.tab="trapModalFocus">
        <header class="modal-head">
          <div class="mod-config-heading"><h3 id="mod-config-title"><FileCog :size="19" />{{ t('打开配置') }} · {{ mod.name }}</h3>
            <p class="muted-text">BepInEx/config/{{ document?.file_name || `${mod.guid}.cfg` }}</p></div>
          <button class="icon-button" :title="t('关闭')" :aria-label="t('关闭')" :disabled="saving || confirming" @click="close"><X :size="18" /></button>
        </header>
        <p v-if="gameBlocked" class="muted-text">{{ t('游戏运行中，配置仅供查看；请关闭游戏后保存。') }}</p>
        <p v-if="error" class="mod-config-error" role="alert">{{ error }}</p>
        <div class="mod-config-editor-area">
          <div ref="container" class="mod-config-editor" />
          <div v-if="loading" class="mod-config-loading" role="status">{{ t('正在加载配置编辑器…') }}</div>
        </div>
        <footer class="mod-config-footer">
          <span role="status">{{ dirty ? t('未保存的配置') : notice || t('Ctrl+S 保存配置') }}</span>
          <div class="button-row">
            <button class="secondary-button" :disabled="loading || saving || confirming" @click="reload"><RefreshCw :size="15" />{{ t('重新加载') }}</button>
            <button class="primary-button" :disabled="!canSave" @click="save"><Save :size="15" />{{ saving ? t('正在保存…') : t('保存') }}</button>
          </div>
        </footer>
      </section>
    </div>
  </Teleport>
</template>
