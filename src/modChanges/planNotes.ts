import { firstInstallNotes } from '../modInstallNotes'
import { sameGuid } from '../modRelations'
import type { InstalledMod, ModRepository } from '../types'
import type { ChangePlan } from './types'
import type { DeepReadonly } from 'vue'

/** Display-only source matching; installed ownership and solving remain backend-owned. */
export function changePlanInstallNotes(plan: DeepReadonly<ChangePlan> | null | undefined, repositories: readonly ModRepository[], mods: readonly InstalledMod[]) {
  if (plan?.status !== 'ready') return []
  const installs = plan.actions.filter(action => ['install', 'upgrade', 'downgrade', 'replace'].includes(action.kind))
  const nodes = plan.nodes.filter(node => installs.some(action => action.node_id === node.node_id))
  return firstInstallNotes(repositories.filter(repo => installs.some(action => action.node_id === `mod:${repo.id}`)
    || nodes.some(node => node.repository_id === repo.id || (repo.plugin_guid && node.members?.some(member =>
      member.guid && sameGuid(member.guid, repo.plugin_guid!))))), mods)
}
