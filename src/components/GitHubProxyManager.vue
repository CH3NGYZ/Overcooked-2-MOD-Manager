<script setup lang="ts">
import { computed, nextTick, reactive, ref, watch } from 'vue'
import { GripVertical, Plus, RefreshCw, Save, Trash2, X } from 'lucide-vue-next'
import ProxyEditorDialog, { type ProxyKind } from './ProxyEditorDialog.vue'
import { usePointerSort } from '../composables/usePointerSort'
import { trapModalFocus } from '../composables/modalFocus'
import type { GitHubProxy, GitHubProxyConfig } from '../types'

const props = defineProps<{
  open: boolean
  busy: boolean
  config: GitHubProxyConfig
  saveConfig: (config: GitHubProxyConfig) => Promise<GitHubProxyConfig>
  refreshConfig: (config: GitHubProxyConfig) => Promise<GitHubProxyConfig>
  t: (text: string) => string
}>()
const emit = defineEmits<{ close: [] }>()
const draft = reactive<GitHubProxyConfig>({ release_proxies: [], raw_proxies: [], github_api_proxies: [], gitee_api_proxies: [] })
const sections = ref<HTMLElement | null>(null)
const manager = ref<HTMLElement | null>(null)
const editorOpen = ref(false)
const editorKind = ref<ProxyKind>('release')
const editingProxy = ref<GitHubProxy | null>(null)
const saving = ref(false)
const saveError = ref('')
const disabled = computed(() => props.busy || saving.value)
let editorTrigger: HTMLElement | null = null
const backdropPressed = ref(false)

const categories = computed(() => [
  { key: 'release' as const, title: props.t('Release 代理'), list: draft.release_proxies },
  { key: 'raw' as const, title: props.t('Raw 代理'), list: draft.raw_proxies },
  { key: 'github-api' as const, title: props.t('GitHub API 代理'), list: draft.github_api_proxies },
  { key: 'gitee-api' as const, title: props.t('Gitee API 代理'), list: draft.gitee_api_proxies },
])
const categoryGroups = computed(() => [
  { key: 'download', title: props.t('下载代理'), categories: categories.value.slice(0, 2) },
  { key: 'api', title: props.t('API 请求代理'), categories: categories.value.slice(2) },
])

function cloneProxy(proxy: GitHubProxy): GitHubProxy {
  return { ...proxy, regions: [...proxy.regions] }
}
function cloneConfig(config: GitHubProxyConfig): GitHubProxyConfig {
  return {
    release_proxies: (config.release_proxies || []).map(cloneProxy),
    raw_proxies: (config.raw_proxies || []).map(cloneProxy),
    github_api_proxies: (config.github_api_proxies || []).map(cloneProxy),
    gitee_api_proxies: (config.gitee_api_proxies || []).map(cloneProxy),
  }
}
function listFor(kind: string, config: GitHubProxyConfig = draft) {
  if (kind === 'release') return config.release_proxies
  if (kind === 'raw') return config.raw_proxies
  if (kind === 'github-api') return config.github_api_proxies
  return config.gitee_api_proxies
}
const sort = usePointerSort(sections, listFor)
const dragging = sort.dragging

watch(() => props.open, async (open) => {
  sort.stop()
  if (open) {
    Object.assign(draft, cloneConfig(props.config))
    saveError.value = ''
    await nextTick()
    manager.value?.focus()
  } else editorOpen.value = false
}, { immediate: true })
// Polling must not replace drafts. Explicit saves synchronize their own response.
watch(() => props.config, (config) => {
  if (!props.open) Object.assign(draft, cloneConfig(config))
})

function close() {
  if (disabled.value || editorOpen.value) return
  sort.stop()
  emit('close')
}
function openEditor(kind: ProxyKind, proxy: GitHubProxy | null, event: Event) {
  if (disabled.value) return
  sort.stop()
  editorTrigger = event.currentTarget as HTMLElement
  editorKind.value = kind
  editingProxy.value = proxy ? cloneProxy(proxy) : null
  saveError.value = ''
  editorOpen.value = true
}
async function closeEditor() {
  if (disabled.value) return
  editorOpen.value = false
  saveError.value = ''
  await nextTick()
  if (editorTrigger?.isConnected) editorTrigger.focus()
  else manager.value?.focus()
}
async function persist(config: GitHubProxyConfig, refresh = false) {
  if (disabled.value) return false
  sort.stop()
  saving.value = true
  saveError.value = ''
  try {
    const saved = await (refresh ? props.refreshConfig(config) : props.saveConfig(config))
    Object.assign(draft, cloneConfig(saved))
    return true
  } catch (error) {
    saveError.value = props.t(error instanceof Error ? error.message : String(error))
    return false
  } finally {
    saving.value = false
  }
}
async function submitProxy(kind: ProxyKind, proxy: GitHubProxy) {
  const proposed = cloneConfig(draft)
  const list = listFor(kind, proposed)
  const index = list.findIndex((entry) => entry.id === proxy.id)
  if (index < 0) list.push(proxy)
  else list[index] = proxy
  if (await persist(proposed)) {
    await nextTick()
    await closeEditor()
  }
}
async function save() {
  if (await persist(cloneConfig(draft))) emit('close')
}
function removeProxy(kind: ProxyKind, index: number) {
  const list = listFor(kind)
  if (disabled.value || list[index]?.builtin) return
  list.splice(index, 1)
}
</script>

