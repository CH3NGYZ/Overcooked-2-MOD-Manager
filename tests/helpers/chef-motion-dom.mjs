// Browser boundary for the real card-motion/reorder modules. Layout positions
// are supplied by each scenario; this does not implement chef ordering or FLIP.
export const box = (left, top, width = 128, height = 200) => ({ left, top, width, height, right: left + width, bottom: top + height })
const dataKey = name => name.slice(5).replace(/-([a-z])/g, (_, c) => c.toUpperCase())
const numbers = text => [...String(text ?? '').matchAll(/-?\d*\.?\d+/g)].map(value => Number(value[0]))
function components(text) {
  const translate = String(text).match(/translate(?:3d)?\(([^)]+)\)/), scale = String(text).match(/scale\(([^)]+)\)/)
  const [x = 0, y = 0] = numbers(translate?.[1]), [sx = 1, sy = sx] = numbers(scale?.[1])
  return [x, y, sx, sy]
}
class AnimationBoundary {
  constructor(element, keyframes, options) { Object.assign(this, { element, keyframes, options, progress: 0, playState: 'running' }) }
  cancel() { this.playState = 'idle'; this.oncancel?.() }
  finish() { this.progress = 1; this.playState = 'finished'; this.onfinish?.() }
  get effective() { return this.playState === 'running' || (this.playState === 'finished' && this.options.fill === 'forwards') }
}
export class ElementBoundary {
  constructor(tagName = 'article', bounds = box(0, 0)) {
    Object.assign(this, { tagName: tagName.toUpperCase(), layout: bounds, style: { opacity: '' }, dataset: {}, children: [],
      parentElement: null, attributes: new Map(), classes: new Set(), animations: [], captured: null, width: 0, height: 0 })
    this.classList = { add: (...names) => names.forEach(name => this.classes.add(name)), remove: (...names) => names.forEach(name => this.classes.delete(name)),
      contains: name => this.classes.has(name), toggle: (name, value = !this.classes.has(name)) => { if (value) this.classes.add(name); else this.classes.delete(name); return value } }
  }
  get isConnected() { return this.tagName === 'BODY' || !!this.parentElement?.isConnected }
  set className(value) { this.classes = new Set(value.split(/\s+/).filter(Boolean)) }
  get className() { return [...this.classes].join(' ') }
  getAttribute(name) { if (name === 'style') return JSON.stringify(this.style); return this.attributes.get(name) ?? null }
  setAttribute(name, value) { if (name === 'style') this.style = JSON.parse(value); this.attributes.set(name, String(value)); if (name.startsWith('data-')) this.dataset[dataKey(name)] = String(value) }
  removeAttribute(name) { this.attributes.delete(name); if (name.startsWith('data-')) delete this.dataset[dataKey(name)] }
  append(...elements) { for (const element of elements) { element.remove(); element.parentElement = this; this.children.push(element) } }
  remove() { if (this.parentElement) this.parentElement.children = this.parentElement.children.filter(element => element !== this); this.parentElement = null }
  replaceWith(element) { const parent = this.parentElement, index = parent?.children.indexOf(this); if (!parent) return; this.remove(); element.parentElement = parent; parent.children.splice(index, 0, element) }
  matches(selector) {
    if (selector.startsWith('.')) return this.classList.contains(selector.slice(1))
    const attr = selector.match(/^\[([^=\]]+)(?:="([^"]*)")?\]$/)
    if (attr) {
      const value = attr[1].startsWith('data-') ? this.dataset[dataKey(attr[1])] : this.attributes.get(attr[1])
      return value !== undefined && (attr[2] === undefined || attr[2] === value)
    }
    return this.tagName === selector.toUpperCase()
  }
  querySelectorAll(selector) { const parts = selector.split(',').map(value => value.trim()); return this.children.flatMap(element => [...(parts.some(part => element.matches(part)) ? [element] : []), ...element.querySelectorAll(selector)]) }
  querySelector(selector) { return this.querySelectorAll(selector)[0] ?? null }
  closest(selector) { return this.matches(selector) ? this : this.parentElement?.closest(selector) ?? null }
  cloneNode(deep) {
    const clone = new ElementBoundary(this.tagName, { ...this.layout })
    Object.assign(clone, { style: { ...this.style }, dataset: { ...this.dataset }, attributes: new Map(this.attributes), classes: new Set(this.classes),
      gridColumns: this.gridColumns, gridGap: this.gridGap, width: this.width, height: this.height, naturalWidth: this.naturalWidth, naturalHeight: this.naturalHeight, complete: this.complete, probeSlots: this.probeSlots })
    if (deep) clone.append(...this.children.map(child => child.cloneNode(true)))
    return clone
  }
  getBoundingClientRect() {
    let { left, top, width, height } = this.layout
    const parent = this.parentElement
    if (parent?.gridColumns && !['absolute', 'fixed'].includes(this.style.position)) {
      const siblings = parent.children.filter(child => !['absolute', 'fixed'].includes(child.style.position))
        .sort((a, b) => Number(a.style.order ?? 0) - Number(b.style.order ?? 0))
      const index = siblings.indexOf(this), gap = parent.gridGap ?? 12
      const origin = parent.getBoundingClientRect()
      left = origin.left + (index % parent.gridColumns) * (width + gap)
      top = origin.top + Math.floor(index / parent.gridColumns) * (height + gap)
    }
    if (this.gridColumns) {
      const count = this.children.filter(child => !['absolute', 'fixed'].includes(child.style.position)).length
      const rows = Math.ceil(count / this.gridColumns), cardHeight = this.children[0]?.layout.height ?? 200
      height = rows * cardHeight + Math.max(0, rows - 1) * (this.gridGap ?? 12)
    }
    if (this.style.position && this.style.position !== 'relative') {
      const origin = this.style.position === 'fixed' ? box(0, 0) : parent?.getBoundingClientRect() ?? box(0, 0)
      left = origin.left + (parseFloat(this.style.left) || 0); top = origin.top + (parseFloat(this.style.top) || 0)
      width = parseFloat(this.style.width) || width; height = parseFloat(this.style.height) || height
    }
    const animation = this.animations.findLast(item => item.effective)
    const from = components(animation?.keyframes[0].transform ?? this.style.transform), to = components(animation?.keyframes.at(-1).transform)
    const progress = animation?.progress ?? 0
    const [x, y, sx, sy] = from.map((value, index) => animation ? value + (to[index] - value) * progress : value)
    return box(left + x, top + y, width * sx, height * sy)
  }
  animate(keyframes, options) { const animation = new AnimationBoundary(this, keyframes, options); this.animations.push(animation); return animation }
  setPointerCapture(id) { this.captured = id }
  hasPointerCapture(id) { return this.captured === id }
  releasePointerCapture() { this.captured = null }
  getContext() { return { drawImage: source => { this.drawnFrom = source } } }
}
export function browserBoundary(t) {
  const descriptors = new Map(), listeners = new Map(), frames = new Map()
  let serial = 0, reduced = false
  const body = new ElementBoundary('body')
  const window = {
    matchMedia: () => ({ matches: reduced }),
    addEventListener(type, callback) { const set = listeners.get(type) ?? new Set(); set.add(callback); listeners.set(type, set) },
    removeEventListener(type, callback) { listeners.get(type)?.delete(callback) },
  }
  for (const [name, value] of Object.entries({ window, document: { body, createElement: tag => new ElementBoundary(tag) }, HTMLElement: ElementBoundary,
    requestAnimationFrame: callback => { frames.set(++serial, callback); return serial }, cancelAnimationFrame: id => frames.delete(id),
    getComputedStyle: element => ({ overflowY: element.style.overflowY ?? 'visible', order: element.style.order ?? '0',
      width: element.style.width || `${element.layout.width}px`, height: element.style.height || `${element.layout.height}px` }) })) {
    descriptors.set(name, Object.getOwnPropertyDescriptor(globalThis, name)); Object.defineProperty(globalThis, name, { configurable: true, writable: true, value })
  }
  t.chefMotionCleanups = []
  t.after(() => {
    for (const cleanup of t.chefMotionCleanups) cleanup()
    for (const [name, descriptor] of descriptors) { if (descriptor) Object.defineProperty(globalThis, name, descriptor); else delete globalThis[name] }
  })
  return { body, setReduced: value => { reduced = value },
    dispatch(type, values = {}) { for (const callback of [...(listeners.get(type) ?? [])]) callback({ preventDefault() {}, stopPropagation() {}, stopImmediatePropagation() {}, ...values }) },
    flushFrame() { const callbacks = [...frames.values()]; frames.clear(); callbacks.forEach(callback => callback(0)) },
    listenerCount: type => listeners.get(type)?.size ?? 0, frameCount: () => frames.size }
}
export function chef(id, left, top = 0, enabled = true, width = 128) {
  const element = new ElementBoundary('article', box(left, top, width))
  element.dataset.chefId = id; element.dataset.chefSource = 'native'
  element.classList.add('chef-tile'); element.classList.toggle('is-enabled', enabled)
  return element
}
