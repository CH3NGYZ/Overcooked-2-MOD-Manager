import assert from 'node:assert/strict'
import test from 'node:test'
import { readFileSync } from 'node:fs'

/**
 * 「一套刷新架构」这条约定的守门测试。
 *
 * ## 这次改动之前是什么样
 *
 * 同一件事在两个页面各有一份实现：MOD 安装页用模块级 `useRepositoryRefresh`，
 * MOD 管理页另存一个页面级 `checkingUpdates`；地图管理页更彻底，直接 `for`
 * 循环调后端命令，和地图下载页的 `useMapSources` 完全没有交汇。表现就是
 * 「在这个页点刷新，那个页看不出在刷」，切走再回来按钮还会假装已经结束。
 *
 * ## 现在锁的是「谁调谁」
 *
 *   · 状态机只有一处：`refreshState.createRefreshState`；
 *   · 「刷一批带 Release 的条目」只有一处：`releaseRefresh.runReleaseBatch`
 *     （MOD 仓库与地图条目共用 —— 它们本质都是 GitHub/Gitee 仓库的 Release）；
 *   · 页面只做域内编排（源文档树、社区发现、结果文案），不自己拼状态或循环。
 *
 * 读源码文本做断言（仓库里 `backend-tag-literals` 也是这个套路）：它锁的不是
 * 实现细节，而是「别再长出第二份状态」。只要有人又在页面里自己存一个
 * `checkingUpdates`、或绕过共享批量实现，这里就会红。
 */
const read = (path) => readFileSync(new URL(`../${path}`, import.meta.url), 'utf8')

const refreshState = read('src/refreshState.ts')
const releaseRefresh = read('src/releaseRefresh.ts')
const repositoryEngine = read('src/composables/useRepositoryRefresh.ts')
const mapEngine = read('src/composables/useMapSources.ts')
const modsLocal = read('src/views/ModsLocalView.vue')
const modsOnline = read('src/views/ModsOnlineView.vue')
const levelsView = read('src/views/LevelsView.vue')
const mapsOnline = read('src/views/MapsOnlineView.vue')
const modStatusTags = read('src/components/ModStatusTags.vue')
const levelSetStatus = read('src/levelSetStatus.ts')
const modRuntimeStatusSource = read('src/modRuntimeStatus.ts')

