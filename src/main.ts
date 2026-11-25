import { createApp } from 'vue'
import App from './App.vue'
import { router } from './router'
import { i18n } from './locale'
import { hasStoredLocale, preselectLocale } from './locale/state'
import { isTauri } from './runtime'
import { api } from './services/tauri'
import 'highlight.js/styles/github-dark.css'
import './styles/global.css'

async function bootstrap() {
  if (!hasStoredLocale()) {
    const installLanguage = isTauri() ? await api.installLanguage().catch(() => null) : null
    preselectLocale(installLanguage, navigator.languages)
  }
  createApp(App).use(router).use(i18n).mount('#app')
}

void bootstrap()
