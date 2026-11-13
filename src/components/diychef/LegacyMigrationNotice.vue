<script setup lang="ts">
import { NAlert, NButton } from 'naive-ui'
import type { ChefTarget } from '../../diychef/types'
import { t } from '../../i18n'
defineProps<{ targets: readonly ChefTarget[]; busy: boolean; importing: boolean; canImport: boolean; targetAvailable: boolean }>()
defineEmits<{ import: []; ignore: [] }>()
</script>

<template>
  <n-alert type="warning" class="chef-migration" data-tutorial="diychef-legacy-migration">
    <div class="chef-migration-row">
      <div class="chef-migration-copy">
        <strong>{{ t('diychef.migrationFound', { count: targets.length }) }}</strong>
        <div>{{ t('diychef.migrationHint') }}</div>
        <div v-if="!targetAvailable">{{ t('diychef.migrationTargetRequired') }}</div>
        <ul><li v-for="target in targets" :key="target.id">{{ target.display_path }} · {{ target.version }}</li></ul>
      </div>
      <div class="chef-migration-actions">
        <n-button size="small" type="primary" :loading="importing" :disabled="!canImport" @click="$emit('import')">{{ t('diychef.migrationImport') }}</n-button>
        <n-button size="small" :disabled="busy" @click="$emit('ignore')">{{ t('diychef.migrationIgnore') }}</n-button>
      </div>
    </div>
  </n-alert>
</template>

<style scoped>
.chef-migration { flex: 0 0 auto; margin-bottom: 12px; }
.chef-migration-row { display: flex; align-items: center; gap: 16px; }
.chef-migration-copy { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.chef-migration-copy ul { margin: 4px 0 0; padding-left: 18px; }
.chef-migration-actions { display: flex; gap: 8px; flex: 0 0 auto; }
</style>
