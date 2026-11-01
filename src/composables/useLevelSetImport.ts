/**
 * 地图 ZIP 的「暂存 → 确认 → 导入」—— 唯一实现。
 *
 * 两个页面共用：地图下载页的「安装地图」来源卡片（本地选择 / 拖入窗口 / 直链），
 * 以及地图管理页的整窗拖放。写两份的代价不是多几十行，而是确认文案、覆盖提示、
 * 暂存清理与「装完重读」的时机各漂各的 —— 用户会在两页看到不同的提示。
 *
 * 后端 `stage_map_source` 先把来源统一成一份可读 ZIP（本地文件原地引用、
 * 直链下载到 cache 并标记 `staged`），识别结果与 `inspect_level_set_zip` 同构，
 * 所以「确认导入」只需要一份实现。
 *
 * 调用方只负责两件事：把来源交进来；导入成功后重读自己的关卡集列表。
 */
import { h, ref } from 'vue'
import { useDialog } from 'naive-ui'
import { t } from '../i18n'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { backendMessageOf } from '../locale/backend'
import type { InstallSource, ZipLevelSet } from '../types'

export function useLevelSetImport(onImported?: () => void | Promise<void>) {
  const message = useNotify()
  const dialog = useDialog()
  /** 正在处理一份来源：暂存 / 识别 / 等确认 / 导入，期间入口按钮转圈并禁用。 */
  const importing = ref(false)

  /**
   * 导入一个来源（本地路径或直链）。
   *
   * `onStaged` 在「暂存并识别成功」之后、确认弹窗之前调用：来源卡片据此收起
   * 自己（弹窗才是接下来的主角），暂存失败时卡片留在原地显示错误。
   * 失败已就地提示，不向调用方抛。
   */
  async function importSource(source: InstallSource, options?: { onStaged?: () => void }) {
    if (importing.value) return
    importing.value = true
    try {
      const staged = await api.stageMapSource(source.kind, source.value)
      options?.onStaged?.()
      await confirmImport(staged.path, staged.sets, staged.staged)
    } catch (reason) {
      message.error(backendMessageOf(reason))
    } finally {
      importing.value = false
    }
  }

  /**
   * 确认并导入一份已识别的 ZIP。
   *
   * `staged` 为真表示 `path` 是后端下载到 cache 的临时文件：不论导入成功、失败
   * 还是用户取消，结束时都要 `discard_map_source` 清掉，否则每装一次地图就在
   * cache 里留一份 ZIP。用户自己的本地文件（`staged` 为假）绝不能删。
   */
  async function confirmImport(path: string, found: ZipLevelSet[], staged: boolean) {
    try {
      if (!found.length) {
        message.warning(t('levelSet.zipWithoutInfo'))
        return
      }
      const selected = found.map((set) => set.dir_name)
      /*
       * 用 Naive 的 dialog 而不是自绘弹窗：项目里所有确认都走它，
       * 视觉与交互（点外部关闭、Esc、焦点）已经统一。
       * 「会覆盖已有版本」时单独说明 —— 旧版本会先进回收站。
       */
      const replacing = found.filter((set) => set.exists)
      const detail = replacing.length
        ? t('levelSet.importConfirmReplace', {
            names: replacing.map((set) => set.name || set.dir_name).join('、'),
          })
        : t('levelSet.importConfirm', { count: found.length })
      /*
       * 弹窗正文用 VNode 而不是拼字符串：每张地图要占一块，随包插件缩进挂在
       * 它下面。字符串里的 `\n` 会被 HTML 折叠成空格，结果是「10 个关卡）随包
       * 插件：…」全挤在一行 —— 哪些插件属于哪张图就看不出来了。
       */
      const approved = await new Promise<boolean>((resolve) => {
        dialog.warning({
          title: t('levelSet.importTitle'),
          content: () =>
            h('div', { class: 'import-confirm' }, [
              h('p', { class: 'import-confirm-lead' }, detail),
              ...found.map((set) =>
                h('div', { class: 'import-confirm-set' }, [
                  h(
                    'div',
                    { class: 'import-confirm-name' },
                    `${set.name || set.dir_name}${set.author ? ` — ${set.author}` : ''}${set.version ? ` v${set.version}` : ''}（${t('levelSet.levelCount', { count: set.level_count })}）`,
                  ),
                  ...(set.plugins?.length
                    ? [
                        h(
                          'div',
                          { class: 'import-confirm-plugins' },
                          t('levelSet.withPlugins', { names: set.plugins.join('、') }),
                        ),
                      ]
                    : []),
                ]),
              ),
            ]),
          positiveText: t('modsOnline.install'),
          negativeText: t('recycle.cancel'),
          onPositiveClick: () => resolve(true),
          onNegativeClick: () => resolve(false),
          onClose: () => resolve(false),
        })
      })
      if (!approved) return
      const result = await api.importLevelSets(path, selected)
      message.success(t('levelSet.imported', { count: result.installed.length }))
      await onImported?.()
    } catch (reason) {
      message.error(backendMessageOf(reason))
    } finally {
      if (staged) await api.discardMapSource(path).catch(() => undefined)
    }
  }

  return { importing, importSource }
}
