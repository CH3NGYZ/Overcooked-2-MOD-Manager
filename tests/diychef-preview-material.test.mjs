import assert from 'node:assert/strict'
import test from 'node:test'
import { DataTexture, RGBAFormat, ShaderLib, UnsignedByteType } from 'three'
import { createChefMaterial } from '../src/diychef/preview/materials.ts'

test('opaque diffuse maps keep zero-alpha effect masks visible instead of clipping the head', () => {
  // Real native shader data can have useful blue RGB with alpha zero. Alpha is
  // an effect mask in that opaque shader, not the silhouette of the mesh.
  const map = new DataTexture(new Uint8Array([18, 51, 61, 0]), 1, 1, RGBAFormat, UnsignedByteType)
  const material = createChefMaterial(map, { _Mode: 0, _Cutoff: 0.5, _Glossiness: 0 })
  try {
    assert.equal(material.map, map)
    assert.equal(material.transparent, false)
    assert.equal(material.alphaTest, 0)
    assert.equal(material.opacity, 1)
    assert.equal(material.roughness, 0.86)
    assert.equal(material.metalness, 0)
  } finally { material.dispose(); map.dispose() }
})

test('cutout and translucent modes preserve their distinct alpha behavior', () => {
  for (const [mode, cutoff, transparent, alphaTest] of [[1, 0.35, false, 0.35], [1, undefined, false, 0.5], [2, 0.5, true, 0], [3, 0.5, true, 0]]) {
    const material = createChefMaterial(null, { _Mode: mode, _Cutoff: cutoff })
    try {
      assert.equal(material.transparent, transparent)
      assert.equal(material.alphaTest, alphaTest)
    } finally { material.dispose() }
  }
})

test('missing or malformed native float values retain a finite soft preview material', () => {
  const material = createChefMaterial(null, { _Glossiness: Number.NaN, _Metallic: Number.POSITIVE_INFINITY })
  const clipped = createChefMaterial(null, { _Mode: 1, _Cutoff: 2, _Glossiness: 2, _Metallic: -1 })
  try {
    assert.equal(material.roughness, 0.86)
    assert.equal(material.metalness, 0)
    assert.equal(material.alphaTest, 0)
    assert.equal(clipped.alphaTest, 1)
    assert.equal(clipped.roughness, 0.08)
    assert.equal(clipped.metalness, 0)
  } finally { material.dispose(); clipped.dispose() }
})

function compileMaterial(material) {
  const shader = { uniforms: {}, vertexShader: ShaderLib.standard.vertexShader, fragmentShader: ShaderLib.standard.fragmentShader }
  material.onBeforeCompile(shader, {})
  return shader
}
const blueBody = { _Mode: 0, _MaskColorR: 0, _MaskColorG: 0.10040569, _MaskColorB: 0.2647059, _MaskColorA: 1 }

test('real player-one body colour reaches the standard shader without treating the dye mask as transparency', () => {
  const map = new DataTexture(new Uint8Array([180, 180, 180, 0]), 1, 1, RGBAFormat, UnsignedByteType)
  const material = createChefMaterial(map, blueBody)
  try {
    const shader = compileMaterial(material)
    assert.deepEqual(shader.uniforms.chefMaskColor.value.toArray(), [0, 0.10040569, 0.2647059])
    assert.equal(material.map, map)
    assert.equal(material.transparent, false)
    assert.equal(material.alphaTest, 0)
    assert.equal(material.opacity, 1)
    assert.equal(material.color.getHex(), 0xffffff, 'mask colouring must not tint every clothing pixel through material.color')
    assert.ok(!shader.fragmentShader.includes('#include <map_fragment>'))
    assert.match(shader.fragmentShader, /sRGBTransferOETF\( vec4\( sampledDiffuseColor\.rgb/)
    assert.match(shader.fragmentShader, /mix\( clamp\( chefTint, 0\.0, 1\.0 \), chefDiffuse, sampledDiffuseColor\.a \)/)
    assert.match(shader.fragmentShader, /sampledDiffuseColor\.rgb = sRGBTransferEOTF/)
    assert.match(shader.fragmentShader, /diffuseColor \*= sampledDiffuseColor/)
    assert.match(shader.fragmentShader, /#include <color_fragment>/, 'standard vertex colour and lighting stages remain available')
  } finally { material.dispose(); map.dispose() }
})

test('uncoloured heads, partial masks and missing textures retain the ordinary material program', () => {
  const map = new DataTexture(new Uint8Array([255, 255, 255, 255]), 1, 1, RGBAFormat, UnsignedByteType)
  try {
    for (const [texture, values] of [
      [map, { _Mode: 0 }], [map, { _MaskColorR: 0, _MaskColorG: 0.1, _MaskColorB: 0.2 }],
      [map, { ...blueBody, _MaskColorG: Number.NaN }], [null, blueBody],
    ]) {
      const material = createChefMaterial(texture, values)
      try {
        const shader = compileMaterial(material)
        assert.equal(shader.uniforms.chefMaskColor, undefined)
        assert.equal(shader.fragmentShader, ShaderLib.standard.fragmentShader)
        assert.notEqual(material.customProgramCacheKey(), 'oc2-clothes-gamma-hard-light-v1')
      } finally { material.dispose() }
    }
  } finally { map.dispose() }
})

test('coloured body instances share their shader program but keep independent finite colour uniforms', () => {
  const map = new DataTexture(new Uint8Array([255, 255, 255, 128]), 1, 1, RGBAFormat, UnsignedByteType)
  const blue = createChefMaterial(map, blueBody)
  const red = createChefMaterial(map, { ...blueBody, _MaskColorR: 2, _MaskColorG: -1, _MaskColorB: 0 })
  try {
    const blueShader = compileMaterial(blue), redShader = compileMaterial(red)
    assert.equal(blue.customProgramCacheKey(), red.customProgramCacheKey())
    assert.notStrictEqual(blueShader.uniforms.chefMaskColor.value, redShader.uniforms.chefMaskColor.value)
    assert.deepEqual(redShader.uniforms.chefMaskColor.value.toArray(), [1, 0, 0])
    redShader.uniforms.chefMaskColor.value.set(0, 1, 0)
    assert.deepEqual(blueShader.uniforms.chefMaskColor.value.toArray(), [0, 0.10040569, 0.2647059])
  } finally { blue.dispose(); red.dispose(); map.dispose() }
})
