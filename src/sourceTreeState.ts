import { ref } from 'vue'
import type { SourceNode } from './types'

/** One ancestry rule for source snapshots, runtime activity and new children. */
export function sourceDescendantIds(nodes: readonly { id: string; parent_id?: string | null }[], id: string): Set<string> {
  const ids = new Set([id])
  let previous = 0
  while (previous !== ids.size) {
    previous = ids.size
    for (const node of nodes) if (node.parent_id && ids.has(node.parent_id)) ids.add(node.id)
  }
  return ids
}

/** Replace only the requested slice; unrelated objects and relative order survive. */
export function replaceScopedItems<T>(current: readonly T[], incoming: readonly T[], matches: (item: T) => boolean): T[] {
  const replacements = incoming.filter(matches)
  let offset = 0
  const next: T[] = []
  for (const item of current) {
    if (!matches(item)) next.push(item)
    else if (offset < replacements.length) next.push(replacements[offset++]!)
  }
  return next.concat(replacements.slice(offset))
}

/** Ancestor activity belongs to its descendants, never to unrelated siblings. */
export function sourceActivity(nodes: readonly { id: string; parent_id?: string | null }[], active: Record<string, boolean>): Record<string, boolean> {
  const flags = { ...active }
  for (const id of Object.keys(active)) for (const child of sourceDescendantIds(nodes, id)) flags[child] = true
  return flags
}

/** Shared source lifecycle; declarations exist before content and survive errors. */
export function createSourceTreeState() {
  const nodes = ref<SourceNode[]>([])
  function declare(declarations: SourceNode[], scopeId?: string) {
    const previous = new Map(nodes.value.map(node => [node.id, node]))
    const scope = scopeId ? sourceDescendantIds([...nodes.value, ...declarations], scopeId) : undefined
    nodes.value = declarations.map(node => {
      const existing = previous.get(node.id)
      if (scope && !scope.has(node.id)) return existing ?? { ...node, status: 'ready' }
      return existing ? { ...node, status: existing.status, error: existing.error } : node
    })
  }
  function descendants(id: string): Set<string> {
    return sourceDescendantIds(nodes.value, id)
  }
  function begin(id: string, preserveDescendants = false) {
    const ids = descendants(id)
    nodes.value = nodes.value.filter(node => preserveDescendants || node.id === id || !ids.has(node.id))
      .map(node => node.id === id ? { ...node, status: 'loading', error: null } : node)
    return ids
  }
  function settle(id: string, error?: string) {
    nodes.value = nodes.value.map(node => node.id === id
      ? { ...node, status: error ? 'error' : 'ready', error: error ?? null } : node)
  }
  function replace(next: SourceNode[]) { nodes.value = next }
  return { nodes, declare, descendants, begin, settle, replace }
}
