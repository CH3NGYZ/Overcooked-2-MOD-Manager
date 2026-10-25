import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// https://vite.dev/config/
export default defineConfig({
  plugins: [vue()],
  // 依赖扫描只看真实入口，避免把 src-tauri/target 下的验证用 html 当作页面。
  optimizeDeps: {
    entries: ['index.html', 'src/**/*.ts', 'src/**/*.vue'],
  },
})
