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

/** 把卡片滚到视口中间（等它渲染出来再滚），供页内跳转复用同一套重试逻辑。 */
export function revealCard(find: () => HTMLElement | null) {
  return scrollIntoViewWhenReady(find)
}

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
 * 地图版的「标签互跳」：
 * 地图管理卡片上的「有更新」标签点击后跳到地图下载页，高亮对应地图卡片；
 * 地图下载页的状态标签点击后跳到地图管理页，高亮绑定到该地图的本地关卡集。
 *
 * 与 MOD 版共用 `FOCUS_QUERY`、`useFocusHighlight` 和高亮时长，差别只在
 * 目标路由与「高亮谁」的语义：MOD 用仓库 ID 串起两页，地图用地图 ID。
 */
/** 跳到地图下载页并高亮指定地图。 */
export function useMapLink() {
  const router = useRouter()
  return {
    linkToMap(mapId?: string) {
      if (!mapId) return
      void router.push({ name: 'maps-online', query: { [FOCUS_QUERY]: mapId } })
    },
  }
}

/**
 * 跳到地图管理页并高亮绑定到该地图的本地关卡集。
 *
 * 传递的是**地图 id**：管理页卡片的 `data-focus-id` 就是绑定的地图 id，
 * 这样「一个地图 ↔ 一个本地关卡集」在两侧用的是同一个锚点值，
 * 不需要页面再按身份做第二次查找。
 */
export function useLevelSetLink() {
  const router = useRouter()
  return {
    linkToLevelSets(mapId?: string) {
      if (!mapId) return
      void router.push({ name: 'levels', query: { [FOCUS_QUERY]: mapId } })
    },
  }
}

/** 卡片上的定位锚点选择器：跨页跳转与页内聚合标签共用同一套 DOM 标记。 */
function focusSelector(id: string) {
  return `[data-focus-id="${CSS.escape(id)}"]`
}

/**
 * 等目标元素出现后滚到视口中间，返回取消函数。
 *
 * 卡片可能还没渲染（Dashboard 未加载、折叠面板未展开），
 * 所以每 100ms 重试一次，最多 3 秒。命中后立刻停止重试 ——
 * 重复调用 scrollIntoView 会不断重启平滑滚动，看起来像抖动。
 */
function scrollIntoViewWhenReady(find: () => HTMLElement | null) {
  let attempts = 0
  let timer: number | undefined
  let done = false

  const cancel = () => {
    done = true
    if (timer !== undefined) window.clearInterval(timer)
    timer = undefined
  }

  const attempt = () => {
    const element = find()
    if (element) {
      element.scrollIntoView({ behavior: 'smooth', block: 'center', inline: 'nearest' })
      cancel()
      return
    }
    attempts += 1
    if (attempts > 30) cancel()
  }

  attempt()
  if (!done) timer = window.setInterval(attempt, 100)
  return cancel
}

/**
 * 页面侧的高亮状态。
 *
 * 直接 watch 路由 query，而不是只在 onMounted 读一次：
 * 这样路由尚未就绪、或在本页内再次跳转时也能正确触发。
 */
export function useFocusHighlight() {
  const router = useRouter()
  const highlightId = ref<string | null>(null)
  let fadeTimer: number | undefined
  let cancelScroll: (() => void) | undefined
  let disposed = false

  function stop() {
    if (fadeTimer !== undefined) window.clearTimeout(fadeTimer)
    fadeTimer = undefined
    cancelScroll?.()
    cancelScroll = undefined
    highlightId.value = null
  }

  function focus(id: string) {
    stop()
    highlightId.value = id
    // 等页面切换过渡结束再滚动，避免和滑入动画抢位置
    void waitForPageReady().then(() => {
      if (disposed) return
      cancelScroll = scrollIntoViewWhenReady(() => document.querySelector<HTMLElement>(focusSelector(id)))
    })
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

  onBeforeUnmount(() => {
    disposed = true
    stop()
  })

  return { highlightId, focus, stop }
}

/* 页头状态标签已改为列表筛选（`useListVisibility`），页内状态定位与脉冲不再需要。 */
