export interface LevelPreview {
  name: string
  name_zh: string
  scene_name: string
  min_order_count: number
  max_order_count: number
  dependencies: string[]
  scene_exists: boolean
  recipe_count: number
  active_recipe_count: number
}
export interface LevelPreviewPicture {
  data_url: string | null
  width: number
  height: number
  state: 'ready' | 'missing' | 'error'
}
export interface LevelPreviewMenuIcon { name:string; data_url:string|null }
export interface LevelPreviewMenuTile {
  pictures:LevelPreviewMenuIcon[]
  modifiers:LevelPreviewMenuIcon[]
  children:number[]
  large:boolean
}
export interface LevelPreviewRecipe {
  name: string
  score: number | null
  active: boolean
  resolved: boolean
  ingredients: string[]
  icon: string | null
  source: string
  error: string | null
  tiles: LevelPreviewMenuTile[]
}
export interface LevelPreviewDetail {
  picture: LevelPreviewPicture | null
  recipes: LevelPreviewRecipe[] | null
}
