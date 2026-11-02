/**
 * 「安装前检查」的展示模型 —— MOD 关系计划与同身份冲突共用一份。
 *
 * 为什么要有这一层：装一个 MOD 与导入一张地图，装之前要处理的东西是同一类
 * （先把占地方的卸掉、再把版本换到区间内、最后补缺件），弹窗长相也该一样。
 * 但两边拿到的东西不同：MOD 侧是 `modRelations.buildInstallPlan` 的计划
 * （`RelationStatus`），地图侧是后端报的 `InstallConflict` 清单。把「翻译成
 * 行」的规则收在这里，弹窗与两个宿主都不用知道对方的类型。
 *
 * 边界：本模块**不做任何卸载/安装**，也不 import Vue 或 IPC —— 它只产出
 * 「有哪些行、每行叫什么、失败时提醒什么」。真正动手的是宿主注入的
 * `execute(row)`，用 `row.kind` + `row.index` 取回它自己的原始条目。
 * 这样 Node 测试可以直接跑真实源码，不需要假后端。
 */
import { pickI18n, t } from './i18n'
import { formatVersion } from './modStatus'
import type { InstallPlan, RelationStatus } from './modRelations'
import type { InstallConflict } from './types'

/** 一行为什么要做。`install-dependency` 是「缺依赖，顺手补上」。 */
export type InstallCheckAction = 'uninstall' | 'upgrade' | 'downgrade' | 'install-dependency'

/** 弹窗里的一行：一件要在装主包之前做完的事。 */
export interface InstallCheckRow {
  /** 稳定标识：执行状态按它记，跨次打开不能撞。 */
  key: string
  /**
   * 行的性质：卸载本地已有内容 / 安装（含换版本）。
   *
   * 弹窗按它决定标签色（卸载是红色警示）与执行顺序（卸载全排在安装前）。
   */
  kind: 'uninstall' | 'install'
  /** 关系种类：只用于解释这一行为什么要做。 */
  action: InstallCheckAction
  /** 动作标签，已翻译（「卸载」/「升级」/「装依赖」）。 */
  actionLabel: string
  /** 展示名：卸载是本地已有内容的名字，安装是目标仓库名。 */
  label: string
  /** 目标版本（安装步骤才有；卸载步骤没有版本可写）。 */
  version?: string
  /** 与本次安装的版本相同：只影响行尾灰标签，不改变动作。 */
  sameVersion?: boolean
  /**
   * 在同类数组里的下标。
   *
   * 弹窗不执行任何动作，它只把这一行交回宿主注入的 `execute`；宿主用
   * `kind` + `index` 取回自己的原始条目。模型里因此不出现 Vue 或 IPC 类型。
   */
  index: number
}

/** 一份待处理的检查：标题 + 说明 + 可执行行 + 只能提醒的条目。 */
export interface InstallCheckModel {
  /** 弹窗标题（两种安装都叫「安装前检查」）。 */
  title: string
  /** 说明行：为什么要在装之前处理这些。 */
  lead: string
  /** 按顺序执行的行：卸载在前，安装在后。 */
  rows: InstallCheckRow[]
  /** 无法自动处理的提醒（已翻译的整句），只解释原因、不给按钮。 */
  blocked: string[]
  /** 行尾「同版本」标记的文案。 */
  sameVersionLabel: string
}

/** 行首动作标签的语言表 key。 */
const ACTION_LABELS: Record<InstallCheckAction, string> = {
  uninstall: 'recycle.uninstall',
  upgrade: 'recycle.upgrade',
  downgrade: 'recycle.downgrade',
  'install-dependency': 'modsOnline.installDependency',
}

/**
 * 这一步是「升级」「降级」还是「装依赖」。
 *
 * 依赖关系上没装过 → 装依赖；装过而新版本更高 → 升级，更低 → 降级。
 * 兼容关系同理：对方存在但不兼容，装的版本比本地高就是升级。
 */
function installActionOf(status: RelationStatus): InstallCheckAction {
  const kind = status.relation.kind ?? 'requires'
  if (kind === 'requires' && !status.installedMod) return 'install-dependency'
  const current = status.installedVersion
  const next = status.release?.tag_name || status.release?.version || ''
  if (!current) return 'install-dependency'
  const compared = compare(current, next)
  if (compared === null) return 'install-dependency'
  return compared > 0 ? 'downgrade' : 'upgrade'
}

/** 只做数值/后缀比较，够判断「升还是降」；解析不出时返回 null。 */
function compare(left: string, right: string): number | null {
  const parse = (value: string) => {
    const match = value.trim().replace(/^[vV]/, '').match(/^(\d+(?:\.\d+)*)(.*)$/)
    if (!match) return null
    return {
      numbers: match[1].split('.').map(Number),
      suffix: match[2],
    }
  }
  const a = parse(left)
  const b = parse(right)
  if (!a || !b) return null
  const length = Math.max(a.numbers.length, b.numbers.length)
  for (let index = 0; index < length; index += 1) {
    const delta = (a.numbers[index] ?? 0) - (b.numbers[index] ?? 0)
    if (delta !== 0) return delta
  }
  // 数字相同：没有后缀的正式版更高
  if (!a.suffix && !b.suffix) return 0
  if (!a.suffix) return 1
  if (!b.suffix) return -1
  return a.suffix.localeCompare(b.suffix)
}

