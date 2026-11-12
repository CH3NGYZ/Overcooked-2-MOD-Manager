<script setup lang="ts">
import { computed } from 'vue'
import { CheckmarkCircle, EllipseOutline } from '@vicons/ionicons5'
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
}>()
const emit = defineEmits<{
  toggle: [chefId: string]
  hat: [chefId: string]
  drag: [event: PointerEvent, chefId: string]
}>()
const draggable = computed(() => props.reorderable && isUsableResource(props.item.chef.resource_state))
</script>

<template>
  <article class="chef-tile" :class="{ 'is-default': item.isDefault, 'is-enabled': item.enabled, 'is-draggable': draggable, 'is-dragging': dragging }"
    :data-chef-id="item.chef.id" :data-chef-source="item.chef.source_kind" @pointerdown="draggable && emit('drag', $event, item.chef.id)" @dragstart.prevent>
    <button type="button" class="chef-tile-main" :data-chef-focus="item.chef.id" aria-haspopup="dialog"
      :aria-label="t('diychef.hatTitle', { name: item.chef.display_name })" :title="item.chef.display_name"
      @click="emit('hat', item.chef.id)">
      <PreviewImage :session-id="sessionId" :chef-id="item.chef.id" :hat-id="item.hat.kind === 'known' ? item.hat.hat_id : null" :name="item.chef.display_name" />
      <span v-if="item.isDefault" class="chef-default-label" :title="t('diychef.defaultHint')">{{ t('diychef.defaultChef') }}</span>
    </button>
    <button type="button" class="chef-toggle" :aria-pressed="item.enabled" :disabled="frozen"
      :aria-disabled="frozen || !item.canToggle" :aria-label="t('diychef.toggle', { name: item.chef.display_name })"
      :aria-description="item.toggleError ? t(`diychef.${item.toggleError}`) : undefined"
      :title="item.toggleError ? t(`diychef.${item.toggleError}`) : t('diychef.toggle', { name: item.chef.display_name })"
      @pointerdown.stop @click.stop="emit('toggle', item.chef.id)">
      <span class="chef-toggle-icons" aria-hidden="true"><CheckmarkCircle class="chef-toggle-on" /><EllipseOutline class="chef-toggle-off" /></span>
    </button>
  </article>
</template>

<style scoped>
.chef-tile { position: relative; height: 218px; box-sizing: border-box; min-width: 0; display: flex; flex-direction: column; border: 1px solid var(--oc2-row-line); border-radius: 12px; background: var(--oc2-row); transition: border-color 180ms ease, box-shadow 180ms ease; }
.chef-tile.is-default { border-radius: 16px; border-color: color-mix(in srgb, var(--oc2-accent) 65%, var(--oc2-row-line)); }
.chef-tile-main { width: 100%; min-width: 0; min-height: 0; flex: 1; display: flex; align-items: center; justify-content: center; padding: 8px 6px 28px; background: transparent; border: 0; border-radius: inherit; font: inherit; color: var(--oc2-text-1); cursor: pointer; }
.chef-default-label { position: absolute; left: 10px; bottom: 8px; max-width: calc(100% - 20px); padding: 2px 6px; border-radius: 6px; background: var(--oc2-row); font-size: 12px; color: var(--oc2-text-2); }
.chef-tile :deep(.chef-portrait) { width: 100%; height: 100%; max-height: 184px; aspect-ratio: auto; }
.chef-tile :deep(.chef-portrait img), .chef-tile :deep(.chef-motion-image) { filter: grayscale(0); transition: filter 220ms ease; }
.chef-tile :deep(.chef-motion-image) { width: 100%; height: 100%; object-fit: contain; }
.chef-tile:not(.is-enabled) :deep(.chef-portrait img), .chef-tile:not(.is-enabled) :deep(.chef-motion-image) { filter: grayscale(1); }
.chef-tile::after { content: ''; position: absolute; inset: 0; border-radius: inherit; background: color-mix(in srgb, var(--oc2-text-3) 12%, transparent); opacity: 0; transition: opacity 220ms ease; pointer-events: none; }
.chef-tile:not(.is-enabled)::after { opacity: 1; }
.chef-toggle { position: absolute; top: 5px; right: 5px; z-index: 1; display: grid; place-items: center; width: 28px; height: 28px; padding: 4px; border: 0; border-radius: 50%; background: var(--oc2-row); color: var(--oc2-text-3); cursor: pointer; transition: color 180ms ease, background-color 180ms ease; }
.is-enabled .chef-toggle { color: var(--oc2-accent); }
.chef-toggle:hover:not(:disabled) { background: var(--oc2-row-hover); }
.chef-toggle:disabled, .chef-toggle[aria-disabled='true'] { cursor: default; }
.chef-toggle:disabled { opacity: .4; }
.chef-toggle svg { width: 20px; height: 20px; }
.chef-toggle-icons { display: grid; width: 20px; height: 20px; }
.chef-toggle-icons svg { grid-area: 1 / 1; transition: transform 180ms ease, opacity 180ms ease; }
.chef-toggle-on { opacity: 0; transform: scale(.65); }
.chef-toggle-off { opacity: 1; transform: scale(1); }
.is-enabled .chef-toggle-on { opacity: 1; transform: scale(1); }
.is-enabled .chef-toggle-off { opacity: 0; transform: scale(.65); }
.chef-tile.is-draggable { user-select: none; touch-action: none; cursor: grab; }
.is-draggable .chef-tile-main { cursor: grab; }
.chef-tile.is-dragging, .is-dragging .chef-tile-main { cursor: grabbing; }
button:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.is-dragging { opacity: 0; }
.chef-drag-preview, .chef-motion-bridge { overflow: hidden; }
.chef-placeholder { border-style: dashed; border-color: color-mix(in srgb, var(--oc2-accent) 55%, var(--oc2-row-line)); background: color-mix(in srgb, var(--oc2-accent) 6%, var(--oc2-row)); }
[data-chef-motion-state='move'], [data-chef-motion-state='preview'], .chef-drag-preview { will-change: transform; }
@media (prefers-reduced-motion: reduce) {
  .chef-tile, .chef-tile::after, .chef-toggle, .chef-toggle-icons svg, .chef-drag-preview { transition: none; }
  .chef-tile :deep(.chef-portrait img), .chef-tile :deep(.chef-motion-image) { transition: none; }
}
</style>
