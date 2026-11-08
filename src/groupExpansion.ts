/** Derive expanded group keys from the available keys and user-closed keys. */
export function expandedGroupNames(
  allKeys: string[],
  collapsedKeys: readonly string[],
  forceExpand: boolean,
): string[] {
  if (forceExpand) return allKeys
  const collapsed = new Set(collapsedKeys)
  return allKeys.filter((key) => !collapsed.has(key))
}

/** Store the closed keys while keeping newly appearing groups open by default. */
export function collapsedGroupNames(allKeys: string[], openKeys: readonly string[]): string[] {
  const open = new Set(openKeys)
  return allKeys.filter((key) => !open.has(key))
}

export function expandGroupName(collapsedKeys: readonly string[], key: string): string[] {
  return collapsedKeys.filter((closedKey) => closedKey !== key)
}
