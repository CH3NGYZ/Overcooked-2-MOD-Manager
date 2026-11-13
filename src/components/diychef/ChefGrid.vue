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
  ? [{ key: 'enabled', label: '', items: props.items }]
  : ['custom', 'native'].map(source => ({
    key: source, label: t(`diychef.${source}Chefs`), items: props.items.filter(item => item.chef.source_kind === source),
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
    <div v-for="group in groups" v-show="group.items.length || !filtered" :key="group.key" class="chef-subgroup" :class="{ 'is-drag-over': enabled && overEnabled }" :data-chef-group="group.key">
      <h3 v-if="group.label">{{ group.label }} <span>{{ group.items.length }}</span></h3>
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
.chef-section + .chef-section { margin-top: 24px; padding-top: 22px; border-top: 1px solid var(--oc2-row-line); }
.chef-section-heading { display: flex; align-items: baseline; flex-wrap: wrap; gap: 6px 15px; margin-bottom: 10px; }
h2 { margin: 0; color: var(--oc2-text-1); font-size: 14px; font-weight: 600; }
h2 span { margin-left: 4px; color: var(--oc2-text-3); font-size: 12px; font-weight: 400; }
.chef-bulk { margin-left: 8px; padding: 2px 7px; border: 1px solid var(--oc2-row-line); border-radius: 6px; background: transparent; color: var(--oc2-accent); font: inherit; font-size: 11px; cursor: pointer; }
.chef-bulk:disabled { opacity: .45; cursor: default; }
.chef-bulk-hint { margin-left: 8px; color: var(--oc2-text-3); font-size: 11px; font-weight: 400; }
.chef-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(128px, 1fr)); gap: 12px; align-items: stretch; }
.chef-grid:empty { min-height: 48px; }
.chef-subgroup + .chef-subgroup { margin-top: 18px; }
.chef-subgroup[data-chef-group='enabled'] { min-height: 80px; border-radius: 12px; }
.chef-subgroup.is-drag-over { outline: 1px dashed color-mix(in srgb, var(--oc2-accent) 55%, transparent); outline-offset: 5px; }
h3 { margin: 0 0 9px; color: var(--oc2-text-2); font-size: 12px; font-weight: 500; }
h3 span { margin-left: 4px; color: var(--oc2-text-3); font-weight: 400; }
</style>
