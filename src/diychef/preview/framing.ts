import { Box3, Mesh, OrthographicCamera, Vector3, type Group } from 'three'
import type { ChefPreviewScene } from './types'
import { DEFAULT_PITCH, DEFAULT_YAW, PREVIEW_WIDTH, PREVIEW_HEIGHT } from './rendererPreset'

export function createChefCamera(group: Group, description: ChefPreviewScene) {
  const declared = description.head_bounds
  const head = new Box3(new Vector3(...declared.min), new Vector3(...declared.max))
  const all = new Box3().setFromObject(group)
  if (head.isEmpty()) head.copy(all)
  if (head.isEmpty()) throw new Error('diychef.previewInvalid')
  const frame = all.isEmpty() ? head : all
  const center = frame.getCenter(new Vector3())
  const size = frame.getSize(new Vector3())
  const aspect = PREVIEW_WIDTH / PREVIEW_HEIGHT
  const baseHalf = Math.max(size.y, size.x / aspect, 0.01) * 0.53
  group.updateMatrixWorld(true)
  const point = new Vector3()
  const camera = new OrthographicCamera(-baseHalf, baseHalf, baseHalf, -baseHalf, 0.001, 1000)
  function update(yaw = DEFAULT_YAW, pitch = DEFAULT_PITCH) {
    const distance = Math.max(size.length() * 3, 3)
    camera.position.set(center.x + Math.sin(yaw) * Math.cos(pitch) * distance,
      center.y + Math.sin(pitch) * distance, center.z + Math.cos(yaw) * Math.cos(pitch) * distance)
    camera.lookAt(center)
    camera.updateMatrixWorld(true)
    // Fit actual vertices, including tall hats, hands and tails, rather than
    // empty corners of a rotated bounding box. Keep the complete chef centered.
    let extent = baseHalf
    group.traverse(object => {
      if (!(object instanceof Mesh)) return
      const positions = object.geometry.getAttribute('position')
      for (let i = 0; i < positions.count; i++) {
        point.fromBufferAttribute(positions, i).applyMatrix4(object.matrixWorld).applyMatrix4(camera.matrixWorldInverse)
        extent = Math.max(extent, Math.abs(point.x) * 1.08 / aspect, Math.abs(point.y) * 1.08)
      }
    })
    camera.left = -extent * aspect
    camera.right = extent * aspect
    camera.bottom = -extent
    camera.top = extent
    camera.updateProjectionMatrix()
  }
  update()
  return { camera, update }
}
