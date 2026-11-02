import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * 「安装前检查」模型的构建规则（`src/installCheck.ts`）。
 *
 * 这一层是弹窗唯一的数据来源：MOD 的关系计划与（MOD / 地图共用的）同身份
 * 冲突清单都翻译成同一个 `InstallCheckModel`，弹窗只负责「按顺序执行 + 报结果」。
 * 所以这里钉的不是像素，而是几条一旦漂移就会真的装错东西的规则：
 *
 *   · 顺序：卸载必须排在安装前面（先腾地方，再补版本，最后补缺件）；
 *   · 动作：装过的更高版本是「升级」、更低是「降级」、没装过是「装依赖」；
 *   · key：带身份而不是裸索引 —— 只带索引会让新一批内容撞上上一批残留的
 *     「已完成」，整步被跳过；
 *   · 冲突行：`index` 必须与传入数组一一对应，宿主才能 `conflicts[row.index]`
 *     取回要卸的那一项（弹窗分不出 MOD 还是地图，域由宿主注入的 execute 决定）。
 *
 * ## 为什么注册解析钩子
 *
 * 源码里的 import 不带扩展名（交给 Vite 补全），Node ESM 不做这件事。这里补一个
 * 只在本测试进程生效的 resolve 钩子，就能直接跑**真实源码**，而不是用
 * `readFileSync` 做正则断言。
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

const { conflictCheckModel, installCheckModelFromPlan } = await import('../src/installCheck.ts')

/* ===== 夹具：只给被测代码真正读到的字段 ===== */

const relation = (overrides = {}) => ({
  guid: 'com.example.dep',
  kind: 'requires',
  ...overrides,
})

const installedMod = (overrides = {}) => ({
  name: '旧版依赖',
  dll_file: 'OldDep.dll',
  guid: 'com.example.old',
  ...overrides,
})

const repository = (overrides = {}) => ({
  id: 'dep-repo',
  name: 'Dep Mod',
  name_i18n: { 'zh-CN': '依赖库' },
  ...overrides,
})

const release = (tag) => ({ tag_name: tag })

const plan = (parts = {}) => ({ uninstall: [], install: [], blocked: [], ...parts })

const status = (overrides = {}) => ({
  relation: relation(),
  state: 'ok',
  installedVersion: null,
  ...overrides,
})

const conflict = (overrides = {}) => ({
  name: '已装地图',
  version: '1.0',
  handle: 'map-dir',
  enabled: true,
  same_version: false,
  ...overrides,
})

/* ===== installCheckModelFromPlan ===== */

test('卸载排在全部安装之前', () => {
  const model = installCheckModelFromPlan(plan({
    uninstall: [status({ state: 'conflict', installedMod: installedMod(), installedVersion: '1.0' })],
    install: [
      status({
        state: 'installable',
        relation: relation({ guid: 'com.example.a' }),
        repository: repository({ id: 'repo-a' }),
        release: release('v2.0.0'),
      }),
    ],
  }), '主包')

  assert.deepEqual(model.rows.map((row) => row.kind), ['uninstall', 'install'])
  assert.equal(model.rows[0].actionLabel, '卸载')
  assert.equal(model.rows[0].label, '旧版依赖')
  assert.equal(model.rows[0].version, undefined, '卸载步骤没有目标版本可写')
  assert.equal(model.rows[1].actionLabel, '安装依赖')
  assert.equal(model.rows[1].label, '依赖库', '安装步骤用仓库的本地化名')
  assert.equal(model.rows[1].version, 'v2.0.0')
})

test('装过更高 / 更低版本分别是升级与降级，没装过是装依赖', () => {
  const rows = installCheckModelFromPlan(plan({
    install: [
      status({
        state: 'installable',
        installedVersion: '1.0',
        installedMod: installedMod(),
        repository: repository({ id: 'repo-up' }),
        release: release('1.2'),
      }),
      status({
        state: 'installable',
        installedVersion: '2.0',
        installedMod: installedMod(),
        relation: relation({ guid: 'com.example.b' }),
        repository: repository({ id: 'repo-down' }),
        release: release('1.5'),
      }),
      status({
        state: 'installable',
        installedVersion: null,
        relation: relation({ guid: 'com.example.c' }),
        repository: repository({ id: 'repo-new' }),
        release: release('1.0'),
      }),
    ],
  }), '主包')

  assert.deepEqual(rows.rows.map((row) => row.action), ['upgrade', 'downgrade', 'install-dependency'])
  assert.deepEqual(rows.rows.map((row) => row.actionLabel), ['升级', '降级', '安装依赖'])
})

