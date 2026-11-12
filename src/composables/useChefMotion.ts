import { nextTick, onBeforeUnmount, type Ref } from 'vue'
import { createChefCardMotion } from '../diychef/cardMotion.ts'

/** Coordinate a real draft mutation with focus/scroll and the page's FLIP lane. */
export function useChefMotion(root: Readonly<Ref<HTMLElement | null>>) {
  const lane = createChefCardMotion(() => root.value)
  let generation = 0
  async function change(chefId: string, mutate: () => void, focus?: () => void | Promise<void>) {
    const ticket = ++generation
    const before = lane.capture(chefId)
    mutate()
    await nextTick()
    if (ticket !== generation) return
    await focus?.()
    if (ticket !== generation) return
    lane.play(before)
  }
  function adopt(chefId: string, ghost: HTMLElement) { lane.adopt(chefId, ghost) }
  function settle(chefId: string) {
    if (lane.hasAdopted()) void change(chefId, () => {})
  }
  function cancel(chefId: string, ghost: HTMLElement) { adopt(chefId, ghost); settle(chefId) }
  function pickup(chefId: string) { generation += 1; return lane.pickup(chefId) }
  onBeforeUnmount(() => { generation += 1; lane.stop() })
  return { change, adopt, settle, cancel, pickup, preview: lane.preview, clearPreview: lane.clearPreview, snapshot: lane.snapshot }
}
