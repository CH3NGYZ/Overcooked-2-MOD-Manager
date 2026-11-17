import { useModChanges } from './store'
import type { ChangeExecutionResult, RootIntent } from './types'

export interface ToggleTargets {
  dllFiles?: readonly string[]
  mapDirectories?: readonly string[]
  enabled: boolean
  wholeNodes?: boolean
}

/** Resolve physical targets through the backend inventory, never a filtered UI list. */
export async function requestToggleChanges(targets: ToggleTargets): Promise<ChangeExecutionResult | null> {
  const changes = useModChanges()
  await changes.refresh()
  const inventory = changes.evaluation.value
  if (!inventory) throw new Error('modChanges.targetChanged')
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
  return changes.run({ roots: unique })
}

export async function requestAllToggleChanges(kind: 'mod' | 'map', enabled: boolean) {
  const changes = useModChanges()
  await changes.refresh()
  const roots = changes.evaluation.value?.nodes.filter(node => !node.subject_tag && node.kind === kind
    && (kind === 'map' ? !!node.map_dir_name : node.dll_files.length > 0))
    .map(node => ({ node_id: node.node_id, enabled })) ?? []
  if (!roots.length) return null
  return changes.run({ roots })
}
