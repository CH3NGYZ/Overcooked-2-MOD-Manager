import type { InstalledMod, InstalledModStatusLabel, Repository } from './types'
import { compareVersions, modsForRepository } from './modStatus.ts'

/**
 * MOD 状态标签：两页共用同一套「文案 + 语义色」，避免两边各写一份。
 *
 * - MOD 管理页：直接渲染后端 `labels`（已含渠道、Tag 绑定、重复副本判定）。
 * - MOD 安装页：按「已安装版本 vs 仓库最新 Release」比较得出。
 *
 * 颜色在 `src/styles/global.css` 的 `.state-tag--*` 里定义。
 */
export type StatusTone = 'ok' | 'warn' | 'idle' | 'muted' | 'danger' | 'violet'

export interface StatusTag {
  key: string
  text: string
  tone: StatusTone
}

/** 后端标签 → 文案与颜色。 */
export const STATUS_META: Record<string, { text: string; tone: StatusTone }> = {
  update_available: { text: '可更新', tone: 'warn' },
  latest: { text: '最新版', tone: 'ok' },
  /** 已安装的是预发布版本：固定排在「最新版」左侧，黄色。 */
  prerelease: { text: '预发布', tone: 'warn' },
  not_release_asset: { text: '已安装', tone: 'idle' },
  unbound: { text: '未绑定', tone: 'muted' },
  bound_repository_missing: { text: '仓库缺失', tone: 'danger' },
  repository_unavailable: { text: '仓库不可用', tone: 'danger' },
  unknown: { text: '版本未知', tone: 'muted' },
  duplicate: { text: '重复', tone: 'danger' },
  duplicate_lower_version: { text: '低版本', tone: 'violet' },
  load_warning: { text: '加载警告', tone: 'warn' },
  disabled: { text: '已禁用', tone: 'muted' },
  /** 包内的纯依赖 DLL：读不出 BepInPlugin 元数据，只作为依赖展示。 */
  dependency: { text: '依赖', tone: 'muted' },
  installed: { text: '已安装', tone: 'idle' },
  // MOD 安装页专用：仓库维度而不是已安装 MOD 维度
  /** 未安装一律灰色，不区分 Release / RAW。 */
  not_installed: { text: '未安装', tone: 'muted' },
  /** RAW（仓库文件）仓库未安装时也显示「未安装」，不再显示「仓库文件」。 */
  not_installed_raw: { text: '未安装', tone: 'muted' },
  not_refreshed: { text: '未刷新', tone: 'muted' },
  refresh_failed: { text: '刷新失败', tone: 'warn' },
  repository_disabled: { text: '未启用', tone: 'muted' },
}

/**
 * 已安装 MOD 的标签列表。
 *
 * 后端 `labels` 为空时（旧快照或测试数据）退回 `status` 单标签。
 * 已安装的是预发布版本时，额外在最前面插一个「预发布」标签
 * （它不参与后端状态判定，只表达「这个版本是 pre-release」）。
 */
export function installedModTags(mod: InstalledMod): StatusTag[] {
  const labels: string[] = mod.labels?.length ? mod.labels : [mod.status]
  const tags = labels.map((label: InstalledModStatusLabel | string) => {
    const meta = STATUS_META[label]
    return { key: label, text: meta?.text ?? label, tone: meta?.tone ?? 'muted' }
  })
  if (mod.prerelease && !tags.some((tag) => tag.key === 'prerelease')) {
    const meta = STATUS_META.prerelease
    tags.unshift({ key: 'prerelease', text: meta.text, tone: meta.tone })
  }
  return tags
}

/** 本地已装版本：优先安装清单记录的 Release tag，其次插件元数据版本。 */
export function localVersion(mod: InstalledMod): string | undefined {
  return mod.repository_binding?.tag || mod.version
}

/**
 * 仓库维度的状态：用「已安装 MOD 的最高版本」与「仓库最新 Release」比较。
 * 后端按真实安装清单判定（含渠道、Tag 绑定、重复副本），能对上这个仓库时优先采信；
 * 只在后端绑定的不是这个仓库时，才退回本地版本比较。
 */
/** 仓库卡片主按钮的动作：安装 / 重装 / 更新。 */
export type InstallAction = 'install' | 'reinstall' | 'update'

/**
 * 由仓库状态推导主按钮。
 *
 * 规则（用户确认）：
 *   常规仓库：未安装 → 安装；已安装且最新 → 重装；已安装且有更新 → 更新。
 *   仓库文件：未安装 → 安装；已安装 → 重装（没有版本比较的意义）。
 *   未刷新 / 刷新失败 / 未启用时按钮仍可点，按「安装」处理，
 *   交给后端在真正安装前再校验一次版本。
 */
export function repositoryInstallAction(status: StatusTag, repo: Repository): InstallAction {
  if (repo.download_mode === 'repository_file') {
    return status.key === 'installed' ? 'reinstall' : 'install'
  }
  switch (status.key) {
    case 'latest':
      return 'reinstall'
    case 'update_available':
      return 'update'
    default:
      return 'install'
  }
}

export function repositoryStatus(repo: Repository, installedMods: InstalledMod[]): StatusTag {
  const tag = (key: keyof typeof STATUS_META): StatusTag => ({
    key,
    text: STATUS_META[key].text,
    tone: STATUS_META[key].tone,
  })

  // RAW（仓库文件）仓库没有 Release 版本，只有「装没装」两种状态
  const isRaw = repo.download_mode === 'repository_file'
  if (!isRaw) {
    if (!repo.enabled) return tag('repository_disabled')
    if (repo.last_error) return tag('refresh_failed')
  }

  const release = repo.cached_releases[0]
  const matches = modsForRepository(installedMods, repo)
  const installed = matches.reduce<InstalledMod | undefined>((best, mod) => {
    if (!best) return mod
    const compared = compareVersions(localVersion(mod), localVersion(best))
    return compared !== null && compared > 0 ? mod : best
  }, undefined)

  if (!installed) return tag(isRaw ? 'not_installed_raw' : release ? 'not_installed' : 'not_refreshed')
  // RAW 已安装：没有版本比较的意义，直接显示「已安装」
  if (isRaw) return tag('installed')

  const boundHere = installed.repository_id === repo.id || !!installed.repository_ids?.includes(repo.id)
  if (boundHere && installed.labels?.length) {
    if (installed.update_available) return tag('update_available')
    if (installed.labels.includes('latest')) return tag('latest')
  }

  if (!release) return tag('unknown')
  const compared = compareVersions(release.version || release.tag_name, localVersion(installed))
  if (compared === null) return tag('unknown')
  return compared > 0 ? tag('update_available') : tag('latest')
}
