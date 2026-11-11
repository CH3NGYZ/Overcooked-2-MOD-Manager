import { onBeforeUnmount, shallowRef } from 'vue'
import type { ChefChange } from '../diychef/types'

/** Confirmation promises are settled on every close and disposal path. */
export function useDiyChefConfirmations(save: () => Promise<boolean>) {
  const changes = shallowRef<ChefChange[]>([])
  const changesOpen = shallowRef(false)
  const leaveOpen = shallowRef(false)
  let finishChanges: ((value: boolean) => void) | null = null
  let finishLeave: ((value: boolean) => void) | null = null

  function resolveChanges(value: boolean) {
    changesOpen.value = false
    finishChanges?.(value)
    finishChanges = null
  }
  function resolveLeave(value: boolean) {
    leaveOpen.value = false
    finishLeave?.(value)
    finishLeave = null
  }
  function confirmChanges(value: ChefChange[]): Promise<boolean> {
    resolveChanges(false)
    changes.value = value
    changesOpen.value = true
    return new Promise((resolve) => { finishChanges = resolve })
  }
  function confirmLeave(): Promise<boolean> {
    resolveLeave(false)
    leaveOpen.value = true
    return new Promise((resolve) => { finishLeave = resolve })
  }
  async function saveAndLeave() {
    leaveOpen.value = false
    try { resolveLeave(await save()) }
    catch { resolveLeave(false) }
  }
  const cancelChanges = () => resolveChanges(false)
  const cancelLeave = () => resolveLeave(false)
  onBeforeUnmount(() => { cancelChanges(); cancelLeave() })
  return { changes, changesOpen, leaveOpen, confirmChanges, confirmLeave, resolveChanges, resolveLeave, cancelChanges, cancelLeave, saveAndLeave }
}
