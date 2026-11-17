import { onScopeDispose, watch, type MaybeRefOrGetter, toValue } from 'vue'
import { registerEvaluationSubject } from './store'
import type { EvaluationSubject } from './types'

/** Detail selections register once in a shared batch instead of per-row requests. */
export function useEvaluationSubject(subject: MaybeRefOrGetter<EvaluationSubject | undefined>) {
  let release: (() => void) | undefined
  const stop = watch(() => toValue(subject), next => {
    release?.()
    release = next ? registerEvaluationSubject(next) : undefined
  }, { immediate: true, deep: true })
  onScopeDispose(() => { stop(); release?.() })
}
