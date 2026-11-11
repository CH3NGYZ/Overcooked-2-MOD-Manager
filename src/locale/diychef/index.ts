import type { DomainMessages } from '../types.ts'
import { diychefAppearance } from './appearance.ts'
import { diychefEditor } from './editor.ts'
import { diychefErrors } from './errors.ts'
import { diychefBackend } from './backend.ts'

export const diychef: DomainMessages = {
  ...diychefEditor,
  ...diychefAppearance,
  ...diychefErrors,
  ...diychefBackend,
}
