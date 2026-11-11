import { MeshStandardMaterial, ShaderChunk, Vector3, type Texture } from 'three'

// OC2_Character_Clothes hard-light blends the player colour, then uses diffuse
// alpha to retain the original colour (alpha=1). The game's PlayerSettings use
// Gamma space; Three samples these sRGB maps in linear space, so blend in sRGB.
const bodyMapFragment = ShaderChunk.map_fragment.replace('diffuseColor *= sampledDiffuseColor;', `
  vec3 chefDiffuse = sRGBTransferOETF( vec4( sampledDiffuseColor.rgb, 1.0 ) ).rgb;
  vec3 chefTint = mix(
    2.0 * chefMaskColor * chefDiffuse,
    1.0 - 2.0 * ( 1.0 - chefMaskColor ) * ( 1.0 - chefDiffuse ),
    step( vec3( 0.5 ), chefMaskColor )
  );
  vec3 chefClothes = mix( clamp( chefTint, 0.0, 1.0 ), chefDiffuse, sampledDiffuseColor.a );
  sampledDiffuseColor.rgb = sRGBTransferEOTF( vec4( chefClothes, 1.0 ) ).rgb;
  diffuseColor *= sampledDiffuseColor;
`)

/** Interpret the material mode before its texture alpha: opaque Unity shaders
 * can store a gloss/effect mask in alpha, so clipping every PNG erases geometry. */
export function createChefMaterial(map: Texture | null, floats: Readonly<Record<string, number>>) {
  const clamp = (value: number | undefined, fallback: number) => typeof value === 'number' && Number.isFinite(value)
    ? Math.max(0, Math.min(1, value)) : fallback
  // Preserve the approved soft portrait at zero gloss. Custom Unity rim and
  // brightness shaders remain approximated by the same static PBR lighting.
  const gloss = clamp(floats._Glossiness, 0)
  const material = new MeshStandardMaterial({
    map,
    roughness: gloss > 0 ? Math.max(0.08, 1 - gloss) : 0.86,
    metalness: clamp(floats._Metallic, 0),
    alphaTest: floats._Mode === 1 ? clamp(floats._Cutoff, 0.5) : 0,
    transparent: floats._Mode === 2 || floats._Mode === 3,
  })
  // Only body parts receive these fields from the backend. Partial or invalid
  // metadata must not accidentally recolour an unrelated texture.
  const mask = ['R', 'G', 'B', 'A'].map(channel => floats[`_MaskColor${channel}`])
  if (map && mask.every(value => typeof value === 'number' && Number.isFinite(value))) {
    const colour = new Vector3(clamp(mask[0], 0), clamp(mask[1], 0), clamp(mask[2], 0))
    material.onBeforeCompile = shader => {
      shader.uniforms.chefMaskColor = { value: colour }
      shader.fragmentShader = `uniform vec3 chefMaskColor;\n${shader.fragmentShader}`
        .replace('#include <map_fragment>', bodyMapFragment)
    }
    material.customProgramCacheKey = () => 'oc2-clothes-gamma-hard-light-v1'
  }
  return material
}
