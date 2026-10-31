import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  // 依赖扫描只看真实入口，避免把 src-tauri/target 下的验证用 html 当作页面。
  optimizeDeps: {
    entries: ['index.html', 'src/**/*.ts', 'src/**/*.vue'],
  },
  // 不要监视构建产物和工具目录：src-tauri/target 有十多万物件、上百 GB，
  // 它的变更事件会把 dev server 主线程拖住（实测首次编译单个 SFC 从 40ms 变成 11s）。
  server: {
    watch: {
      ignored: ['**/src-tauri/**', '**/dist/**', '**/.dsh-ops/**', '**/.tmp/**'],
    },
  },
})
