import { nextTick, onBeforeUnmount, type Ref } from 'vue'
import { createChefCardMotion, type ChefMotionIds } from '../diychef/cardMotion.ts'
import { pausePreviewWork } from '../diychef/preview/queue.ts'

/** Coordinate a real draft mutation with focus/scroll and the page's FLIP lane. */
export function useChefMotion(root: Readonly<Ref<HTMLElement | null>>) {
  const lane = createChefCardMotion(() => root.value)
  let generation = 0
  let releasePreviews: (() => void) | undefined
  function pausePreviews() { releasePreviews ??= pausePreviewWork() }
  function resumePreviews() { releasePreviews?.(); releasePreviews = undefined }
  async function change(chefId: ChefMotionIds, mutate: () => void, focus?: () => void | Promise<void>) {
    const ticket = ++generation
    pausePreviews()
    try {
      const before = lane.capture(chefId)
      mutate()
      await nextTick()
      if (ticket !== generation) return
      await focus?.()
      if (ticket !== generation) return
      lane.play(before)
    } finally {
      // The draft action completes immediately; portraits wait for every
      // movement to finish, including a flight interrupted by another action.
      if (ticket === generation) void lane.whenIdle().then(() => { if (ticket === generation) resumePreviews() })
    }
  }
  function adopt(chefId: string, ghost: HTMLElement) { lane.adopt(chefId, ghost) }
  function settle(chefId: ChefMotionIds) {
    if (lane.hasAdopted()) void change(chefId, () => {})
  }
  function cancel(chefId: string, ghost: HTMLElement) { adopt(chefId, ghost); settle(chefId) }
  function pickup(chefId: string, memberIds?: readonly string[]) {
    generation += 1
    pausePreviews()
    try {
      const pickup = lane.pickup(chefId, memberIds)
      if (!pickup) resumePreviews()
      return pickup
    } catch (error) { resumePreviews(); throw error }
  }
  onBeforeUnmount(() => { generation += 1; lane.stop(); resumePreviews() })
  return { change, adopt, settle, cancel, pickup, preview: lane.preview, clearPreview: lane.clearPreview, snapshot: lane.snapshot }
}
