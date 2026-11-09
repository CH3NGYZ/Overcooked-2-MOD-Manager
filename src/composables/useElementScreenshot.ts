import { ref } from 'vue'
import { api } from '../services/tauri'

/** Render the current scroll content without moving the user's viewport. */
export function useElementScreenshot() {
  const copying = ref(false)
  async function copy(element: HTMLElement) {
    if (copying.value) return
    copying.value = true
    try {
      const { toBlob } = await import('html-to-image')
      await document.fonts.ready
      await Promise.all([...element.querySelectorAll('img')].map(image => image.decode().catch(() => {})))
      const surface = element.closest('.app-modal') ?? element
      // SVG foreignObject can reflow flex/grid and container units differently.
      // Freeze measured boxes on a disposable copy, never on the live preview.
      const snapshot = element.cloneNode(true) as HTMLElement
      const originals = [element, ...element.querySelectorAll<HTMLElement>('*')]
      const copies = [snapshot, ...snapshot.querySelectorAll<HTMLElement>('*')]
      originals.forEach((original, index) => {
        const copy = copies[index]
        const style = getComputedStyle(original)
        for (const property of style) copy.style.setProperty(property, style.getPropertyValue(property))
        const box = original.getBoundingClientRect()
        if (box.width && box.height) {
          copy.style.boxSizing = 'border-box'
          copy.style.width = `${box.width}px`
          copy.style.height = `${box.height}px`
          copy.style.flexShrink = '0'
        }
      })
      Object.assign(snapshot.style, { position: 'fixed', left: '-100000px', top: '0', width: `${element.clientWidth}px`, height: `${element.scrollHeight}px`, overflow: 'visible', maxHeight: 'none' })
      document.body.append(snapshot)
      let blob: Blob | null
      try { blob = await toBlob(snapshot, {
        width: element.clientWidth,
        height: element.scrollHeight,
        pixelRatio: window.devicePixelRatio,
        backgroundColor: getComputedStyle(surface).backgroundColor,
        style: { position: 'static', left: 'auto', top: 'auto', overflow: 'visible', height: `${element.scrollHeight}px`, maxHeight: 'none', flex: 'none' },
        filter: node => !(node instanceof HTMLElement && node.hasAttribute('data-screenshot-exclude')),
      }) } finally { snapshot.remove() }
      if (!blob) throw new Error('Screenshot was empty')
      await api.copyImage(Array.from(new Uint8Array(await blob.arrayBuffer())))
    } finally {
      copying.value = false
    }
  }
  return { copying, copy }
}
