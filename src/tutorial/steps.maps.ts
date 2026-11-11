import type { TutorialStep } from './types'

// Map cards share the MOD interaction patterns, so each page needs one overview.
export const mapsOnlineSteps: TutorialStep[] = [{
  id: 'maps-online-page', route: 'maps-online',
  titleKey: 'tutorial.maps.online.title', bodyKey: 'tutorial.maps.online.body',
  target: '[data-tutorial="maps-online-page"]', padding: 4,
}]

export const levelsSteps: TutorialStep[] = [{
  id: 'levels-page', route: 'levels',
  titleKey: 'tutorial.maps.local.title', bodyKey: 'tutorial.maps.local.body',
  target: '[data-tutorial="levels-page"]', padding: 4,
}]
