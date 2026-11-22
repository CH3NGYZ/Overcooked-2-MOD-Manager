<script setup lang="ts">
import { NProgress } from 'naive-ui'

defineProps<{ label: string; rows: readonly { label: string; value: string }[]; progress?: number | null }>()
</script>

<template>
  <table class="status-table" :aria-label="label">
    <tbody>
      <tr v-for="row in rows" :key="row.label"><th scope="row">{{ row.label }}</th><td>{{ row.value }}</td></tr>
      <tr v-if="progress !== undefined">
        <td colspan="2">
          <div role="progressbar" :aria-label="label" :aria-valuenow="progress ?? undefined">
            <NProgress type="line" :percentage="progress ?? 100" :processing="progress === null" :show-indicator="false" :height="4" aria-hidden="true" />
          </div>
        </td>
      </tr>
    </tbody>
  </table>
</template>

<style scoped>
.status-table { width: 100%; border-collapse: collapse; font-size: 12px; }
.status-table th, .status-table td { padding: 8px 10px; border: 1px solid var(--oc2-row-line); }
.status-table th { text-align: start; font-weight: 400; background: var(--oc2-sunken); }
.status-table td { text-align: end; font-variant-numeric: tabular-nums; overflow-wrap: anywhere; }
@media (prefers-reduced-motion: reduce) { .status-table :deep(.n-progress-graph-line-fill::after) { animation: none; } }
</style>
