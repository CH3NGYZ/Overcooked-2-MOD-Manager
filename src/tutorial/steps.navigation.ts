import type { TutorialStep } from './types'

/** Explain each sidebar entry before the following content step changes route. */
export const sidebarNavSteps = {
  overview: {
    id: 'sidebar-overview', titleKey: 'tutorial.chrome.overview.title', bodyKey: 'tutorial.chrome.overview.body',
    target: '[data-tutorial="nav-overview"]', placement: 'right', padding: 0,
  },
  modsOnline: {
    id: 'sidebar-mods-online', titleKey: 'tutorial.chrome.modsOnline.title', bodyKey: 'tutorial.chrome.modsOnline.body',
    target: '[data-tutorial="nav-mods-online"]', placement: 'right', padding: 0,
  },
  modsLocal: {
    id: 'sidebar-mods-local', titleKey: 'tutorial.chrome.modsLocal.title', bodyKey: 'tutorial.chrome.modsLocal.body',
    target: '[data-tutorial="nav-mods-local"]', placement: 'right', padding: 0,
  },
  mapsOnline: {
    id: 'sidebar-maps-online', titleKey: 'tutorial.chrome.mapsOnline.title', bodyKey: 'tutorial.chrome.mapsOnline.body',
    target: '[data-tutorial="nav-maps-online"]', placement: 'right', padding: 0,
  },
  levels: {
    id: 'sidebar-levels', titleKey: 'tutorial.chrome.levels.title', bodyKey: 'tutorial.chrome.levels.body',
    target: '[data-tutorial="nav-levels"]', placement: 'right', padding: 0,
  },
  rename: {
    id: 'sidebar-rename', titleKey: 'tutorial.chrome.rename.title', bodyKey: 'tutorial.chrome.rename.body',
    target: '[data-tutorial="nav-rename"]', placement: 'right', padding: 0,
  },
  diychef: {
    id: 'sidebar-diychef', titleKey: 'tutorial.chrome.diychef.title', bodyKey: 'tutorial.chrome.diychef.body',
    target: '[data-tutorial="nav-diychef"]', placement: 'right', padding: 0,
  },
  tools: {
    id: 'sidebar-tools', titleKey: 'tutorial.chrome.tools.title', bodyKey: 'tutorial.chrome.tools.body',
    target: '[data-tutorial="nav-tools"]', placement: 'right', padding: 0,
  },
  settings: {
    id: 'sidebar-settings', titleKey: 'tutorial.chrome.settings.title', bodyKey: 'tutorial.chrome.settings.body',
    target: '[data-tutorial="nav-settings"]', placement: 'right', padding: 0,
  },
} satisfies Record<string, TutorialStep>
