import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

/**
 * 头像预取的「请求谁」规则。
 *
 * 真实缺口：地图安装页的卡片早就传了 `provider` / `owner`，但页面从没调用过
 * 预取，于是地图卡片永远只有平台图标；而补调用时最容易被顺手复制一份去重
 * 逻辑 —— 两页各写一套后，「同一个 owner 只请求一次」就会在某一页失效。
 *
 * 所以这里锁两件事：
 *   1. 纯逻辑（键归一化、批内去重、跳过已知）用真实代码跑；
 *   2. 两个安装页都调用**同一个** `prefetchAvatars`，且它自己不再实现去重。
 *
 * `src/avatars.ts` 依赖 Vue 的 `ref` 与 Tauri IPC，Node 里 import 不进来，
 * 所以第二条只能读源码断言（与 `release-cache-wiring`、`clear-avatar-cache`
 * 同一做法）。纯逻辑因此拆在 `src/avatarTargets.ts`，可以真跑。
 */
const { avatarKey, pendingAvatarTargets } = await import('../src/avatarTargets.ts')

const AVATARS = readFileSync(new URL('../src/avatars.ts', import.meta.url), 'utf8').replace(/\r\n/g, '\n')
const MODS_ONLINE = readFileSync(
  new URL('../src/views/ModsOnlineView.vue', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')
const MAPS_ONLINE = readFileSync(
  new URL('../src/views/MapsOnlineView.vue', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')
const PROVIDER_ICON = readFileSync(
  new URL('../src/components/ProviderIcon.vue', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

test('键归一化：provider 进键，owner 忽略大小写与首尾空白', () => {
  assert.equal(avatarKey('github', 'BepInEx'), 'github:bepinex')
  assert.equal(avatarKey('github', '  BepInEx '), 'github:bepinex')
  // 同名 owner 在两个平台是两个人，不能合成一个请求
  assert.notEqual(avatarKey('github', 'foo'), avatarKey('gitee', 'foo'))
})

test('同一个 owner 只出一个目标，保留第一次出现的原文与顺序', () => {
  const targets = pendingAvatarTargets(
    [
      { provider: 'github', username: 'BepInEx' },
      { provider: 'gitee', username: 'azhe' },
      { provider: 'github', username: 'bepinex' },
      { provider: 'github', username: ' bepinex ' },
    ],
    () => false,
  )
  assert.deepEqual(
    targets.map((target) => target.owner),
    ['BepInEx', 'azhe'],
    '重复 owner 必须只留一个，且用第一次出现的原文',
  )
  assert.deepEqual(targets.map((target) => target.key), ['github:bepinex', 'gitee:azhe'])
})

test('没有 username 的条目跳过（社区搜索结果常见）', () => {
  const targets = pendingAvatarTargets(
    [
      { provider: 'github' },
      { provider: 'github', username: '' },
      { provider: 'github', username: '   ' },
      { provider: 'github', username: 'ok' },
    ],
    () => false,
  )
  assert.deepEqual(targets.map((target) => target.owner), ['ok'])
})

test('已缓存或已请求过的 key 不再出现在待请求列表里', () => {
  const known = new Set(['github:bepinex'])
  const targets = pendingAvatarTargets(
    [
      { provider: 'github', username: 'BepInEx' },
      { provider: 'github', username: 'fresh' },
    ],
    (key) => known.has(key),
  )
  assert.deepEqual(targets.map((target) => target.owner), ['fresh'])
})

test('返回的目标只有 key / provider / owner 三个字段', () => {
  const [target] = pendingAvatarTargets([{ provider: 'gitee', username: 'Azhe' }], () => false)
  assert.deepEqual(target, { key: 'gitee:azhe', provider: 'gitee', owner: 'Azhe' })
})

test('去重规则只有一份：avatars.ts 走共享纯函数，不自己再实现一遍', () => {
  assert.match(
    AVATARS,
    /pendingAvatarTargets\(/,
    '预取必须调用 avatarTargets 的 pendingAvatarTargets',
  )
  assert.ok(
    !/new Map<string/.test(AVATARS),
    'avatars.ts 不应保留自己的去重表实现（去重只在 avatarTargets.ts）',
  )
})

test('地图安装页也调用同一个预取入口', () => {
  assert.match(MAPS_ONLINE, /import \{ prefetchAvatars \} from '\.\.\/avatars'/, '地图安装页应引入预取')
  assert.match(MAPS_ONLINE, /prefetchAvatars\(maps\.value\)/, '进页面时先预取一次已有列表')
  assert.match(
    MAPS_ONLINE,
    /watch\(maps, \(list\) => \{ void prefetchAvatars\(list\) \}/,
    '列表加载/刷新完成后要再预取一次（immediate + watch 同一口径）',
  )
})

test('MOD 安装页与地图安装页用同一份预取，页面里不出现第二套请求', () => {
  assert.match(MODS_ONLINE, /prefetchAvatars\(repositories\.value\)/)
  for (const [page, source] of [['MOD 安装页', MODS_ONLINE], ['地图安装页', MAPS_ONLINE]]) {
    assert.ok(
      !/repositoryAvatars\(/.test(source),
      `${page}不应直接调 api.repositoryAvatars（请求只走 avatars.ts）`,
    )
  }
})

test('头像链路不加 cache-busting（12 小时 TTL 由后端负责）', () => {
  for (const [name, source] of [['avatars.ts', AVATARS], ['ProviderIcon.vue', PROVIDER_ICON]]) {
    assert.ok(
      !/\?t=|\?v=|cacheBust|\bDate\.now\(\)/.test(source),
      `${name} 不能给头像地址拼时间戳等参数，否则每次渲染都会绕过缓存`,
    )
  }
})
