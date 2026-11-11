/** Keep the Fox lighting reference while framing the complete chef portrait. */
export const PREVIEW_VERSION = 'chef-body-v1'
export const PREVIEW_WIDTH = 512
export const PREVIEW_HEIGHT = 640
export const RENDER_WIDTH = 1024
export const RENDER_HEIGHT = 1280
export const DEFAULT_YAW = -22 * Math.PI / 180
export const DEFAULT_PITCH = Math.atan2(0.12, 3)
export const LIGHTS = {
  sky: 0xfff4e8, ground: 0x787c99, hemisphere: 2.1,
  key: { color: 0xfff2e1, intensity: 2.7, position: [-2, 3, 4] },
  fill: { color: 0xe5efff, intensity: 1.1, position: [3, 1, 2] },
  rim: { color: 0xffead7, intensity: 1.9, position: [1, 2, -3] },
} as const
