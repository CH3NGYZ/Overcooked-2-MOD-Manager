/**
 * 窗口级文件拖放 —— MOD 与地图四个页面共用的唯一实现。
 *
 * ## 为什么从 `InstallSourceCard` 里搬出来
 *
 * 拖放是**窗口**能力，不是某张卡片的属性。监听原先挂在来源卡片的 `onMounted`
 * 上，于是「能不能拖」取决于「这页碰巧挂没挂那张卡片」：MOD 安装页与地图下载页
 * 能拖，地图管理页根本没有入口，把一个 ZIP 拖进窗口毫无反应。现在改成页面自己
 * 声明「我这页接受什么、拖进来做什么」，卡片只负责显示高亮。
 *
 * ## 三个不变量
 *
 * 1. Tauri 的 `onDragDropEvent` 全局只注册**一次**：用模块级 promise 占位，
 *    并发首次调用不会注册出两份监听（两份会让一次拖入触发两次安装）。
 * 2. 消费者按注册顺序入栈，事件只交给**最后注册**的那个。切页是 `out-in`
 *    过渡（旧页先卸载），正常不会有第二个；万一两个同时在线，以新页为准。
 * 3. 扩展名判定与「点击选择文件」共用 `dropAccept.ts`，两个入口不会各判一套。
 */
import { onBeforeUnmount, onMounted, ref, shallowRef } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { isTauri } from '../runtime'
import { t } from '../i18n'
import { useNotify, type Notify } from '../useNotify'
import { acceptLabel, matchesFileAccept } from '../dropAccept'

/**
 * 拖拽是否悬停在窗口上。
 *
 * 同一时刻只有一个页面可见，所以全局共用一份，不按页面各存一个 ref：
 * 全窗提示层读的也是它。
 */
export const dragActive = ref(false)

/** 当前页面接受的文件类型（全窗提示层用它拼文案）；没有消费者时为空。 */
export const dropAcceptTypes = ref<string[]>([])

/**
 * 当前页面的提示文案来源。
 *
 * 发布页拖入是「读取待发布的 ZIP」，其他页面是「松手安装」，同一句默认文案
 * 会让作者以为发布页也会装包。存函数而不是存字符串：切换语言时提示条要跟着变。
 */
export const dropHintSource = shallowRef<(() => string) | null>(null)

interface DropConsumer {
  accept: string[]
  notify: Notify
  /** 上一条来源还在处理：这次拖入直接忽略，避免并发两条安装流程。 */
  busy?: () => boolean
  /** 本页专属的提示文案；不传就用默认的「松开即可安装」。 */
  hint?: () => string
  onDrop: (path: string) => void
}

const consumers: DropConsumer[] = []
/** 监听只注册一次；用 promise 而不是布尔量，避免并发首次调用各注册一份。 */
let listener: Promise<void> | null = null

/** 提示层文案取最后注册的消费者：它就是当前可见页面。 */
function syncAcceptTypes() {
  const current = consumers[consumers.length - 1]
  dropAcceptTypes.value = current ? [...current.accept] : []
  dropHintSource.value = current?.hint ?? null
}

async function registerListener() {
  if (listener || !isTauri()) return
  listener = (async () => {
    try {
      await getCurrentWebview().onDragDropEvent((event) => {
        const payload = event.payload
        if (payload.type === 'over' || payload.type === 'enter') {
          dragActive.value = true
          return
        }
        if (payload.type === 'leave') {
          dragActive.value = false
          return
        }
        if (payload.type !== 'drop') return
        dragActive.value = false
        const consumer = consumers[consumers.length - 1]
        const path = payload.paths?.[0]
        if (!consumer || !path) return
        if (consumer.busy?.()) return
        if (!matchesFileAccept(path, consumer.accept)) {
          consumer.notify.warning(
            t('tools.onlyFileTypesAreSupported', { types: acceptLabel(consumer.accept) }),
          )
          return
        }
        consumer.onDrop(path)
      })
    } catch (reason) {
      // 注册失败只让拖放不可用，页面其余功能照常；但必须留痕，不能静默
      console.error('[drop] listener registration failed', reason)
      listener = null
    }
  })()
  await listener
}

/**
 * 让当前页面接收整窗拖放。
 *
 * 在 `setup` 里调用一次（页面级），组件卸载时自动注销；同一页面不要注册两份
 * —— 后注册的那份会顶掉前一份，等于前一份白写。
 */
export function useFileDrop(options: {
  /** 接受的扩展名（不带点），与来源卡片的 `accept` 传同一份。 */
  accept: string[]
  /** 忙碌判定：返回 true 时忽略这次拖入。 */
  busy?: () => boolean
  /** 本页专属的提示文案；不传就用默认的「松开即可安装」。 */
  hint?: () => string
  /** 收到一个通过校验的本地绝对路径（Tauri 给的是路径，不是浏览器的 File）。 */
  onDrop: (path: string) => void
}) {
  const notify = useNotify()
  const consumer: DropConsumer = {
    accept: options.accept,
    notify,
    busy: options.busy,
    hint: options.hint,
    onDrop: options.onDrop,
  }

  onMounted(() => {
    consumers.push(consumer)
    syncAcceptTypes()
    void registerListener()
  })

  onBeforeUnmount(() => {
    const index = consumers.indexOf(consumer)
    if (index >= 0) consumers.splice(index, 1)
    syncAcceptTypes()
    // 拖到一半切页：高亮跟着页面走，不留一个悬空的全窗提示
    if (!consumers.length) dragActive.value = false
  })

  return { dragActive }
}
