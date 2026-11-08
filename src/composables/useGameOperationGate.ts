import { ref } from 'vue'
import { useDashboard } from './useDashboardStore'

// One application-level notice survives page changes and never resumes a
// blocked operation when dismissed. The user must explicitly try again.
export const gameOperationNotice = ref(false)

export function useGameOperationGate() {
  const { game } = useDashboard()
  function blockModOperation() {
    if (!game.value?.kitchen_running) return false
    gameOperationNotice.value = true
    return true
  }
  return { blockModOperation }
}