<template>
  <Teleport to="body">
    <Transition name="modal-fade">
      <div v-if="open" class="modal-backdrop github-proxy-backdrop" tabindex="-1" @pointerdown="backdropPressed = $event.target === $event.currentTarget" @click.self="backdropPressed && close()" @keydown.esc.stop.prevent="close">
        <section ref="manager" class="modal github-proxy-modal" role="dialog" aria-modal="true" aria-labelledby="github-proxy-title" tabindex="-1" :inert="editorOpen" :aria-hidden="editorOpen || undefined" @keydown.tab="trapModalFocus">
          <header class="modal-head">
            <div>
              <h3 id="github-proxy-title">{{ t('GitHub 代理配置') }}</h3>
              <p class="muted-text">{{ t('自动直连先直连再尝试代理；自动代理先尝试远程代理，再尝试内置代理，最后尝试直连。') }}</p>
              <p class="muted-text">{{ t('点击代理信息可编辑；拖拽把手调整顺序，靠近列表边缘自动滚动。') }}</p>
            </div>
            <button class="icon-button" :disabled="disabled" :title="t('关闭')" @click="close"><X :size="17" /></button>
          </header>

          <div ref="sections" class="github-proxy-sections" :class="{ 'github-proxy-sorting': dragging }">
            <section v-for="group in categoryGroups" :key="group.key" class="github-proxy-group">
              <div class="github-proxy-group-head"><h4>{{ group.title }}</h4></div>
              <div class="github-proxy-group-items">
                <section v-for="category in group.categories" :key="category.key" class="github-proxy-section">
                  <div class="github-proxy-section-head">
                    <h5>{{ category.title }}</h5>
                    <span class="muted-text">{{ category.list.length }}</span>
                  </div>
                  <div v-if="!category.list.length" class="empty-state">{{ t('暂无代理') }}</div>
                  <article v-for="(proxy, index) in category.list" :key="proxy.id" class="github-proxy-row" :class="{ 'github-proxy-row-dragging': dragging?.id === proxy.id }" :data-sort-kind="category.key" :data-sort-id="proxy.id">
                    <button class="icon-button small github-proxy-drag" :disabled="disabled" :title="t('拖拽排序，也可使用上下方向键')" :aria-label="t('拖拽排序，也可使用上下方向键') + ' · ' + proxy.name" @pointerdown="sort.begin($event, category.key, proxy.id)" @keydown="sort.moveWithKeyboard($event, category.key, proxy.id)"><GripVertical :size="16" /></button>
                    <label class="ui-switch-control" :title="t('启用代理')">
                      <input v-model="proxy.enabled" class="ui-switch-input" type="checkbox" :disabled="disabled" :aria-label="t('启用代理') + ' · ' + proxy.name" />
                      <span class="ui-switch" aria-hidden="true"><span class="ui-switch-thumb"></span></span>
                    </label>
                    <span v-if="proxy.remote || proxy.builtin" class="badge github-proxy-builtin">{{ t(proxy.remote ? '远程' : '内置') }}</span>
                    <div class="github-proxy-fields" :class="{ 'github-proxy-fields-described': proxy.description.trim() }">
                      <input :value="proxy.name" readonly :disabled="disabled" :aria-label="t('代理名称')" :title="t('编辑代理') + ' · ' + proxy.name" @click="openEditor(category.key, proxy, $event)" @keydown.enter.prevent="openEditor(category.key, proxy, $event)" />
                      <input :value="proxy.regions.join(', ')" readonly :disabled="disabled" :aria-label="t('代理地区')" :placeholder="t('全部地区')" :title="proxy.regions.join(', ') || t('全部地区')" @click="openEditor(category.key, proxy, $event)" @keydown.enter.prevent="openEditor(category.key, proxy, $event)" />
                      <input :value="proxy.template" readonly :disabled="disabled" :aria-label="t('代理模板')" :title="proxy.template" @click="openEditor(category.key, proxy, $event)" @keydown.enter.prevent="openEditor(category.key, proxy, $event)" />
                      <input v-if="proxy.description.trim()" :value="proxy.description" readonly :disabled="disabled" :aria-label="t('说明')" :title="proxy.description" @click="openEditor(category.key, proxy, $event)" @keydown.enter.prevent="openEditor(category.key, proxy, $event)" />
                    </div>
                    <div class="github-proxy-actions">
                      <button v-if="!proxy.builtin" class="icon-button small danger-icon" :disabled="disabled" :title="t('删除')" @click="removeProxy(category.key, index)"><Trash2 :size="14" /></button>
                    </div>
                  </article>
                </section>
              </div>
            </section>
          </div>

          <p v-if="saveError && !editorOpen" class="github-proxy-error" role="alert">{{ saveError }}</p>
          <footer class="modal-actions">
            <button class="secondary-button" :disabled="disabled" @click="openEditor('release', null, $event)"><Plus :size="15" />{{ t('添加代理') }}</button>
            <button class="secondary-button" :disabled="disabled" @click="persist(cloneConfig(draft), true)"><RefreshCw :size="15" />{{ t('同步内置代理') }}</button>
            <button class="primary-button" :disabled="disabled" @click="save"><Save :size="15" />{{ t('保存代理') }}</button>
          </footer>
        </section>
        <ProxyEditorDialog :open="editorOpen" :kind="editorKind" :proxy="editingProxy" :busy="disabled" :error="saveError" :t="t" @close="closeEditor" @submit="submitProxy" />
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.github-proxy-backdrop {
  z-index: 40;
  inset: 0 auto auto 0;
  width: var(--layout-viewport-width);
  height: var(--layout-viewport-height);
  transform: scale(var(--ui-zoom));
  transform-origin: top left;
}
.github-proxy-modal {
  width: min(1120px, 100%);
  max-height: min(calc(var(--layout-viewport-height) * .85), calc(var(--layout-viewport-height) - 40px));
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.github-proxy-modal .modal-head { margin: 0; align-items: flex-start; }
.github-proxy-modal:focus { outline: none; }
.github-proxy-modal .modal-head h3 { font-size: 16px; margin: 0; }
.github-proxy-modal .modal-head p { margin: 6px 0 0; font-size: 11px; line-height: 1.45; }
.github-proxy-sections { min-height: 0; overflow: auto; display: grid; gap: 12px; overscroll-behavior: contain; overflow-anchor: none; }
.github-proxy-group { display: grid; gap: 10px; padding: 10px; border: 1px solid var(--line-strong); border-radius: 8px; background: var(--surface-2); }
.github-proxy-group-head { display: flex; align-items: center; min-height: 24px; padding-bottom: 8px; border-bottom: 1px solid var(--line); }
.github-proxy-group-head h4 { margin: 0; color: var(--text); font-size: 13px; }
.github-proxy-group-items { display: grid; gap: 10px; }
.github-proxy-section { display: grid; gap: 8px; padding: 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--surface); }
.github-proxy-section-head { display: flex; align-items: center; justify-content: space-between; }
.github-proxy-section-head .muted-text { font-size: 11px; }
.github-proxy-section-head h5 { margin: 0; font-size: 12px; }
.github-proxy-row { display: flex; align-items: center; gap: 6px; padding: 6px; border: 1px solid var(--line); background: var(--surface-2); border-radius: 6px; min-width: 0; }
.github-proxy-drag { flex: 0 0 20px; padding: 2px; cursor: grab; touch-action: none; }
.github-proxy-sorting { user-select: none; cursor: grabbing; }
.github-proxy-row-dragging { border-color: var(--accent); background: var(--surface-3); }
.github-proxy-row-dragging .github-proxy-drag { cursor: grabbing; }
.github-proxy-builtin { flex: 0 0 auto; white-space: nowrap; }
.github-proxy-fields { display: grid; grid-template-columns: minmax(50px, 120px) minmax(40px, 110px) minmax(0, 1fr); min-width: 0; flex: 1; gap: 6px; }
.github-proxy-fields-described { grid-template-columns: minmax(50px, 120px) minmax(40px, 110px) minmax(0, 1.5fr) minmax(0, 1fr); }
.github-proxy-fields input { min-width: 0; width: 100%; border: 1px solid var(--line); border-radius: 5px; padding: 7px 6px; color: var(--text); background: var(--surface); font-size: 11px; text-overflow: ellipsis; cursor: pointer; }
.github-proxy-fields input:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
.github-proxy-actions { flex: 0 0 24px; display: flex; justify-content: flex-end; }
.github-proxy-error { margin: 0; color: var(--danger); font-size: 12px; overflow-wrap: anywhere; }
.github-proxy-modal footer { margin-top: 0; flex-wrap: wrap; }
</style>
