import { onScopeDispose, shallowRef } from 'vue'

export type ConfirmationRequest = {
  title: string
  message: string
  danger?: boolean
  acknowledge?: boolean
}

export function useConfirmationDialog() {
  const confirmation = shallowRef<ConfirmationRequest | null>(null)
  let resolveRequest: ((confirmed: boolean) => void) | undefined
  let disposed = false

  function answerConfirmation(confirmed: boolean) {
    const resolve = resolveRequest
    resolveRequest = undefined
    confirmation.value = null
    resolve?.(confirmed)
  }

  function requestConfirmation(request: ConfirmationRequest): Promise<boolean> {
    // Ignore repeated clicks without replacing the choice already on screen.
    if (disposed || confirmation.value) return Promise.resolve(false)
    return new Promise((resolve) => {
      resolveRequest = resolve
      confirmation.value = request
    })
  }

  onScopeDispose(() => {
    disposed = true
    answerConfirmation(false)
  })

  return { confirmation, requestConfirmation, answerConfirmation }
}
