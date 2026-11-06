import { computed, reactive, watch } from 'vue'
import { I18N_FIELD_ORDER, blankI18nField, defaultI18nValues, followDefaults, i18nDraftFrom, i18nTableOf, missingRequiredLanguages, type I18nFieldId } from '../mapPublishI18n'
import type { MapPublishMetadata, PublishZipInspection } from '../types'

import { RELATION_KINDS, emptyRelationTables, recordToRows, relationsToRecord, overrideDrafts, buildOverrides, relationIssue, type PublishRelationOverride } from '../mapPublishRelations'

/** 资料草稿负责默认语言、作者修改标记和关系表，不持有发布会话。 */
export function useMapPublishDraft() {
  const draft = reactive({
    levelSetName: '', levelSetNameZh: '', author: '',
    nameI18n: blankI18nField(), groupI18n: blankI18nField(), noteI18n: blankI18nField(),
    ...emptyRelationTables(),
    releaseRelations: [] as PublishRelationOverride[],
  })
  let applying = false
  const defaults = () => ({ levelSetName: draft.levelSetName, levelSetNameZh: draft.levelSetNameZh, author: draft.author })
  watch(() => [draft.levelSetName, draft.levelSetNameZh, draft.author], () => {
    if (applying) return
    for (const field of I18N_FIELD_ORDER) draft[field.id] = followDefaults(draft[field.id], field.id, defaults())
  }, { flush: 'sync' })

  function reset(metadata: MapPublishMetadata | null) {
    applying = true
    try {
      draft.levelSetName = metadata?.level_set_name ?? ''
      draft.levelSetNameZh = metadata?.level_set_name_zh ?? ''
      draft.author = metadata?.author ?? ''
      for (const kind of RELATION_KINDS) draft[kind] = recordToRows(metadata?.[kind] ?? {})
      draft.releaseRelations = overrideDrafts(metadata?.release_relations)
      draft.nameI18n = i18nDraftFrom('nameI18n', metadata?.name_i18n, defaults())
      draft.groupI18n = i18nDraftFrom('groupI18n', metadata?.group_i18n, defaults())
      draft.noteI18n = i18nDraftFrom('noteI18n', metadata?.note_i18n, defaults())
    } finally { applying = false }
  }

  function applyInspection(zip: PublishZipInspection) {
    draft.levelSetName = zip.level_set_name
    draft.levelSetNameZh = zip.level_set_name_zh
    draft.author = zip.author
    if (!draft.requires.length) draft.requires = recordToRows(zip.requires)
  }

  const missingFields = computed(() => I18N_FIELD_ORDER.flatMap(field =>
    missingRequiredLanguages(draft[field.id]).map(language => `i18n-${field.id}-${language}`)))

  function setI18nValue(field: I18nFieldId, language: string, value: string) {
    draft[field].values[language] = value
    draft[field].touched[language] = true
  }

  function buildFields() {
    const table = (field: I18nFieldId) => {
      const values = i18nTableOf(draft[field])
      return Object.keys(values).length ? values : defaultI18nValues(field, defaults())
    }
    return {
      level_set_name: draft.levelSetName.trim(), level_set_name_zh: draft.levelSetNameZh.trim(), author: draft.author.trim(),
      name_i18n: table('nameI18n'), group_i18n: table('groupI18n'), note_i18n: table('noteI18n'),
      requires: relationsToRecord(draft.requires), compatible: relationsToRecord(draft.compatible), conflicts: relationsToRecord(draft.conflicts),
      release_relations: buildOverrides(draft.releaseRelations),
    }
  }
  const invalidRelation = computed(() => relationIssue(draft, draft.releaseRelations))
  return { draft, reset, applyInspection, missingFields, invalidRelation, setI18nValue, buildFields }
}
