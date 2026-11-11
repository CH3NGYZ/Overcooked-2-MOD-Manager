import type { TutorialStep } from './types'

// Explain the editor without changing a name, copying text or pushing to a game.
export const renameSteps: TutorialStep[] = [{
  id: 'rename-editor', route: 'rename',
  titleKey: 'tutorial.rename.editor.title', bodyKey: 'tutorial.rename.editor.body',
  target: '[data-tutorial="rename-editor"]', padding: 4,
}]
