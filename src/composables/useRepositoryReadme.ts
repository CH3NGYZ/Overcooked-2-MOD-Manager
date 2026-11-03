/**
 * 仓库 README 的加载与取消。
 *
 * ## 为什么抽出来
 *
 * MOD 安装页的仓库详情（`RepositoryDetail.vue`）要展示 README：请求 → 取消 →
 * 切条目重置这条链最容易写漏。地图详情**不**用这条链 —— 地图的说明来自
 * 条目自己的 `note_i18n`，长文跟着版本（Release note 正文）走，仓库 README
 * 是仓库级的，一个仓库多张地图时会串味。
 *
 * 两份实现里只要有一处忘了 cancel，切换条目的慢旧响应就会覆盖新条目的
 * README —— 用户点开 B 却看到 A 的说明，而且没有任何报错。
 *
 * ## 触发时机
 *
 * `sourceId` 变化即重新拉取（含首次）：抽屉打开时条目 id 才有值，关闭后变回
 * `null`，此时取消在途请求并清空结果，下次打开不会先闪一下上一个条目的内容。
 */
import { onUnmounted, ref, watch } from 'vue'
import { api } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import type { RepositoryReadme } from '../types'

export function useRepositoryReadme(sourceId: () => string | null | undefined) {
  const readme = ref<RepositoryReadme | null>(null)
  const loading = ref(false)
  const error = ref('')
  /** 当前在途请求的 id（后端按它做单飞与取消）。 */
  let requestId: string | null = null

  /**
   * 拉取当前条目的 README。
   *
   * `exists = false` 是**正常结果**（仓库没有 README.md）：面板据此整块收起，
   * 与「请求失败」分开 —— 把「没有说明」显示成红色错误会让用户以为读坏了。
   */
  async function load() {
    const id = sourceId()
    cancel()
    readme.value = null
    error.value = ''
    if (!id) return
    const request = crypto.randomUUID()
    requestId = request
    loading.value = true
    try {
      const result = await api.repositoryReadme(id, request)
      if (requestId === request) readme.value = result
    } catch (reason) {
      if (requestId === request) error.value = backendMessageOf(reason)
    } finally {
      if (requestId === request) {
        loading.value = false
        requestId = null
      }
    }
  }

  function cancel() {
    if (requestId) void api.cancelRepositoryReadme(requestId).catch(() => undefined)
    requestId = null
    loading.value = false
  }

  watch(sourceId, () => { void load() }, { immediate: true })
  onUnmounted(cancel)

  return { readme, loading, error, reload: load }
}
