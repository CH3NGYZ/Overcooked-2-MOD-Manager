import { t } from '../i18n'
import type { ChefHat } from './types'

export function hatLabel(hat: ChefHat): string {
  if (hat.kind === 'auto') return t('diychef.hatAuto')
  if (hat.kind === 'none') return t('diychef.hatNone')
  if (hat.kind === 'native') {
    if (hat.config_value === 'Fancy') return t('diychef.hatFancy')
    if (hat.config_value === 'Santa') return t('diychef.hatSanta')
    if (hat.config_value === 'Baseballcap') return t('diychef.hatBaseball')
  }
  return hat.display_name
}
