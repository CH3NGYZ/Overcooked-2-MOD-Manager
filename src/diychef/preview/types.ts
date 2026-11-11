/** A vertex is position.xyz, normal.xyz, uv.xy (eight f32 LE), followed by u32 LE indices. */
export interface ChefPreviewBounds { min: [number, number, number]; max: [number, number, number] }
export interface ChefPreviewMesh {
  name: string
  geometry_asset: string
  vertex_count: number
  index_count: number
  texture_asset: string | null
  texture_scale: [number, number]
  texture_offset: [number, number]
  material_floats: Record<string, number>
}
export interface ChefPreviewScene {
  fingerprint: string
  estimated_bytes?: number
  scene_version: number
  status: 'complete' | 'partial' | 'unavailable'
  meshes: ChefPreviewMesh[]
  head_bounds: ChefPreviewBounds
  diagnostics: string[]
}
