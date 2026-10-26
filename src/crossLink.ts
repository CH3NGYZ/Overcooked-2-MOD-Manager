import { onBeforeUnmount, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { waitForPageReady } from './pageTransition'

/**
 * 两个页面之间的「标签互跳」：
 * MOD 管理卡片上的状态标签点击后跳到 MOD 安装页，高亮对应仓库卡片 3 秒；
 * MOD 安装页的状态标签点击后跳到 MOD 管理页，高亮绑定到该仓库的 MOD 卡片。
 *
 * 跳转目标通过路由 query `focus` 传递，只用于高亮，不参与任何业务判断。
 */
export const FOCUS_QUERY = 'focus'
/** 高亮持续时间（毫秒）。 */
export const FOCUS_DURATION = 3000

/** 跳到 MOD 安装页并高亮指定仓库。 */
export function useRepositoryLink() {
  const router = useRouter()
  return {
    linkToRepository(repositoryId: string) {
      if (!repositoryId) return
      void router.push({ name: 'mods-online', query: { [FOCUS_QUERY]: repositoryId } })
    },
  }
}

/** 跳到 MOD 管理页并高亮绑定到该仓库的 MOD。 */
export function useLocalModLink() {
  const router = useRouter()
  return {
    linkToLocalMods(repositoryId: string) {
      if (!repositoryId) return
      void router.push({ name: 'mods-local', query: { [FOCUS_QUERY]: repositoryId } })
    },
  }
}

/**
 * 页面侧的高亮状态。
 *
 * 直接 watch 路由 query，而不是只在 onMounted 读一次：
 * 这样路由尚未就绪、或在本页内再次跳转时也能正确触发。
 * 卡片可能还没渲染（Dashboard 未加载、折叠面板未展开），
 * 所以滚动用短间隔重试，直到目标元素出现或超时。
 */
export function useFocusHighlight() {
  const router = useRouter()
  const highlightId = ref<string | null>(null)
  let fadeTimer: number | undefined
  let scrollTimer: number | undefined

  function clearTimers() {
    if (fadeTimer !== undefined) window.clearTimeout(fadeTimer)
    if (scrollTimer !== undefined) window.clearInterval(scrollTimer)
    fadeTimer = undefined
    scrollTimer = undefined
  }

  function stop() {
    clearTimers()
    highlightId.value = null
  }

  /** 目标卡片出现后滚动到视口中间；每 100ms 重试，最多 3 秒。 */
  function scrollWhenReady(id: string) {
    const selector = `[data-focus-id="${CSS.escape(id)}"]`
    let attempts = 0
    const attempt = () => {
      const element = document.querySelector<HTMLElement>(selector)
      if (element) {
        element.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'nearest' })
        if (scrollTimer !== undefined) window.clearInterval(scrollTimer)
        scrollTimer = undefined
        return
      }
      attempts += 1
      if (attempts > 30 && scrollTimer !== undefined) {
        window.clearInterval(scrollTimer)
        scrollTimer = undefined
      }
    }
    attempt()
    if (scrollTimer !== undefined) window.clearInterval(scrollTimer)
    scrollTimer = window.setInterval(attempt, 100)
  }

  function focus(id: string) {
    clearTimers()
    highlightId.value = id
    // 等页面切换过渡结束再滚动，避免和滑入动画抢位置
    void waitForPageReady().then(() => scrollWhenReady(id))
    fadeTimer = window.setTimeout(stop, FOCUS_DURATION)
  }

  // 消费 query 后立刻抹掉，避免刷新或前进后退时重复高亮
  watch(
    () => router.currentRoute.value.query[FOCUS_QUERY],
    (value) => {
      const id = typeof value === 'string' ? value : Array.isArray(value) ? value[0] : null
      if (!id) return
      focus(id)
      const { [FOCUS_QUERY]: _removed, ...rest } = router.currentRoute.value.query
      void router.replace({ name: router.currentRoute.value.name ?? undefined, query: rest })
    },
    { immediate: true },
  )

  onBeforeUnmount(stop)

  return { highlightId, focus, stop }
}
