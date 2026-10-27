import { createApp } from 'vue'
import App from './App.vue'
import { router } from './router'
import { installSingleClickSelection } from './textSelection'
import { applyInstallLanguage, locale } from './i18n'
import { api } from './services/tauri'
import 'highlight.js/styles/github-dark.css'
import './styles/global.css'

// 只保留按住拖动选择：屏蔽双击选词 / 三击选段（输入框除外）
installSingleClickSelection()

/**
 * 首启语言：安装程序写下的语言优先。
 *
 * 必须在 `mount` **之前**完成，否则会先按 zh-CN 渲染一帧再切换 —— 用户能看到
 * 界面文字闪一下。`get_install_language` 只是一次本地文件读取，延迟可忽略，
 * 所以这里等它，而不是挂载后再补。
 *
 * 失败（文件缺失、内容非法、非 Tauri 环境）时静默保持默认语言；
 * 已存在用户选择过的语言时，`applyInstallLanguage` 内部直接返回，不覆盖。
 */
async function bootstrap() {
  try {
    applyInstallLanguage(await api.installLanguage())
  } catch {
    // 浏览器预览或调用被拒绝：保持默认语言即可
  }
  // locale 的 watch 只在值变化时触发；首启没有变化时也要同步一次 <html lang>
  document.documentElement.lang = locale.value
  createApp(App).use(router).mount('#app')
}

void bootstrap()
