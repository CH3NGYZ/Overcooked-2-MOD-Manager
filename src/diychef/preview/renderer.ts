import { ACESFilmicToneMapping, DirectionalLight, HemisphereLight, Scene, SRGBColorSpace, WebGLRenderer } from 'three'
import { createChefCamera } from './framing'
import { DEFAULT_PITCH, DEFAULT_YAW, LIGHTS, PREVIEW_WIDTH, PREVIEW_HEIGHT, RENDER_WIDTH, RENDER_HEIGHT } from './rendererPreset'
import type { PreparedChef } from './scene'

export class ChefRenderer {
  readonly canvas = document.createElement('canvas')
  private renderer: WebGLRenderer
  private scene = new Scene()
  private current: PreparedChef | null = null
  private frame: ReturnType<typeof createChefCamera> | null = null
  private lost = false
  constructor() {
    this.renderer = new WebGLRenderer({ canvas: this.canvas, antialias: true, alpha: true, preserveDrawingBuffer: true })
    this.renderer.setPixelRatio(1)
    this.renderer.setSize(RENDER_WIDTH, RENDER_HEIGHT, false)
    this.renderer.outputColorSpace = SRGBColorSpace
    this.renderer.toneMapping = ACESFilmicToneMapping
    this.renderer.toneMappingExposure = 1.16
    this.renderer.setClearColor(0x000000, 0)
    this.scene.add(new HemisphereLight(LIGHTS.sky, LIGHTS.ground, LIGHTS.hemisphere))
    for (const spec of [LIGHTS.key, LIGHTS.fill, LIGHTS.rim]) {
      const light = new DirectionalLight(spec.color, spec.intensity)
      light.position.set(spec.position[0], spec.position[1], spec.position[2])
      this.scene.add(light)
    }
    this.canvas.addEventListener('webglcontextlost', this.onLost)
    this.canvas.addEventListener('webglcontextrestored', this.onRestored)
  }
  private onLost = (event: Event) => { event.preventDefault(); this.lost = true }
  private onRestored = () => { this.lost = false; if (this.current) this.render() }
  setChef(chef: PreparedChef) {
    this.clear()
    this.current = chef
    this.scene.add(chef.group)
    this.frame = createChefCamera(chef.group, chef.description)
    this.render()
  }
  render(yaw = DEFAULT_YAW, pitch = DEFAULT_PITCH) {
    if (this.lost) throw new Error('diychef.previewContextLost')
    if (!this.frame) return
    this.frame.update(yaw, pitch)
    this.renderer.render(this.scene, this.frame.camera)
  }
  async capture(): Promise<Blob> {
    if (this.lost) throw new Error('diychef.previewContextLost')
    const output = document.createElement('canvas')
    output.width = PREVIEW_WIDTH
    output.height = PREVIEW_HEIGHT
    const context = output.getContext('2d')
    if (!context) throw new Error('diychef.previewUnavailable')
    context.imageSmoothingQuality = 'high'
    context.drawImage(this.canvas, 0, 0, PREVIEW_WIDTH, PREVIEW_HEIGHT)
    return new Promise((resolve, reject) => output.toBlob(blob => blob ? resolve(blob) : reject(new Error('diychef.previewUnavailable')), 'image/png'))
  }
  clear(preserveCanvas = false) {
    if (this.current) { this.scene.remove(this.current.group); this.current.dispose() }
    this.current = null
    this.frame = null
    this.renderer.renderLists.dispose()
    if (!preserveCanvas) this.renderer.clear()
  }
  dispose() {
    this.clear()
    this.canvas.removeEventListener('webglcontextlost', this.onLost)
    this.canvas.removeEventListener('webglcontextrestored', this.onRestored)
    this.canvas.remove()
    this.renderer.dispose()
    this.renderer.forceContextLoss()
  }
}