/**
 * 提醒文案：说清「跟谁」「差在哪」。
 *
 * 这些条目无法自动处理，所以文案必须能让用户自己判断该怎么做：
 * 兼容关系要说明需要的区间，冲突要说明该卸载，缺依赖要说明缺什么。
 */
function blockedText(status: RelationStatus): string {
  const kind = status.relation.kind ?? 'requires'
  const target = status.installedMod?.name ?? status.relation.guid
  const min = status.relation.min_version?.trim()
  const max = status.relation.max_version?.trim()
  const range = min && max ? `${min} ~ ${max}` : min ? `≥${min}` : max ? `≤${max}` : ''

  if (kind === 'compatible') {
    // 兼容列表：只能提醒版本不对，卸载对方没有意义
    return range
      ? t('modsOnline.isIncompatibleAVersionInIsRequired', {
        name: target, version: status.installedVersion ?? '', range,
      })
      : t('modsOnline.isIncompatiblePleaseCheck', {
        name: target, version: status.installedVersion ?? '',
      })
  }
  if (kind === 'conflicts') {
    return t('modsOnline.conflictsPleaseUninstallIt', {
      name: target, version: status.installedVersion ?? '',
    })
  }
  // requires：缺或版本不符
  return range
    ? t('modsOnline.missingDependencyAVersionInIsRequired', { name: target, range })
    : t('modsOnline.missingDependency2', { name: target })
}

/**
 * MOD 关系计划 → 检查模型。
 *
 * 顺序固定为 卸载 → 改版本 → 装依赖：先腾地方，再补版本，最后补缺件。
 * 反过来的话，「装依赖」可能装上一个马上要被卸掉的版本。
 */
export function installCheckModelFromPlan(
  plan: InstallPlan,
  targetLabel: string,
): InstallCheckModel {
  const rows: InstallCheckRow[] = []

  plan.uninstall.forEach((status, index) => {
    rows.push({
      key: `uninstall-${status.installedMod?.dll_file ?? status.relation.guid}`,
      kind: 'uninstall',
      action: 'uninstall',
      actionLabel: t(ACTION_LABELS.uninstall),
      label: status.installedMod?.name ?? status.relation.guid,
      index,
    })
  })

  plan.install.forEach((status, index) => {
    const action = installActionOf(status)
    const version = status.release?.tag_name || status.release?.version || ''
    rows.push({
      key: `install-${status.repository?.id ?? status.relation.guid}-${version}`,
      kind: 'install',
      action,
      actionLabel: t(ACTION_LABELS[action]),
      label: status.repository
        ? pickI18n(status.repository.name_i18n, status.repository.name)
        : status.relation.guid,
      version: version || undefined,
      index,
    })
  })

  return {
    title: t('modsOnline.preInstallCheck'),
    lead: t('modsOnline.beforeInstallingTheseRelationsMustBeHandledFirst', { name: targetLabel }),
    rows,
    blocked: plan.blocked.map(blockedText),
    sameVersionLabel: t('tools.installConflictSameVersion'),
  }
}

export interface ConflictCheckOptions {
  /**
   * 目标名（要装的那个包/地图的名字）。
   *
   * 只影响说明行；不传时用通用那句「以下已安装内容与本次安装的身份相同…」，
   * 因为本地 ZIP 与直链没有可靠的目标名（暂存路径对用户没有意义）。
   */
  targetLabel?: string
  /** 直接给说明行（已翻译），优先于 `targetLabel`。 */
  intro?: string
}

/**
 * 同身份冲突 → 检查模型：只有「卸载」一种行。
 *
 * 行的下标与传入数组一一对应，宿主的 `execute(row)` 可以直接
 * `conflicts[row.index]` 取回要卸的那一项 —— 弹窗本身不认识 MOD 或地图，
 * 也就不会为两种域各写一份卸载调用。
 *
 * 版本号走 `formatVersion`，与安装详情里冲突块显示同一口径（去掉尾部 0，
 * 空版本写 `—`）：同一个已装项在两个弹窗里必须长得一样。
 */
export function conflictCheckModel(
  conflicts: readonly InstallConflict[],
  options: ConflictCheckOptions = {},
): InstallCheckModel {
  const rows: InstallCheckRow[] = conflicts.map((conflict, index) => ({
    key: `uninstall-${conflict.handle}`,
    kind: 'uninstall',
    action: 'uninstall',
    actionLabel: t(ACTION_LABELS.uninstall),
    label: conflict.name || conflict.handle,
    version: formatVersion(conflict.version) || '—',
    sameVersion: conflict.same_version,
    index,
  }))

  const name = options.targetLabel?.trim()
  return {
    title: t('modsOnline.preInstallCheck'),
    lead: options.intro
      ?? (name
        ? t('tools.beforeInstallingTargetMustBeUninstalledFirst', { name })
        : t('tools.installConflictHint')),
    rows,
    blocked: [],
    sameVersionLabel: t('tools.installConflictSameVersion'),
  }
}
