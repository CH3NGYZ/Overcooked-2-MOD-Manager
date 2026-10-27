import type { InstalledMod, Repository } from './types'

/**
 * 仓库刷新排队：把「有本地已安装 MOD 的仓库」排到最前。
 *
 * 用户的核心诉求是「打开管理器就能立刻看到自己装的 MOD 有没有更新」。
 * 整批刷新并发只有 4，仓库几十个时排在后面的要等很久；而实际需要结果的
 * 往往只有自己装过的那几个。按「已安装优先」排队，第一批并发就能拿到
 * 关心的结果，不必等几十个未安装仓库慢慢刷完。
 *
 * 规则：
 *   - 只看本地有没有装过，不看仓库当前是否启用（启用过滤由调用方先做，
 *     禁用仓库本来就不参与刷新）；
 *   - 一条 MOD 可能登记多个归属仓库（`repository_ids`），命中任一即算已安装；
 *   - 结果按仓库 id 去重：同一个仓库可能被多个已安装 MOD 绑定
 *     （多副本、多包），只应出现在队列里一次；
 *   - 其余仓库保持原顺序，不重排未安装的那一批。
 */
export function prioritizeInstalledRepositories(
  repos: Repository[],
  installed: InstalledMod[],
): Repository[] {
  const installedIds = new Set<string>()
  for (const mod of installed) {
    for (const id of mod.repository_ids ?? []) installedIds.add(id)
    if (mod.repository_id) installedIds.add(mod.repository_id)
  }

  const seen = new Set<string>()
  const prioritized: Repository[] = []
  const rest: Repository[] = []
  for (const repo of repos) {
    if (seen.has(repo.id)) continue
    seen.add(repo.id)
    if (installedIds.has(repo.id)) prioritized.push(repo)
    else rest.push(repo)
  }
  return [...prioritized, ...rest]
}
