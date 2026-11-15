import { Box3, Mesh, OrthographicCamera, Vector3, type BufferAttribute, type Group } from 'three'
import type { ChefPreviewScene } from './types'
import { DEFAULT_PITCH, DEFAULT_YAW, PREVIEW_WIDTH, PREVIEW_HEIGHT } from './rendererPreset'

export type ChefFramePoint = readonly [number, number, number]
export interface ChefFrameBounds { min: [number, number, number]; max: [number, number, number] }
export interface ChefFrameSelection { points: ChefFramePoint[]; bounds: ChefFrameBounds; rejected: number }

const MAX_DECLARED_HEAD_SPAN = 8
const MAX_DECLARED_HEAD_COORDINATE = 100

function isFinitePoint(point: ChefFramePoint) { return point.every(Number.isFinite) }
function isFiniteBounds(bounds: ChefFrameBounds) {
  return bounds.min.every(Number.isFinite) && bounds.max.every(Number.isFinite)
    && bounds.min.every((value, axis) => value <= bounds.max[axis])
}
function isUsableAnchor(bounds: ChefFrameBounds) {
  return isFiniteBounds(bounds)
    && bounds.min.concat(bounds.max).every(value => Math.abs(value) <= MAX_DECLARED_HEAD_COORDINATE)
    && bounds.min.every((value, axis) => bounds.max[axis] - value <= MAX_DECLARED_HEAD_SPAN)
}
function boundsOf(points: readonly ChefFramePoint[]): ChefFrameBounds | null {
  if (!points.length) return null
  const min: [number, number, number] = [Infinity, Infinity, Infinity]
  const max: [number, number, number] = [-Infinity, -Infinity, -Infinity]
  for (const point of points) for (let axis = 0; axis < 3; axis++) {
    min[axis] = Math.min(min[axis], point[axis]); max[axis] = Math.max(max[axis], point[axis])
  }
  return { min, max }
}

/** Robust fallback for malformed/absurd head_bounds metadata. */
export function robustChefBounds(points: readonly ChefFramePoint[]): ChefFrameBounds | null {
  const finite = points.filter(isFinitePoint)
  if (!finite.length) return null
  const min: [number, number, number] = [0, 0, 0], max: [number, number, number] = [0, 0, 0]
  for (let axis = 0; axis < 3; axis++) {
    const values = finite.map(point => point[axis]).sort((a, b) => a - b)
    const median = values[Math.floor(values.length / 2)]
    const deviations = values.map(value => Math.abs(value - median)).sort((a, b) => a - b)
    const margin = Math.max(deviations[Math.floor(deviations.length / 2)] * 8, 2)
    min[axis] = median - margin; max[axis] = median + margin
  }
  return { min, max }
}