test('行的 key 带身份，不同批次不会撞', () => {
  const build = (repoId, version) => plan({
    install: [status({
      state: 'installable',
      repository: repository({ id: repoId }),
      release: release(version),
    })],
  })
  const first = installCheckModelFromPlan(build('repo-a', '1.0.0'), '主包')
  const second = installCheckModelFromPlan(build('repo-b', '2.0.0'), '主包')
  const again = installCheckModelFromPlan(build('repo-a', '1.0.0'), '主包')

  assert.equal(first.rows[0].key, again.rows[0].key, '同一份内容每次打开 key 必须一致')
  assert.notEqual(first.rows[0].key, second.rows[0].key, '不同仓库/版本不能共用 key')
  assert.match(first.rows[0].key, /repo-a/)
  assert.match(first.rows[0].key, /1\.0\.0/)
})

test('无法自动处理的关系只提醒，不混进可执行行', () => {
  const model = installCheckModelFromPlan(plan({
    blocked: [status({
      state: 'conflict',
      relation: relation({ guid: 'com.example.blocker', kind: 'conflicts' }),
      installedVersion: '3.1',
      installedMod: installedMod({ name: '挡路的包' }),
    })],
  }), '主包')

  assert.equal(model.rows.length, 0, '提醒不进执行列表')
  assert.equal(model.blocked.length, 1)
  assert.match(model.blocked[0], /挡路的包/)
  assert.match(model.blocked[0], /3\.1/)
  assert.match(model.lead, /主包/)
  assert.equal(model.title, '安装前检查')
})

/* ===== conflictCheckModel ===== */

test('冲突全部翻成「卸载」行，下标与传入数组一一对应', () => {
  const conflicts = [
    conflict({ name: '地图甲', handle: 'map-a', version: '1.2.0', same_version: true }),
    conflict({ name: '', handle: 'map-b', version: '', same_version: false }),
  ]
  const model = conflictCheckModel(conflicts, { targetLabel: '新地图' })

  assert.equal(model.rows.length, 2)
  assert.deepEqual(model.rows.map((row) => row.kind), ['uninstall', 'uninstall'])
  assert.deepEqual(model.rows.map((row) => row.action), ['uninstall', 'uninstall'])
  assert.deepEqual(model.rows.map((row) => row.index), [0, 1], 'index 必须对齐原始数组')
  assert.equal(model.rows[0].actionLabel, '卸载')
  assert.equal(model.rows[0].label, '地图甲')
  assert.equal(model.rows[0].version, '1.2', '版本号与安装详情同一口径（去掉尾部 0）')
  assert.equal(model.rows[0].sameVersion, true)
  assert.equal(model.rows[1].label, 'map-b', '没有名字时退回卸载句柄')
  assert.equal(model.rows[1].version, '—', '没有版本时占位，不留空白')
  assert.equal(model.rows[1].sameVersion, false)
  assert.deepEqual(model.blocked, [], '冲突处理里没有「只能提醒」的条目')
})

test('说明行三种来源：intro 优先，其次目标名，最后通用那句', () => {
  const conflicts = [conflict()]

  const withIntro = conflictCheckModel(conflicts, { targetLabel: '新地图', intro: '自己写的说明' })
  assert.equal(withIntro.lead, '自己写的说明')

  const withTarget = conflictCheckModel(conflicts, { targetLabel: '新地图' })
  assert.match(withTarget.lead, /「新地图」/)

  const bare = conflictCheckModel(conflicts)
  assert.match(bare.lead, /已安装内容/, '没有目标名时用通用的冲突说明')
})

test('冲突行的 key 带卸载句柄，同句柄不会重复', () => {
  const model = conflictCheckModel([
    conflict({ handle: 'map-a' }),
    conflict({ handle: 'map-b', name: '地图乙' }),
  ], {})
  assert.deepEqual(model.rows.map((row) => row.key), ['uninstall-map-a', 'uninstall-map-b'])
})
