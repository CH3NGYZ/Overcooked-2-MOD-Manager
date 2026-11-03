import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * 教程示例数据的守门测试。
 *
 * ## 为什么需要它
 *
 * 示例数据只在「教程运行中、且真实列表为空」时出现在 MOD 管理页上，
 * 这是个很容易在后续改动中悄悄失效的开关：
 *
 *   - 合并函数写错方向（真实非空时仍混进假卡）→ 用户看到自己不存在的 MOD；
 *   - `stopTutorialDemo()` 忘了清空 → 教程结束后页面上留着两张假卡；
 *   - 两条示例的 `dll_file` / `guid` 撞车 → 卡片按同一个键展开、开关与删除
 *     互相覆盖，行为诡异；
 *   - 重复开始（重播时先 exit 再 start）叠成四张卡。
 *
 * 所以这里从真实模块出发逐条钉住。示例名字走 `i18n.global.t`，Node 里没有
 * 语言存储，回落到默认语言（zh-CN），只断言「不是 key 原文、且带示例字样」。
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
    // 名字必须看得懂是示例，不能是语言表 key 原文
    for (const mod of mods) {
      assert.ok(mod.title && mod.title.length > 0, '示例 MOD 必须有标题')
      assert.ok(!mod.title.startsWith('tutorial.'), `标题不应是 key 原文：${mod.title}`)
      assert.match(mod.title, /示例|Sample|サンプル|예시|範例/)
      assert.equal(mod.enabled, true)
    }
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
