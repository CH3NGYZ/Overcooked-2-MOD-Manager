import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * 教程示例数据的守门测试。
 *
 * ## 为什么需要它
 *
 * 示例数据有两个域，规则不一样，两边都容易在后续改动中悄悄失效：
 *
 *   - 本地 MOD：只在「教程运行中、且真实列表为空」时补两张卡 —— 合并函数写错
 *     方向（真实非空时仍混进假卡）会让用户看到自己不存在的 MOD；
 *   - 在线列表（来源 / 仓库 / 地图）：教程运行期间**整体替换**。这几步讲的是
 *     固定的「源分组 → 卡片 → 详情」路径，真实配置为空或首启连不上源时根本
 *     没有东西可指。替换必须同时换来源，否则 `repositoryIsAvailable` 会因为
 *     「source_id 找不到所属源」把示例仓库全部滤掉。
 *
 * 共同要钉住的还有：
 *
 *   - `stopTutorialDemo()` 忘了清空 → 教程结束后页面上留着假卡；
 *   - 示例条目的键（`dll_file` / `guid` / 仓库与地图 id）撞车 → 卡片按同一个键
 *     展开、开关与删除互相覆盖，行为诡异；
 *   - 重复开始（重播时先 exit 再 start）叠成两倍卡。
 *
 * 所以这里从真实模块出发逐条钉住。示例名字走 `i18n.global.t`，Node 里没有
 * 语言存储，回落到默认语言（zh-CN），因此只断言「不是 key 原文」，不锁死具体
 * 文案 —— 换名字不该让测试红，名字在两页之间对不上才是问题。
 *
 * ## 为什么注册解析钩子
 *
 * 源码里的 import 不带扩展名（交给 Vite 补全），Node ESM 不做这件事
 * （与 `tests/tutorial-steps.test.mjs` 同一套写法）。
 */
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try {
        return nextResolve(`${specifier}.ts`, context)
      } catch {
        // 不是单文件模块，继续试目录索引
      }
      try {
        return nextResolve(`${specifier}/index.ts`, context)
      } catch {
        // 交给 Node 自己报错，错误里带原始 specifier 更好查
      }
    }
    return nextResolve(specifier, context)
  },
})

const {
  demoActive,
  startTutorialDemo,
  stopTutorialDemo,
  withDemoMods,
  withDemoRepositories,
  withDemoSources,
  withDemoMaps,
  demoReadme,
  isDemoId,
  isDemoOwner,
} = await import('../src/tutorial/demo.ts')

/** 一条真实 MOD 的最小形状；合并函数在任何状态下都不该改动它。 */
function realMod() {
  return {
    name: 'Real Mod',
    dll_file: 'RealMod.dll',
    enabled: true,
    version_source: 'metadata',
    update_available: false,
    duplicate_guid: false,
    status: 'latest',
  }
}

test('未进入教程时合并函数原样返回真实数据', () => {
  const mods = [realMod()]
  assert.equal(withDemoMods(mods), mods)
  assert.equal(withDemoMods([]).length, 0)
})

test('教程运行中且列表为空时补上示例卡片', () => {
  startTutorialDemo()
  try {
    assert.equal(demoActive.value, true)
    const mods = withDemoMods([])
    assert.equal(mods.length, 2)
    // 标题取语言表里的社区 MOD 名（BepInEx 配置管理器 / 街机 MOD），
    // 但必须来自 `t()`、不是 key 原文，且两张卡不能同名
    for (const mod of mods) {
      assert.ok(mod.title && mod.title.length > 0, '示例 MOD 必须有标题')
      assert.ok(!mod.title.startsWith('tutorial.'), `标题不应是 key 原文：${mod.title}`)
      assert.equal(mod.enabled, true)
    }
    assert.equal(new Set(mods.map((mod) => mod.title)).size, mods.length, '两张示例 MOD 不能同名')
  } finally {
    stopTutorialDemo()
  }
})

test('教程运行中但真实列表非空时不注入示例', () => {
  startTutorialDemo()
  try {
    const mods = [realMod()]
    assert.equal(withDemoMods(mods), mods)
    assert.equal(withDemoMods(mods).length, 1)
  } finally {
    stopTutorialDemo()
  }
})

test('教程结束立刻回到真实数据，示例被清空', () => {
  startTutorialDemo()
  assert.equal(withDemoMods([]).length, 2)
  stopTutorialDemo()
  assert.equal(demoActive.value, false)
  assert.equal(withDemoMods([]).length, 0)
})

test('重复开始不会叠加示例卡片', () => {
  startTutorialDemo()
  startTutorialDemo()
  try {
    assert.equal(withDemoMods([]).length, 2)
  } finally {
    stopTutorialDemo()
  }
})

test('示例数据的唯一键互不冲突', () => {
  startTutorialDemo()
  try {
    const mods = withDemoMods([])
    // 卡片展开键、开关与删除都按 dll_file 定位，重名会让两张卡互相覆盖
    assert.equal(new Set(mods.map((mod) => mod.dll_file)).size, mods.length)
    assert.equal(new Set(mods.map((mod) => mod.guid)).size, mods.length)
  } finally {
    stopTutorialDemo()
  }
})

/** 一条真实来源 / 仓库 / 地图的最小形状；只用于「整体替换 + 结束后回原引用」。 */
function realSource() {
  return { id: 'real.source', nickname: 'Real Source', url: 'https://example.com/repositories.toml', builtin: false }
}

function realRepository() {
  return { id: 'real.repo', name: 'Real Repo', enabled: true, cached_releases: [] }
}

