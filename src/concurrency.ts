/**
 * 限并发地跑一批任务。
 *
 * ## 为什么要单独一个文件
 *
 * 刷新链路里有两处需要「最多同时跑 N 组」：仓库 / 地图的 Release 刷新，
 * 以及仓库源文档的递归加载。两处各写一遍 worker 循环必然漂移
 * （一边改了上限、另一边还按老值跑），所以收口成一个纯函数。
 *
 * 不依赖 Vue / Naive / i18n：`tests/concurrency.test.mjs` 用 `node --test`
 * 直接 import 这份源码（Node 的 TS 类型擦除，没有 Vite 转译）。
 *
 * ## 约定
 *
 * - 结果**按输入下标归位**，不是完成顺序：调用方拼错误文案、把结果写回
 *   原数组时不需要再自己重排。
 * - `limit` 传 0 / 负数 / 非整数时按 1 处理：宁可慢，也不要变成
 *   「不限并发」把上游打爆。
 * - worker 抛错时停止派发新任务并向上抛（不像 `Promise.all` 那样只等全部
 *   完成）—— 刷新链路的每个 worker 都自己把失败转成文案，抛错说明是
 *   编排层的问题，继续跑没有意义。
 */
export async function mapWithConcurrency<T, R>(
  items: readonly T[],
  limit: number,
  worker: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
  const size = Number.isFinite(limit) ? Math.max(1, Math.floor(limit)) : 1
  if (!items.length) return []

  const results = new Array<R>(items.length)
  let next = 0
  /* 派发下标在同一个同步段内读-改-写，事件循环单线程，不存在两个 worker 抢到同一格。 */
  const runner = async () => {
    for (;;) {
      const index = next
      next += 1
      if (index >= items.length) return
      results[index] = await worker(items[index], index)
    }
  }
  await Promise.all(Array.from({ length: Math.min(size, items.length) }, runner))
  return results
}
