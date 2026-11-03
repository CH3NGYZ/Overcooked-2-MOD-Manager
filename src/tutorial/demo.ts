import { ref } from 'vue'
import { i18n } from '../locale/index.ts'
import type { InstalledMod } from '../types'

/**
 * 教程演示数据：只存在于前端内存里，不落盘、不进 Dashboard 快照。
 *
 * ## 为什么需要
 *
 * 教程里有一组步骤是「指着一张本地卡片讲」，最典型的是 MOD 管理页的卡片。
 * 新用户磁盘上什么都没有，列表为空时卡片与页头分段器都不渲染，那些步骤
 * 就只能退化成居中的说明卡 —— 高亮框没东西可框，用户也看不到那张卡长什么样。
 *
 * 所以在教程运行期间、且真实列表为空时，前端补两张示例卡：分组、卡片、
 * 批量分段器都照常渲染，高亮框落在真实几何上。只补 MOD：地图两页的教程
 * 只高亮侧栏，不再需要卡片。
 *
 * ## 边界（改动时不要越线）
 *
 *   · 只在**页面展示层**合并（`ModsLocalView` 的 `mods`），不写进
 *     Dashboard 快照、配置或任何后端命令 —— 教程结束后页面 computed 当场
 *     回到真实数据，没有需要清理的残留状态。
 *   · 真实列表非空时一律不注入：有真卡就不该看见假卡。
 *   · 示例名字都带「示例 / Sample」字样（`tutorial.demo.*`），用户看得出
 *     这不是自己装的东西。
 *   · 教程浮层接管全部点击（点哪儿都是「下一步」），示例卡片不可能被点到，
 *     因此不会触发开关、删除、安装这些写操作。
 */

/** 教程正在运行：只有它为真时，合并函数才会考虑示例数据。 */
export const demoActive = ref(false)

/** 当前这批示例数据。`startTutorialDemo()` 重建，`stopTutorialDemo()` 清空。 */
let demoMods: InstalledMod[] = []

function t(key: string): string {
  return i18n.global.t(key)
}

/**
 * 两张示例 MOD。
 *
 * 故意不设 `plugin_name`：卡片标题的取值顺序是
 * `plugin_name → title → product_name → name`（`ModsLocalView` 的 `modTitle`），
 * 设了 `plugin_name` 就绕过了语言切换，所以本地化名字写在 `title`/`name` 上。
 */
function buildDemoMods(): InstalledMod[] {
  return [
    {
      name: t('tutorial.demo.modA'),
      title: t('tutorial.demo.modA'),
      product_name: 'TutorialDemoModA',
      original_filename: 'TutorialDemoModA.dll',
      guid: 'oc2.tutorial.demo.mod-a',
      company: 'OC2 MOD Manager',
      copyright: t('tutorial.demo.author'),
      dll_file: 'TutorialDemoModA.dll',
      enabled: true,
      version: '1.2.0',
      version_source: 'metadata',
      update_available: false,
      duplicate_guid: false,
      manual_install: true,
      is_dependency: false,
      has_config: true,
      has_logs: false,
      status: 'latest',
    },
    {
      name: t('tutorial.demo.modB'),
      title: t('tutorial.demo.modB'),
      product_name: 'TutorialDemoModB',
      original_filename: 'TutorialDemoModB.dll',
      guid: 'oc2.tutorial.demo.mod-b',
      company: 'OC2 MOD Manager',
      copyright: t('tutorial.demo.author'),
      dll_file: 'TutorialDemoModB.dll',
      enabled: true,
      version: '1.0.4',
      version_source: 'metadata',
      update_available: false,
      duplicate_guid: false,
      manual_install: true,
      is_dependency: false,
      has_config: false,
      has_logs: false,
      status: 'latest',
    },
  ]
}

/**
 * 教程开始时调用（`engine.startTutorial`）。
 *
 * 每次都重建：界面语言可能在两次教程之间被改过，示例名字要跟着走。
 * 已经在演示态时直接返回，重复开始不会叠两批数据。
 */
export function startTutorialDemo(): void {
  if (demoActive.value) return
  demoMods = buildDemoMods()
  demoActive.value = true
}

/** 教程结束时调用（`engine.finishTutorial`），必须在 settled 回调之前，让启动刷新面对真实数据。 */
export function stopTutorialDemo(): void {
  demoActive.value = false
  demoMods = []
}

/**
 * 本地 MOD 列表的展示合并：非空就用真实数据，空的才补示例。
 *
 * 未在演示态、或真实列表非空时返回**原引用**（不复制、不新建数组）：
 * 调用方多半是 computed，同一个引用能让下游的 `watch`/`computed` 原地不动。
 */
export function withDemoMods(real: readonly InstalledMod[]): InstalledMod[] {
  if (!demoActive.value || real.length > 0) return real as InstalledMod[]
  return demoMods
}
