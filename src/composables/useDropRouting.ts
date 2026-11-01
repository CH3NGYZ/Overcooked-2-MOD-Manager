/**
 * 拖放的跨域路由：这个包到底属于 MOD 域还是地图域。
 *
 * 两个域用**同一份**判定（后端 `classify_drop_source`）：含 `info` 就是地图包，
 * 即使它还带着作者随包的修复插件。判定放在拖入那一刻，而不是等装完再发现 ——
 * 把地图包拖到 MOD 页时，`analyze_mod_source` 会给出「0 个 DLL · 0 个 MOD」的
 * 空预览，而且「立即安装」还真能点、还真会喷彩花，用户以为装上了。
 *
 * 判错域时不静默失败：弹窗说清楚这是什么包，并给一条「去安装」的跳转。跳转
 * 前把路径存进模块级 `pendingDrop`，目标页挂载后自己取走接着走安装流程 ——
 * 否则用户跳到正确页面还得重新拖一次。
 */
import { ref } from 'vue'
import { useDialog } from 'naive-ui'
import { useRouter } from 'vue-router'
import { t } from '../i18n'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { backendMessageOf } from '../locale/backend'

/** 拖放归属的两个域。 */
export type DropDomain = 'mod' | 'map'

/**
 * 跳转到另一个域之前暂存的待装路径。
 *
 * 模块级而不是路由参数：路径里有反斜杠、中文与空格，塞进 URL 要编码再解，
 * 而这里只有「跳一次、取一次」这一种用途，取走即清空，不会残留。
 */
const pendingDrop = ref<{ path: string; domain: DropDomain } | null>(null)

/** 目标页取走属于自己域的待装路径；没有就返回 null。 */
export function takePendingDrop(domain: DropDomain): string | null {
  const pending = pendingDrop.value
  if (!pending || pending.domain !== domain) return null
  pendingDrop.value = null
  return pending.path
}

export function useDropRouting() {
  const dialog = useDialog()
  const router = useRouter()
  const message = useNotify()

  /** 询问是否去另一个域的安装页；返回 true 表示用户点了「去安装」。 */
  function offerOtherDomain(target: DropDomain) {
    return new Promise<boolean>((resolve) => {
      dialog.warning({
        title: target === 'map' ? t('tools.mapPackageDetected') : t('tools.modPackageDetected'),
        content:
          target === 'map' ? t('tools.mapPackageDetectedHint') : t('tools.modPackageDetectedHint'),
        positiveText: t('tools.goToInstall'),
        negativeText: t('recycle.cancel'),
        onPositiveClick: () => resolve(true),
        onNegativeClick: () => resolve(false),
        onClose: () => resolve(false),
      })
    })
  }

  /**
   * 判断拖进来的文件是否属于当前域。
   *
   * 返回 `true`：交给当前域的安装流程；`false`：已经提示过或已经跳转，
   * 调用方必须停下（不要再打开来源卡片，也不要开始解析）。
   */
  async function acceptDrop(path: string, want: DropDomain): Promise<boolean> {
    let kind: Awaited<ReturnType<typeof api.classifyDropSource>>
    try {
      kind = await api.classifyDropSource(path)
    } catch (reason) {
      message.error(backendMessageOf(reason))
      return false
    }
    if (kind.kind === 'unknown') {
      message.warning(t('tools.unknownPackageHint'))
      return false
    }
    if (kind.kind === want) return true

    const target: DropDomain = kind.kind === 'map' ? 'map' : 'mod'
    const go = await offerOtherDomain(target)
    if (!go) return false
    pendingDrop.value = { path, domain: target }
    await router.push(target === 'map' ? { name: 'maps-online' } : { name: 'mods-online' })
    return false
  }

  return { acceptDrop }
}
