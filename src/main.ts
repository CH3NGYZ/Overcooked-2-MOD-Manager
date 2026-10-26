import { createApp } from 'vue'
import App from './App.vue'
import { router } from './router'
import { installSingleClickSelection } from './textSelection'
import 'highlight.js/styles/github-dark.css'
import './styles/global.css'

// 只保留按住拖动选择：屏蔽双击选词 / 三击选段（输入框除外）
installSingleClickSelection()

createApp(App).use(router).mount('#app')
