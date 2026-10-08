<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from 'vue'
import { Save, X } from 'lucide-vue-next'
import type { GitHubProxy } from '../types'
import { trapModalFocus } from '../composables/modalFocus'

export type ProxyKind = 'release' | 'raw' | 'github-api' | 'gitee-api'
const props = defineProps<{
  open: boolean
  kind: ProxyKind
  proxy: GitHubProxy | null
  busy: boolean
  error: string
  t: (text: string) => string
}>()
const emit = defineEmits<{
  close: []
  submit: [kind: ProxyKind, proxy: GitHubProxy]
}>()
const nameInput = ref<HTMLInputElement | null>(null)
const form = reactive({ kind: 'release' as ProxyKind, name: '', template: '', regions: '', description: '', enabled: true })
const valid = computed(() => !!form.name.trim() && !!form.template.trim())
const backdropPressed = ref(false)

watch(() => props.open, async (open) => {
  if (!open) return
  Object.assign(form, {
    kind: props.kind,
    name: props.proxy?.name ?? '',
    template: props.proxy?.template ?? '',
    regions: props.proxy?.regions.join(', ') ?? '',
    description: props.proxy?.description ?? '',
    enabled: props.proxy?.enabled ?? true,
  })
  await nextTick()
  nameInput.value?.focus()
}, { immediate: true })

function close() { if (!props.busy) emit('close') }
function submit() {
  if (!valid.value || props.busy) return
  emit('submit', form.kind, {
    id: props.proxy?.id ?? `${form.kind}-${crypto.randomUUID()}`,
    name: form.name.trim(),
    template: form.template.trim(),
    regions: form.regions.split(/[,，]/).map((value) => value.trim()).filter(Boolean),
    description: form.description.trim(),
    enabled: form.enabled,
    builtin: props.proxy?.builtin ?? false,
    remote: props.proxy?.remote ?? false,
  })
}
</script>

<template>
  <Transition name="modal-fade">
    <div v-if="open" class="modal-backdrop proxy-editor-backdrop" @pointerdown="backdropPressed = $event.target === $event.currentTarget" @click.self="backdropPressed && close()" @keydown.esc.stop.prevent="close">
      <form class="modal proxy-editor-modal" role="dialog" aria-modal="true" aria-labelledby="proxy-editor-title" @submit.prevent="submit" @keydown.tab.stop="trapModalFocus">
        <header class="modal-head">
          <h3 id="proxy-editor-title">{{ t(proxy ? '编辑代理' : '添加代理') }}</h3>
          <button type="button" class="icon-button" :disabled="busy" :title="t('关闭')" @click="close"><X :size="17" /></button>
        </header>
        <fieldset :disabled="busy" class="proxy-editor-fields">
          <label>{{ t('代理类型') }}
            <select v-model="form.kind" :disabled="!!proxy">
              <option value="release">{{ t('Release 代理') }}</option>
              <option value="raw">{{ t('Raw 代理') }}</option>
              <option value="github-api">{{ t('GitHub API 代理') }}</option>
              <option value="gitee-api">{{ t('Gitee API 代理') }}</option>
            </select>
          </label>
          <label>{{ t('代理名称') }}<input ref="nameInput" v-model="form.name" required /></label>
          <label>{{ t('代理地区') }}<input v-model="form.regions" :placeholder="t('地区，可用逗号分隔')" /></label>
          <label class="proxy-editor-wide">{{ t('代理模板') }}<input v-model="form.template" required :placeholder="t('例如：https://proxy.example/{url} 或 https://proxy.example{url_path}')" /></label>
          <small class="muted-text proxy-editor-wide">{{ t('可用模板字段：{url}、{url_path}、{owner}、{repo}、{tag}、{asset}、{branch}、{path}、{url_no_scheme}、{github_path}') }}</small>
          <label class="proxy-editor-wide">{{ t('说明') }}<textarea v-model="form.description" rows="2" /></label>
          <label class="ui-switch-control proxy-editor-enabled">
            <input v-model="form.enabled" class="ui-switch-input" type="checkbox" />
            <span class="ui-switch" aria-hidden="true"><span class="ui-switch-thumb"></span></span>
            {{ t('启用代理') }}
          </label>
        </fieldset>
        <p v-if="error" class="proxy-editor-error" role="alert">{{ error }}</p>
        <footer class="modal-actions">
          <button type="button" class="secondary-button" :disabled="busy" @click="close">{{ t('取消') }}</button>
          <button type="submit" class="primary-button" :disabled="busy || !valid"><Save :size="15" />{{ t('保存代理') }}</button>
        </footer>
      </form>
    </div>
  </Transition>
</template>

<style scoped>
.proxy-editor-backdrop { z-index: 1; }
.proxy-editor-modal { width: min(620px, 100%); max-height: calc(var(--layout-viewport-height) - 40px); overflow-y: auto; }
.proxy-editor-fields { border: 0; padding: 0; margin: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 14px; min-width: 0; }
.proxy-editor-fields label:not(.proxy-editor-enabled) { display: grid; gap: 7px; min-width: 0; font-size: 12px; color: var(--muted); }
.proxy-editor-fields input:not([type="checkbox"]), .proxy-editor-fields textarea { width: 100%; min-width: 0; border: 1px solid var(--line-strong); border-radius: 5px; padding: 9px; color: var(--text); background: var(--surface-2); font: inherit; }
.proxy-editor-fields select { width: 100%; }
.proxy-editor-fields textarea { resize: vertical; min-height: 50px; }
.proxy-editor-wide { grid-column: 1 / -1; overflow-wrap: anywhere; }
.proxy-editor-enabled { font-size: 12px; }
.proxy-editor-error { font-size: 12px; color: var(--danger); overflow-wrap: anywhere; }
</style>
