import { BufferAttribute, BufferGeometry } from 'three'

/** The backend already triangulates, reflects and bakes skin/TRS. This layer
 * validates the wire buffer and never parses a model format a second time. */
export function decodeGeometry(buffer: ArrayBuffer, vertices: number, indices: number) {
  if (!Number.isSafeInteger(vertices) || vertices < 0 || vertices > 2_000_000 ||
      !Number.isSafeInteger(indices) || indices < 0 || indices > 6_000_000 || indices % 3 !== 0 ||
      buffer.byteLength !== vertices * 32 + indices * 4) throw new Error('diychef.previewInvalid')
  const view = new DataView(buffer)
  const positions = new Float32Array(vertices * 3)
  const normals = new Float32Array(vertices * 3)
  const uv = new Float32Array(vertices * 2)
  for (let i = 0; i < vertices; i++) {
    for (let j = 0; j < 8; j++) {
      const value = view.getFloat32(i * 32 + j * 4, true)
      if (!Number.isFinite(value)) throw new Error('diychef.previewInvalid')
      if (j < 3) positions[i * 3 + j] = value
      else if (j < 6) normals[i * 3 + j - 3] = value
      else uv[i * 2 + j - 6] = value
    }
  }
  const elements = new Uint32Array(indices)
  for (let i = 0; i < indices; i++) {
    const value = view.getUint32(vertices * 32 + i * 4, true)
    if (value >= vertices) throw new Error('diychef.previewInvalid')
    elements[i] = value
  }
  const geometry = new BufferGeometry()
  geometry.setAttribute('position', new BufferAttribute(positions, 3))
  geometry.setAttribute('normal', new BufferAttribute(normals, 3))
  geometry.setAttribute('uv', new BufferAttribute(uv, 2))
  geometry.setIndex(new BufferAttribute(elements, 1))
  geometry.computeBoundingBox()
  geometry.computeBoundingSphere()
  return geometry
}
