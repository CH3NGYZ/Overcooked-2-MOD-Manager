import { Group, Mesh, SRGBColorSpace, TextureLoader, type MeshStandardMaterial, type Texture } from 'three'
import { api } from '../../services/tauri'
import type { ChefPreviewScene } from './types'
import { decodeGeometry } from './geometry'
import { createChefMaterial } from './materials'

export interface PreparedChef {
  group: Group
  description: ChefPreviewScene
  dispose: () => void
}

export async function prepareChef(sessionId: string, description: ChefPreviewScene, signal?: AbortSignal): Promise<PreparedChef> {
  const group = new Group()
  const textures = new Map<string, Texture>()
  const materials: MeshStandardMaterial[] = []
  const geometries: ReturnType<typeof decodeGeometry>[] = []
  let disposed = false
  const dispose = () => {
    if (disposed) return
    disposed = true
    geometries.forEach(item => item.dispose())
    materials.forEach(item => item.dispose())
    textures.forEach(item => {
      item.dispose()
      item.image?.close?.()
      item.image?.removeAttribute?.('src')
      item.image = null
    })
    geometries.length = 0
    materials.length = 0
    textures.clear()
    group.clear()
  }
  async function texture(id: string) {
    signal?.throwIfAborted()
    const existing = textures.get(id)
    if (existing) return existing
    const bytes = await api.diyChefPreviewAsset(sessionId, id)
    signal?.throwIfAborted()
    const url = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))
    try {
      const value = await new TextureLoader().loadAsync(url)
      value.colorSpace = SRGBColorSpace
      // Backend PNG has image rows in display order, like the Fox PNG fixture.
      value.flipY = true
      textures.set(id, value)
      signal?.throwIfAborted()
      return value
    } finally { URL.revokeObjectURL(url) }
  }
  try {
    signal?.throwIfAborted()
    // Each admitted scene reads serially; the cold pipeline bounds how many
    // scenes can hold decoded assets while waiting for the single renderer.
    for (const source of description.meshes) {
      signal?.throwIfAborted()
      const bytes = await api.diyChefPreviewAsset(sessionId, source.geometry_asset)
      signal?.throwIfAborted()
      const geometry = decodeGeometry(bytes, source.vertex_count, source.index_count)
      geometries.push(geometry)
      const map = source.texture_asset ? await texture(source.texture_asset) : null
      signal?.throwIfAborted()
      // Transform UVs on the geometry so sharing a texture never changes another part.
      const uv = geometry.getAttribute('uv')
      for (let i = 0; i < uv.count; i++) uv.setXY(i,
        uv.getX(i) * source.texture_scale[0] + source.texture_offset[0],
        uv.getY(i) * source.texture_scale[1] + source.texture_offset[1])
      const material = createChefMaterial(map, source.material_floats ?? {})
      materials.push(material)
      const mesh = new Mesh(geometry, material)
      mesh.name = source.name
      group.add(mesh)
    }
    signal?.throwIfAborted()
    return { group, description, dispose }
  } catch (error) { dispose(); throw error }
}
