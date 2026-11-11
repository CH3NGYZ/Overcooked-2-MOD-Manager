<script setup lang="ts">
import { computed, defineAsyncComponent, onBeforeUnmount, shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import { t } from '../../i18n'
import type { ChefEditorSnapshot, ChefItem, HatReference } from '../../diychef/types'
import { isUsableResource } from '../../diychef/draft'
import { diyChefDemoImage, isDiyChefDemoSession } from '../../tutorial/demoDiyChef'
import AppModal from '../AppModal.vue'
import HatPicker from './HatPicker.vue'

const PreviewCanvas = defineAsyncComponent(() => import('./PreviewCanvas.vue'))
const props = defineProps<{
  show: boolean; snapshot: ChefEditorSnapshot; chef: ChefItem | null; hat: HatReference | null
  enabled: boolean; frozen: boolean
}>()
const emit = defineEmits<{ 'update:show': [value: boolean]; apply: [chefId: string, hat: HatReference] }>()
const selectedId = shallowRef<string | null>(null)
const previewState = shallowRef<'loading' | 'ready' | 'partial' | 'error'>('loading')
const retry = shallowRef(0)
const selectedHat = computed(() => props.snapshot.hats.find((hat) => hat.id === selectedId.value))
const canApply = computed(() => !props.frozen && !!selectedHat.value && isUsableResource(selectedHat.value.resource_state))
const demonstrating = computed(() => isDiyChefDemoSession(props.snapshot.session_id))
const demoImage = computed(() => diyChefDemoImage(props.snapshot.session_id, props.chef?.id ?? '', selectedId.value))

function interceptSave(event: KeyboardEvent) {
  if (!props.show || !(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 's') return
  event.preventDefault()
  event.stopImmediatePropagation()
}
watch(() => [props.show, props.chef?.id], () => {
  if (props.show) {
    selectedId.value = props.hat?.kind === 'known' ? props.hat.hat_id : null
    previewState.value = demonstrating.value ? 'ready' : 'loading'
    window.addEventListener('keydown', interceptSave, true)
  } else window.removeEventListener('keydown', interceptSave, true)
}, { immediate: true })
onBeforeUnmount(() => window.removeEventListener('keydown', interceptSave, true))
function apply() {
  if (!canApply.value || !props.chef || !selectedId.value) return
  emit('apply', props.chef.id, { kind: 'known', hat_id: selectedId.value })
  emit('update:show', false)
}
</script>

<template>
  <AppModal :show="show" :title="t('diychef.hatTitle', { name: chef?.display_name ?? '' })" max-width="min(820px, 94vw)" fill :scrollable="false"
    @update:show="emit('update:show', $event)">
    <div v-if="chef" class="chef-hat-dialog" data-tutorial="diychef-hat-dialog">
      <div class="chef-hat-preview">
        <img v-if="show && demoImage" class="chef-hat-demo" :src="demoImage" :alt="chef.display_name" draggable="false" />
        <PreviewCanvas v-else-if="show && selectedId && !demonstrating" :key="`${chef.id}:${retry}`" :session-id="snapshot.session_id" :chef-id="chef.id" :hat-id="selectedId" @state="previewState = $event" />
        <div v-else class="chef-hat-preview-empty" :aria-label="t('diychef.previewUnavailable')" />
      </div>
      <div class="chef-hat-selection">
        <p v-if="hat?.kind === 'unresolved'" class="chef-hat-unknown">{{ t('diychef.currentUnknownHat', { name: hat.raw_value }) }}</p>
        <HatPicker :hats="snapshot.hats" :selected="selectedId" @select="selectedId = $event" />
        <div v-if="previewState === 'partial' || previewState === 'error' || !selectedId" class="chef-hat-status">
          <p role="status">{{ t('diychef.previewUnavailableHint') }}</p>
          <n-button v-if="selectedId" size="tiny" @click="retry += 1">{{ t('diychef.retry') }}</n-button>
        </div>
      </div>
    </div>
    <p v-if="!enabled" class="chef-hat-session-note">{{ t('diychef.hatDisabledHint') }}</p>
    <template #footer>
      <n-button size="small" data-tutorial="diychef-hat-cancel" @click="emit('update:show', false)">{{ t('diychef.cancel') }}</n-button>
      <n-button size="small" type="primary" :disabled="!canApply" @click="apply">{{ t(previewState === 'error' || previewState === 'partial' ? 'diychef.hatApplyWithoutPreview' : 'diychef.hatApply') }}</n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.chef-hat-dialog { display: grid; grid-template-columns: minmax(0, min(42%, 307.2px)) minmax(0, 1fr); gap: 20px; width: 100%; min-height: 0; flex: 1 1 0; align-items: stretch; }
.chef-hat-preview { display: flex; min-width: 0; min-height: 0; align-self: stretch; }
.chef-hat-preview :deep(.chef-preview) { flex: 1; min-height: 0; }
.chef-hat-preview :deep(.chef-preview-stage) { flex: 1; min-height: 0; aspect-ratio: auto; }
.chef-hat-preview-empty { width: 100%; height: 100%; background: var(--oc2-row); border-radius: 12px; }
.chef-hat-demo { width: 100%; height: 100%; min-height: 0; object-fit: contain; user-select: none; }
.chef-hat-status { margin-top: 10px; color: var(--oc2-text-3); font-size: 12px; }
.chef-hat-selection { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
.chef-hat-selection :deep(.chef-hat-picker) { flex: 1; min-height: 0; }
.chef-hat-selection :deep(.chef-hat-options) { min-height: 0; overflow-y: auto; overscroll-behavior: contain; }
.chef-hat-unknown { margin: 0 0 10px; color: var(--oc2-text-3); font-size: 12px; overflow-wrap: anywhere; }
.chef-hat-session-note { flex: 0 0 auto; margin: 5px 0 0; color: var(--oc2-text-3); font-size: 12px; }
</style>
