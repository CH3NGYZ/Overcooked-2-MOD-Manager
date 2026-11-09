import { ref } from 'vue'
import { api } from '../services/tauri'

/** Render the current scroll content without moving the user's viewport. */
export function useElementScreenshot() {
  const copying = ref(false)
  async function copy(element: HTMLElement, options: { width?: number; pixelRatio?: number; prepare?: (snapshot: HTMLElement) => void } = {}) {
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
      const inherited = getComputedStyle(element)
      for (const property of inherited) {
        if (property.startsWith('--') || ['font-family', 'color', 'line-height'].includes(property)) {
          snapshot.style.setProperty(property, inherited.getPropertyValue(property))
        }
      }
      const width = options.width ?? element.clientWidth
      Object.assign(snapshot.style, { position: 'fixed', left: '-100000px', top: '0', width: `${width}px`, height: 'auto', overflow: 'visible', maxHeight: 'none', flex: 'none', pointerEvents: 'none' })
      snapshot.querySelectorAll('[data-screenshot-exclude]').forEach(node => node.remove())
      document.body.append(snapshot)
      let blob: Blob | null
      try {
        await Promise.all([...snapshot.querySelectorAll('img')].map(image => image.decode().catch(() => {})))
        options.prepare?.(snapshot)
        // Measure the export layout first, then freeze every box before SVG rendering.
        const nodes = [snapshot, ...snapshot.querySelectorAll<HTMLElement>('*')]
        const measured = nodes.map(node => {
          const style = getComputedStyle(node)
          return { node, properties: [...style].map(key => [key, style.getPropertyValue(key)]), box: node.getBoundingClientRect() }
        })
        const height = snapshot.scrollHeight
        for (const { node, properties, box } of measured) {
          for (const [key, value] of properties) node.style.setProperty(key, value)
          if (box.width && box.height) Object.assign(node.style, { boxSizing: 'border-box', width: `${box.width}px`, height: `${box.height}px`, flexShrink: '0' })
        }
        blob = await toBlob(snapshot, {
          width,
          height,
          pixelRatio: options.pixelRatio ?? window.devicePixelRatio,
          backgroundColor: getComputedStyle(surface).backgroundColor,
          style: { position: 'static', left: 'auto', top: 'auto', overflow: 'visible', height: `${height}px`, maxHeight: 'none', flex: 'none' },
        })
      } finally { snapshot.remove() }
      if (!blob) throw new Error('Screenshot was empty')
      await api.copyImage(Array.from(new Uint8Array(await blob.arrayBuffer())))
    } finally {
      copying.value = false
    }
  }
  return { copying, copy }
}
