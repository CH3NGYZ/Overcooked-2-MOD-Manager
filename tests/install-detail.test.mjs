import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * 「安装详情」模型的构建规则（`src/installDetail.ts`）。
 *
 * 这一层是「把一个包说清楚」的唯一实现：MOD 安装预览与地图 ZIP 导入都翻译成
 * 同一个 `InstallDetailModel`，两个弹窗只负责渲染。所以这里测的不是像素，而是
 * **同一份信息在两种安装里必须长得一样** —— 卡片标题怎么兜底、右侧那列写版本
 * 还是写「依赖」、随包插件算不算条目、空包能不能点「立即安装」、日志什么时候
 * 出现。这些规则散的散在两个组件里时，用户会在两处看到两套说法。
 *
 * ## 为什么注册解析钩子
 *
 * 源码里的 import 不带扩展名（交给 Vite 补全），Node ESM 不做这件事。这里补一个
 * 只在本测试进程生效的 resolve 钩子，就能直接跑**真实源码**，而不是用
 * `readFileSync` 做正则断言 —— 后者在仓库里已经因为文件搬家误报过两次。
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
  levelSetImportDetail, modPreviewDetail, fileNameOf, dllDisplayName,
} = await import('../src/installDetail.ts')

/** 一个地图集（字段与后端 `ZipLevelSet` 一致）。 */
function levelSet(overrides = {}) {
  return {
    dir_name: 'shz',
    name: 'SHZ',
    name_zh: '音乐厨房',
    author: '菊花梨',
    version: '0.89',
    level_count: 10,
    file_count: 12,
    exists: false,
    plugins: [],
    ...overrides,
  }
}

/** 一个随包插件（字段与后端 `ZipLevelSetPlugin` 一致）。 */
function plugin(overrides = {}) {
  return {
    path: 'OC2DIYLevel/OC2DIYLevel.dll',
    file_name: 'OC2DIYLevel.dll',
    role: 'mod',
    name: 'Overcooked2 DIYLevel Plugin',
    guid: 'dev.gua.overcooked.diylevel',
    version: '0.10',
    title: 'OC2DIYLevel',
    product_name: 'OC2DIYLevel',
    company: null,
    original_filename: 'OC2DIYLevel.dll',
    ...overrides,
  }
}

/** 一份 MOD 安装预览（字段与后端 `ModPreview` 一致）。 */
function modPreview(overrides = {}) {
  return {
    preview_id: 'preview-1',
    kind: 'package',
    package_name: '多地图包',
    file_name: 'bundle.zip',
    total_dlls: 1,
    mod_count: 1,
    dependency_count: 0,
    entries: [],
    origin: 'manual',
    release_tag: '',
    release_body: '',
    ...overrides,
  }
}

const LABELS_OF = (entry) => entry.fields.map((field) => field.label)

test('地图包：一个地图集是一张卡，随包插件挂在它下面', () => {
  const model = levelSetImportDetail('D:\\下载\\SHZ v0.4.zip', [
    levelSet({ exists: true, plugins: [plugin()] }),
  ])

  assert.equal(model.title, '安装详情')
  // 头部：ZIP 文件名 + 类型 + 计数（插件也算 DLL）
  assert.equal(model.head.name, 'SHZ v0.4.zip')
  assert.equal(model.head.kind, '地图包')
  assert.equal(model.head.summary, '1 个 DLL · 1 个地图集')
  assert.equal(model.confirmable, true)
  assert.equal(model.releaseLog, null, '地图 ZIP 没有 Release 日志')

  const [card] = model.entries
  // 标题用中文名（作者与玩家平时叫的那个），右侧那列是版本
  assert.equal(card.title, '音乐厨房')
  assert.equal(card.subtitle, '菊花梨 · 10 个关卡')
  assert.equal(card.meta, '0.89')
  assert.equal(card.note, '已有同名地图集：安装会先把旧版本移入回收站')
  // 两列成对：中文名/英文名、作者/版本、关卡数/文件数，安装目录单独占整行
  assert.deepEqual(LABELS_OF(card), ['中文名', '英文名', '作者', '版本', '关卡数', '文件数', '安装目录'])
  assert.deepEqual(card.fields.map((field) => field.value), ['音乐厨房', 'SHZ', '菊花梨', '0.89', '10', '12', 'shz'])
  assert.equal(card.fields[6].wide, true, '目录名占整行')
  assert.equal(card.fields[6].mono, true, '目录名用等宽字体')

  // 插件是独立条目，九项元数据与 MOD 预览一致
  assert.equal(card.children.length, 1)
  const [child] = card.children
  assert.equal(child.title, 'Overcooked2 DIYLevel Plugin')
  assert.equal(child.subtitle, 'OC2DIYLevel/OC2DIYLevel.dll')
  assert.equal(child.meta, '0.10')
  assert.equal(child.tag, '随包插件', '右侧标签说明它不是独立 MOD，而是这张图自带的')
  assert.deepEqual(LABELS_OF(child), ['名称', '版本', '标题', '产品名', '作者', '依赖', 'GUID', '原始文件名', '文件'])
  assert.equal(child.fields[5].value, '否', '读得出 BepInPlugin 的 DLL 不是依赖库')
  assert.equal(child.fields[6].value, 'dev.gua.overcooked.diylevel')
  assert.equal(child.fields[8].value, 'OC2DIYLevel/OC2DIYLevel.dll', '文件一栏写 ZIP 内路径')
})

