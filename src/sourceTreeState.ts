import { ref } from 'vue'
import type { SourceNode } from './types'

/** Shared source lifecycle; declarations exist before content and survive errors. */
export function createSourceTreeState() {
  const nodes = ref<SourceNode[]>([])
  function declare(declarations: SourceNode[]) {
    const previous = new Map(nodes.value.map(node => [node.id, node]))
    nodes.value = declarations.map(node => {
      const existing = previous.get(node.id)
      return existing ? { ...node, status: existing.status, error: existing.error } : node
    })
  }
  function descendants(id: string): Set<string> {
    const ids = new Set([id])
    let changed = true
    while (changed) {
      changed = false
      for (const node of nodes.value) {
        if (node.parent_id && ids.has(node.parent_id) && !ids.has(node.id)) {
          ids.add(node.id); changed = true
        }
      }
    }
    return ids
  }
  function begin(id: string) {
    const ids = descendants(id)
    nodes.value = nodes.value.filter(node => node.id === id || !ids.has(node.id))
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
