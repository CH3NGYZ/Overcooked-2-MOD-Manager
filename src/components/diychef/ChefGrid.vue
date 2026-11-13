<script setup lang="ts">
import { computed } from 'vue'
import { t } from '../../i18n'
import type { ChefTileItem } from '../../diychef/projection'
import ChefTile from './ChefTile.vue'

const props = defineProps<{
  items: ChefTileItem[]; sessionId: string; enabled: boolean; filtered: boolean; frozen: boolean
  reorderable: boolean; draggingId: string | null; draggingIds?: ReadonlySet<string>; overEnabled: boolean; selectedIds?: ReadonlySet<string>; bulkDisabled: boolean
}>()
const emit = defineEmits<{
  select: [chefId: string]; hat: [chefId: string]; drag: [event: PointerEvent, chefId: string]; bulkEnable: []; bulkDisable: []
}>()
const groups = computed(() => props.enabled
  ? [{ key: 'enabled', items: props.items }]
  : ['custom', 'native'].map(source => ({
    key: source, items: props.items.filter(item => item.chef.source_kind === source),
  })))
</script>

<template>
  <section v-if="items.length || !filtered" class="chef-section" :data-chef-section="enabled ? 'enabled' : 'disabled'" :aria-label="t(enabled ? 'diychef.enabled' : 'diychef.disabled')">
    <div class="chef-section-heading">
      <h2>{{ t(enabled ? 'diychef.enabled' : 'diychef.disabled') }} <span>{{ items.length }}</span>
        <button type="button" class="chef-bulk" :disabled="frozen || bulkDisabled" :title="t(enabled ? 'diychef.disableAllHint' : 'diychef.enableAllHint')" @pointerdown.stop @click.stop="enabled ? emit('bulkDisable') : emit('bulkEnable')">
          {{ t(enabled ? 'diychef.disableAll' : 'diychef.enableAll') }}
        </button>
        <span v-if="filtered" class="chef-bulk-hint">{{ t('diychef.bulkFilteredHint') }}</span>
      </h2>
    </div>
    <div v-for="group in groups" :key="group.key" class="chef-subgroup" :class="{ 'is-drag-over': enabled && overEnabled, 'is-empty': !group.items.length }" :data-chef-group="group.key">
      <div class="chef-grid">
      <ChefTile v-for="item in group.items" :key="item.chef.id" :item="item" :session-id="sessionId"
        :frozen="frozen" :reorderable="reorderable" :dragging="draggingIds?.has(item.chef.id) || draggingId === item.chef.id" :selected="selectedIds?.has(item.chef.id)"
        @select="emit('select', $event)" @hat="emit('hat', $event)" @drag="(event, id) => emit('drag', event, id)" />
      </div>
    </div>
  </section>
</template>

<style scoped>
.chef-section { margin-top: 0; }
.chef-section[data-chef-section='disabled'] { min-height: 80px; }
.chef-section + .chef-section { margin-top: 18px; padding-top: 14px; border-top: 1px solid var(--oc2-row-line); }
.chef-section-heading { display: flex; align-items: baseline; flex-wrap: wrap; gap: 4px 10px; margin-bottom: 7px; }
h2 { margin: 0; display: flex; align-items: center; flex-wrap: wrap; gap: 6px; color: var(--oc2-text-3); font-size: 12px; font-weight: 500; }
h2 span { color: var(--oc2-text-3); font-size: 11px; font-weight: 400; }
.chef-bulk { padding: 2px 3px; border: 0; border-radius: 4px; background: transparent; color: var(--oc2-text-3); font: inherit; font-size: 11px; cursor: pointer; transition: color 150ms ease; }
.chef-bulk:hover:not(:disabled), .chef-bulk:focus-visible { color: var(--oc2-accent); }
.chef-bulk:focus-visible { outline: 1px solid var(--oc2-accent); outline-offset: 2px; }
.chef-bulk:disabled { opacity: .45; cursor: default; }
.chef-bulk-hint { margin-left: 8px; color: var(--oc2-text-3); font-size: 11px; font-weight: 400; }
.chef-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(128px, 1fr)); gap: 12px; align-items: stretch; }
/* Keep empty destination grids mounted: dragLayout can fill them with slots. */
.chef-subgroup:not(.is-empty) ~ .chef-subgroup:not(.is-empty) { margin-top: 14px; padding-top: 14px; border-top: 1px solid var(--oc2-row-line); }
.chef-subgroup[data-chef-group='enabled'] { min-height: 80px; border-radius: 12px; }
.chef-subgroup.is-drag-over { outline: 1px dashed color-mix(in srgb, var(--oc2-accent) 55%, transparent); outline-offset: 5px; }
@media (prefers-reduced-motion: reduce) { .chef-bulk { transition: none; } }
</style>
