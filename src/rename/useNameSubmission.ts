import { computed, onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import { useDashboard } from '../composables/useDashboardStore'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { nameHistory, type NameAction, type NameDraft } from './history'

export function useNameSubmission() {
  const { game, loadBase } = useDashboard()
  const { busy } = nameHistory
  const notify = useNotify()
  const ready = shallowRef(false)
  const action = computed<NameAction>(() => busy.value ?? (ready.value ? 'push' : 'copy'))
  let mounted = false
  let revision = 0
  let timer: number | undefined

  function cancelCheck() {
    revision += 1
    window.clearTimeout(timer)
  }

  async function refresh() {
    cancelCheck()
    if (!mounted || document.hidden) return
    const current = revision
    const available = await api.colorfulNameReady().catch(() => false)
    if (current !== revision) return
    ready.value = available === true
    if (game.value?.kitchen_running) timer = window.setTimeout(refresh, 3000)
  }

  watch([() => game.value?.path, () => game.value?.kitchen_running], () => {
    ready.value = false
    void refresh()
  })
  onMounted(() => {
    mounted = true
    void refresh()
    if (!game.value) void loadBase()
    window.addEventListener('focus', refresh)
    document.addEventListener('visibilitychange', refresh)
  })
  onBeforeUnmount(() => {
    mounted = false
    cancelCheck()
    window.removeEventListener('focus', refresh)
    document.removeEventListener('visibilitychange', refresh)
  })

  async function submit(draft: NameDraft) {
    if (busy.value) return
    const operation = action.value
    try {
      const result = await nameHistory.submit(operation, draft, operation === 'copy' ? api.copyText : api.pushColorfulName)
      if (!result) return
      if (!result.ok) notify.error(backendMessageOf(result.message))
      else {
        notify.success(operation === 'copy' ? t('rename.copiedPasteItIntoHostutilitiesColorfulNameMenu')
          : result.message ? translateBackendMessage(result.message) : t('rename.pushedToHostutilities'))
        return
      }
    } catch (reason) {
      notify.error(backendMessageOf(reason))
    }
    if (operation === 'push') {
      ready.value = false
      void refresh()
    }
  }

  return { action, busy, submit }
}
