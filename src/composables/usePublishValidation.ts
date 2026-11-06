import { nextTick, onBeforeUnmount, ref, type Ref } from 'vue'

/** 必填反馈只管理 DOM 生命周期，不决定业务缺项。 */
export function usePublishValidation(root: Ref<HTMLElement | null>) {
  const metadataSection = ref<'i18n' | 'relations'>('i18n')
  let timer: ReturnType<typeof setTimeout> | undefined
  let target: HTMLElement | null = null
  let epoch = 0
  function clear() {
    epoch += 1
    clearTimeout(timer)
    target?.classList.remove('validation-pulse')
    target = null
  }
  async function pulse(field: string) {
    clear()
    const current = epoch
    if (field.startsWith('i18n-')) metadataSection.value = 'i18n'
    if (field.startsWith('relation-default-')) metadataSection.value = 'relations'
    await nextTick()
    if (current !== epoch) return
    target = root.value?.querySelector<HTMLElement>(`[data-validation-field="${field}"]`) ?? null
    if (!target) return
    const fieldTarget = target
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches
    fieldTarget.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'center' })
    const input = fieldTarget.querySelector<HTMLElement>('input, textarea') ?? fieldTarget
    input.focus({ preventScroll: true })
    fieldTarget.classList.remove('validation-pulse')
    void fieldTarget.offsetWidth
    fieldTarget.classList.add('validation-pulse')
    timer = setTimeout(clear, 1600)
  }
  onBeforeUnmount(clear)
  return { metadataSection, pulse, clear }
}
