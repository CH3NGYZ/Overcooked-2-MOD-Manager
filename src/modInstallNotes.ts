import { ref, shallowRef } from 'vue'
import { pickI18n } from './i18n'
import { modsForRepository } from './modStatus'
import { installedForGuid } from './modRelations'
import type { InstalledMod, ModRepository } from './types'

export interface ModInstallNote { id: string; name: string; text: string }

/** 与卡片共用语言回退及仓库绑定规则；安装过的 MOD 无须重复阅读。 */
export function firstInstallNotes(
  repositories: readonly ModRepository[], mods: readonly InstalledMod[],
): ModInstallNote[] {
  const seen = new Set<string>()
  return repositories.flatMap((repo) => {
    if (seen.has(repo.id) || modsForRepository(mods, repo).length
      || (repo.plugin_guid && installedForGuid(mods, repo.plugin_guid).length)) return []
    seen.add(repo.id)
    const text = pickI18n(repo.note_i18n, repo.note ?? '').trim()
    return text ? [{ id: repo.id, name: pickI18n(repo.name_i18n, repo.name), text }] : []
  })
}

export const modInstallNote = shallowRef<ModInstallNote | null>(null)
export const noteSecondsRemaining = ref(0)
let pending: ModInstallNote[] = []
let settle: ((ready: boolean) => void) | undefined
let timer: ReturnType<typeof setInterval> | undefined
let deadline = 0

function clearTimer() {
  if (timer !== undefined) clearInterval(timer)
  timer = undefined
}

function showNext() {
  clearTimer()
  modInstallNote.value = pending.shift() ?? null
  if (!modInstallNote.value) { finishModInstallNotes(true); return }
  deadline = Date.now() + 5000
  noteSecondsRemaining.value = 5
  timer = setInterval(() => {
    noteSecondsRemaining.value = Math.max(0, Math.ceil((deadline - Date.now()) / 1000))
    if (!noteSecondsRemaining.value) clearTimer()
  }, 100)
}

/** 一批说明全部通过才放行；取消整批，且不会保留跨安装的“已读”状态。 */
export function confirmModInstallNotes(notes: readonly ModInstallNote[]): Promise<boolean> {
  if (!notes.length) return Promise.resolve(true)
  // 不替换正在阅读的另一批，避免并行入口串扰或跳过说明。
  if (settle) return Promise.resolve(false)
  return new Promise((resolve) => {
    settle = resolve
    pending = [...notes]
    showNext()
  })
}

export function continueModInstallNote() {
  if (!modInstallNote.value || Date.now() < deadline) return
  showNext()
}

export function finishModInstallNotes(ready = false) {
  clearTimer()
  pending = []
  modInstallNote.value = null
  noteSecondsRemaining.value = 0
  const resolve = settle
  settle = undefined
  resolve?.(ready)
}
