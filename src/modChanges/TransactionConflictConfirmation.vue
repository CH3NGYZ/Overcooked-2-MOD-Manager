<script setup lang="ts">
import { nextTick, shallowRef, useTemplateRef, watch, type DeepReadonly } from 'vue'
import { NAlert, NButton } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import { t } from '../i18n'
import ChangeEvidenceList from './ChangeEvidenceList.vue'
import type { ChangePlan } from './types'
import { transactionRecoveryUnavailable } from './transactionRecovery'

const props = defineProps<{ plan: DeepReadonly<Pick<ChangePlan, 'plan_id' | 'recovery_conflicts' | 'recovery_approval'>> }>()
const emit = defineEmits<{ approve: [planId: string, approval: string] }>()
const review = shallowRef<typeof props.plan | null>(null)
const heading = useTemplateRef<HTMLElement>('heading')
function open() {
  if (props.plan.recovery_approval && !transactionRecoveryUnavailable()) review.value = props.plan
}
function approve(event: MouseEvent) {
  const selected = review.value
  review.value = null
  if (event.detail <= 1 && selected === props.plan && selected.recovery_approval && !transactionRecoveryUnavailable()) {
    emit('approve', selected.plan_id, selected.recovery_approval)
  }
}
watch([() => props.plan, transactionRecoveryUnavailable], () => { review.value = null }, { flush: 'sync' })
watch(heading, async element => { await nextTick(); element?.focus({ preventScroll: true }) }, { flush: 'post' })
</script>

<template>
  <NButton type="warning" :disabled="!plan.recovery_approval || transactionRecoveryUnavailable()"
    data-plan-action="review-recovery-conflict" @click="open">{{ t('modChanges.transactionRecovery.reviewConflict') }}</NButton>
  <AppModal :show="!!review" :title="t('modChanges.transactionRecovery.conflictTitle')" :z-index="2602"
    max-width="min(620px, 96%)" :mask-closable="false" :auto-focus="false" @update:show="show => { if (!show) review = null }">
    <div v-if="review" data-recovery-conflict-confirmation class="conflict-confirmation">
      <NAlert type="warning" :bordered="false">
        <p ref="heading" tabindex="-1">{{ t('modChanges.transactionRecovery.conflictDescription') }}</p>
      </NAlert>
      <section v-for="conflict in review.recovery_conflicts" :key="conflict.transaction_id">
        <code>{{ conflict.transaction_id }}</code>
        <ChangeEvidenceList :rows="conflict.paths.map(path => ({ path }))" />
      </section>
    </div>
    <template #footer>
      <div class="conflict-actions">
        <NButton data-plan-action="cancel-recovery-conflict" @click="review = null">{{ t('common.action.cancel') }}</NButton>
        <NButton type="warning" data-plan-action="confirm-recovery-conflict" @click="approve"
          @keydown.enter="(event: KeyboardEvent) => { if (event.repeat) event.preventDefault() }">
          {{ t('modChanges.transactionRecovery.confirmConflict') }}
        </NButton>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.conflict-confirmation { min-width: 0; overflow-wrap: anywhere; }
.conflict-confirmation p { margin: 0; }
.conflict-confirmation section { margin-block-start: 8px; }
.conflict-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: 8px; width: 100%; }
.conflict-actions :deep(.n-button) { max-width: 100%; height: auto; min-height: var(--n-height); }
.conflict-actions :deep(.n-button__content) { white-space: normal; }
</style>
