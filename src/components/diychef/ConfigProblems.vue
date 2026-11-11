<script setup lang="ts">
import { NButton } from 'naive-ui'
import { t } from '../../i18n'
import { translateBackendMessage } from '../../locale/backend'
import type { ChefProblem } from '../../diychef/projection'

defineProps<{ problems: ChefProblem[]; frozen: boolean }>()
const emit = defineEmits<{ remove: [rowId: string]; hat: [chefId: string] }>()
</script>

<template>
  <details v-if="problems.length" class="chef-problems">
    <summary>{{ t('diychef.problems', { count: problems.length }) }}</summary>
    <div v-for="problem in problems" :key="problem.row.row_id" class="chef-problem-row">
      <div class="chef-problem-text">
        <code>{{ problem.row.raw_display }}</code>
        <span v-for="diagnostic in problem.diagnostics" :key="diagnostic.id" :class="{ blocking: diagnostic.save_blocking }">{{ translateBackendMessage(diagnostic.code) }}</span>
        <span v-if="!problem.diagnostics.length">{{ t(problem.row.category === 'duplicate' ? 'diychef.duplicateRow' : 'diychef.unresolvedRow') }}</span>
      </div>
      <div class="chef-problem-actions">
        <n-button v-if="problem.chefId" size="tiny" :disabled="frozen" @click="emit('hat', problem.chefId)">{{ t('diychef.changeHat') }}</n-button>
        <n-button size="tiny" :disabled="frozen" @click="emit('remove', problem.row.row_id)">{{ t('diychef.removeProblem') }}</n-button>
      </div>
    </div>
  </details>
</template>

<style scoped>
.chef-problems { margin-top: 14px; border: 1px solid var(--oc2-row-line); background: var(--oc2-row); border-radius: 10px; font-size: 12px; }
summary { padding: 10px 12px; cursor: pointer; color: var(--oc2-text-2); }
summary:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.chef-problem-row { display: flex; align-items: center; gap: 12px; padding: 10px 12px; border-top: 1px solid var(--oc2-row-line); }
.chef-problem-text { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: 4px; }
code { overflow-wrap: anywhere; white-space: pre-wrap; color: var(--oc2-text-1); }
.chef-problem-text span { color: var(--oc2-text-3); }
.chef-problem-text span.blocking { color: var(--oc2-accent); }
.chef-problem-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 6px; }
</style>