function realMap() {
  return { id: 'real.map', name: 'Real Map', enabled: true, cached_releases: [] }
}

test('未进入教程时在线列表的合并函数原样返回真实数据', () => {
  const sources = [realSource()]
  const repositories = [realRepository()]
  const maps = [realMap()]
  assert.equal(withDemoSources(sources), sources)
  assert.equal(withDemoRepositories(repositories), repositories)
  assert.equal(withDemoMaps(maps), maps)
  assert.equal(withDemoSources([]).length, 0)
  assert.equal(withDemoRepositories([]).length, 0)
  assert.equal(withDemoMaps([]).length, 0)
})

test('教程期间在线来源 / 仓库 / 地图整体替换真实数据', () => {
  startTutorialDemo()
  const sources = [realSource()]
  const repositories = [realRepository()]
  const maps = [realMap()]
  try {
    const demoSources = withDemoSources(sources)
    const demoRepositories = withDemoRepositories(repositories)
    const demoMaps = withDemoMaps(maps)

    // 整体替换：真实条目一条都不能混进来，否则用户会看到自己配置里没有的卡
    assert.ok(demoSources.length > 0)
    assert.ok(demoRepositories.length > 0)
    assert.ok(demoMaps.length > 0)
    assert.ok(!demoSources.includes(sources[0]))
    assert.ok(!demoRepositories.includes(repositories[0]))
    assert.ok(!demoMaps.includes(maps[0]))

    // 卡片能不能显示，取决于「所属源在不在同一批来源里」
    // （`repositoryIsAvailable` 查不到 source_id 就把仓库判成不可用）
    const sourceIds = new Set(demoSources.map((source) => source.id))
    for (const repo of demoRepositories) {
      assert.ok(sourceIds.has(repo.source_id), `示例仓库的 source_id 没有对应来源：${repo.source_id}`)
    }

    // 卡片上的版本号与下载量读 cached_releases[0]，空数组会渲染成空壳
    for (const repo of demoRepositories) {
      assert.ok(repo.cached_releases.length > 0, '示例仓库必须有 Release')
      assert.ok(repo.cached_releases[0].assets.length > 0, '示例 Release 必须有资产')
    }
    for (const map of demoMaps) {
      assert.ok(map.cached_releases.length > 0, '示例地图必须有 Release')
      assert.ok(map.level_set_name && map.author, '示例地图必须有关卡集身份')
    }

    // 在线仓库与本地示例 MOD 讲的是同一批名字：教程从「安装页」跳到「管理页」，
    // 用户看到的应该还是同两张卡，不能这边叫 A、那边叫 B
    const localTitles = withDemoMods([])
      .map((mod) => mod.title)
      .sort()
    assert.deepEqual(demoRepositories.map((repo) => repo.name).sort(), localTitles)
    assert.ok(!/^tutorial\./.test(demoRepositories[0].name), '仓库名不该是 key 原文')

    // 示例地图仍带「示例」字样：那一步讲的是「内容包长什么样」，不是某个具体社区地图
    for (const map of demoMaps) {
      assert.match(map.name, /示例|Sample|サンプル|예시|範例/)
    }
  } finally {
    stopTutorialDemo()
  }
})

test('教程结束后在线列表立刻回到真实数据的原引用', () => {
  startTutorialDemo()
  const sources = [realSource()]
  const repositories = [realRepository()]
  const maps = [realMap()]
  assert.notEqual(withDemoSources(sources), sources)
  stopTutorialDemo()
  assert.equal(demoActive.value, false)
  // 必须是同一个引用：下游是 computed，换成新数组会白跑一轮分组重建
  assert.equal(withDemoSources(sources), sources)
  assert.equal(withDemoRepositories(repositories), repositories)
  assert.equal(withDemoMaps(maps), maps)
})

test('示例来源 / 仓库 / 地图的 id 都带示例前缀且互不重复', () => {
  startTutorialDemo()
  try {
    const ids = [
      ...withDemoSources([]).map((source) => source.id),
      ...withDemoRepositories([]).map((repo) => repo.id),
      ...withDemoMaps([]).map((map) => map.id),
    ]
    for (const id of ids) assert.ok(isDemoId(id), `示例条目的 id 必须带示例前缀：${id}`)
    assert.equal(new Set(ids).size, ids.length)
  } finally {
    stopTutorialDemo()
  }
})

test('示例仓库的 README 不请求后端，真实 id 与非演示态都返回 null', () => {
  // 非演示态：即便 id 长得像示例也拿不到内容
  assert.equal(demoReadme('oc2.tutorial.demo.repo-a'), null)
  startTutorialDemo()
  try {
    const [repo] = withDemoRepositories([])
    const readme = demoReadme(repo.id)
    assert.ok(readme, '演示态下示例仓库必须有内置说明')
    assert.equal(readme.exists, true)
    assert.ok(readme.content.length > 0)
    // 真实仓库照旧走后端（这里只验证「不被误判成示例」）
    assert.equal(demoReadme('real.repo'), null)
  } finally {
    stopTutorialDemo()
  }
  // 教程结束立刻失效：抽屉不能继续显示示例说明
  assert.equal(demoReadme('oc2.tutorial.demo.repo-a'), null)
})

test('示例 owner 被头像预取跳过，真实 owner 不受影响', () => {
  startTutorialDemo()
  try {
    const [repo] = withDemoRepositories([])
    assert.ok(isDemoOwner(repo.username))
    assert.equal(isDemoOwner('octocat'), false)
    assert.equal(isDemoOwner(undefined), false)
    assert.equal(isDemoOwner(''), false)
  } finally {
    stopTutorialDemo()
  }
})
