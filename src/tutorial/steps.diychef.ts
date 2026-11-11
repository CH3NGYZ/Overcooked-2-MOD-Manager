import type { TutorialContext, TutorialStep } from './types'
import { diyChefDemoIds } from './demoDiyChef'

const PAGE = '[data-tutorial="diychef-page"]'
const HAT_DIALOG = '[data-tutorial="diychef-hat-dialog"]'
const MORE_BUTTON = '[data-tutorial="diychef-more"] button'
const MORE_ITEM = '[data-tutorial="diychef-more-default"]'

async function closeHat(ctx: TutorialContext) {
  if (!document.querySelector(HAT_DIALOG)) return
  ctx.click('[data-tutorial="diychef-hat-cancel"]')
  await ctx.waitForGone(HAT_DIALOG)
}

/** Four everyday actions, using only the editor's in-memory tutorial snapshot. */
export const diyChefSteps: TutorialStep[] = [
  {
    id: 'diychef-default', route: 'diychef',
    titleKey: 'tutorial.diychef.default.title', bodyKey: 'tutorial.diychef.default.body',
    target: `${PAGE} .chef-tile.is-default`, padding: 4,
    prepare: closeHat,
  },
  {
    id: 'diychef-drag', route: 'diychef',
    titleKey: 'tutorial.diychef.drag.title', bodyKey: 'tutorial.diychef.drag.body',
    target: '[data-tutorial="diychef-grid"]', padding: 4,
    prepare: closeHat,
  },
  {
    id: 'diychef-hats', route: 'diychef',
    titleKey: 'tutorial.diychef.hats.title', bodyKey: 'tutorial.diychef.hats.body',
    target: HAT_DIALOG, padding: 4,
    async prepare(ctx) {
      if (document.querySelector(HAT_DIALOG)) return
      ctx.click(`${PAGE} [data-chef-id="${diyChefDemoIds.fox}"] .chef-tile-main`)
      await ctx.waitFor(HAT_DIALOG)
    },
    cleanup: closeHat,
  },
  {
    id: 'diychef-more', route: 'diychef',
    titleKey: 'tutorial.diychef.more.title', bodyKey: 'tutorial.diychef.more.body',
    target: () => {
      const item = document.querySelector(MORE_ITEM)
      return item ? [item.closest('.n-dropdown-menu') ?? item] : []
    },
    padding: 4,
    async prepare(ctx) {
      await closeHat(ctx)
      if (!document.querySelector(`${MORE_BUTTON}[aria-expanded="true"]`)) ctx.click(MORE_BUTTON)
      await ctx.waitFor(MORE_ITEM)
    },
    async cleanup(ctx) {
      // Demo exit closes the controlled menu before its leave animation ends.
      // Checking the lingering option would click the trigger and reopen it.
      if (document.querySelector(`${MORE_BUTTON}[aria-expanded="true"]`)) ctx.click(MORE_BUTTON)
      await ctx.waitForGone(MORE_ITEM)
    },
  },
]
