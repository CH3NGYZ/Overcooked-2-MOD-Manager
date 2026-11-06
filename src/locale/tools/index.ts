import type { DomainMessages } from '../types.ts'
import { toolsUiDiagnostics } from './diagnostics.ts'
import { toolsUiInstall } from './install.ts'
import { toolsUiRelations } from './relations.ts'
import { toolsUiRepository } from './repository.ts'
import { toolsUiSettings } from './settings.ts'

/** 统一域入口；业务只依赖语言 key，不依赖文案子表的位置。 */
export const toolsUi: DomainMessages = {
  ...toolsUiDiagnostics,
  ...toolsUiInstall,
  ...toolsUiRelations,
  ...toolsUiRepository,
  ...toolsUiSettings,
}
