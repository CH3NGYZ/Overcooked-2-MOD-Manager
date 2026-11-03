import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * 教程步骤表的守门测试。
 *
 * ## 为什么需要它
 *
 * 教程的步骤散在 7 个 `steps.*.ts` 里，由不同人同时写。最典型的坏法不是
 * 类型错误，而是**运行时静默失真**：
 *
 *   - 两个人用了同一个 `id`（引擎按数组下标跑，不会报错，但「从某步开始」
 *     与排查日志会指向错的那一步）；
 *   - `titleKey` / `bodyKey` 拼错或忘了写进语言表（`t()` 未知 key 原样返回，
 *     界面上直接显示 `tutorial.mods.foo.title`）；
 *   - 七种语言的某一种漏了（`Localized` 类型只在编译期管字段存在，
 *     空字符串能过）。
 *
 * 所以这里从**真实步骤表**出发，逐条把这三个问题钉死。
 *
 * ## 为什么注册解析钩子
 *
 * 源码里的 import 不带扩展名（交给 Vite 补全），Node ESM 不做这件事
 * （与 `tests/install-detail.test.mjs` 同一套写法）。
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

const { tutorialSteps } = await import('../src/tutorial/steps.ts')

/** 允许出现在 `route` 里的页面（`NAV_ORDER` + 侧栏底部两项）。 */
const ROUTES = new Set(['overview', 'mods-online', 'mods-local', 'maps-online', 'levels', 'rename', 'tools', 'settings'])

/** 契约第 5 节的完整 id 列表与顺序；教程内容改了要先改这份契约。 */
const EXPECTED_IDS = [
  'intro',
  'sidebar',
  'titlebar-game',
  'titlebar-zoom',
  'titlebar-language',
  'titlebar-theme-mode',
  'titlebar-accent',
  'overview-column-mods',
  'overview-card-bepinex',
  'overview-card-online-mods',
  'overview-card-local-mods',
  'overview-column-maps',
  'overview-card-diy-level',
  'overview-card-online-maps',
  'overview-card-local-maps',
  'sidebar-mods-online',
  'mods-online-search',
  'mods-online-card',
  'mods-online-detail',
  'mods-detail-tabs',
  'mods-detail-version',
  'mods-detail-install',
  'sidebar-mods-local',
  'mods-local-tags',
  'mods-local-bulk',
  'mods-local-actions',
  'mods-local-open-dir',
  'mods-local-card',
  'sidebar-maps-online',
  'sidebar-levels',
  'tools-sponsor',
  'tools-logs',
  'tools-recycle',
  'tools-cache',
  'tools-advanced',
  'settings-game-runtime',
  'settings-game-path',
  'settings-bepinex',
  'settings-diylevel',
  'settings-switches',
  'settings-performance',
  'settings-update',
]

/** 全部教程语言表（都登记在 `tutorial` 域下，key 是「域内相对 key」）。 */
async function loadTutorialTables() {
  const modules = [
    '../src/locale/tutorialChrome.ts',
    '../src/locale/tutorialOverview.ts',
    '../src/locale/tutorialMods.ts',
    '../src/locale/tutorialTools.ts',
    '../src/locale/tutorialSettings.ts',
  ]
  const tables = {}
  const missing = []
  for (const path of modules) {
    try {
      const loaded = await import(path)
      const table = Object.values(loaded)[0]
      if (table && typeof table === 'object') Object.assign(tables, table)
    } catch {
      missing.push(path.replace('../src/locale/', ''))
    }
  }
  return { tables, missing }
}

test('步骤 id 唯一、顺序与契约一致', () => {
  const ids = tutorialSteps.map((step) => step.id)
  assert.equal(new Set(ids).size, ids.length, `id 重复：${ids.filter((id, i) => ids.indexOf(id) !== i).join(', ')}`)
  assert.deepEqual(ids, EXPECTED_IDS, '步骤增删或调序后要同步契约第 5 节与本测试')
})

test('每一步都有 id / 标题 key / 正文 key，且是教程域下的静态 key', () => {
  for (const step of tutorialSteps) {
    assert.match(step.id, /^[a-z0-9]+(-[a-z0-9]+)*$/, `id 不是 kebab-case：${step.id}`)
    assert.ok(step.titleKey, `${step.id} 缺 titleKey`)
    assert.ok(step.bodyKey, `${step.id} 缺 bodyKey`)
    assert.notEqual(step.titleKey, step.bodyKey, `${step.id} 的标题与正文用了同一个 key`)
    for (const key of [step.titleKey, step.bodyKey]) {
      assert.ok(key.startsWith('tutorial.'), `${step.id} 的 key 不在 tutorial 域：${key}`)
      assert.doesNotMatch(key, /\s/, `${step.id} 的 key 含空白：${key}`)
    }
  }
})

test('route 只能是真实存在的页面', () => {
  for (const step of tutorialSteps) {
    if (!step.route) continue
    assert.ok(ROUTES.has(step.route), `${step.id} 指向不存在的路由：${step.route}`)
  }
})

test('目标要么是选择器字符串，要么是函数（不允许空值）', () => {
  for (const step of tutorialSteps) {
    if (step.target === undefined) continue
    const kind = typeof step.target
    assert.ok(kind === 'string' || kind === 'function', `${step.id} 的 target 类型不对：${kind}`)
    if (kind === 'string') assert.ok(step.target.length > 0, `${step.id} 的 target 是空字符串`)
  }
})

test('每一步的标题与正文在语言表里五语齐全', async () => {
  const { tables, missing } = await loadTutorialTables()
  assert.deepEqual(missing, [], `语言表文件缺失：${missing.join(', ')}`)
  const problems = []
  for (const step of tutorialSteps) {
    for (const fullKey of [step.titleKey, step.bodyKey]) {
      const relative = fullKey.slice('tutorial.'.length)
      const entry = tables[relative]
      if (!entry) {
        problems.push(`${step.id}: 语言表缺少 ${fullKey}`)
        continue
      }
      for (const locale of ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']) {
        const text = entry[locale]
        if (typeof text !== 'string' || !text.trim()) problems.push(`${step.id}: ${fullKey} 的 ${locale} 为空`)
      }
    }
  }
  assert.deepEqual(problems, [], `\n${problems.slice(0, 20).join('\n')}`)
})

test('文案长度放得进默认卡片（380px）', async () => {
  const { tables } = await loadTutorialTables()
  const problems = []
  for (const step of tutorialSteps) {
    const title = tables[step.titleKey.slice('tutorial.'.length)]?.['zh-CN'] ?? ''
    const body = tables[step.bodyKey.slice('tutorial.'.length)]?.['zh-CN'] ?? ''
    if (title.length > 20) problems.push(`${step.id}: 标题 ${title.length} 字，超过 20`)
    if (body.length > 110) problems.push(`${step.id}: 正文 ${body.length} 字，超过 110`)
  }
  assert.deepEqual(problems, [], `\n${problems.join('\n')}`)
})
