import { pickI18n } from './i18n'
import { modsForRepository } from './modStatus'
import { installedForGuid } from './modRelations'
import type { InstalledMod, ModRepository } from './types'

export interface ModInstallNote { id: string; name: string; text: string }

/** 只有关联安装清单的本地 MOD 才算管理器已安装；线上绑定本身不代表读过说明。 */
export function firstInstallNotes(
  repositories: readonly ModRepository[], mods: readonly InstalledMod[],
): ModInstallNote[] {
  const managedMods = mods.filter((mod) => mod.package_id?.trim())
  const seen = new Set<string>()
  return repositories.flatMap((repo) => {
    if (seen.has(repo.id) || modsForRepository(managedMods, repo).length
      || (repo.plugin_guid && installedForGuid(managedMods, repo.plugin_guid).length)) return []
    seen.add(repo.id)
    const text = pickI18n(repo.note_i18n, repo.note ?? '').trim()
    return text ? [{ id: repo.id, name: pickI18n(repo.name_i18n, repo.name), text }] : []
  })
}
