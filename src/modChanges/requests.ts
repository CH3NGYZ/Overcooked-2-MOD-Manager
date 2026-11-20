import { useDashboard } from '../composables/useDashboardStore'
import { useModChanges } from './store'
import type { ChangeExecutionResult, DependencyRepairTarget, RootIntent } from './types'

/** Exact release previews still request that release, unlike an installed owner's dependency action. */
export function requestDependencyRepair(target: DependencyRepairTarget) {
  return useModChanges().run(target.candidate_id == null
    ? { repair_dependencies_of: target.node_id }
    : { roots: [{ ...target, enabled: true }] })
}

export interface ToggleTargets {
  dllFiles?: readonly string[]
  mapDirectories?: readonly string[]
  enabled: boolean
  wholeNodes?: boolean
}

/** Resolve physical targets through the backend inventory, never a filtered UI list. */
export async function requestToggleChanges(targets: ToggleTargets): Promise<ChangeExecutionResult | null> {
  const changes = useModChanges()
  const directMap = targets.mapDirectories?.length === 1 && !targets.dllFiles?.length
  const directMod = targets.dllFiles?.length === 1 && !targets.mapDirectories?.length
  const tryDirectToggle = !targets.wholeNodes && (directMap || directMod)
  // Capture the card's scan before an evaluation or game-directory switch can finish.
  const generation = tryDirectToggle ? useDashboard().modsGeneration.value : 0
  if (tryDirectToggle && !generation) throw new Error('modChanges.targetChanged')
  const inventory = await changes.refresh()
  const roots: RootIntent[] = []
  for (const dll of targets.dllFiles ?? []) {
    const matches = inventory.nodes.filter(node => !node.subject_tag && node.dll_files.includes(dll))
    if (matches.length !== 1) throw new Error('modChanges.targetChanged')
    const node = matches[0]!
    const member = node.members?.find(entry => entry.dll_file === dll)
    if (!targets.wholeNodes && node.dll_files.length > 1 && !member) throw new Error('modChanges.targetChanged')
    roots.push({ node_id: node.node_id, enabled: targets.enabled,
      ...(!targets.wholeNodes && member ? { member_key: member.key } : {}) })
  }
  for (const dir of targets.mapDirectories ?? []) {
    const matches = inventory.nodes.filter(node => !node.subject_tag && node.map_dir_name === dir)
    if (matches.length !== 1) throw new Error('modChanges.targetChanged')
    roots.push({ node_id: matches[0]!.node_id, enabled: targets.enabled })
  }
  const unique = roots.filter((root, index) => roots.findIndex(peer =>
    peer.node_id === root.node_id && peer.member_key === root.member_key) === index)
  if (!unique.length) throw new Error('modChanges.targetChanged')
  return changes.run({ roots: unique,
    ...(tryDirectToggle ? { toggle_scope: { generation, revision: inventory.revision } } : { bulk_toggle: true }),
  })
}

export async function requestAllToggleChanges(kind: 'mod' | 'map', enabled: boolean) {
  const changes = useModChanges()
  await changes.refresh()
  const roots = changes.evaluation.value?.nodes.filter(node => !node.subject_tag && node.kind === kind
    && (kind === 'map' ? !!node.map_dir_name : node.dll_files.length > 0))
    .map(node => ({ node_id: node.node_id, enabled })) ?? []
  if (!roots.length) return null
  return changes.run({ roots, bulk_toggle: true })
}
