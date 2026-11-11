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
  dropTarget?: boolean
  dropAfter?: boolean
}>()
const emit = defineEmits<{
  toggle: [chefId: string]
  hat: [chefId: string]
  drag: [event: PointerEvent, chefId: string]
}>()
const draggable = computed(() => props.reorderable && isUsableResource(props.item.chef.resource_state))
</script>

<template>
  <article class="chef-tile" :class="{ 'is-default': item.isDefault, 'is-enabled': item.enabled, 'is-draggable': draggable, 'is-dragging': dragging, 'is-drop-target': dropTarget, 'drop-after': dropAfter }"
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
      <CheckmarkCircle v-if="item.enabled" aria-hidden="true" /><EllipseOutline v-else aria-hidden="true" />
    </button>
  </article>
</template>

<style scoped>
.chef-tile { position: relative; min-width: 0; display: flex; flex-direction: column; border: 1px solid var(--oc2-row-line); border-radius: 12px; background: var(--oc2-row); }
.chef-tile.is-default { grid-column: span 2; border-radius: 16px; border-color: color-mix(in srgb, var(--oc2-accent) 65%, var(--oc2-row-line)); }
.chef-tile-main { width: 100%; min-width: 0; min-height: 184px; flex: 1; display: flex; align-items: center; justify-content: center; padding: 8px 6px; background: transparent; border: 0; border-radius: inherit; font: inherit; color: var(--oc2-text-1); cursor: pointer; }
.chef-default-label { position: absolute; left: 10px; bottom: 8px; max-width: calc(100% - 20px); padding: 2px 6px; border-radius: 6px; background: var(--oc2-row); font-size: 12px; color: var(--oc2-text-2); }
.chef-tile :deep(.chef-portrait) { width: 100%; max-height: 184px; }
.is-default .chef-tile-main { padding: 8px 20px 28px; }
.is-default :deep(.chef-portrait) { width: 144px; }
.chef-tile:not(.is-enabled) :deep(.chef-portrait img) { filter: grayscale(1); }
.chef-tile:not(.is-enabled)::after { content: ''; position: absolute; inset: 0; border-radius: inherit; background: color-mix(in srgb, var(--oc2-text-3) 12%, transparent); pointer-events: none; }
.chef-toggle { position: absolute; top: 5px; right: 5px; z-index: 1; display: grid; place-items: center; width: 28px; height: 28px; padding: 4px; border: 0; border-radius: 50%; background: var(--oc2-row); color: var(--oc2-text-3); cursor: pointer; }
.chef-toggle[aria-pressed='true'] { color: var(--oc2-accent); }
.chef-toggle:hover:not(:disabled) { background: var(--oc2-row-hover); }
.chef-toggle:disabled, .chef-toggle[aria-disabled='true'] { cursor: default; }
.chef-toggle:disabled { opacity: .4; }
.chef-toggle svg { width: 20px; height: 20px; }
.chef-tile.is-draggable { user-select: none; touch-action: none; cursor: grab; }
.is-draggable .chef-tile-main { cursor: grab; }
.chef-tile.is-dragging, .is-dragging .chef-tile-main { cursor: grabbing; }
button:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.is-dragging { opacity: .55; }
.is-drop-target::before { content: ''; position: absolute; top: 5px; bottom: 5px; left: -8px; width: 3px; border-radius: 3px; background: var(--oc2-accent); pointer-events: none; }
.is-drop-target.drop-after::before { left: auto; right: -8px; }
</style>