test('刷新状态机只有一处实现，两个域都用它', () => {
  // 状态机自己：条目级 / 源级 / 批次级 + 并发 + 结果
  for (const symbol of ['refreshing', 'queued', 'refreshingSources', 'refreshingAll', 'entryStateUnavailable', 'checkingBound', 'outcome']) {
    assert.match(refreshState, new RegExp(`\\b${symbol}\\b`), `状态机缺 ${symbol}`)
  }
  assert.match(refreshState, /mapWithConcurrency\(/, '并发上限必须走共享限并发工具')
  for (const [name, source] of [
    ['useRepositoryRefresh', repositoryEngine],
    ['useMapSources', mapEngine],
  ]) {
    assert.match(source, /createRefreshState\(REFRESH_CONCURRENCY\)/, `${name} 必须用共享状态机`)
    assert.match(source, /REFRESH_CONCURRENCY = 4\b/, `${name} 的并行上限必须是 4`)
    // 页面级标志由状态机持有，模块里不许再自己 new 一份
    assert.doesNotMatch(source, /const checkingBound = ref\(/, `${name} 不许自己再造批次态`)
    assert.doesNotMatch(source, /const refreshing = ref<Record/, `${name} 不许自己再造条目标志`)
  }
})

test('「刷一批带 Release 的条目」只有一处实现，两个域共用', () => {
  assert.match(releaseRefresh, /export function runReleaseBatch/)
  assert.match(releaseRefresh, /export async function refreshReleaseItem/)
  assert.match(releaseRefresh, /export interface ReleaseFetchSpec/)
  assert.match(releaseRefresh, /export function prioritizeByIds/)
  // 两个域都得走它，而不是各自 mapWithConcurrency 一遍
  assert.match(repositoryEngine, /runReleaseBatch<Repository>\(/)
  assert.match(mapEngine, /runReleaseBatch<MapInfo>\(/)
  /*
   * 「刷一个条目」的三件事 —— 请求 / 成功写回 / 失败写回 —— 每个域只定义一次
   * （`repositoryReleaseSpec` / `mapReleaseSpec`），单条刷新与批量刷新展开的是
   * 同一份定义。这样落地时机只在 `releaseRefresh` 里实现一次，
   * 域里不会各写一遍 try/catch 再各自决定什么时候写回。
   */
  for (const [name, source, spec] of [
    ['useRepositoryRefresh', repositoryEngine, 'repositoryReleaseSpec'],
    ['useMapSources', mapEngine, 'mapReleaseSpec'],
  ]) {
    assert.match(source, new RegExp(`function ${spec}\\(`), `${name} 要有自己的 spec 定义`)
    assert.match(source, new RegExp(`\\.\\.\\.${spec}\\(`), `${name} 的批量刷新要展开同一份 spec`)
    assert.match(source, /ReleaseFetchSpec/, `${name} 要用共享的 spec 类型`)
  }
})

test('单条结果当场落地：排队标记在轮到时清掉，不等整批结束', () => {
  /*
   * 以前这里写成 `if (refreshing.value[id])` —— 那一刻它必然还没在飞，
   * 条件永远不成立，排队标记要等整批 `finally` 才清。表现就是所有卡片一直
   * 显示「刷新中」，直到最后一个请求回来才一起变成结论：用户看到的是
   * 「结果一批一起回来」，而不是刷一个回来一个。
   */
  const runBatch = refreshState.slice(refreshState.indexOf('const runBatch'))
  assert.match(runBatch, /writeFlag\(queued, batch\.idOf\(item\), false\)/)
  assert.doesNotMatch(runBatch, /if \(refreshing\.value\[batch\.idOf\(item\)\]\)/)
  // 写回必须早于关标志，否则会先闪一帧「不在刷新中、结论还是旧的」
  const itemBody = releaseRefresh.slice(
    releaseRefresh.indexOf('export async function refreshReleaseItem'),
    releaseRefresh.indexOf('export function prioritizeByIds'),
  )
  assert.ok(
    itemBody.indexOf('spec.apply?.') < itemBody.indexOf('state.setItemFlag(item.id, false)'),
    '成功写回必须早于关闭「刷新中」标志',
  )
  assert.ok(
    itemBody.indexOf('spec.fail?.') < itemBody.indexOf('state.setItemFlag(item.id, false)'),
    '失败写回必须早于关闭「刷新中」标志',
  )
})

test('整批标志不参与卡片判定：进度只能按条目粒度显示', () => {
  /*
   * `refreshingAll` 覆盖整批（源树 → 社区发现 → 全部条目），要亮到按钮恢复
   * 可点为止 —— 它是**门控**用的。卡片判定如果吃它，整批期间每张卡都被按住，
   * 逐个返回的结果被压成「整批一起回来」，正是用户报的现象。
   *
   * 卡片只认 `entryStateUnavailable`：它只在条目集合被清空重建的空窗期里为真，
   * 列表填回来就必须关掉。
   */
  assert.match(modRuntimeStatusSource, /state\.entryStateUnavailable/, '卡片要读条目集合兜底')
  assert.doesNotMatch(modRuntimeStatusSource, /refreshingAll/, '卡片判定不许读整批标志')
  assert.match(modStatusTags, /entryStateUnavailable/, '标签组件要拿到条目集合兜底')
  assert.doesNotMatch(modStatusTags, /refreshingAll/, '标签组件不许拿整批标志')

  // 两个域都必须在进入 Release 阶段之前关掉兜底，否则又是整批按住
  assert.match(
    repositoryEngine,
    /setEntryStateUnavailable\(false\)[\s\S]{0,400}?runRepositoryBatch\(available/,
    '仓库侧进入 Release 阶段前要先关掉条目集合兜底',
  )
  assert.match(
    mapEngine,
    /setEntryStateUnavailable\(false\)[\s\S]{0,400}?refreshReleases\(maps\.value/,
    '地图侧进入 Release 阶段前要先关掉条目集合兜底',
  )
})

test('排队中也算「刷新中」：整批一开始所有目标都变灰', () => {
  /*
   * 并发只有 4，20 个仓库里 16 个在排队。只把「正在飞」当刷新中的话，
   * 点一次检查更新只有 4 张卡变灰，用户会以为只查了四个。
   */
  assert.match(refreshState, /queued/, '状态机要有队列态')
  assert.match(refreshState, /writeFlag\(queued/, 'batch 开始时整批入队')
  // 两个域的「要不要显示刷新中」都要把排队算进去
  assert.match(modRuntimeStatusSource, /queued\[repo\.id\]/, 'MOD 侧排队也算刷新中')
  assert.match(read('src/levelSetStatus.ts'), /refreshing\.queued\?\.has\(/, '地图侧排队也算刷新中')
  // 页面把队列状态喂进判定
  assert.match(modsLocal, /queued: queued\.value/)
  assert.match(levelsView, /queued: new Set\(Object\.keys\(queued\.value\)\)/)
})

test('已安装优先下沉到批量实现，所有刷新入口自动生效', () => {
  // MOD 侧：整源 / 分组 / 刷新全部 / 检查更新 都经过 runRepositoryBatch
  assert.match(repositoryEngine, /prioritizeInstalledRepositories\(\[\.\.\.items\], mods\.value\)/)
  assert.match(repositoryEngine, /runRepositoryBatch\(/)
  // 地图侧：已装地图 id 由页面给（引擎不认识关卡集），但排序仍然走共享函数
  assert.match(mapEngine, /prioritizeByIds\(items, priorityIds\)/)
  assert.match(mapEngine, /priorityIds/)
})

test('MOD 管理页只走共享入口，不再自己拼刷新批次', () => {
  assert.match(modsLocal, /refreshBoundRepositories\(/, '检查更新必须调引擎入口')
  assert.doesNotMatch(modsLocal, /const checkingUpdates = ref\(/, '页面级批次态会随切页丢失，必须删掉')
  assert.doesNotMatch(modsLocal, /refreshMany\(/, '页面不许自己拼批量刷新，否则状态不会与安装页共享')
  // 卡片标签与页头聚合共用同一份判定
  assert.match(modsLocal, /runtimeModStatus/, '卡片/聚合的判定要共用 modRuntimeStatus')
})

test('卡片标签由自订阅组件渲染，不依赖折叠插槽重渲染', () => {
  /*
   * naive-ui 的 `CollapseItemContent` 用稳定插槽渲染内容，父级重渲染时
   * `shouldUpdateComponent` 会跳过它 —— 标签如果只吃页面塞进来的 props，
   * 刷新状态永远显示不出来（页头变了、卡片不动）。组件自己订阅刷新状态才行。
   */
  assert.match(modStatusTags, /useRepositoryRefresh\(\)/, '标签组件要自己读刷新状态')
  assert.match(modStatusTags, /runtimeModStatus\(/, '标签内容与页头聚合共用同一份判定')
  assert.match(modsLocal, /<ModStatusTags :dll-file="mod\.dll_file" \/>/, '卡片要改用该组件')
  assert.doesNotMatch(modsLocal, /v-for="tag in modTags\(mod\)"/, '旧的页面内联标签必须删掉')
})

test('地图管理页改用共享单例，不再自己存一份地图数据或循环调后端', () => {
  assert.match(levelsView, /useMapSources\(/, '地图列表与刷新状态必须来自单例')
  assert.match(levelsView, /refreshBoundMaps\(/, '检查更新必须调引擎入口')
  assert.doesNotMatch(levelsView, /api\.refreshMapReleases\(/, '页面不许自己循环刷新 Release')
  assert.doesNotMatch(levelsView, /const maps = ref<MapInfo\[\]>\(\[\]\)/, '两页各存一份地图数据会各刷各的')
  assert.doesNotMatch(levelsView, /const checkingUpdates = ref\(/, '页面级批次态必须删掉')
})

test('两个下载页的忙碌判定都用单例导出的 anyRefreshing', () => {
  for (const [name, source] of [
    ['MapsOnlineView', mapsOnline],
    ['ModsOnlineView', modsOnline],
  ]) {
    assert.doesNotMatch(source, /const anyRefreshing = computed\(/,
      `${name} 自己重算会漏掉 checkingBound，管理页在检查时下载页按钮不禁用`)
    assert.match(source, /anyRefreshing/)
  }
})

test('关卡集卡片能显示「刷新中」，并与版本结论互斥', () => {
  assert.match(levelSetStatus, /LevelSetRefreshingHint/)
  assert.match(levelSetStatus, /refreshing\.maps\.has\(/)
  assert.match(levelSetStatus, /refreshing\.sources\.has\(/)
  // 列表清空重建期间绑定查不到，这时也必须显示「刷新中」而不是「未绑定」
  assert.match(levelSetStatus, /refreshing\.entryStateUnavailable === true/)
  assert.match(levelSetStatus, /'tools\.refreshing'/)
  assert.match(levelsView, /refreshingHint/, '页面要把实时刷新状态传进标签判定')
})

test('刷新结果有统一结构，两个域都能回传摘要', () => {
  for (const symbol of ['trigger', 'refreshed', 'updated', 'errors', 'finishedAt']) {
    assert.match(refreshState, new RegExp(`\\b${symbol}\\b`), `RefreshOutcome 缺 ${symbol}`)
  }
  assert.match(repositoryEngine, /publishOutcome\(\{/, '整批刷新要写一份结果')
  assert.match(mapEngine, /publishOutcome\(\{/,'地图整源刷新要写一份结果')
  // 摘要文案与页头标签共用同一套格式化
  const outcomeText = read('src/refreshOutcomeText.ts')
  assert.match(outcomeText, /export function describeRefreshOutcome/)
  assert.match(read('src/components/RefreshResultTag.vue'), /describeRefreshOutcome\(/)
})
