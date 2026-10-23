import type { TutorialLocale, TutorialScenario, TutorialStepConfig } from '../types'

export type View = 'overview' | 'framework' | 'installed' | 'repositories' | 'sponsor'
export type TutorialView = View
export type TutorialStep = Omit<TutorialStepConfig, 'title_i18n' | 'description_i18n'> & {
  view?: View | ''
  scenario: TutorialScenario
  title_i18n: Record<TutorialLocale, string>
  description_i18n: Record<TutorialLocale, string>
}

export type TutorialTranslate = (source: string) => string

export const tutorialViews: readonly View[] = ['overview', 'framework', 'installed', 'repositories', 'sponsor']

export const tutorialLocaleOptions: Array<{ code: TutorialLocale; label: string }> = [
  { code: 'zh-CN', label: '简体中文' },
  { code: 'zh-TW', label: '繁體中文' },
  { code: 'en', label: 'English' },
  { code: 'ko', label: '한국어' },
  { code: 'ja', label: '日本語' },
]

export const tutorialScenarioOptions: Array<{ value: TutorialScenario; label: string }> = [
  { value: 'live', label: '使用当前真实数据' },
  { value: 'overview', label: '教程总览数据' },
  { value: 'framework-missing', label: '框架未安装' },
  { value: 'framework-channel', label: '框架安装渠道' },
  { value: 'framework-installing', label: '框架安装进度' },
  { value: 'framework-ready', label: '框架已安装' },
  { value: 'framework-backup', label: '框架备份' },
  { value: 'mod-management', label: 'MOD 管理' },
  { value: 'repository-loading', label: '仓库源加载中' },
  { value: 'repository-ready', label: '仓库源已加载' },
  { value: 'source-manager', label: '仓库源管理菜单' },
  { value: 'repository-editor', label: '仓库编辑菜单' },
  { value: 'repository-install', label: '仓库安装' },
  { value: 'release-select', label: '选择 Release' },
  { value: 'download-progress', label: '下载进度' },
  { value: 'sponsor', label: '赞助渠道' },
  { value: 'completion', label: '教程完成' },
]

export const tutorialRepositoryLoadingStepIds: ReadonlySet<string> = new Set([
  'repository-loading', 'repository-refresh',
])

export function normalizeTutorialView(view?: string): View | undefined {
  return tutorialViews.some((item) => item === view) ? view as View : undefined
}

export function normalizeTutorialScenario(value?: string): TutorialScenario {
  return tutorialScenarioOptions.some((item) => item.value === value)
    ? value as TutorialScenario
    : 'live'
}

export function normalizeTutorialTextMap(
  values: Partial<Record<TutorialLocale, string>> | undefined,
  fallback: string,
): Record<TutorialLocale, string> {
  return tutorialLocaleOptions.reduce((result, option) => {
    result[option.code] = values?.[option.code]?.trim() || (option.code === 'zh-CN' ? fallback : '')
    return result
  }, {} as Record<TutorialLocale, string>)
}

export function normalizeTutorialStep(step: TutorialStepConfig): TutorialStep {
  // Saved copies of the former built-in step still contain its obsolete DOM path.
  let selector = step.selector
  if (step.scenario === 'mod-management') {
    if (step.id === 'mod-package-child-actions'
      && selector === '#app > div > main > div.main-scroll > section > div.panel.table-panel > div.mod-row-shell.mod-package-shell.has-mod-details > div.table-row.mod-package-child.mod-package-child-first > div.mod-controls') {
      selector = '[data-mod-file="ConfigurationManager.dll"] [data-tutorial="mod-actions"]'
    } else if (step.id === 'mod-package-child-details'
      && selector === '#app > div > main > div.main-scroll > section > div.panel.table-panel > div:nth-child(3) > div > div.mod-cell > div > div > button') {
      selector = '[data-mod-file="ConfigurationManager.dll"] [data-tutorial="mod-details-toggle"]'
    }
  }
  return {
    ...step,
    selector,
    view: normalizeTutorialView(step.view) ?? '',
    scenario: normalizeTutorialScenario(step.scenario),
    title_i18n: normalizeTutorialTextMap(step.title_i18n, step.title),
    description_i18n: normalizeTutorialTextMap(step.description_i18n, step.description),
  }
}

export function cloneTutorialSteps(steps: TutorialStepConfig[]): TutorialStep[] {
  return steps.map(normalizeTutorialStep)
}

export function tutorialStepText(
  step: TutorialStep,
  field: 'title' | 'description',
  locale: TutorialLocale,
  translate: TutorialTranslate,
): string {
  const map = field === 'title' ? step.title_i18n : step.description_i18n
  return map[locale]?.trim() || map['zh-CN']?.trim() || translate(step[field])
}

export function resolveTutorialView(steps: TutorialStep[], index: number): View | undefined {
  for (let current = index; current >= 0; current -= 1) {
    const view = normalizeTutorialView(steps[current]?.view)
    if (view) return view
  }
  return undefined
}

export function compactTutorialTextMap(values: Record<TutorialLocale, string>): Partial<Record<TutorialLocale, string>> {
  const result: Partial<Record<TutorialLocale, string>> = {}
  for (const option of tutorialLocaleOptions) {
    const value = values[option.code]?.trim()
    if (value) result[option.code] = value
  }
  return result
}

export function tutorialEditorStepValues(steps: TutorialStep[]): TutorialStepConfig[] {
  return steps.map((step, index) => ({
    id: step.id.trim() || `custom-${index + 1}`,
    selector: step.selector.trim(),
    ...(step.view ? { view: step.view } : {}),
    ...(step.scenario && step.scenario !== 'live' ? { scenario: step.scenario } : {}),
    title: step.title_i18n['zh-CN']?.trim() || step.title.trim(),
    description: step.description_i18n['zh-CN']?.trim() || step.description.trim(),
    title_i18n: compactTutorialTextMap(step.title_i18n),
    description_i18n: compactTutorialTextMap(step.description_i18n),
  }))
}

// Japanese text is optional so tutorial files written for the original four
// languages keep importing; missing Japanese falls back to the Chinese copy.
const requiredTutorialLocales = tutorialLocaleOptions.filter((option) => option.code !== 'ja')

export function validateTutorialLanguageMaps(steps: TutorialStep[]): boolean {
  return steps.every((step) => requiredTutorialLocales.every((option) =>
    !!step.title_i18n[option.code]?.trim() && !!step.description_i18n[option.code]?.trim()))
}

export const tutorialFormatTemplate = `version = 1

[[steps]]
id = "my-tutorial-step"
selector = ".page-intro"
view = "overview"
scenario = "live"
title = "中文标题"
description = "中文说明"

[steps.title_i18n]
"zh-CN" = "简体中文标题"
"zh-TW" = "繁體中文標題"
en = "English title"
ko = "한국어 제목"
ja = "日本語タイトル"

[steps.description_i18n]
"zh-CN" = "简体中文说明"
"zh-TW" = "繁體中文說明"
en = "English description"
ko = "한국어 설명"
ja = "日本語の説明"
`
