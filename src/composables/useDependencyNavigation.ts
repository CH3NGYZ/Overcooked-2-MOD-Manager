import { onScopeDispose, shallowRef } from 'vue'
import type { DependencyRow } from '../dependencyRows'
import { t } from '../i18n'
import { useNotify } from '../useNotify'

/** 所有依赖入口共用一个确认流程；展示快照保留至退出动画结束。 */
export const dependencyNavigationOpen = shallowRef(false)
export const pendingDependencyNavigation = shallowRef<DependencyRow>()
let active: { owner: object; resolve: () => DependencyRow | undefined; navigate: (row: DependencyRow) => void } | undefined

function sameTarget(before: DependencyRow, current: DependencyRow | undefined): current is DependencyRow {
  const old = before.jump, next = current?.jump
  return !!old && !!next && before.key === current?.key && before.requirement === current.requirement
    && old.id === next.id && old.targetVersion === next.targetVersion
    && old.versionChange?.direction === next.versionChange?.direction
    && old.versionChange?.installedVersion === next.versionChange?.installedVersion
    && old.versionChange?.targetVersion === next.versionChange?.targetVersion
}

export function cancelDependencyNavigation() {
  dependencyNavigationOpen.value = false
  active = undefined
}

export function confirmDependencyNavigation() {
  if (!dependencyNavigationOpen.value || !active || !pendingDependencyNavigation.value) return
  const request = active
  const current = request.resolve()
  const snapshot = pendingDependencyNavigation.value
  cancelDependencyNavigation()
  if (sameTarget(snapshot, current)) request.navigate(current)
}

export function afterDependencyNavigationLeave() {
  if (!dependencyNavigationOpen.value) pendingDependencyNavigation.value = undefined
}

/** 调用方提供重读当前身份的 getter，确认前不捕获陈旧行或选择另一个候选。 */
export function useDependencyNavigation(navigate: (row: DependencyRow) => void) {
  const message = useNotify()
  const owner = {}
  onScopeDispose(() => { if (active?.owner === owner) cancelDependencyNavigation() })
  function request(resolve: () => DependencyRow | undefined) {
    const row = resolve()
    if (!row?.jump) return
    if (row.jump.repair) {
      const target = { ...row.jump.repair }
      void import('../modChanges/requests').then(({ requestDependencyRepair }) => requestDependencyRepair(target))
        .catch(reason => { void import('../locale/backend').then(({ backendMessageOf }) => message.error(backendMessageOf(reason))) })
      return
    }
    if (row.jump.targetVersion === '' || (!row.jump.targetVersion && row.jump.versionChange)) {
      message.warning(t('tools.dependencyTargetUnavailable')); return
    }
    if (dependencyNavigationOpen.value) return
    if (!row.jump.versionChange) { navigate(row); return }
    pendingDependencyNavigation.value = { ...row, jump: { ...row.jump,
      versionChange: { ...row.jump.versionChange } } }
    active = { owner, resolve, navigate }
    dependencyNavigationOpen.value = true
  }
  return { request }
}
