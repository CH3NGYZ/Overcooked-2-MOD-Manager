import type { InstalledModStatusLabel } from './types'
import { installedModStatusOrder } from './installedModOrdering'
import { t } from './i18n'

export type ModStatusCode = InstalledModStatusLabel | 'checking' | 'pending'

export const MOD_STATUS_TEXT: Record<ModStatusCode, string> = {
  update_available: '有更新', latest: '最新版', not_release_asset: '不是 Release 资产, 无法检测更新',
  unbound: '未绑定在线仓库，无法检测更新', bound_repository_missing: '已绑定仓库未加载，无法检测更新',
  repository_unavailable: '刷新失败，无法检测更新', unknown: '版本信息不足，无法检测更新', duplicate: '重复',
  duplicate_lower_version: '存在更高版本，建议卸载', load_warning: '存在更高版本，建议卸载', disabled: '已禁用', checking: '检查中', pending: '待检查',
}

export const MOD_STATUS_ORDER = installedModStatusOrder as readonly ModStatusCode[]

export function installedModLabelText(code: string) { return code in MOD_STATUS_TEXT ? t(MOD_STATUS_TEXT[code as ModStatusCode]) : code }
export function installedModLabelClass(code: string) {
  if (code === 'disabled') return 'mod-disabled'
  if (['update_available', 'checking', 'pending', 'duplicate'].includes(code)) return 'warning'
  if (code === 'latest') return 'success'
  if (['repository_unavailable', 'load_warning', 'duplicate_lower_version'].includes(code)) return 'danger'
  return 'muted'
}
export function installedModLabelActionable(code: string) { return code === 'update_available' || code === 'latest' }