test('地图包：标题按「中文名 → 英文名 → 目录名」兜底，没有插件就没有下级条目', () => {
  const model = levelSetImportDetail('/tmp/maps.zip', [
    levelSet({ dir_name: 'a', name_zh: null, name: 'Packed' }),
    levelSet({ dir_name: 'b', name_zh: '  ', name: '' }),
  ])

  assert.equal(model.head.summary, '0 个 DLL · 2 个地图集')
  assert.deepEqual(model.entries.map((entry) => entry.title), ['Packed', 'b'])
  assert.deepEqual(model.entries[0].children, [], '没有插件时 children 是空数组，模板不用判空')
  assert.equal(model.entries[1].subtitle, '菊花梨 · 10 个关卡')
  assert.equal(model.entries[0].note, undefined, '不覆盖时不该出现提示')
})

test('地图包：读不出元数据的插件照样打「随包插件」标签，并退回文件名当标题', () => {
  const model = levelSetImportDetail('/tmp/maps.zip', [
    levelSet({
      plugins: [plugin({
        path: 'plugins/lib/UnityEngine.dll',
        file_name: 'UnityEngine.dll',
        role: 'dependency',
        name: null,
        guid: null,
        version: null,
        title: null,
        product_name: null,
      })],
    }),
  ])

  const [child] = model.entries[0].children
  assert.equal(child.title, 'UnityEngine', '没有插件名就用去掉 .dll 的文件名')
  assert.equal(child.tag, '随包插件', '角色是 dependency 也照样是「随包插件」，归属比角色更重要')
  assert.equal(child.meta, '—', '标签占掉「依赖」之后，版本号一栏老实写破折号')
  assert.equal(child.fields[5].value, '是')
  assert.equal(child.fields[1].value, '—', '没有版本号写破折号')
})

test('MOD 预览：类型标签、计数摘要与「没有 DLL 就只能取消」', () => {
  const empty = modPreviewDetail(modPreview({ total_dlls: 0, mod_count: 0 }))
  assert.equal(empty.confirmable, false, '零 DLL 时安装按钮必须禁用')
  assert.deepEqual(empty.entries, [])
  assert.deepEqual(empty.emptyHint, [
    '这个压缩包里没有 DLL 文件',
    '没有可安装的内容，无法继续。如果这是地图包，请到地图安装页导入。',
  ])
  assert.equal(empty.head.kind, 'MOD 包')
  assert.equal(empty.head.summary, '0 个 DLL · 0 个 MOD · 0 个依赖')

  const single = modPreviewDetail(modPreview({
    kind: 'single',
    total_dlls: 2,
    mod_count: 1,
    dependency_count: 1,
    entries: [plugin(), plugin({ path: 'lib/BepInEx.dll', role: 'dependency', guid: null, version: null })],
  }))
  assert.equal(single.head.kind, '单个 MOD')
  assert.equal(single.confirmable, true)
  assert.equal(single.entries[1].tag, '依赖', '依赖项靠标签说明身份，版本号一栏不再被占用')
  assert.equal(single.entries[1].meta, '—')
  assert.equal(single.entries[0].tag, undefined, '普通 MOD 不打标签')
  assert.equal(single.emptyHint.length, 2, '空态文案常驻模型里，有内容时不渲染')
})

test('Release 日志只在「仓库来源 + 有版本 + 有正文」时出现', () => {
  const withLog = modPreviewDetail(modPreview({
    origin: 'repository', release_tag: 'v1.2.0', release_body: '## 改动\n- 修好了',
  }))
  assert.equal(withLog.releaseLog.tag, 'v1.2.0')
  assert.match(withLog.releaseLog.html, /<h2>改动<\/h2>/, '日志正文要走 Markdown 渲染')

  // 本地文件 / 直链：没有版本概念
  assert.equal(modPreviewDetail(modPreview()).releaseLog, null)
  // RAW 仓库：有来源没有 Release
  assert.equal(modPreviewDetail(modPreview({ origin: 'repository' })).releaseLog, null)
  // 有版本但这次没写日志：不显示空方框
  assert.equal(modPreviewDetail(modPreview({
    origin: 'repository', release_tag: 'v1.0.0', release_body: '   ',
  })).releaseLog, null)
})

test('两种安装共用同一套标题与底栏文案', () => {
  const mod = modPreviewDetail(modPreview())
  const map = levelSetImportDetail('/tmp/a.zip', [levelSet()])
  assert.equal(mod.title, map.title)
  assert.equal(mod.confirmLabel, map.confirmLabel)
  assert.equal(mod.cancelLabel, map.cancelLabel)
  assert.equal(mod.confirmLabel, '立即安装')
  assert.equal(mod.cancelLabel, '取消')
})

test('路径归一：Windows 反斜杠与 POSIX 斜杠都取到文件名', () => {
  assert.equal(fileNameOf('D:\\下载\\map v1.2.zip'), 'map v1.2.zip')
  assert.equal(fileNameOf('/Users/a/Downloads/map.ZIP'), 'map.ZIP')
  assert.equal(fileNameOf('OC2DIYLevel/Own.Plugin.dll'), 'Own.Plugin.dll')
  assert.equal(fileNameOf(''), '')
  assert.equal(dllDisplayName('plugins/Author.Thing.DLL'), 'Author.Thing', '只去掉 .dll 后缀，不动名字里的点')
})
