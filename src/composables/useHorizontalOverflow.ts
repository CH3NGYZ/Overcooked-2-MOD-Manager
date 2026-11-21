import { shallowRef, watch, type Ref } from 'vue'

export function useHorizontalOverflow(
  container: Readonly<Ref<HTMLElement | null>>,
  content: Readonly<Ref<HTMLElement | null>>,
) {
  const overflowing = shallowRef(false)
  watch([container, content], ([box, inner], _previous, onCleanup) => {
    if (!box || !inner || typeof ResizeObserver === 'undefined') {
      overflowing.value = false
      return
    }
    const measure = () => { overflowing.value = box.scrollWidth > box.clientWidth }
    // 内容和父容器都可能变宽，例如切换语言或缩放窗口。
    const observer = new ResizeObserver(measure)
    observer.observe(box)
    observer.observe(inner)
    measure()
    onCleanup(() => observer.disconnect())
  }, { flush: 'post' })
  return overflowing
}
