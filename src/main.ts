import { createApp } from 'vue'
import App from './App.vue'
import { router } from './router'
import 'highlight.js/styles/github-dark.css'
import './styles/global.css'

createApp(App).use(router).mount('#app')
