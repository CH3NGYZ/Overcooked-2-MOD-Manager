import { nextTick, onBeforeUnmount, shallowRef, watch } from 'vue'
import { useRouter } from 'vue-router'
import { FOCUS_QUERY, INSTALL_TAG_QUERY } from '../crossLink'
import { waitForPageReady } from '../pageTransition'
import type { ModRepository } from '../types'

export interface DependencyInstallTarget { repositoryId: string; tag: string; request: number }

/** 只准备现有安装面板；不刷新 Release，不调用下载/安装。 */
export function useDependencyInstallGuide(options: {
  repositories: () => readonly ModRepository[]
  loading: () => boolean
  reveal: (repository: ModRepository) => void
  focus: (repositoryId: string) => void
  open: (repositoryId: string) => void
  unavailable: () => void
}) {
  const router = useRouter()
  const target = shallowRef<DependencyInstallTarget>()
  let generation = 0
  let disposed = false
  let consuming: number | undefined
  let cancelWait: (() => void) | undefined

  function cancel() {
    generation += 1
    cancelWait?.()
    cancelWait = undefined
    consuming = undefined
    target.value = undefined
  }
  function consume(repositoryId: string, tag: string) {
    const route = router.currentRoute.value
    if (route.query[FOCUS_QUERY] !== repositoryId || route.query[INSTALL_TAG_QUERY] !== tag) return
    consuming = generation
    const { [FOCUS_QUERY]: _focus, [INSTALL_TAG_QUERY]: _tag, ...query } = route.query
    void router.replace({ name: route.name ?? undefined, query })
  }
  async function begin(repositoryId: string, tag: string) {
    cancel()
    const request = generation
    // 路由提交时立即消费：若等布局/过渡后再 replace，会取消用户随后发起的新跳转。
    consume(repositoryId, tag)
    // 异步源树尚未到达时等待当前缓存；加载完成后仍缺失，明确报错而非选最新版。
    const repository = await new Promise<ModRepository | undefined>((resolve) => {
      let stop: (() => void) | undefined
      let timer: ReturnType<typeof setTimeout> | undefined
      let settled = false
      const finish = (repo?: ModRepository) => {
        if (settled) return
        settled = true
        stop?.()
        if (timer) clearTimeout(timer)
        resolve(repo)
      }
      cancelWait = () => finish()
      stop = watch([options.repositories, options.loading], () => {
        const repo = options.repositories().find(entry => entry.id === repositoryId)
        if (repo?.cached_releases.some(release => release.tag_name === tag) || !options.loading()) finish(repo)
      }, { immediate: true })
      if (settled) stop()
      else timer = setTimeout(() => finish(), 10000)
    })
    if (disposed || request !== generation) return
    cancelWait = undefined
    if (!repository?.cached_releases.some(release => release.tag_name === tag)) {
      options.unavailable()
      return
    }
    options.reveal(repository)
    await nextTick()
    await waitForPageReady()
    if (disposed || request !== generation) return
    // reveal/fade 期间可能换源或刷新缓存，再次校验同一实际 tag。
    const current = options.repositories().find(entry => entry.id === repositoryId)
    if (!current?.cached_releases.some(release => release.tag_name === tag)) {
      options.unavailable()
      return
    }
    target.value = { repositoryId, tag, request }
    options.focus(repositoryId)
    options.open(repositoryId)
  }

  watch(() => router.currentRoute.value, route => {
    if (route.name !== 'mods-online') { cancel(); return }
    const id = route.query[FOCUS_QUERY], tag = route.query[INSTALL_TAG_QUERY]
    if (typeof id === 'string' && id && typeof tag === 'string' && tag) void begin(id, tag)
    else if (!id && !tag && consuming === generation) consuming = undefined
    else cancel()
  }, { immediate: true, flush: 'sync' })
  onBeforeUnmount(() => { disposed = true; cancel() })
  return { target, cancel }
}
