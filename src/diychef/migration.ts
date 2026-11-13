import type { ChefTarget } from './types.ts'

/** A refresh generation alone must not re-show a session-dismissed target set. */
export function legacyTargetSignature(targets: readonly ChefTarget[], gamePath: string): string {
  return JSON.stringify([gamePath, targets.map(target => [target.id, target.version]).sort((a, b) => String(a[0]).localeCompare(String(b[0])))])
}

// Intentionally session-only: neither editor drafts nor user configuration own this choice.
export const ignoredLegacyTargets = new Set<string>()
