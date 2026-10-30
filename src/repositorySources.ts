import { t } from './i18n'
import { normalizeRepositoryRegion } from './repositoryAvailability'

/**
 * 仓库源地区的展示辅助。
 *
 * 判定逻辑在 `repositoryAvailability.ts`，那里不依赖 i18n，
 * 可以给 Node 测试直接引用；这里只保留需要翻译的文案。
 * 转出 `sourceMatchesRegion` 是为了让页面按来源筛源时不必多引一个模块。
 */
export { sourceMatchesRegion } from './repositoryAvailability'

/** 渠道展示文案（页头用）。 */
export function repositoryRegionLabel(region: string | undefined): string {
  return normalizeRepositoryRegion(region) === 'chinese_mainland'
    ? t('tools.mainlandChina')
    : t('tools.international')
}
