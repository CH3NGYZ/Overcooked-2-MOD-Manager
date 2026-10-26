/**
 * 只保留「按住拖选」，屏蔽双击选词与三击选段。
 *
 * 浏览器默认：`mousedown` 的 `detail` 为 2 表示双击的第二下，会立刻按词选中；
 * 3 表示三击，选中整段。这里在 `detail > 1` 时阻止默认行为，
 * 单次按下（detail === 1）不受影响，因此按住拖动仍能正常选中文字。
 * 输入框等可编辑元素保持浏览器原生行为，避免影响编辑体验。
 */
function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  const tag = target.tagName
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true
  return target.isContentEditable
}

export function installSingleClickSelection() {
  document.addEventListener('mousedown', (event) => {
    if (event.detail > 1 && !isEditable(event.target)) event.preventDefault()
  }, { capture: true })
  // 双击的文本选择在部分 WebView 上晚于 mousedown 生效，兜底再清一次
  document.addEventListener('dblclick', (event) => {
    if (isEditable(event.target)) return
    window.getSelection()?.removeAllRanges()
  }, { passive: true })
}
