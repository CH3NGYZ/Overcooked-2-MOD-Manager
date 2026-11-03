/**
 * 教程进度：哪些步骤已经看过。
 *
 * 存进 `config.toml` 的 `tutorial_watched_steps`，由后端做并集保存。
 * 这里只放纯函数：引擎与 Node 测试共用同一套「怎么算看过」的规则，
 * 不在两处各写一遍 Set 逻辑。
 */

/** 去掉空白与重复，保留首次出现的顺序。 */
export function normalizeStepIds(ids: readonly string[]): string[] {
  const seen = new Set<string>()
  const normalized: string[] = []
  for (const raw of ids) {
    const id = raw.trim()
    if (!id || seen.has(id)) continue
    seen.add(id)
    normalized.push(id)
  }
  return normalized
}

/**
 * 还没看过的步骤，顺序沿用步骤表。
 *
 * `when` 的过滤由调用方先做：这里只认 id，不认平台条件。
 */
export function unwatchedSteps<T extends { id: string }>(
  steps: readonly T[],
  watched: readonly string[],
): T[] {
  const seen = new Set(normalizeStepIds(watched))
  return steps.filter((step) => !seen.has(step.id))
}

/** 把一批新看过的 id 并进已有集合（已有顺序不变，新的追加在后）。 */
export function mergeWatchedSteps(
  watched: readonly string[],
  justWatched: readonly string[],
): string[] {
  return normalizeStepIds([...watched, ...justWatched])
}
