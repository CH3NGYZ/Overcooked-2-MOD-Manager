/**
 * 全应用统一的通知入口。
 *
 * 直接 `useMessage()` 的话，每条提示都吃 `n-message-provider` 的全局
 * `duration`（3000ms）。带完整路径的错误动辄上百字符，3 秒读不完。
 *
 * 这层包装做两件事：
 *   1. 按内容长度算停留时长（见 `notifyDuration.ts`）
 *   2. 长内容自动加关闭按钮，并且**悬停暂停**
 *
 * 用法与 `useMessage()` 完全一致（`notify.success(text)`），所以各组件
 * 只需把 `useMessage` 换成 `useNotify`。显式传 `{ duration }` 时以调用方
 * 为准 —— 少数需要「不自动消失」的场景仍可覆盖。
 */
import { useMessage } from 'naive-ui'
import type { MessageApi, MessageOptions, MessageReactive } from 'naive-ui'
import { messageDuration, messageNeedsCloseButton } from './notifyDuration'
import { gameOperationNotice } from './composables/useGameOperationGate'
import { translateBackendMessage } from './locale/backend'

type Content = Parameters<MessageApi['success']>[0]

/** 把内容与调用方选项合成为最终的 Naive 选项。 */
function withReadableDuration(content: Content, options?: MessageOptions): MessageOptions {
  const duration = options?.duration ?? messageDuration(content)
  return {
    // 长提示悬停暂停：用户读到一半把鼠标放上去，倒计时就停住
    keepAliveOnHover: true,
    ...options,
    // closable 只在调用方没显式指定时才按长度推断
    closable: options?.closable ?? messageNeedsCloseButton(content),
    ...(duration === undefined ? {} : { duration }),
  }
}

export interface Notify {
  create: (content: Content, options?: MessageOptions) => MessageReactive
  info: (content: Content, options?: MessageOptions) => MessageReactive
  success: (content: Content, options?: MessageOptions) => MessageReactive
  warning: (content: Content, options?: MessageOptions) => MessageReactive
  error: (content: Content, options?: MessageOptions) => MessageReactive
  loading: (content: Content, options?: MessageOptions) => MessageReactive
  destroyAll: () => void
}

/**
 * 取统一通知入口。
 *
 * 必须仍从 `useMessage()` 取底层 API：它依赖 `n-message-provider`
 * 的注入，脱离组件上下文（setup 之外）调用会失败。
 */
export function useNotify(): Notify {
  const message = useMessage()
  /*
   * loading 不按长度算时长：它表示「进行中」，由调用方拿返回的句柄
   * 自己 destroy。传了 duration 反而会中途消失。
   */
  return {
    create: (content, options) => message.create(content, withReadableDuration(content, options)),
    info: (content, options) => message.info(content, withReadableDuration(content, options)),
    success: (content, options) => message.success(content, withReadableDuration(content, options)),
    warning: (content, options) => message.warning(content, withReadableDuration(content, options)),
    error: (content, options) => {
      // A process may start after the click check. Backend refusal must still
      // surface the same blocking notice, even when its caller adds context.
      if (typeof content === 'string'
        && (content.includes('game.running')
          || content.includes(translateBackendMessage('game.running')))) {
        gameOperationNotice.value = true
      }
      return message.error(content, withReadableDuration(content, options))
    },
    loading: (content, options) => message.loading(content, { duration: 0, ...options }),
    destroyAll: () => message.destroyAll(),
  }
}