function axisMargin(head: ChefFrameBounds, axis: number) {
  const span = Math.max(head.max[axis] - head.min[axis], 0.01)
  return axis === 1 ? Math.max(span * 4, 2) : Math.max(span * 3, 1.5)
}
/** Wide enough for normal body/hat/hand/tail parts, narrow enough for detached meshes. */
export function chefFrameEnvelope(head: ChefFrameBounds): ChefFrameBounds {
  return {
    min: head.min.map((value, axis) => value - axisMargin(head, axis)) as [number, number, number],
    max: head.max.map((value, axis) => value + axisMargin(head, axis)) as [number, number, number],
  }
}
/** Filters one mesh without retaining its vertices; exposed for pure regression tests. */
export function filterChefMeshPoints(positions: readonly ChefFramePoint[], envelope: ChefFrameBounds): ChefFrameSelection {
  const accepted = positions.filter(point => isFinitePoint(point) && point.every((value, axis) => value >= envelope.min[axis] && value <= envelope.max[axis]))
  return { points: accepted, bounds: boundsOf(accepted) ?? { min: [0, 0, 0], max: [0, 0, 0] }, rejected: positions.length - accepted.length }
}
/** Pure helper for callers/tests that already have a finite point list. */
export function selectChefFramePoints(input: readonly ChefFramePoint[], declared: ChefFrameBounds): ChefFrameSelection {
  const finite = input.filter(isFinitePoint)
  const anchor = isUsableAnchor(declared) ? declared : robustChefBounds(finite)
  if (!anchor) return { points: [], bounds: { min: [0, 0, 0], max: [0, 0, 0] }, rejected: input.length }
  const filtered = filterChefMeshPoints(finite, chefFrameEnvelope(anchor))
  const bounds = isUsableAnchor(declared) ? (boundsOf([anchor.min, anchor.max, ...filtered.points]) ?? anchor) : (boundsOf(filtered.points) ?? anchor)
  return { points: filtered.points, bounds, rejected: input.length - filtered.points.length }
}
function boundsCorners(bounds: ChefFrameBounds): ChefFramePoint[] {
  const points: ChefFramePoint[] = []
  for (const x of [bounds.min[0], bounds.max[0]]) for (const y of [bounds.min[1], bounds.max[1]]) for (const z of [bounds.min[2], bounds.max[2]]) points.push([x, y, z])
  return points
}
function pointList(group: Group): ChefFramePoint[] {
  const result: ChefFramePoint[] = [], source = new Vector3()
  group.traverse(object => {
    if (!(object instanceof Mesh)) return
    const positions = object.geometry.getAttribute('position')
    for (let i = 0; i < positions.count; i++) { source.fromBufferAttribute(positions, i).applyMatrix4(object.matrixWorld); result.push([source.x, source.y, source.z]) }
  })
  return result
}
function meshBounds(object: Mesh, envelope: ChefFrameBounds, source: Vector3): ChefFrameBounds | null {
  const positions = object.geometry.getAttribute('position') as BufferAttribute
  const min: [number, number, number] = [Infinity, Infinity, Infinity], max: [number, number, number] = [-Infinity, -Infinity, -Infinity]
  let accepted = false
  for (let i = 0; i < positions.count; i++) {
    source.fromBufferAttribute(positions, i).applyMatrix4(object.matrixWorld)
    const point: ChefFramePoint = [source.x, source.y, source.z]
    if (!isFinitePoint(point) || !point.every((value, axis) => value >= envelope.min[axis] && value <= envelope.max[axis])) continue
    accepted = true
    for (let axis = 0; axis < 3; axis++) { min[axis] = Math.min(min[axis], point[axis]); max[axis] = Math.max(max[axis], point[axis]) }
  }
  return accepted ? { min, max } : null
}
export function createChefCamera(group: Group, description: ChefPreviewScene) {
  group.updateMatrixWorld(true)
  const declared = description.head_bounds
  const anchor = isUsableAnchor(declared) ? declared : robustChefBounds(pointList(group))
  if (!anchor) throw new Error('diychef.previewInvalid')
  const envelope = chefFrameEnvelope(anchor), meshFrames: ChefFrameBounds[] = [], source = new Vector3()
  group.traverse(object => { if (object instanceof Mesh) { const bounds = meshBounds(object, envelope, source); if (bounds) meshFrames.push(bounds) } })
  if (isUsableAnchor(declared)) meshFrames.push(declared)
  const frameBounds = boundsOf(meshFrames.flatMap(bounds => [bounds.min, bounds.max]))
  if (!frameBounds) throw new Error('diychef.previewInvalid')
  const frame = new Box3(new Vector3(...frameBounds.min), new Vector3(...frameBounds.max)), center = frame.getCenter(new Vector3()), size = frame.getSize(new Vector3())
  const aspect = PREVIEW_WIDTH / PREVIEW_HEIGHT, baseHalf = Math.max(size.y, size.x / aspect, 0.01) * 0.53, point = new Vector3(), framePoints = meshFrames.flatMap(boundsCorners)
  const camera = new OrthographicCamera(-baseHalf, baseHalf, baseHalf, -baseHalf, 0.001, 1000)
  function update(yaw = DEFAULT_YAW, pitch = DEFAULT_PITCH) {
    const distance = Math.max(size.length() * 3, 3)
    camera.position.set(center.x + Math.sin(yaw) * Math.cos(pitch) * distance, center.y + Math.sin(pitch) * distance, center.z + Math.cos(yaw) * Math.cos(pitch) * distance)
    camera.lookAt(center); camera.updateMatrixWorld(true)
    let extent = baseHalf
    for (const sourcePoint of framePoints) { point.set(sourcePoint[0], sourcePoint[1], sourcePoint[2]).applyMatrix4(camera.matrixWorldInverse); extent = Math.max(extent, Math.abs(point.x) * 1.08 / aspect, Math.abs(point.y) * 1.08) }
    camera.left = -extent * aspect; camera.right = extent * aspect; camera.bottom = -extent; camera.top = extent; camera.updateProjectionMatrix()
  }
  update()
  return { camera, update }
}
