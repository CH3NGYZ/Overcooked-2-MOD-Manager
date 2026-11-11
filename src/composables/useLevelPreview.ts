import { computed, ref, watch } from 'vue'
import { api } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { demoActive, demoLevelPreview, demoLevelPreviewDetail, isDemoId } from '../tutorial/demo'
import type { LevelPreview, LevelPreviewPicture, LevelPreviewRecipe, LevelSetInfo } from '../types'

/** Each dialog session owns its results; late IPC responses cannot change a newer selection. */
export function useLevelPreview() {
  const show = ref(false)
  const target = ref<LevelSetInfo | null>(null)
  const levels = ref<LevelPreview[]>([])
  const selected = ref(0)
  const query = ref('')
  const loading = ref(false)
  const error = ref('')
  const picture = ref<LevelPreviewPicture | null>(null)
  const recipes = ref<LevelPreviewRecipe[] | null>(null)
  const pictureBusy = ref(false)
  const recipeBusy = ref(false)
  const pictureError = ref('')
  const recipeError = ref('')
  let session = 0
  let request = 0
  const details = new Map<number, {picture?: LevelPreviewPicture; recipes?: LevelPreviewRecipe[]}>()
  const current = computed(() => levels.value[selected.value])
  const visible = computed(() => levels.value.map((level, index) => ({level, index})).filter(({level}) =>
    [level.name, level.name_zh, level.scene_name].join(' ').toLocaleLowerCase().includes(query.value.trim().toLocaleLowerCase())))
  async function loadDetail() {
    const set = target.value
    if (!set || !current.value || !show.value) return
    if (demoActive.value && !isDemoId(set.dir_name)) return
    const ticket = ++request
    const generation = session
    const index = selected.value
    const cached = details.get(index)
    picture.value = cached?.picture ?? null
    recipes.value = cached?.recipes ?? null
    pictureError.value = recipeError.value = ''
    async function load(part: 'picture' | 'recipes') {
      const busy = part === 'picture' ? pictureBusy : recipeBusy
      busy.value = !cached?.[part]
      if (!busy.value) return
      try {
        const result = demoLevelPreviewDetail(set!) ?? await api.levelPreviewDetail(set!.dir_name, set!.enabled, index, part)
        if (generation !== session) return
        const entry = details.get(index) ?? {}
        if (result.picture) entry.picture = result.picture
        if (result.recipes) entry.recipes = result.recipes
        details.set(index, entry)
        if (ticket !== request) return
        if (part === 'picture') picture.value = result.picture
        else recipes.value = result.recipes
      } catch (e) {
        if (ticket === request) (part === 'picture' ? pictureError : recipeError).value = backendMessageOf(e)
      } finally {
        if (ticket === request) busy.value = false
      }
    }
    await Promise.all([load('picture'), load('recipes')])
  }
  async function open(set: LevelSetInfo) {
    if (demoActive.value && !isDemoId(set.dir_name)) return
    const generation = ++session
    ++request
    target.value = {...set}
    show.value = true
    loading.value = true
    error.value = ''
    levels.value = []
    details.clear()
    selected.value = 0
    query.value = ''
    picture.value = null
    recipes.value = null
    pictureBusy.value = recipeBusy.value = false
    try {
      const result = demoLevelPreview(set) ?? await api.levelSetPreview(set.dir_name, set.enabled)
      if (session !== generation) return
      levels.value = result
      loading.value = false
      void loadDetail()
    } catch (e) {
      if (session === generation) error.value = backendMessageOf(e)
    } finally {
      if (session === generation) loading.value = false
    }
  }
  watch(selected, loadDetail)
  watch(show, value => { if (!value) { ++session; ++request } })
  // A preview already open before replay must not keep a real target in the tutorial.
  watch(demoActive, () => { ++session; ++request; show.value = false }, { flush: 'sync' })
  function afterLeave() {
    if (show.value) return
    target.value = null
    levels.value = []
    picture.value = null
    recipes.value = null
    details.clear()
  }
  function retry() { details.delete(selected.value); void loadDetail() }
  return {show, target, levels, selected, query, loading, error, picture, recipes, pictureBusy, recipeBusy,
    pictureError, recipeError, current, visible, open, retry, afterLeave}
}
