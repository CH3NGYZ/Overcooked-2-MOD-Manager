/**
 * Monaco 的 ESM 子路径没有随包提供类型声明（package.json exports 只映射 .js 实体），
 * 这里为项目实际用到的入口补最小声明，避免 vue-tsc 报「找不到模块」。
 */
declare module 'monaco-editor/editor/editor.api.js' {
  export * from 'monaco-editor'
}

declare module 'monaco-editor/languages/definitions/ini/register.js' {
  // 注册 ini 语言的副作用模块，没有导出
  const noop: void
  export default noop
}

declare module 'monaco-editor/editor/editor.worker.js?worker' {
  /** Vite 的 ?worker 导入产出 Worker 构造函数。 */
  const WorkerConstructor: new () => Worker
  export default WorkerConstructor
}
