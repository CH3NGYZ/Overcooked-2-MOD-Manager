import { ref, watch } from 'vue'

export type RepositoryLayout = 'list' | 'grid'

const LAYOUT_STORAGE_KEY = 'oc2-repo-layout'

/**
 * Repository card layout is shared between the topbar switch (rendered by
 * App.vue, left of the language selector) and the card list in
 * RepositoriesView, so it lives in one module instead of being lifted through
 * props and emits.
 *
 * Grid cards are the default; the choice is persisted so a restart keeps the
 * layout the user last picked.
 */
export const repositoryLayout = ref<RepositoryLayout>(
  localStorage.getItem(LAYOUT_STORAGE_KEY) === 'list' ? 'list' : 'grid',
)

watch(repositoryLayout, (value) => localStorage.setItem(LAYOUT_STORAGE_KEY, value))
