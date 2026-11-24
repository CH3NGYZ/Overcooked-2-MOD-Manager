import { computed, ref, shallowRef } from 'vue'
import { appUpdateBlocksOperations } from '../appUpdateState'
import { demoActive } from '../tutorial/demo'
import { useModChanges } from './store'
import { isChangePlanLocked } from './workflow'
import type { TransactionRecoveryEntry } from './transactionRecoveryTypes'

export const transactionRecoveries = shallowRef<TransactionRecoveryEntry[]>([])
export const transactionRecoveryVisible = ref(false)
export const transactionRecoveryLoading = ref(false)
export const transactionRecoveryReadError = shallowRef<unknown>(null)

export function transactionRecoveryUnavailable() {
  return appUpdateBlocksOperations() || demoActive.value || isChangePlanLocked(useModChanges().state.value?.phase)
}

export const showTransactionRecovery = computed(() => transactionRecoveryVisible.value && !transactionRecoveryUnavailable())

export function openTransactionRecovery() {
  if (!transactionRecoveryUnavailable()) transactionRecoveryVisible.value = true
}
