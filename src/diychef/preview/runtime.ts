import { ChefRenderer } from './renderer'
let instance: ChefRenderer | null = null
let idleTimer: ReturnType<typeof setTimeout> | undefined
export function chefRenderer() {
  if (idleTimer) clearTimeout(idleTimer)
  idleTimer = undefined
  return instance ??= new ChefRenderer()
}
export function releaseChefRenderer() {
  if (idleTimer) clearTimeout(idleTimer)
  idleTimer = setTimeout(() => { instance?.dispose(); instance = null; idleTimer = undefined }, 5000)
}
