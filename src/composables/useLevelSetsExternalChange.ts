import { onBeforeUnmount, onMounted, type Ref } from 'vue'
import { watchLevelSetsChanged } from '../services/tauri'
import { createChangeGate } from '../levelSetsChangeGate'
import type { UnlistenFn } from '@tauri-apps/api/event'

/**
 * 订阅后端 `level-sets-changed`：用户在资源管理器里加/删/改 `levels` 下的
 * 目录时，让页面自己重读，不用手动点刷新。
 *
 * 收口的只有订阅生命周期与「这一声该不该读」（见
 * `src/levelSetsChangeGate.ts`）。页面仍然拥有重读实现 —— 两个管理页的
 * 重读内容不同（关卡集要连地图列表一起读，MOD 页要重扫停用目录）。
 */
export function useLevelSetsExternalChange(
  reload: () => void | Promise<void>,
  isBusy: () => boolean,
): void {
  const gate = createChangeGate({ delay: 400, isBusy, reload })
  let unlisten: UnlistenFn | null = null
  let disposed = false

  onMounted(() => {
    void watchLevelSetsChanged(() => gate.notify()).then((off) => {
      // 订阅是异步完成的：挂载期间就被卸载时立刻退订，不要留悬挂监听。
      if (disposed) off()
      else unlisten = off
    })
  })

  onBeforeUnmount(() => {
    disposed = true
    unlisten?.()
    unlisten = null
    gate.dispose()
  })
}

/** 页面忙碌标志的只读形态（`ref` 或函数都收）。 */
export type BusySource = Ref<boolean> | (() => boolean)

export function busyReader(source: BusySource): () => boolean {
  return typeof source === 'function' ? source : () => source.value
}
