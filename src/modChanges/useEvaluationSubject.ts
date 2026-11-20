import { computed, onScopeDispose, watch, type MaybeRefOrGetter, toValue } from 'vue'
import { registerEvaluationSubject } from './store'
import type { EvaluationSubject } from './types'

/** Detail selections register once in a shared batch instead of per-row requests. */
export function useEvaluationSubject(subject: MaybeRefOrGetter<EvaluationSubject | undefined>) {
  let release: (() => void) | undefined
  const current = computed(() => toValue(subject))
  // A refreshed Release object with the same identity is not a new selection.
  const stop = watch([() => current.value?.repository_id, () => current.value?.tag,
    () => current.value?.map_id, () => current.value?.dll_file, () => current.value?.map_dir_name], () => {
    release?.()
    const next = current.value
    release = next ? registerEvaluationSubject(next) : undefined
  }, { immediate: true })
  onScopeDispose(() => { stop(); release?.() })
}
