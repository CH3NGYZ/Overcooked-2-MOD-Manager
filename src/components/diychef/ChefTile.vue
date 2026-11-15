<script setup lang="ts">
import { computed } from 'vue'
import { NCheckbox } from 'naive-ui'
import { t } from '../../i18n'
import { isUsableResource } from '../../diychef/draft'
import type { ChefTileItem } from '../../diychef/projection'
import PreviewImage from './PreviewImage.vue'

const props = defineProps<{
  item: ChefTileItem
  sessionId: string
  frozen: boolean
  reorderable: boolean
  dragging?: boolean
  selected?: boolean
}>()
const emit = defineEmits<{
  select: [chefId: string]
  hat: [chefId: string]
  drag: [event: PointerEvent, chefId: string]
}>()
const draggable = computed(() => props.reorderable && isUsableResource(props.item.chef.resource_state))
const selectable = computed(() => isUsableResource(props.item.chef.resource_state))
const description = computed(() => [props.item.chef.display_name, props.item.isDefault ? t('diychef.defaultChef') : ''].filter(Boolean).join(' · '))
</script>

<template>
    <article class="chef-tile" :class="{ 'is-default': item.isDefault, 'is-enabled': item.enabled, 'is-identity-conflict': item.identityConflict, 'is-selected': selected, 'is-draggable': draggable, 'is-dragging': dragging }"
    :data-chef-id="item.chef.id" :data-chef-source="item.chef.source_kind" :data-chef-order="item.catalogOrder" @pointerdown="draggable && emit('drag', $event, item.chef.id)" @dragstart.prevent>
    <button type="button" class="chef-tile-main" :data-chef-focus="item.chef.id" aria-haspopup="dialog"
      :aria-label="t('diychef.hatTitle', { name: description })" :title="description"
      @click="emit('hat', item.chef.id)">
      <PreviewImage :session-id="sessionId" :chef-id="item.chef.id" :hat-id="item.hat.kind === 'known' ? item.hat.hat_id : null" :name="item.chef.display_name" />
    </button>
    <span class="chef-select" @pointerdown.stop @click.stop>
      <n-checkbox :checked="selected" :disabled="frozen || !selectable" :aria-label="t('diychef.select', { name: item.chef.display_name })"
        :title="!selectable ? t('diychef.unavailable') : t('diychef.select', { name: item.chef.display_name })"
        @update:checked="emit('select', item.chef.id)" />
    </span>
  </article>
</template>

<style scoped>
.chef-tile { position: relative; height: 218px; box-sizing: border-box; min-width: 0; display: flex; flex-direction: column; border: 1px solid var(--oc2-row-line); border-radius: 12px; background: var(--oc2-row); transition: border-color 180ms ease, box-shadow 180ms ease; }
.chef-tile.is-default { border-radius: 16px; border-color: var(--oc2-accent); box-shadow: inset 0 0 0 1px var(--oc2-accent); }
.chef-tile.is-selected { outline: 2px dashed color-mix(in srgb, var(--oc2-accent) 80%, var(--oc2-text-1)); outline-offset: 3px; background: color-mix(in srgb, var(--oc2-accent) 9%, var(--oc2-row)); }
.chef-tile.is-identity-conflict { border-color: var(--oc2-danger, #e5484d); background: color-mix(in srgb, var(--oc2-danger, #e5484d) 14%, var(--oc2-row)); box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--oc2-danger, #e5484d) 72%, transparent); }
.chef-tile-main { width: 100%; min-width: 0; min-height: 0; flex: 1; display: flex; align-items: center; justify-content: center; padding: 8px 6px; background: transparent; border: 0; border-radius: inherit; font: inherit; color: var(--oc2-text-1); cursor: pointer; }
.chef-tile :deep(.chef-portrait) { width: 100%; height: 100%; max-height: none; aspect-ratio: auto; }
.chef-tile :deep(.chef-motion-image) { width: 100%; height: 100%; object-fit: contain; }
.chef-select { position: absolute; top: 7px; right: 7px; z-index: 1; display: grid; place-items: center; width: 28px; height: 28px; border-radius: 6px; background: transparent; opacity: .24; transition: opacity 160ms ease; }
.chef-select :deep(.n-checkbox) { width: 100%; height: 100%; display: flex; align-items: center; justify-content: center; }
.chef-tile:hover .chef-select, .chef-tile:focus-within .chef-select, .chef-tile.is-selected .chef-select { opacity: 1; }
.chef-tile.is-draggable { user-select: none; touch-action: none; cursor: grab; }
.is-draggable .chef-tile-main { cursor: grab; }
.chef-tile.is-dragging, .is-dragging .chef-tile-main { cursor: grabbing; }
button:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.is-dragging { opacity: 0; }
.chef-drag-preview, .chef-motion-bridge { overflow: hidden; }
.chef-drag-preview :deep(.chef-drag-count) { position: absolute; bottom: 8px; right: 8px; min-width: 23px; padding: 2px 6px; border-radius: 12px; background: var(--oc2-accent); color: var(--oc2-bg); font-weight: 700; text-align: center; }
.chef-placeholder { border-style: dashed; border-color: color-mix(in srgb, var(--oc2-accent) 55%, var(--oc2-row-line)); background: color-mix(in srgb, var(--oc2-accent) 6%, var(--oc2-row)); }
[data-chef-motion-state='move'], [data-chef-motion-state='preview'], .chef-drag-preview { will-change: transform; }
@media (prefers-reduced-motion: reduce) {
  .chef-tile, .chef-select, .chef-drag-preview { transition: none; }
}
</style>
