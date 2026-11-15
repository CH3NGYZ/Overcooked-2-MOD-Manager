import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

registerHooks({ resolve(specifier, context, next) {
  if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
    try { return next(`${specifier}.ts`, context) } catch { /* normal module resolution */ }
  }
  return next(specifier, context)
} })

const { selectChefFramePoints } = await import('../src/diychef/preview/framing.ts')
const head = { min: [-0.5, 0.5, -0.5], max: [0.5, 1.3, 0.5] }

test('framing keeps the normal body, hat, hands and tail envelope', () => {
  const result = selectChefFramePoints([
    [0, 1.8, 0],       // tall hat
    [-0.7, 0.6, 0.2],  // hand
    [0.2, 0.1, -0.8],  // tail/body
  ], head)
  assert.equal(result.rejected, 0)
  assert.deepEqual(result.bounds.min, [-0.7, 0.1, -0.8])
  assert.deepEqual(result.bounds.max, [0.5, 1.8, 0.5])
})

test('a detached extreme Y component does not change framing bounds', () => {
  const base = selectChefFramePoints([
    [0, 1.8, 0], [-0.7, 0.6, 0.2], [0.2, 0.1, -0.8],
  ], head)
  const withOutlier = selectChefFramePoints([
    [0, 1.8, 0], [-0.7, 0.6, 0.2], [0.2, 0.1, -0.8], [0.1, 1000, 0],
  ], head)
  assert.equal(withOutlier.rejected, 1)
  assert.deepEqual(withOutlier.bounds, base.bounds)
})

test('an extreme point inside a head mesh is filtered just like a detached mesh', () => {
  const result = selectChefFramePoints([
    [-0.2, 0.8, 0], [0.2, 1.1, 0], [0, -500, 0],
  ], head)
  assert.equal(result.rejected, 1)
  assert.deepEqual(result.bounds.min, [-0.5, 0.5, -0.5])
  assert.deepEqual(result.bounds.max, [0.5, 1.3, 0.5])
})

test('invalid declared bounds fall back to finite geometry without admitting a huge outlier', () => {
  const result = selectChefFramePoints([
    [-0.4, 0.4, 0], [0.4, 1.4, 0], [0, 1000, 0],
  ], { min: [Number.NaN, 0, 0], max: [0, 0, 0] })
  assert.equal(result.rejected, 1)
  assert.deepEqual(result.bounds.min, [-0.4, 0.4, 0])
  assert.deepEqual(result.bounds.max, [0.4, 1.4, 0])
})

test('non-finite vertices never enter camera framing', () => {
  const result = selectChefFramePoints([
    [0, 0.8, 0], [Number.POSITIVE_INFINITY, 0, 0], [0, Number.NaN, 0],
  ], head)
  assert.equal(result.rejected, 2)
  assert.equal(result.points.length, 1)
})
